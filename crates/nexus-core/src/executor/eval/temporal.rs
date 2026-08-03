//! Date/time + duration arithmetic. Detects tagged temporal values (see
//! [`super::temporal_value`]) and legacy ISO datetime strings, and
//! implements add/subtract/difference between datetimes and durations
//! (both directions).
//!
//! Operands may arrive either as a tagged intermediate temporal value
//! (`date`/`datetime`/`duration`/... built by `fn_temporal.rs`'s
//! constructors) or as a plain ISO string (a literal the parser never
//! routed through a temporal constructor). [`coerce_temporal_instant`]
//! bridges the former down to the latter at each function's entry point, so
//! the chrono parsing below — unchanged from before the typed-value work —
//! only ever has to deal with one shape.

use super::super::engine::Executor;
use super::temporal_parse;
use super::temporal_retag;
use super::temporal_value;
use crate::{Error, Result};
use chrono::{Datelike, TimeZone, Timelike};
use serde_json::Value;

/// Combines `years * 12 + months` into a total month delta using checked
/// arithmetic. Both components come from user-controlled duration literals
/// (e.g. `duration({years: 9223372036854775807})`), so a plain `*`/`+`
/// panics on overflow in debug builds and silently wraps in release.
fn checked_total_months(years: i64, months: i64) -> Result<i64> {
    years
        .checked_mul(12)
        .and_then(|m| m.checked_add(months))
        .ok_or_else(|| {
            Error::CypherExecution(
                "duration arithmetic overflow: year/month component exceeds i64 range".to_string(),
            )
        })
}

/// Combines `days*86400 + hours*3600 + minutes*60 + seconds` into a total
/// second delta using checked arithmetic, for the same reason as
/// [`checked_total_months`].
fn checked_duration_secs(days: i64, hours: i64, minutes: i64, seconds: i64) -> Result<i64> {
    let overflow = || {
        Error::CypherExecution(
            "duration arithmetic overflow: day/hour/minute/second component exceeds i64 range"
                .to_string(),
        )
    };
    let d = days.checked_mul(86400).ok_or_else(overflow)?;
    let h = hours.checked_mul(3600).ok_or_else(overflow)?;
    let m = minutes.checked_mul(60).ok_or_else(overflow)?;
    d.checked_add(h)
        .and_then(|dh| dh.checked_add(m))
        .and_then(|dhm| dhm.checked_add(seconds))
        .ok_or_else(overflow)
}

/// Applies a signed month delta (positive to add, negative to subtract) to
/// a (year, month) pair using checked arithmetic throughout — including the
/// final `i64 -> i32` year narrowing, which chrono's own `NaiveDate::with_year`
/// takes as a bare `i32` and would otherwise wrap silently for out-of-range
/// years. Returns a Cypher error instead of panicking or wrapping.
fn checked_month_rollover(
    current_year: i32,
    current_month: u32,
    signed_total_months: i64,
) -> Result<(i32, u32)> {
    let overflow = || {
        Error::CypherExecution(
            "date arithmetic overflow: resulting year/month is out of range".to_string(),
        )
    };
    let new_month = (current_month as i64)
        .checked_add(signed_total_months)
        .ok_or_else(overflow)?;
    let zero_based = new_month.checked_sub(1).ok_or_else(overflow)?;
    let year_offset = zero_based.div_euclid(12);
    let final_month = (zero_based.rem_euclid(12) + 1) as u32;
    let final_year_i64 = (current_year as i64)
        .checked_add(year_offset)
        .ok_or_else(overflow)?;
    let final_year = i32::try_from(final_year_i64).map_err(|_| overflow())?;
    Ok((final_year, final_month))
}

/// The number of days in `month` of `year` (1-based month, Gregorian leap
/// rule). Used to CLAMP a day-of-month after a year/month rollover — e.g.
/// `date('2015-01-31') + duration({months: 1})` must land on
/// `2015-02-28`, not silently no-op back to `2015-01-31` because day 31
/// doesn't exist in February (chrono's own `with_month`/`with_year`
/// return `None` for an out-of-range day, which every call site below
/// used to treat as "do nothing" instead of "clamp the day", matching
/// Neo4j's own month-rollover clamping behaviour).
fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
            if leap { 29 } else { 28 }
        }
        _ => 30,
    }
}

/// Bridges a tagged temporal *instant* (date/localtime/localdatetime/time/
/// datetime) down to its canonical ISO string, so the chrono-based parsing
/// in this module — which predates the typed-value representation — only
/// ever has to handle `Value::String`. Non-instant values (durations,
/// plain strings, anything else) pass through unchanged.
fn coerce_temporal_instant(value: &Value) -> Value {
    if temporal_value::is_temporal_instant(value) {
        if let Some(rendered) = temporal_value::canonicalize_temporal(value) {
            return Value::String(rendered);
        }
    }
    value.clone()
}

/// Combines a whole-second delta with a nanosecond remainder into a single
/// checked `chrono::Duration`, for the date+time arithmetic branches that
/// need nanosecond precision (`Temporal8.feature` scenarios 4/5 pin exact
/// nanosecond-precision sums).
fn checked_seconds_and_nanos_delta(duration_secs: i64, nanos: i32) -> Result<chrono::Duration> {
    let overflow = || {
        Error::CypherExecution(
            "duration arithmetic overflow: seconds component is outside chrono's representable range".to_string(),
        )
    };
    let secs_delta = chrono::Duration::try_seconds(duration_secs).ok_or_else(overflow)?;
    let nanos_delta = chrono::Duration::nanoseconds(i64::from(nanos));
    secs_delta.checked_add(&nanos_delta).ok_or_else(overflow)
}

/// Applies a duration's day/hour/minute/second/nanosecond component to a
/// bare time-of-day, wrapping modulo 24h. Per Neo4j/openCypher TCK
/// `Temporal8.feature` scenarios 2-3: a duration's `years`/`months`
/// component has no fixed length against a dateless instant and is
/// ignored entirely (already excluded — this only ever receives
/// `days`/`seconds`/`nanos`); the `days` component contributes a multiple
/// of a full 24h period, which the `rem_euclid` wrap cancels out
/// regardless of its magnitude, so it's folded in unconditionally rather
/// than special-cased away.
fn apply_duration_to_time_of_day(
    hour: u32,
    minute: u32,
    second: u32,
    nanosecond: u32,
    days: i64,
    seconds: i64,
    nanos: i32,
    negate: bool,
) -> (u32, u32, u32, u32) {
    const NANOS_PER_DAY: i128 = 86_400_000_000_000;
    let base: i128 = (i128::from(hour) * 3600 + i128::from(minute) * 60 + i128::from(second))
        * 1_000_000_000
        + i128::from(nanosecond);
    let delta: i128 =
        (i128::from(days) * 86_400 + i128::from(seconds)) * 1_000_000_000 + i128::from(nanos);
    let signed_delta = if negate { -delta } else { delta };
    let total = (base + signed_delta).rem_euclid(NANOS_PER_DAY);
    let total_seconds = (total / 1_000_000_000) as i64;
    let nanosecond = (total % 1_000_000_000) as u32;
    let hour = (total_seconds / 3600) as u32;
    let minute = ((total_seconds % 3600) / 60) as u32;
    let second = (total_seconds % 60) as u32;
    (hour, minute, second, nanosecond)
}

/// Applies (or, when `negate`, subtracts) a duration's components to an
/// already-retagged temporal instant, dispatching on its kind, and
/// re-renders the result via [`temporal_value::canonicalize_temporal`].
///
/// This builds the result arithmetically from `tagged`'s own already-parsed
/// components (via [`temporal_value::date_components`]/`time_components`/
/// `offset_seconds`/`zone_name`) instead of re-parsing `dt_str` through a
/// `chrono` string-format specifier: those specifiers (RFC3339, `"%Y-%m-
/// %dT%H:%M:%S"`) require a fixed field width Neo4j's own canonical
/// rendering doesn't always have (e.g. no seconds field at all when
/// they're zero — see `temporal_value::render_time_of_day`'s doc comment),
/// so a string this module's own renderer produces would not necessarily
/// round-trip back through them. Building from `tagged`'s components
/// instead sidesteps that mismatch entirely — the input only had to parse
/// *once*, via [`temporal_retag::retag_canonical_string`], not twice.
fn apply_duration_to_tagged_instant(
    tagged: &Value,
    years: i64,
    months: i64,
    days: i64,
    hours: i64,
    minutes: i64,
    seconds: i64,
    nanos: i32,
    negate: bool,
) -> Result<Value> {
    use temporal_value::TemporalKind;

    let sign: i64 = if negate { -1 } else { 1 };
    let overflow_date = || {
        Error::CypherExecution(
            "date arithmetic overflow: result is outside the representable date range".to_string(),
        )
    };
    let overflow_dt = || {
        Error::CypherExecution(
            "datetime arithmetic overflow: result is outside chrono's representable date range"
                .to_string(),
        )
    };

    match temporal_value::temporal_kind(tagged) {
        Some(TemporalKind::Date) => {
            let (year, month, day) =
                temporal_value::date_components(tagged).ok_or_else(overflow_date)?;
            let mut result =
                chrono::NaiveDate::from_ymd_opt(year, month, day).ok_or_else(overflow_date)?;

            if years != 0 || months != 0 {
                let total_months = checked_total_months(years, months)?
                    .checked_mul(sign)
                    .ok_or_else(overflow_date)?;
                let (final_year, final_month) =
                    checked_month_rollover(result.year(), result.month(), total_months)?;
                let clamped_day = result.day().min(days_in_month(final_year, final_month));
                if let Some(new_dt) = result
                    .with_day(1)
                    .and_then(|d| d.with_year(final_year))
                    .and_then(|d| d.with_month(final_month))
                    .and_then(|d| d.with_day(clamped_day))
                {
                    result = new_dt;
                }
            }

            // A date-only instant has no time-of-day to absorb
            // hours/minutes/seconds against, but a whole-day overflow in
            // those fields still shifts the date: fold
            // `hours*3600 + minutes*60 + seconds` into a whole-day count
            // via plain (truncating-toward-zero) integer division —
            // Neo4j truncates here, it does NOT floor — discarding the
            // sub-day, and any sub-second/nanosecond, remainder (there is
            // nowhere for it to go on a `Date`), and add it to `days`
            // BEFORE `sign` is applied, exactly like `days` itself.
            //
            // What the openCypher TCK (`Temporal8.feature` Scenario [1]
            // row 3) actually pins is the PRE-SIGN placement, not the
            // rounding mode: `{..., hours: 16.5, minutes: 12.5,
            // seconds: 70.5, ...}` -> normalized total seconds 122293
            // (i.e. 1 day 9h58m13.5s beyond `days`); `1984-10-11 + <dur>`
            // needs the extra day to land on `1997-10-11` (not
            // `1997-10-10`), and `1984-10-11 - <dur>` needs it to land on
            // `1971-10-12` (not `1971-10-13`) — both directions only agree
            // when the extra day is folded into `days` pre-sign, not
            // post-sign (a post-sign fold would subtract, not add, the
            // extra day in the `diff` direction). `div_euclid` would also
            // satisfy that pre-sign placement, but it floors: a negative
            // sub-day remainder with no `days` component
            // (`duration({seconds: -1})`) would floor to `-1` and invent a
            // phantom day shift (`1984-10-11 -> 1984-10-10`) that Neo4j
            // does not produce — Neo4j's own duration arithmetic
            // truncates toward zero, matching plain `/`.
            let dayless_seconds = checked_duration_secs(0, hours, minutes, seconds)?;
            let extra_days = dayless_seconds / 86400;
            let total_days = days.checked_add(extra_days).ok_or_else(overflow_date)?;
            let signed_days = total_days.checked_mul(sign).ok_or_else(overflow_date)?;
            let delta = chrono::Duration::try_days(signed_days).ok_or_else(overflow_date)?;
            result = result.checked_add_signed(delta).ok_or_else(overflow_date)?;

            let candidate = temporal_value::make_date(result.year(), result.month(), result.day());
            Ok(Value::String(
                temporal_value::canonicalize_temporal(&candidate).unwrap_or_default(),
            ))
        }
        Some(kind @ (TemporalKind::LocalDateTime | TemporalKind::DateTime)) => {
            let (year, month, day) =
                temporal_value::date_components(tagged).ok_or_else(overflow_dt)?;
            let (hour, minute, second, nanosecond) =
                temporal_value::time_components(tagged).ok_or_else(overflow_dt)?;
            let date = chrono::NaiveDate::from_ymd_opt(year, month, day).ok_or_else(overflow_dt)?;
            let time = chrono::NaiveTime::from_hms_nano_opt(hour, minute, second, nanosecond)
                .ok_or_else(overflow_dt)?;
            let mut result = chrono::NaiveDateTime::new(date, time);

            if years != 0 || months != 0 {
                let total_months = checked_total_months(years, months)?
                    .checked_mul(sign)
                    .ok_or_else(overflow_dt)?;
                let (final_year, final_month) =
                    checked_month_rollover(result.year(), result.month(), total_months)?;
                let clamped_day = result.day().min(days_in_month(final_year, final_month));
                if let Some(new_dt) = result
                    .with_day(1)
                    .and_then(|d| d.with_year(final_year))
                    .and_then(|d| d.with_month(final_month))
                    .and_then(|d| d.with_day(clamped_day))
                {
                    result = new_dt;
                }
            }

            let duration_secs = checked_duration_secs(days, hours, minutes, seconds)?
                .checked_mul(sign)
                .ok_or_else(overflow_dt)?;
            let signed_nanos =
                i32::try_from(i64::from(nanos).checked_mul(sign).ok_or_else(overflow_dt)?)
                    .map_err(|_| overflow_dt())?;
            let delta = checked_seconds_and_nanos_delta(duration_secs, signed_nanos)?;
            result = result.checked_add_signed(delta).ok_or_else(overflow_dt)?;

            let candidate = if kind == TemporalKind::DateTime {
                let offset = temporal_value::offset_seconds(tagged).unwrap_or(0);
                let zone = temporal_value::zone_name(tagged).map(str::to_string);
                temporal_value::make_datetime(
                    result.year(),
                    result.month(),
                    result.day(),
                    result.hour(),
                    result.minute(),
                    result.second(),
                    result.nanosecond(),
                    offset,
                    zone,
                )
            } else {
                temporal_value::make_localdatetime(
                    result.year(),
                    result.month(),
                    result.day(),
                    result.hour(),
                    result.minute(),
                    result.second(),
                    result.nanosecond(),
                )
            };
            Ok(Value::String(
                temporal_value::canonicalize_temporal(&candidate).unwrap_or_default(),
            ))
        }
        Some(kind @ (TemporalKind::LocalTime | TemporalKind::Time)) => {
            let (hour, minute, second, nanosecond) =
                temporal_value::time_components(tagged).ok_or_else(overflow_dt)?;
            // `days`/`hours`/`minutes`/`seconds` here are the legacy
            // decomposed 7-tuple fields (`seconds` is the seconds-OF-
            // MINUTE remainder, not the duration's total) —
            // `apply_duration_to_time_of_day` needs the total day/second
            // count `duration_components` itself carries, so reconstitute
            // it via the same helper the date+time branch above uses for
            // its own seconds delta.
            let total_seconds = checked_duration_secs(days, hours, minutes, seconds)?;
            let (nh, nm, ns, nns) = apply_duration_to_time_of_day(
                hour,
                minute,
                second,
                nanosecond,
                0,
                total_seconds,
                nanos,
                negate,
            );
            let candidate = if kind == TemporalKind::Time {
                let offset = temporal_value::offset_seconds(tagged).unwrap_or(0);
                temporal_value::make_time(nh, nm, ns, nns, offset)
            } else {
                temporal_value::make_localtime(nh, nm, ns, nns)
            };
            Ok(Value::String(
                temporal_value::canonicalize_temporal(&candidate).unwrap_or_default(),
            ))
        }
        _ => Ok(Value::Null),
    }
}

/// Computes the duration between two already-retagged temporal instants,
/// dispatching on whether either side carries a time part. Both
/// date-only (`Date`) and date+time-bearing (`LocalDateTime`/`DateTime`)
/// pairs are supported, mirroring the legacy RFC3339/`NaiveDate`
/// fallback's own two cases; a mixed pairing this can't resolve (e.g.
/// either side is a bare time-of-day with no date) returns `None` so the
/// caller can fall through. Sub-second precision is intentionally left at
/// `0` here, same as the pre-existing behaviour this replaces — full
/// nanosecond-precision `datetime - datetime` is not part of this
/// module's TCK-verified scope.
fn tagged_datetime_difference(left: &Value, right: &Value) -> Option<Value> {
    let left_time = temporal_value::time_components(left);
    let right_time = temporal_value::time_components(right);

    if left_time.is_none() && right_time.is_none() {
        let (ly, lm, ld) = temporal_value::date_components(left)?;
        let (ry, rm, rd) = temporal_value::date_components(right)?;
        let l = chrono::NaiveDate::from_ymd_opt(ly, lm, ld)?;
        let r = chrono::NaiveDate::from_ymd_opt(ry, rm, rd)?;
        let days = l.signed_duration_since(r).num_days();
        return Some(temporal_value::make_duration(0, days, 0, 0));
    }

    let (ly, lm, ld) = temporal_value::date_components(left)?;
    let (lh, lmin, ls, _) = left_time?;
    let (ry, rm, rd) = temporal_value::date_components(right)?;
    let (rh, rmin, rs, _) = right_time?;
    let left_offset = temporal_value::offset_seconds(left).unwrap_or(0);
    let right_offset = temporal_value::offset_seconds(right).unwrap_or(0);

    let l = chrono::NaiveDate::from_ymd_opt(ly, lm, ld)?.and_hms_opt(lh, lmin, ls)?;
    let r = chrono::NaiveDate::from_ymd_opt(ry, rm, rd)?.and_hms_opt(rh, rmin, rs)?;
    let left_utc_seconds = l.and_utc().timestamp() - i64::from(left_offset);
    let right_utc_seconds = r.and_utc().timestamp() - i64::from(right_offset);
    let total_seconds = left_utc_seconds - right_utc_seconds;
    let days = total_seconds / 86400;
    let remaining_seconds = total_seconds % 86400;
    Some(temporal_value::make_duration(0, days, remaining_seconds, 0))
}

/// Scales a tagged `duration` value by a floating-point `scalar` — the
/// shared algorithm behind both `duration * number` and `duration /
/// number` (division is scaling by `1.0 / number`). Mirrors Neo4j's own
/// `DurationValue.multipliedBy`: months scale first, any fractional-month
/// remainder carries into days via the same average-month-length constant
/// [`temporal_parse::parse_iso_duration`] uses for string parsing, then any
/// fractional-day remainder carries into seconds, then any
/// fractional-second remainder carries into nanoseconds. Verified against
/// the openCypher TCK's `Temporal8.feature` scenario 7 expectation table
/// (both the `* 2`/`/ 2` integer-fast-path rows and the `* 0.5`/`/ 0.5`
/// rows, which exercise the full fractional carry chain).
fn scale_duration(duration: &Value, scalar: f64) -> Result<Value> {
    let overflow = || {
        Error::CypherExecution(
            "duration arithmetic overflow: scaled component exceeds i64 range".to_string(),
        )
    };
    let Some((months, days, seconds, nanos)) = temporal_value::duration_components(duration) else {
        return Err(Error::CypherExecution(
            "expected a DURATION value to scale".to_string(),
        ));
    };

    let months_total = months as f64 * scalar;
    let months_int = months_total.trunc();
    let months_frac = months_total - months_int;

    let days_total = days as f64 * scalar
        + months_frac * (temporal_parse::AVG_SECONDS_PER_MONTH / temporal_parse::SECONDS_PER_DAY);
    let days_int = days_total.trunc();
    let days_frac = days_total - days_int;

    let seconds_total = seconds as f64 * scalar + days_frac * temporal_parse::SECONDS_PER_DAY;
    let seconds_int = seconds_total.trunc();
    let seconds_frac = seconds_total - seconds_int;

    // Truncate (not round) the final sub-nanosecond remainder: verified
    // against the openCypher TCK's `Temporal8.feature` scenario 7 `/ 2`
    // row, where a source `nanoseconds: 1` scaled by `0.5` renders with
    // NO nanosecond digit at all (`...M8S`, not `...M8.000000001S`) —
    // Neo4j discards the sub-nanosecond fraction rather than rounding it
    // up to the nearest nanosecond.
    let nanos_total = f64::from(nanos) * scalar + seconds_frac * 1_000_000_000.0;
    let nanos_final = nanos_total.trunc();

    let months_i64 = temporal_parse::to_i64(months_int).ok_or_else(overflow)?;
    let days_i64 = temporal_parse::to_i64(days_int).ok_or_else(overflow)?;
    let seconds_i64 = temporal_parse::to_i64(seconds_int).ok_or_else(overflow)?;
    let nanos_i64 = temporal_parse::to_i64(nanos_final).ok_or_else(overflow)?;
    let nanos_i32 = i32::try_from(nanos_i64).map_err(|_| overflow())?;

    Ok(temporal_value::make_duration(
        months_i64,
        days_i64,
        seconds_i64,
        nanos_i32,
    ))
}

impl Executor {
    /// True when `value` is usable as the duration side of datetime
    /// arithmetic: either a tagged `duration`, or a stored (plain
    /// `Value::String`) canonical duration rendering — see
    /// [`temporal_retag`]'s module doc for the storage design decision
    /// this re-derives against.
    pub(in crate::executor) fn is_duration_object(value: &Value) -> bool {
        temporal_retag::retag_duration(value).is_some()
    }

    /// True when `value` is usable as the instant side of datetime
    /// arithmetic: a tagged temporal instant, a stored (plain
    /// `Value::String`) canonical instant rendering, or (backward
    /// compatibility for any code path that still hands this a raw
    /// literal) a plain ISO date/datetime string that isn't an exact
    /// canonical rendering.
    pub(in crate::executor) fn is_datetime_string(value: &Value) -> bool {
        if temporal_value::is_temporal_instant(value) {
            return true;
        }
        if let Value::String(s) = value {
            if let Some(tagged) = temporal_retag::retag_canonical_string(s) {
                return temporal_value::is_temporal_instant(&tagged);
            }
            return chrono::DateTime::parse_from_rfc3339(s).is_ok()
                || chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").is_ok()
                || chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok();
        }
        false
    }

    /// Extract duration components as (years, months, days, hours,
    /// minutes, seconds, nanoseconds).
    ///
    /// Re-derives `value` into a tagged `duration` first (via
    /// [`temporal_retag::retag_duration`] — handles both an
    /// already-tagged value and a stored canonical string alike), then
    /// reads its normalized `(months, days, seconds, nanos)` fields and
    /// re-expands them into this legacy seven-component shape (`years`
    /// always `0` — the tagged value already folds years into `months`)
    /// so the arithmetic below, which predates the typed-value
    /// representation, needs minimal further changes.
    pub(in crate::executor) fn extract_duration_components(
        value: &Value,
    ) -> (i64, i64, i64, i64, i64, i64, i32) {
        let Some(tagged) = temporal_retag::retag_duration(value) else {
            return (0, 0, 0, 0, 0, 0, 0);
        };
        if let Some((months, days, seconds, nanos)) = temporal_value::duration_components(&tagged) {
            let hours = seconds / 3600;
            let rem = seconds % 3600;
            let minutes = rem / 60;
            let secs = rem % 60;
            return (0, months, days, hours, minutes, secs, nanos);
        }
        (0, 0, 0, 0, 0, 0, 0)
    }

    /// Try to add datetime + duration
    pub(in crate::executor) fn try_datetime_add(
        &self,
        left: &Value,
        right: &Value,
    ) -> Result<Option<Value>> {
        // datetime + duration
        if Self::is_datetime_string(left) && Self::is_duration_object(right) {
            return self.datetime_add_duration(left, right).map(Some);
        }
        // duration + datetime (commutative)
        if Self::is_duration_object(left) && Self::is_datetime_string(right) {
            return self.datetime_add_duration(right, left).map(Some);
        }
        Ok(None)
    }

    /// Try to add duration + duration
    pub(in crate::executor) fn try_duration_add(
        &self,
        left: &Value,
        right: &Value,
    ) -> Result<Option<Value>> {
        if let (Some((m1, d1, s1, n1)), Some((m2, d2, s2, n2))) = (
            temporal_retag::retag_duration(left)
                .and_then(|v| temporal_value::duration_components(&v)),
            temporal_retag::retag_duration(right)
                .and_then(|v| temporal_value::duration_components(&v)),
        ) {
            let overflow = |unit: &str| {
                Error::CypherExecution(format!(
                    "duration arithmetic overflow: {unit} component exceeds i64 range"
                ))
            };
            let months = m1.checked_add(m2).ok_or_else(|| overflow("months"))?;
            let days = d1.checked_add(d2).ok_or_else(|| overflow("days"))?;
            let seconds = s1.checked_add(s2).ok_or_else(|| overflow("seconds"))?;
            let nanos = n1.checked_add(n2).ok_or_else(|| overflow("nanoseconds"))?;
            return Ok(Some(temporal_value::make_duration(
                months, days, seconds, nanos,
            )));
        }
        Ok(None)
    }

    /// Try `duration * number` or `number * duration` (commutative) — see
    /// [`scale_duration`] and `Temporal8.feature` scenario 7. The
    /// duration side may be a tagged value or a stored canonical string
    /// (re-derived via [`temporal_retag::retag_duration`]); the number
    /// side must be an actual JSON `Value::Number` (no string coercion).
    pub(in crate::executor) fn try_duration_multiply(
        left: &Value,
        right: &Value,
    ) -> Result<Option<Value>> {
        if let (Some(duration), Some(scalar)) =
            (temporal_retag::retag_duration(left), right.as_f64())
        {
            return scale_duration(&duration, scalar).map(Some);
        }
        if let (Some(scalar), Some(duration)) =
            (left.as_f64(), temporal_retag::retag_duration(right))
        {
            return scale_duration(&duration, scalar).map(Some);
        }
        Ok(None)
    }

    /// Try `duration / number` — see [`scale_duration`] and
    /// `Temporal8.feature` scenario 7. `number / duration` has no
    /// meaning in Cypher and is intentionally not handled here.
    pub(in crate::executor) fn try_duration_divide(
        left: &Value,
        right: &Value,
    ) -> Result<Option<Value>> {
        let (Some(duration), Some(scalar)) = (temporal_retag::retag_duration(left), right.as_f64())
        else {
            return Ok(None);
        };
        if scalar == 0.0 {
            return Err(Error::TypeMismatch {
                expected: "non-zero".to_string(),
                actual: "division by zero".to_string(),
            });
        }
        scale_duration(&duration, 1.0 / scalar).map(Some)
    }

    /// Try to subtract datetime - duration
    pub(in crate::executor) fn try_datetime_subtract(
        &self,
        left: &Value,
        right: &Value,
    ) -> Result<Option<Value>> {
        if Self::is_datetime_string(left) && Self::is_duration_object(right) {
            return self.datetime_subtract_duration(left, right).map(Some);
        }
        Ok(None)
    }

    /// Try to compute datetime - datetime (returns duration)
    pub(in crate::executor) fn try_datetime_diff(
        &self,
        left: &Value,
        right: &Value,
    ) -> Result<Option<Value>> {
        if Self::is_datetime_string(left) && Self::is_datetime_string(right) {
            return self.datetime_difference(left, right).map(Some);
        }
        Ok(None)
    }

    /// Try to subtract duration - duration
    pub(in crate::executor) fn try_duration_subtract(
        &self,
        left: &Value,
        right: &Value,
    ) -> Result<Option<Value>> {
        if let (Some((m1, d1, s1, n1)), Some((m2, d2, s2, n2))) = (
            temporal_retag::retag_duration(left)
                .and_then(|v| temporal_value::duration_components(&v)),
            temporal_retag::retag_duration(right)
                .and_then(|v| temporal_value::duration_components(&v)),
        ) {
            let overflow = |unit: &str| {
                Error::CypherExecution(format!(
                    "duration arithmetic overflow: {unit} component exceeds i64 range"
                ))
            };
            let months = m1.checked_sub(m2).ok_or_else(|| overflow("months"))?;
            let days = d1.checked_sub(d2).ok_or_else(|| overflow("days"))?;
            let seconds = s1.checked_sub(s2).ok_or_else(|| overflow("seconds"))?;
            let nanos = n1.checked_sub(n2).ok_or_else(|| overflow("nanoseconds"))?;
            return Ok(Some(temporal_value::make_duration(
                months, days, seconds, nanos,
            )));
        }
        Ok(None)
    }

    /// Add duration to datetime
    pub(in crate::executor) fn datetime_add_duration(
        &self,
        datetime: &Value,
        duration: &Value,
    ) -> Result<Value> {
        let datetime = coerce_temporal_instant(datetime);
        let (years, months, days, hours, minutes, seconds, nanos) =
            Self::extract_duration_components(duration);

        if let Value::String(dt_str) = &datetime {
            // Every tagged instant this module ever receives renders, via
            // `coerce_temporal_instant`, to exactly one of this module's
            // own canonical ISO shapes — re-derive it directly (see
            // `apply_duration_to_tagged_instant`'s doc comment for why
            // that's not the same as the legacy chrono-format-string
            // parsing below, which a seconds-omitted rendering can't
            // necessarily round-trip through).
            if let Some(tagged) = temporal_retag::retag_canonical_string(dt_str) {
                return apply_duration_to_tagged_instant(
                    &tagged, years, months, days, hours, minutes, seconds, nanos, false,
                );
            }

            // Legacy fallback: a raw literal that isn't an exact
            // canonical rendering (e.g. a user-typed RFC3339 string that
            // never routed through a temporal constructor).
            // Try RFC3339 format first
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(dt_str) {
                let mut result = dt.with_timezone(&chrono::Utc);

                // Add years and months using checked arithmetic
                if years != 0 || months != 0 {
                    let total_months = checked_total_months(years, months)?;
                    let (final_year, final_month) =
                        checked_month_rollover(result.year(), result.month(), total_months)?;
                    let clamped_day = result.day().min(days_in_month(final_year, final_month));

                    if let Some(new_dt) = result
                        .with_day(1)
                        .and_then(|d| d.with_year(final_year))
                        .and_then(|d| d.with_month(final_month))
                        .and_then(|d| d.with_day(clamped_day))
                    {
                        result = new_dt;
                    }
                }

                // Add days, hours, minutes, seconds, nanoseconds
                let duration_secs = checked_duration_secs(days, hours, minutes, seconds)?;
                let delta = checked_seconds_and_nanos_delta(duration_secs, nanos)?;
                result = result.checked_add_signed(delta).ok_or_else(|| {
                    Error::CypherExecution(
                        "datetime arithmetic overflow: result is outside chrono's representable date range".to_string(),
                    )
                })?;

                return Ok(Value::String(result.to_rfc3339()));
            }

            // Try NaiveDateTime format
            if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(dt_str, "%Y-%m-%dT%H:%M:%S") {
                let mut result = dt;

                // Add years and months
                if years != 0 || months != 0 {
                    let total_months = checked_total_months(years, months)?;
                    let (final_year, final_month) =
                        checked_month_rollover(result.year(), result.month(), total_months)?;
                    let clamped_day = result.day().min(days_in_month(final_year, final_month));

                    if let Some(new_dt) = result
                        .with_day(1)
                        .and_then(|d| d.with_year(final_year))
                        .and_then(|d| d.with_month(final_month))
                        .and_then(|d| d.with_day(clamped_day))
                    {
                        result = new_dt;
                    }
                }

                // Add days, hours, minutes, seconds, nanoseconds
                let duration_secs = checked_duration_secs(days, hours, minutes, seconds)?;
                let delta = checked_seconds_and_nanos_delta(duration_secs, nanos)?;
                result = result.checked_add_signed(delta).ok_or_else(|| {
                    Error::CypherExecution(
                        "datetime arithmetic overflow: result is outside chrono's representable date range".to_string(),
                    )
                })?;

                return Ok(Value::String(
                    result.format("%Y-%m-%dT%H:%M:%S").to_string(),
                ));
            }

            // Try NaiveDate format
            if let Ok(dt) = chrono::NaiveDate::parse_from_str(dt_str, "%Y-%m-%d") {
                let mut result = dt;

                // Add years and months
                if years != 0 || months != 0 {
                    let total_months = checked_total_months(years, months)?;
                    let (final_year, final_month) =
                        checked_month_rollover(result.year(), result.month(), total_months)?;
                    let clamped_day = result.day().min(days_in_month(final_year, final_month));

                    if let Some(new_dt) = result
                        .with_day(1)
                        .and_then(|d| d.with_year(final_year))
                        .and_then(|d| d.with_month(final_month))
                        .and_then(|d| d.with_day(clamped_day))
                    {
                        result = new_dt;
                    }
                }

                // Add days. Hours/minutes/seconds/nanoseconds have no
                // fixed calendar length against a date-only instant and
                // are ignored entirely (matches Neo4j: `Date + Duration`
                // considers only the duration's year/month/day fields).
                let delta = chrono::Duration::try_days(days).ok_or_else(|| {
                    Error::CypherExecution(
                        "duration arithmetic overflow: days component is outside chrono's representable range".to_string(),
                    )
                })?;
                result = result.checked_add_signed(delta).ok_or_else(|| {
                    Error::CypherExecution(
                        "date arithmetic overflow: result is outside chrono's representable date range".to_string(),
                    )
                })?;

                return Ok(Value::String(result.format("%Y-%m-%d").to_string()));
            }
        }

        Ok(Value::Null)
    }

    /// Subtract duration from datetime
    pub(in crate::executor) fn datetime_subtract_duration(
        &self,
        datetime: &Value,
        duration: &Value,
    ) -> Result<Value> {
        let datetime = coerce_temporal_instant(datetime);
        let (years, months, days, hours, minutes, seconds, nanos) =
            Self::extract_duration_components(duration);

        if let Value::String(dt_str) = &datetime {
            // See the mirror comment in `datetime_add_duration`.
            if let Some(tagged) = temporal_retag::retag_canonical_string(dt_str) {
                return apply_duration_to_tagged_instant(
                    &tagged, years, months, days, hours, minutes, seconds, nanos, true,
                );
            }

            // Legacy fallback: a raw literal that isn't an exact
            // canonical rendering.
            // Try RFC3339 format first
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(dt_str) {
                let mut result = dt.with_timezone(&chrono::Utc);

                // Subtract years and months
                if years != 0 || months != 0 {
                    let total_months = checked_total_months(years, months)?;
                    let negated_months = total_months.checked_neg().ok_or_else(|| {
                        Error::CypherExecution(
                            "duration arithmetic overflow: negating year/month delta exceeds i64 range".to_string(),
                        )
                    })?;
                    let (final_year, final_month) =
                        checked_month_rollover(result.year(), result.month(), negated_months)?;
                    let clamped_day = result.day().min(days_in_month(final_year, final_month));

                    if let Some(new_dt) = result
                        .with_day(1)
                        .and_then(|d| d.with_year(final_year))
                        .and_then(|d| d.with_month(final_month))
                        .and_then(|d| d.with_day(clamped_day))
                    {
                        result = new_dt;
                    }
                }

                // Subtract days, hours, minutes, seconds, nanoseconds
                let duration_secs = checked_duration_secs(days, hours, minutes, seconds)?;
                let delta = checked_seconds_and_nanos_delta(duration_secs, nanos)?;
                result = result.checked_sub_signed(delta).ok_or_else(|| {
                    Error::CypherExecution(
                        "datetime arithmetic overflow: result is outside chrono's representable date range".to_string(),
                    )
                })?;

                return Ok(Value::String(result.to_rfc3339()));
            }

            // Try NaiveDateTime format
            if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(dt_str, "%Y-%m-%dT%H:%M:%S") {
                let mut result = dt;

                // Subtract years and months
                if years != 0 || months != 0 {
                    let total_months = checked_total_months(years, months)?;
                    let negated_months = total_months.checked_neg().ok_or_else(|| {
                        Error::CypherExecution(
                            "duration arithmetic overflow: negating year/month delta exceeds i64 range".to_string(),
                        )
                    })?;
                    let (final_year, final_month) =
                        checked_month_rollover(result.year(), result.month(), negated_months)?;
                    let clamped_day = result.day().min(days_in_month(final_year, final_month));

                    if let Some(new_dt) = result
                        .with_day(1)
                        .and_then(|d| d.with_year(final_year))
                        .and_then(|d| d.with_month(final_month))
                        .and_then(|d| d.with_day(clamped_day))
                    {
                        result = new_dt;
                    }
                }

                // Subtract days, hours, minutes, seconds, nanoseconds
                let duration_secs = checked_duration_secs(days, hours, minutes, seconds)?;
                let delta = checked_seconds_and_nanos_delta(duration_secs, nanos)?;
                result = result.checked_sub_signed(delta).ok_or_else(|| {
                    Error::CypherExecution(
                        "datetime arithmetic overflow: result is outside chrono's representable date range".to_string(),
                    )
                })?;

                return Ok(Value::String(
                    result.format("%Y-%m-%dT%H:%M:%S").to_string(),
                ));
            }

            // Try NaiveDate format
            if let Ok(dt) = chrono::NaiveDate::parse_from_str(dt_str, "%Y-%m-%d") {
                let mut result = dt;

                // Subtract years and months
                if years != 0 || months != 0 {
                    let total_months = checked_total_months(years, months)?;
                    let negated_months = total_months.checked_neg().ok_or_else(|| {
                        Error::CypherExecution(
                            "duration arithmetic overflow: negating year/month delta exceeds i64 range".to_string(),
                        )
                    })?;
                    let (final_year, final_month) =
                        checked_month_rollover(result.year(), result.month(), negated_months)?;
                    let clamped_day = result.day().min(days_in_month(final_year, final_month));

                    if let Some(new_dt) = result
                        .with_day(1)
                        .and_then(|d| d.with_year(final_year))
                        .and_then(|d| d.with_month(final_month))
                        .and_then(|d| d.with_day(clamped_day))
                    {
                        result = new_dt;
                    }
                }

                // Subtract days. Hours/minutes/seconds/nanoseconds have
                // no fixed calendar length against a date-only instant
                // and are ignored entirely (matches Neo4j: `Date -
                // Duration` considers only the duration's
                // year/month/day fields).
                let delta = chrono::Duration::try_days(days).ok_or_else(|| {
                    Error::CypherExecution(
                        "duration arithmetic overflow: days component is outside chrono's representable range".to_string(),
                    )
                })?;
                result = result.checked_sub_signed(delta).ok_or_else(|| {
                    Error::CypherExecution(
                        "date arithmetic overflow: result is outside chrono's representable date range".to_string(),
                    )
                })?;

                return Ok(Value::String(result.format("%Y-%m-%d").to_string()));
            }
        }

        Ok(Value::Null)
    }

    /// Compute difference between two datetimes (returns a tagged
    /// `duration` value — see [`temporal_value::make_duration`] — so it
    /// canonicalizes to an ISO string at the projection boundary instead of
    /// leaking a raw `{...}` object).
    pub(in crate::executor) fn datetime_difference(
        &self,
        left: &Value,
        right: &Value,
    ) -> Result<Value> {
        let left = coerce_temporal_instant(left);
        let right = coerce_temporal_instant(right);
        if let (Value::String(left_str), Value::String(right_str)) = (&left, &right) {
            // See the mirror comment in `datetime_add_duration` — re-derive
            // both operands' own components directly rather than
            // re-parsing through a `chrono` format string that a
            // seconds-omitted canonical rendering may not round-trip
            // through.
            if let (Some(l_tagged), Some(r_tagged)) = (
                temporal_retag::retag_canonical_string(left_str),
                temporal_retag::retag_canonical_string(right_str),
            ) {
                if let Some(result) = tagged_datetime_difference(&l_tagged, &r_tagged) {
                    return Ok(result);
                }
            }

            // Legacy fallback: a raw literal that isn't an exact
            // canonical rendering.
            // Try RFC3339 format
            let left_dt = chrono::DateTime::parse_from_rfc3339(left_str)
                .map(|dt| dt.with_timezone(&chrono::Utc));
            let right_dt = chrono::DateTime::parse_from_rfc3339(right_str)
                .map(|dt| dt.with_timezone(&chrono::Utc));

            if let (Ok(l), Ok(r)) = (left_dt, right_dt) {
                let diff = l.signed_duration_since(r);
                let total_seconds = diff.num_seconds();
                let days = total_seconds / 86400;
                let remaining_seconds = total_seconds % 86400;

                return Ok(temporal_value::make_duration(0, days, remaining_seconds, 0));
            }

            // Try NaiveDate format
            let left_date = chrono::NaiveDate::parse_from_str(left_str, "%Y-%m-%d");
            let right_date = chrono::NaiveDate::parse_from_str(right_str, "%Y-%m-%d");

            if let (Ok(l), Ok(r)) = (left_date, right_date) {
                let diff = l.signed_duration_since(r);
                let days = diff.num_days();

                return Ok(temporal_value::make_duration(0, days, 0, 0));
            }
        }

        Ok(Value::Null)
    }
}
