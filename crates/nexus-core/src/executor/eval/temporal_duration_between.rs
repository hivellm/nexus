//! `duration.between`/`duration.inMonths`/`duration.inDays`/
//! `duration.inSeconds` — the four Neo4j "duration between two temporals"
//! builtins (openCypher TCK `Temporal10.feature`).
//!
//! Mirrors java.time's `Temporal.until(Temporal, ChronoUnit)` algorithm,
//! which is what Neo4j's own `DurationValue.between` is built on:
//!
//! - `duration.between(a, b)` computes months first (the largest whole
//!   number of calendar months that doesn't overshoot `b`), then whole
//!   days from the months-shifted point, then a seconds+nanos remainder
//!   from the further days-shifted point — so `a + result == b` exactly.
//! - `duration.inMonths(a, b)` / `duration.inDays(a, b)` /
//!   `duration.inSeconds(a, b)` are each an INDEPENDENT single-unit count
//!   directly between `a` and `b` (not a component of the cascade above —
//!   e.g. `inDays` between two `date`s is the full day span, not the
//!   1-13-day remainder `between` would leave after subtracting whole
//!   months).
//!
//! ## Operand coercion rules (verified against every row of
//! `Temporal10.feature`'s five scenario tables)
//!
//! - If EITHER operand has no date part (`time`/`localtime`), the result
//!   never carries months/days — Neo4j drops the other operand's date
//!   entirely and compares bare time-of-day only.
//! - If EITHER operand has no offset (`date`/`localtime`/`localdatetime`),
//!   any offset the OTHER operand carries (`time`/`datetime`) is dropped:
//!   the comparison uses local wall-clock fields only. Only when BOTH
//!   operands carry an offset (`time`+`time`, `datetime`+`datetime`,
//!   `time`+`datetime`) does the comparison use their real elapsed
//!   (offset-normalized) difference.
//! - A `date` operand paired with any time-bearing kind contributes
//!   midnight (`00:00:00`) as its synthetic time-of-day.

use super::temporal::{checked_month_rollover, days_in_month};
use super::temporal_retag;
use super::temporal_value;
use crate::{Error, Result};
use chrono::{Datelike, NaiveDate};
use serde_json::Value;

/// One operand of a `duration.between`-family comparison, reduced to an
/// optional calendar date (absent for a bare `time`/`localtime`) plus a
/// nanosecond-of-day offset.
///
/// `a`'s own fields are always its raw local reading, verbatim; `b`'s may
/// be re-expressed in `a`'s offset first — see [`normalize_pair`]'s doc
/// comment for why the anchor matters.
struct NormalizedInstant {
    date: Option<NaiveDate>,
    nanos_of_day: i64,
}

const NANOS_PER_DAY: i64 = 86_400_000_000_000;

fn overflow(context: &str) -> Error {
    Error::CypherExecution(format!("duration arithmetic overflow: {context}"))
}

/// Bridges either an already-tagged temporal *instant* value or a stored
/// canonical-string rendering into the tagged intermediate shape this
/// module's accessors need — the same two accepted shapes
/// `temporal::datetime_difference` uses for the `-` operator, factored out
/// here since every entry point below needs it for both operands.
fn retag_instant(value: &Value) -> Option<Value> {
    if temporal_value::is_temporal_instant(value) {
        return Some(value.clone());
    }
    if let Value::String(s) = value {
        let candidate = temporal_retag::retag_canonical_string(s)?;
        if temporal_value::is_temporal_instant(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// Builds a [`NormalizedInstant`] from an operand's raw components plus an
/// explicit `shift_nanos` delta (`0` for an operand that keeps its own raw
/// local reading — always the case for `a`; a real delta for `b` when
/// [`normalize_pair`] re-expresses it in `a`'s offset). When the shift is
/// non-zero and the operand also carries a date, it can push the
/// nanosecond-of-day outside `[0, NANOS_PER_DAY)` — that overflow is
/// carried into the date itself (a large enough offset delta legitimately
/// shifts the re-expressed instant onto the next or previous calendar
/// day).
fn build_normalized(
    date: Option<(i32, u32, u32)>,
    (hour, minute, second, nanosecond): (u32, u32, u32, u32),
    shift_nanos: i64,
) -> Result<NormalizedInstant> {
    let raw_nanos_of_day = i64::from(hour) * 3_600_000_000_000
        + i64::from(minute) * 60_000_000_000
        + i64::from(second) * 1_000_000_000
        + i64::from(nanosecond);
    let adjusted_nanos = raw_nanos_of_day + shift_nanos;

    let Some((year, month, day)) = date else {
        // No date part at all: the shifted value stands on its own, with
        // no day boundary to reflow across.
        return Ok(NormalizedInstant {
            date: None,
            nanos_of_day: adjusted_nanos,
        });
    };

    let base_date = NaiveDate::from_ymd_opt(year, month, day)
        .ok_or_else(|| overflow("operand date is outside the representable range"))?;
    let day_carry = adjusted_nanos.div_euclid(NANOS_PER_DAY);
    let normalized_nanos = adjusted_nanos.rem_euclid(NANOS_PER_DAY);
    let shifted_date = if day_carry == 0 {
        base_date
    } else {
        let delta = chrono::Duration::try_days(day_carry)
            .ok_or_else(|| overflow("offset-adjusted day shift is out of range"))?;
        base_date
            .checked_add_signed(delta)
            .ok_or_else(|| overflow("offset-adjusted date is outside the representable range"))?
    };
    Ok(NormalizedInstant {
        date: Some(shifted_date),
        nanos_of_day: normalized_nanos,
    })
}

/// Retags and normalizes both operands, returning `None` (the caller's
/// cue to yield `Value::Null`, matching every other lenient builtin in
/// this codebase) when either fails to resolve to a temporal instant.
/// The returned `Option<(NaiveDate, NaiveDate)>` is `Some` only when BOTH
/// operands carry a date part (see the module doc comment's coercion
/// rules) — callers match on it directly instead of separately checking
/// a `has_date` flag and re-unwrapping `a_norm.date`/`b_norm.date`, which
/// can never disagree with each other by construction.
///
/// `a` is always the anchor: its date and time-of-day are used exactly as
/// read, never offset-shifted. `b` is re-expressed in `a`'s offset (the
/// same real instant, a different local reading) only when both operands
/// carry one — mirroring java.time's `ChronoZonedDateTimeImpl.until`,
/// which calls `end.withZoneSameInstant(from's offset)` before running the
/// months/days/seconds cascade on `from`'s own local fields. Shifting
/// BOTH operands into a third frame (e.g. UTC) instead — this module's
/// previous approach — silently breaks `a + between(a, b) == b` whenever
/// that shift crosses a day boundary the *unshifted* comparison would
/// not have: `duration.between(datetime('2015-03-01T00:30+02:00'),
/// datetime('2015-04-01T00:30+02:00'))` must be `'P1M'` (a full calendar
/// month, since both operands read `00:30` in their own, shared, offset)
/// — shifting both to UTC first (`2015-02-28T22:30` /
/// `2015-03-31T22:30`) anchors the months step on `2015-02-28` instead of
/// `2015-03-01`, overshoots by 3 days once the month is added back
/// (`2015-03-28` -> `2015-03-31`), and wrongly renders `'P1M3D'`.
/// `in_days`/`in_seconds` are unaffected by this anchor choice — their
/// result is one linear `(day_diff, nanos_diff)` difference, which is
/// translation-invariant regardless of which operand's offset serves as
/// the shared reference; only the months/days *cascade* (`between`'s own
/// steps, and `in_months`) actually re-anchors a calendar date against
/// itself (`add_months_clamped`), where the choice of anchor is not just
/// a re-labelling.
///
/// ## Named-zone donation
///
/// An operand with NO offset of its own (`date`/`localtime`/
/// `localdatetime`) paired against a `datetime` that carries a REAL named
/// zone (not just a fixed numeric offset — see
/// [`temporal_value::zone_name`]) does not simply compare bare local
/// fields the way it does against a fixed-offset partner: it resolves its
/// OWN effective offset through that SAME zone first (borrowing the
/// zoned operand's date when the offset-less side itself has none),
/// exactly the way a bare `LocalDateTime` gains a real, DST-correct
/// offset the instant it's combined with a `ZoneId` in java.time. This
/// matters only when the zone's offset actually differs between the two
/// operands' wall-clock readings (a DST transition falls between them);
/// donating a FIXED numeric offset instead would always net a zero shift
/// (donating `X` to a partner and then computing `X - X` cancels
/// regardless of `X`'s value), which is exactly the pre-existing "compare
/// bare local fields" behavior already verified against every
/// non-zoned `Temporal10.feature` fixture — so this is a value-preserving
/// generalization, not a behavior change, for every pair that doesn't
/// involve a named zone. Verified against `Temporal10.feature` scenario
/// [8]'s six-row "daylight saving time day" table (each row donates in a
/// different direction/shape: `datetime`+`localdatetime`,
/// `datetime`+`localtime`, `localdatetime`+`datetime`,
/// `localtime`+`datetime`, `date`+`datetime`, `datetime`+`date`).
fn normalize_pair(
    a: &Value,
    b: &Value,
) -> Result<
    Option<(
        Option<(NaiveDate, NaiveDate)>,
        NormalizedInstant,
        NormalizedInstant,
    )>,
> {
    let Some(a) = retag_instant(a) else {
        return Ok(None);
    };
    let Some(b) = retag_instant(b) else {
        return Ok(None);
    };

    let a_date = temporal_value::date_components(&a);
    let b_date = temporal_value::date_components(&b);
    let a_time = temporal_value::time_components(&a).unwrap_or((0, 0, 0, 0));
    let b_time = temporal_value::time_components(&b).unwrap_or((0, 0, 0, 0));
    let mut a_offset = temporal_value::offset_seconds(&a);
    let mut b_offset = temporal_value::offset_seconds(&b);

    if a_offset.is_none() {
        a_offset = donate_offset_via_named_zone(&b, a_date, b_date, a_time);
    }
    if b_offset.is_none() {
        b_offset = donate_offset_via_named_zone(&a, b_date, a_date, b_time);
    }

    let use_offset = a_offset.is_some() && b_offset.is_some();

    let b_shift_nanos = if use_offset {
        i64::from(a_offset.unwrap_or(0) - b_offset.unwrap_or(0)) * 1_000_000_000
    } else {
        0
    };

    let a_norm = build_normalized(a_date, a_time, 0)?;
    let b_norm = build_normalized(b_date, b_time, b_shift_nanos)?;

    let dates = match (a_norm.date, b_norm.date) {
        (Some(ad), Some(bd)) => Some((ad, bd)),
        _ => None,
    };

    Ok(Some((dates, a_norm, b_norm)))
}

/// If `donor` is a `datetime` carrying a real named zone, resolves
/// `own_date` (falling back to `donor_date` when the recipient itself has
/// no date part) and `own_time` through that zone via chrono-tz, returning
/// the DST-correct effective offset. Returns `None` (no donation — the
/// recipient then keeps comparing via bare local fields, the pre-existing
/// behavior) when `donor` doesn't carry a named zone at all, OR when
/// resolving it fails for any reason — most notably an unresolvable zone
/// name. That specifically covers a value re-derived from a STORED
/// property string via [`retag_instant`]/`temporal_retag::retag_canonical_string`:
/// the retag path's bracket parsing does not validate the zone name is a
/// real IANA identifier (unlike the map/string CONSTRUCTOR paths — see
/// `temporal_retag::is_valid_named_zone`'s doc comment for why that
/// asymmetry exists), so a property already sitting in storage as
/// `'...Z[Bogus/Zone]'` retags into a tagged `datetime` carrying that
/// unresolvable name. `duration.between` reading such a value back must
/// degrade gracefully (comparing bare local fields, exactly as it would
/// for a fixed-offset partner), never hard-error a query over data that
/// was already written — a best-effort DST refinement is not something a
/// stored value's unrelated corruption should be able to break.
fn donate_offset_via_named_zone(
    donor: &Value,
    own_date: Option<(i32, u32, u32)>,
    donor_date: Option<(i32, u32, u32)>,
    own_time: (u32, u32, u32, u32),
) -> Option<i32> {
    let zone = temporal_value::zone_name(donor)?;
    // A `datetime` carrying a zone name always has its own date
    // (`zone_name` only ever returns `Some` for `TemporalKind::DateTime`,
    // which `date_components` always resolves) — `own_date.or(donor_date)`
    // borrows it only when the recipient itself lacks a date
    // (`localtime`/`time`); this fallback is purely defensive.
    let (year, month, day) = own_date.or(donor_date)?;
    let (hour, minute, second, nanosecond) = own_time;
    let (offset, _) = temporal_retag::resolve_timezone_string(
        zone, year, month, day, hour, minute, second, nanosecond,
    )
    .ok()?;
    Some(offset)
}

/// `year*12 + (month-1)`, packed with the day-of-month as a tie-breaker
/// (`* 32 + day`) — the same packed representation java.time's
/// `LocalDate.until(..., MONTHS)` uses to get a single integer division
/// (truncating toward zero) to produce the correct whole-month count
/// regardless of the two months' different lengths.
fn packed_month_day(date: NaiveDate) -> i64 {
    let proleptic_month = i64::from(date.year()) * 12 + i64::from(date.month() as i32 - 1);
    proleptic_month * 32 + i64::from(date.day())
}

/// Adjusts `end_date` back/forward by one day when its time-of-day hasn't
/// "caught up" with `start`'s yet — mirrors java.time's
/// `LocalDateTime.until` heuristic: a partial final month/day must round
/// toward zero (e.g. `2014-07-21T21:40` -> `2015-07-21T21:39` is 11
/// months 30 days short of a full year, not exactly 12 months), which a
/// bare calendar-field subtraction would otherwise over-count by one
/// unit whenever the end's time-of-day is earlier in the day than the
/// start's (or, symmetrically, under-count by one when going backward in
/// time and the end's time-of-day is later).
fn adjust_end_date(
    start_date: NaiveDate,
    start_nanos: i64,
    end_date: NaiveDate,
    end_nanos: i64,
) -> Result<NaiveDate> {
    if end_date > start_date && end_nanos < start_nanos {
        end_date
            .checked_sub_signed(chrono::Duration::days(1))
            .ok_or_else(|| overflow("end-date adjustment underflowed the representable range"))
    } else if end_date < start_date && end_nanos > start_nanos {
        end_date
            .checked_add_signed(chrono::Duration::days(1))
            .ok_or_else(|| overflow("end-date adjustment overflowed the representable range"))
    } else {
        Ok(end_date)
    }
}

/// Whole calendar months from `(start_date, start_nanos)` to
/// `(end_date, end_nanos)`, truncated toward zero (see the module doc
/// comment's cascade description). Shared by `duration.inMonths` (called
/// directly against the raw operands) and `between`'s own months-first
/// cascade step (called against its running intermediate point).
fn months_between(
    start_date: NaiveDate,
    start_nanos: i64,
    end_date: NaiveDate,
    end_nanos: i64,
) -> Result<i64> {
    let adjusted_end = adjust_end_date(start_date, start_nanos, end_date, end_nanos)?;
    Ok((packed_month_day(adjusted_end) - packed_month_day(start_date)) / 32)
}

/// Whole days from `(start_date, start_nanos)` to `(end_date, end_nanos)`,
/// using the same end-date adjustment heuristic as [`months_between`].
/// Shared by `duration.inDays` (raw operands) and `between`'s
/// months-shifted days step.
fn days_between(
    start_date: NaiveDate,
    start_nanos: i64,
    end_date: NaiveDate,
    end_nanos: i64,
) -> Result<i64> {
    let adjusted_end = adjust_end_date(start_date, start_nanos, end_date, end_nanos)?;
    Ok(adjusted_end.signed_duration_since(start_date).num_days())
}

/// The full elapsed `(whole_seconds, nanos)` remainder between two
/// points, each optionally date-bearing (`None` when neither operand in
/// the pair has a date part — see [`normalize_pair`]). Division truncates
/// toward zero, keeping `seconds`/`nanos` the same sign — verified
/// against `Temporal10.feature` scenario `[11]`'s "seconds and subseconds
/// have different signs" table.
fn seconds_nanos_between(
    start_date: Option<NaiveDate>,
    start_nanos: i64,
    end_date: Option<NaiveDate>,
    end_nanos: i64,
) -> Result<(i64, i32)> {
    let day_diff: i128 = match (start_date, end_date) {
        (Some(s), Some(e)) => i128::from(e.signed_duration_since(s).num_days()),
        _ => 0,
    };
    let total_nanos = day_diff
        .checked_mul(i128::from(NANOS_PER_DAY))
        .and_then(|d| d.checked_add(i128::from(end_nanos)))
        .and_then(|d| d.checked_sub(i128::from(start_nanos)))
        .ok_or_else(|| overflow("elapsed time exceeds the representable range"))?;
    let whole_seconds = total_nanos / 1_000_000_000;
    let nanos_remainder = total_nanos % 1_000_000_000;
    let seconds = i64::try_from(whole_seconds)
        .map_err(|_| overflow("elapsed seconds exceed the representable range"))?;
    let nanos = i32::try_from(nanos_remainder)
        .map_err(|_| overflow("elapsed nanosecond remainder is out of range"))?;
    Ok((seconds, nanos))
}

/// Adds a whole-month delta to `date`, clamping the day-of-month to the
/// target month's length (matching the same rollover/clamp rule
/// `temporal::apply_duration_to_tagged_instant` applies for `+ duration`
/// arithmetic — e.g. `2015-01-31` plus 1 month clamps to `2015-02-28`,
/// never overflows into March).
fn add_months_clamped(date: NaiveDate, months: i64) -> Result<NaiveDate> {
    if months == 0 {
        return Ok(date);
    }
    let (final_year, final_month) = checked_month_rollover(date.year(), date.month(), months)?;
    let clamped_day = date.day().min(days_in_month(final_year, final_month));
    NaiveDate::from_ymd_opt(final_year, final_month, clamped_day)
        .ok_or_else(|| overflow("month-shifted date is outside the representable range"))
}

/// `duration.between(a, b)`: the full months-then-days-then-seconds
/// cascade, so `a + result == b`. Returns `Value::Null` when either
/// operand isn't a resolvable temporal instant (parity with every other
/// lenient builtin function in this dispatch chain).
pub(in crate::executor) fn between(a: &Value, b: &Value) -> Result<Value> {
    let Some((dates, a_norm, b_norm)) = normalize_pair(a, b)? else {
        return Ok(Value::Null);
    };
    let Some((a_date, b_date)) = dates else {
        let (seconds, nanos) =
            seconds_nanos_between(None, a_norm.nanos_of_day, None, b_norm.nanos_of_day)?;
        return Ok(temporal_value::make_duration(0, 0, seconds, nanos));
    };

    let months = months_between(a_date, a_norm.nanos_of_day, b_date, b_norm.nanos_of_day)?;
    let after_months = add_months_clamped(a_date, months)?;

    let days = days_between(
        after_months,
        a_norm.nanos_of_day,
        b_date,
        b_norm.nanos_of_day,
    )?;
    let after_days = if days == 0 {
        after_months
    } else {
        let delta = chrono::Duration::try_days(days)
            .ok_or_else(|| overflow("day-shifted date is out of range"))?;
        after_months
            .checked_add_signed(delta)
            .ok_or_else(|| overflow("day-shifted date is outside the representable range"))?
    };

    let (seconds, nanos) = seconds_nanos_between(
        Some(after_days),
        a_norm.nanos_of_day,
        Some(b_date),
        b_norm.nanos_of_day,
    )?;

    Ok(temporal_value::make_duration(months, days, seconds, nanos))
}

/// `duration.inMonths(a, b)`: whole months directly between `a` and `b`
/// (zero, per the module doc comment's coercion rules, when either
/// operand has no date part).
pub(in crate::executor) fn in_months(a: &Value, b: &Value) -> Result<Value> {
    let Some((dates, a_norm, b_norm)) = normalize_pair(a, b)? else {
        return Ok(Value::Null);
    };
    let Some((a_date, b_date)) = dates else {
        return Ok(temporal_value::make_duration(0, 0, 0, 0));
    };
    let months = months_between(a_date, a_norm.nanos_of_day, b_date, b_norm.nanos_of_day)?;
    Ok(temporal_value::make_duration(months, 0, 0, 0))
}

/// `duration.inDays(a, b)`: whole days directly between `a` and `b` (zero
/// when either operand has no date part).
pub(in crate::executor) fn in_days(a: &Value, b: &Value) -> Result<Value> {
    let Some((dates, a_norm, b_norm)) = normalize_pair(a, b)? else {
        return Ok(Value::Null);
    };
    let Some((a_date, b_date)) = dates else {
        return Ok(temporal_value::make_duration(0, 0, 0, 0));
    };
    let days = days_between(a_date, a_norm.nanos_of_day, b_date, b_norm.nanos_of_day)?;
    Ok(temporal_value::make_duration(0, days, 0, 0))
}

/// `duration.inSeconds(a, b)`: the full elapsed seconds+nanos directly
/// between `a` and `b` (the whole elapsed span, not `between`'s sub-day
/// remainder — a date-bearing pair still contributes its full day-count
/// worth of seconds here).
pub(in crate::executor) fn in_seconds(a: &Value, b: &Value) -> Result<Value> {
    let Some((dates, a_norm, b_norm)) = normalize_pair(a, b)? else {
        return Ok(Value::Null);
    };
    let (start_date, end_date) = match dates {
        Some((a_date, b_date)) => (Some(a_date), Some(b_date)),
        None => (None, None),
    };
    let (seconds, nanos) = seconds_nanos_between(
        start_date,
        a_norm.nanos_of_day,
        end_date,
        b_norm.nanos_of_day,
    )?;
    Ok(temporal_value::make_duration(0, 0, seconds, nanos))
}

#[cfg(test)]
mod tests {
    //! Every case below is transcribed directly from the openCypher TCK's
    //! `Temporal10.feature` (`crates/nexus-core/tests/tck/opencypher/
    //! features/expressions/temporal/Temporal10.feature`), one `#[test]`
    //! per scenario table, asserting the exact canonical ISO-8601 string
    //! the TCK's `Then` clause pins — including scenario `[1]`'s Stockholm
    //! `datetime(...)` string-literal pair and scenario `[8]`'s full
    //! "daylight saving time day" table (see
    //! `named_zone_donation_tests` below), both of which exercise the
    //! named-zone offset donation [`super::donate_offset_via_named_zone`]
    //! implements. Scenario `[9]`/`[10]`
    //! (`date('-999999999-01-01')` / `'+999999999-12-31'`) remain out of
    //! scope: those years are outside `chrono::NaiveDate`'s representable
    //! range, so the operand itself cannot be constructed regardless of
    //! this module's own logic — unrelated to timezone support.

    use super::super::super::engine::Executor;
    use super::*;
    use temporal_value::{make_date, make_datetime, make_localdatetime, make_localtime, make_time};

    fn rendered(v: Result<Value>) -> String {
        let v = v.expect("duration-between computation must not error for these fixtures");
        temporal_value::canonicalize_temporal(&v)
            .expect("result must be a tagged duration that canonicalizes")
    }

    /// Renders either shape a temporal value may arrive in — a still-tagged
    /// intermediate `Value::Object`, or an already-canonicalized
    /// `Value::String` (`Executor::datetime_add_duration`'s own return
    /// shape) — to its ISO-8601 string.
    fn iso(v: &Value) -> String {
        temporal_value::canonicalize_temporal(v)
            .or_else(|| v.as_str().map(str::to_string))
            .unwrap_or_default()
    }

    /// Asserts `between(a, b)` renders exactly `expected`, AND that
    /// actually adding the computed duration back to `a` (via the real
    /// `+` datetime-arithmetic path, `Executor::try_datetime_add` —
    /// independent code from this module) reproduces `b` exactly,
    /// live-verifying the `a + between(a, b) == b` invariant rather than
    /// trusting a hand-derived expected string alone.
    fn assert_between_and_invariant(a: &Value, b: &Value, expected: &str) {
        let dur = between(a, b).expect("between must not error for these fixtures");
        assert_eq!(
            temporal_value::canonicalize_temporal(&dur).as_deref(),
            Some(expected),
            "between({a:?}, {b:?})"
        );
        let executor = Executor::default();
        let added = executor
            .try_datetime_add(a, &dur)
            .expect("a + between(a, b) must not error")
            .expect("a + duration must be recognized as datetime arithmetic");
        let a_str = iso(a);
        let b_str = iso(b);
        let added_str = iso(&added);
        assert_eq!(
            added_str, b_str,
            "invariant a + between(a, b) == b failed: {a_str} + {expected} = {added_str}, expected {b_str}"
        );
    }

    #[test]
    fn between_anchors_the_months_days_cascade_on_a_not_a_shared_utc_frame() {
        // Regression for a months/days cascade bug: shifting BOTH
        // offset-bearing operands into a third (UTC) frame before running
        // the cascade re-anchors the months step on the wrong calendar
        // date whenever that shift crosses a day boundary the unshifted
        // comparison would not have. `a` must stay at its own raw local
        // reading throughout; only `b` is re-expressed in `a`'s offset.
        let a = make_datetime(2015, 3, 1, 0, 30, 0, 0, 7200, None);
        // Same offset on both sides (+02:00) as `a`, exactly one calendar
        // month later at the identical time-of-day: must be a clean
        // `P1M`, not `P1M3D` (the old UTC-anchored bug's answer).
        let b_same_time = make_datetime(2015, 4, 1, 0, 30, 0, 0, 7200, None);
        assert_between_and_invariant(&a, &b_same_time, "P1M");

        // 30 minutes earlier than the row above: the whole-month step
        // must fall back to zero (the target time-of-day hasn't "caught
        // up" yet), leaving a pure 30-day-plus-23h30m span.
        let b_30_min_earlier = make_datetime(2015, 4, 1, 0, 0, 0, 0, 7200, None);
        assert_between_and_invariant(&a, &b_30_min_earlier, "P30DT23H30M");
        assert_eq!(
            rendered(in_months(&a, &b_30_min_earlier)),
            "PT0S",
            "inMonths must agree with between's own months-cascade step"
        );

        // Z-offset control: the same shape, both operands at offset 0
        // (`Z`) instead of `+02:00` — still a clean `P1M`, confirming the
        // fix isn't specific to a non-zero anchor offset.
        let a_utc = make_datetime(2015, 3, 1, 0, 30, 0, 0, 0, None);
        let b_utc = make_datetime(2015, 4, 1, 0, 30, 0, 0, 0, None);
        assert_between_and_invariant(&a_utc, &b_utc, "P1M");
    }

    #[test]
    fn between_scenario_1_splits_at_boundaries() {
        let cases = [
            (
                make_localdatetime(2018, 1, 1, 12, 0, 0, 0),
                make_localdatetime(2018, 1, 2, 10, 0, 0, 0),
                "PT22H",
            ),
            (
                make_localdatetime(2018, 1, 2, 10, 0, 0, 0),
                make_localdatetime(2018, 1, 1, 12, 0, 0, 0),
                "PT-22H",
            ),
            (
                make_localdatetime(2018, 1, 1, 10, 0, 0, 200_000_000),
                make_localdatetime(2018, 1, 2, 10, 0, 0, 100_000_000),
                "PT23H59M59.9S",
            ),
            (
                make_localdatetime(2018, 1, 2, 10, 0, 0, 100_000_000),
                make_localdatetime(2018, 1, 1, 10, 0, 0, 200_000_000),
                "PT-23H-59M-59.9S",
            ),
        ];
        for (a, b, expected) in cases {
            assert_eq!(rendered(between(&a, &b)), expected, "between({a:?}, {b:?})");
        }
    }

    #[test]
    fn between_scenario_2_computes_full_cascade() {
        let date_1984 = make_date(1984, 10, 11);
        let date_2015_06_24 = make_date(2015, 6, 24);
        let ldt_2016 = make_localdatetime(2016, 7, 21, 21, 45, 22, 142_000_000);
        let dt_2015_07 = make_datetime(2015, 7, 21, 21, 40, 32, 142_000_000, 3600, None);
        let lt_16_30 = make_localtime(16, 30, 0, 0);
        let t_16_30 = make_time(16, 30, 0, 0, 3600);
        let lt_14_30 = make_localtime(14, 30, 0, 0);
        let t_14_30 = make_time(14, 30, 0, 0, 0);
        let ldt_2015_07 = make_localdatetime(2015, 7, 21, 21, 40, 32, 142_000_000);
        let dt_2014_07 = make_datetime(2014, 7, 21, 21, 40, 36, 143_000_000, 7200, None);

        let cases = [
            (date_1984.clone(), date_2015_06_24.clone(), "P30Y8M13D"),
            (
                date_1984.clone(),
                ldt_2016.clone(),
                "P31Y9M10DT21H45M22.142S",
            ),
            (
                date_1984.clone(),
                dt_2015_07.clone(),
                "P30Y9M10DT21H40M32.142S",
            ),
            (date_1984.clone(), lt_16_30.clone(), "PT16H30M"),
            (date_1984.clone(), t_16_30.clone(), "PT16H30M"),
            (lt_14_30.clone(), date_2015_06_24.clone(), "PT-14H-30M"),
            (lt_14_30.clone(), ldt_2016.clone(), "PT7H15M22.142S"),
            (lt_14_30.clone(), dt_2015_07.clone(), "PT7H10M32.142S"),
            (lt_14_30.clone(), lt_16_30.clone(), "PT2H"),
            (lt_14_30.clone(), t_16_30.clone(), "PT2H"),
            (t_14_30.clone(), date_2015_06_24.clone(), "PT-14H-30M"),
            (t_14_30.clone(), ldt_2016.clone(), "PT7H15M22.142S"),
            (t_14_30.clone(), dt_2015_07.clone(), "PT6H10M32.142S"),
            (t_14_30.clone(), lt_16_30.clone(), "PT2H"),
            (t_14_30.clone(), t_16_30.clone(), "PT1H"),
            (
                ldt_2015_07.clone(),
                date_2015_06_24.clone(),
                "P-27DT-21H-40M-32.142S",
            ),
            (ldt_2015_07.clone(), ldt_2016.clone(), "P1YT4M50S"),
            (ldt_2015_07.clone(), dt_2015_07.clone(), "PT0S"),
            (ldt_2015_07.clone(), lt_16_30.clone(), "PT-5H-10M-32.142S"),
            (ldt_2015_07.clone(), t_16_30.clone(), "PT-5H-10M-32.142S"),
            (
                dt_2014_07.clone(),
                date_2015_06_24.clone(),
                "P11M2DT2H19M23.857S",
            ),
            (dt_2014_07.clone(), ldt_2016.clone(), "P2YT4M45.999S"),
            (dt_2014_07.clone(), dt_2015_07.clone(), "P1YT59M55.999S"),
            (dt_2014_07.clone(), lt_16_30.clone(), "PT-5H-10M-36.143S"),
            (dt_2014_07.clone(), t_16_30.clone(), "PT-4H-10M-36.143S"),
        ];
        for (a, b, expected) in cases {
            assert_eq!(rendered(between(&a, &b)), expected, "between({a:?}, {b:?})");
        }
    }

    #[test]
    fn in_months_scenario_3_matches_between_months_component() {
        let date_1984 = make_date(1984, 10, 11);
        let date_2015_06_24 = make_date(2015, 6, 24);
        let ldt_2016 = make_localdatetime(2016, 7, 21, 21, 45, 22, 142_000_000);
        let dt_2015_07 = make_datetime(2015, 7, 21, 21, 40, 32, 142_000_000, 3600, None);
        let lt_16_30 = make_localtime(16, 30, 0, 0);
        let t_16_30 = make_time(16, 30, 0, 0, 3600);
        let lt_14_30 = make_localtime(14, 30, 0, 0);
        let t_14_30 = make_time(14, 30, 0, 0, 0);
        let ldt_2015_07 = make_localdatetime(2015, 7, 21, 21, 40, 32, 142_000_000);
        let dt_2014_07 = make_datetime(2014, 7, 21, 21, 40, 36, 143_000_000, 7200, None);

        let cases = [
            (date_1984.clone(), date_2015_06_24.clone(), "P30Y8M"),
            (date_1984.clone(), ldt_2016.clone(), "P31Y9M"),
            (date_1984.clone(), dt_2015_07.clone(), "P30Y9M"),
            (date_1984.clone(), lt_16_30.clone(), "PT0S"),
            (date_1984.clone(), t_16_30.clone(), "PT0S"),
            (lt_14_30.clone(), date_2015_06_24.clone(), "PT0S"),
            (lt_14_30.clone(), ldt_2016.clone(), "PT0S"),
            (lt_14_30.clone(), dt_2015_07.clone(), "PT0S"),
            (t_14_30.clone(), date_2015_06_24.clone(), "PT0S"),
            (t_14_30.clone(), ldt_2016.clone(), "PT0S"),
            (t_14_30.clone(), dt_2015_07.clone(), "PT0S"),
            (ldt_2015_07.clone(), date_2015_06_24.clone(), "PT0S"),
            (ldt_2015_07.clone(), ldt_2016.clone(), "P1Y"),
            (ldt_2015_07.clone(), dt_2015_07.clone(), "PT0S"),
            (ldt_2015_07.clone(), lt_16_30.clone(), "PT0S"),
            (ldt_2015_07.clone(), t_16_30.clone(), "PT0S"),
            (dt_2014_07.clone(), date_2015_06_24.clone(), "P11M"),
            (dt_2014_07.clone(), ldt_2016.clone(), "P2Y"),
            (dt_2014_07.clone(), dt_2015_07.clone(), "P1Y"),
            (dt_2014_07.clone(), lt_16_30.clone(), "PT0S"),
            (dt_2014_07.clone(), t_16_30.clone(), "PT0S"),
        ];
        for (a, b, expected) in cases {
            assert_eq!(
                rendered(in_months(&a, &b)),
                expected,
                "inMonths({a:?}, {b:?})"
            );
        }
    }

    #[test]
    fn in_months_scenario_7_negative_big_units() {
        let cases = [
            (make_date(2018, 3, 11), make_date(2016, 6, 24), "P-1Y-8M"),
            (
                make_date(2018, 7, 21),
                make_datetime(2016, 7, 21, 21, 40, 32, 142_000_000, 3600, None),
                "P-1Y-11M",
            ),
            (
                make_localdatetime(2018, 7, 21, 21, 40, 32, 142_000_000),
                make_date(2016, 7, 21),
                "P-2Y",
            ),
            (
                make_datetime(2018, 7, 21, 21, 40, 36, 143_000_000, 7200, None),
                make_localdatetime(2016, 7, 21, 21, 40, 36, 143_000_000),
                "P-2Y",
            ),
            (
                make_datetime(2018, 7, 21, 21, 40, 36, 143_000_000, 18_000, None),
                make_datetime(1984, 7, 21, 22, 40, 36, 143_000_000, 7200, None),
                "P-33Y-11M",
            ),
        ];
        for (a, b, expected) in cases {
            assert_eq!(
                rendered(in_months(&a, &b)),
                expected,
                "inMonths({a:?}, {b:?})"
            );
        }
    }

    #[test]
    fn in_days_scenario_4() {
        let date_1984 = make_date(1984, 10, 11);
        let date_2015_06_24 = make_date(2015, 6, 24);
        let ldt_2016 = make_localdatetime(2016, 7, 21, 21, 45, 22, 142_000_000);
        let dt_2015_07 = make_datetime(2015, 7, 21, 21, 40, 32, 142_000_000, 3600, None);
        let lt_16_30 = make_localtime(16, 30, 0, 0);
        let t_16_30 = make_time(16, 30, 0, 0, 3600);
        let lt_14_30 = make_localtime(14, 30, 0, 0);
        let t_14_30 = make_time(14, 30, 0, 0, 0);
        let ldt_2015_07 = make_localdatetime(2015, 7, 21, 21, 40, 32, 142_000_000);
        let dt_2014_07 = make_datetime(2014, 7, 21, 21, 40, 36, 143_000_000, 7200, None);

        let cases = [
            (date_1984.clone(), date_2015_06_24.clone(), "P11213D"),
            (date_1984.clone(), ldt_2016.clone(), "P11606D"),
            (date_1984.clone(), dt_2015_07.clone(), "P11240D"),
            (date_1984.clone(), lt_16_30.clone(), "PT0S"),
            (date_1984.clone(), t_16_30.clone(), "PT0S"),
            (lt_14_30.clone(), date_2015_06_24.clone(), "PT0S"),
            (lt_14_30.clone(), ldt_2016.clone(), "PT0S"),
            (lt_14_30.clone(), dt_2015_07.clone(), "PT0S"),
            (t_14_30.clone(), date_2015_06_24.clone(), "PT0S"),
            (t_14_30.clone(), ldt_2016.clone(), "PT0S"),
            (t_14_30.clone(), dt_2015_07.clone(), "PT0S"),
            (ldt_2015_07.clone(), date_2015_06_24.clone(), "P-27D"),
            (ldt_2015_07.clone(), ldt_2016.clone(), "P366D"),
            (ldt_2015_07.clone(), dt_2015_07.clone(), "PT0S"),
            (ldt_2015_07.clone(), lt_16_30.clone(), "PT0S"),
            (ldt_2015_07.clone(), t_16_30.clone(), "PT0S"),
            (dt_2014_07.clone(), date_2015_06_24.clone(), "P337D"),
            (dt_2014_07.clone(), ldt_2016.clone(), "P731D"),
            (dt_2014_07.clone(), dt_2015_07.clone(), "P365D"),
            (dt_2014_07.clone(), lt_16_30.clone(), "PT0S"),
            (dt_2014_07.clone(), t_16_30.clone(), "PT0S"),
        ];
        for (a, b, expected) in cases {
            assert_eq!(rendered(in_days(&a, &b)), expected, "inDays({a:?}, {b:?})");
        }
    }

    #[test]
    fn in_seconds_scenario_5() {
        let date_1984 = make_date(1984, 10, 11);
        let date_2015_06_24 = make_date(2015, 6, 24);
        let ldt_2016 = make_localdatetime(2016, 7, 21, 21, 45, 22, 142_000_000);
        let dt_2015_07 = make_datetime(2015, 7, 21, 21, 40, 32, 142_000_000, 3600, None);
        let lt_16_30 = make_localtime(16, 30, 0, 0);
        let t_16_30 = make_time(16, 30, 0, 0, 3600);
        let lt_14_30 = make_localtime(14, 30, 0, 0);
        let t_14_30 = make_time(14, 30, 0, 0, 0);
        let ldt_2015_07 = make_localdatetime(2015, 7, 21, 21, 40, 32, 142_000_000);
        let dt_2014_07 = make_datetime(2014, 7, 21, 21, 40, 36, 143_000_000, 7200, None);

        let cases = [
            (date_1984.clone(), date_2015_06_24.clone(), "PT269112H"),
            (date_1984.clone(), ldt_2016.clone(), "PT278565H45M22.142S"),
            (date_1984.clone(), dt_2015_07.clone(), "PT269781H40M32.142S"),
            (date_1984.clone(), lt_16_30.clone(), "PT16H30M"),
            (date_1984.clone(), t_16_30.clone(), "PT16H30M"),
            (lt_14_30.clone(), date_2015_06_24.clone(), "PT-14H-30M"),
            (lt_14_30.clone(), ldt_2016.clone(), "PT7H15M22.142S"),
            (lt_14_30.clone(), dt_2015_07.clone(), "PT7H10M32.142S"),
            (lt_14_30.clone(), lt_16_30.clone(), "PT2H"),
            (lt_14_30.clone(), t_16_30.clone(), "PT2H"),
            (t_14_30.clone(), date_2015_06_24.clone(), "PT-14H-30M"),
            (t_14_30.clone(), ldt_2016.clone(), "PT7H15M22.142S"),
            (t_14_30.clone(), dt_2015_07.clone(), "PT6H10M32.142S"),
            (t_14_30.clone(), lt_16_30.clone(), "PT2H"),
            (t_14_30.clone(), t_16_30.clone(), "PT1H"),
            (
                ldt_2015_07.clone(),
                date_2015_06_24.clone(),
                "PT-669H-40M-32.142S",
            ),
            (ldt_2015_07.clone(), ldt_2016.clone(), "PT8784H4M50S"),
            (ldt_2015_07.clone(), dt_2015_07.clone(), "PT0S"),
            (ldt_2015_07.clone(), lt_16_30.clone(), "PT-5H-10M-32.142S"),
            (ldt_2015_07.clone(), t_16_30.clone(), "PT-5H-10M-32.142S"),
            (
                dt_2014_07.clone(),
                date_2015_06_24.clone(),
                "PT8090H19M23.857S",
            ),
            (dt_2014_07.clone(), ldt_2016.clone(), "PT17544H4M45.999S"),
            (dt_2014_07.clone(), dt_2015_07.clone(), "PT8760H59M55.999S"),
            (dt_2014_07.clone(), lt_16_30.clone(), "PT-5H-10M-36.143S"),
            (dt_2014_07.clone(), t_16_30.clone(), "PT-4H-10M-36.143S"),
        ];
        for (a, b, expected) in cases {
            assert_eq!(
                rendered(in_seconds(&a, &b)),
                expected,
                "inSeconds({a:?}, {b:?})"
            );
        }
    }

    #[test]
    fn in_seconds_scenario_6_fraction_sign_flip() {
        let a = make_localdatetime(2014, 7, 21, 21, 40, 36, 143_000_000);
        let b = make_localdatetime(2014, 7, 21, 21, 40, 36, 142_000_000);
        assert_eq!(rendered(in_seconds(&a, &b)), "PT-0.001S");
    }

    #[test]
    fn in_seconds_scenario_11_mixed_signs() {
        let lt = |s: &str| -> Value {
            let parts: Vec<&str> = s.split(['.', ':']).collect();
            let hour: u32 = parts[0].parse().unwrap();
            let minute: u32 = parts[1].parse().unwrap();
            let second: u32 = parts[2].parse().unwrap();
            let nanos: u32 = match parts.get(3) {
                Some(frac) => format!("{frac:0<9}").parse().unwrap(),
                None => 0,
            };
            make_localtime(hour, minute, second, nanos)
        };
        let cases = [
            ("12:34:54.7", "12:34:54.3", "PT-0.4S"),
            ("12:34:54.3", "12:34:54.7", "PT0.4S"),
            ("12:34:54.7", "12:34:55.3", "PT0.6S"),
            ("12:34:54.7", "12:44:55.3", "PT10M0.6S"),
            ("12:44:54.7", "12:34:55.3", "PT-9M-59.4S"),
            ("12:34:56", "12:34:55.7", "PT-0.3S"),
            ("12:34:56", "12:44:55.7", "PT9M59.7S"),
            ("12:44:56", "12:34:55.7", "PT-10M-0.3S"),
            ("12:34:56.3", "12:34:54.7", "PT-1.6S"),
            ("12:34:54.7", "12:34:56.3", "PT1.6S"),
        ];
        for (lhs, rhs, expected) in cases {
            assert_eq!(
                rendered(in_seconds(&lt(lhs), &lt(rhs))),
                expected,
                "inSeconds({lhs}, {rhs})"
            );
        }
    }

    #[test]
    fn in_seconds_scenario_12_no_difference() {
        let values = [
            make_localtime(9, 15, 0, 0),
            make_time(9, 15, 0, 0, 1800),
            make_date(2020, 5, 4),
            make_localdatetime(2020, 5, 4, 9, 15, 0, 0),
            make_datetime(2020, 5, 4, 9, 15, 0, 0, 1800, None),
        ];
        for v in values {
            assert_eq!(
                rendered(in_seconds(&v, &v)),
                "PT0S",
                "inSeconds({v:?}, {v:?})"
            );
        }
    }

    #[test]
    fn scenario_13_propagates_null_for_every_function_in_the_family() {
        assert_eq!(between(&Value::Null, &Value::Null).unwrap(), Value::Null);
        assert_eq!(in_months(&Value::Null, &Value::Null).unwrap(), Value::Null);
        assert_eq!(in_days(&Value::Null, &Value::Null).unwrap(), Value::Null);
        assert_eq!(in_seconds(&Value::Null, &Value::Null).unwrap(), Value::Null);
    }

    // ── Named-zone offset donation (Temporal10.feature scenarios [1]/[8]) ──

    #[test]
    fn between_scenario_1_stockholm_pair_splits_at_a_dst_fall_back_boundary() {
        // Both operands already carry their own explicit offset (this is
        // the `datetime('...[Zone]')` STRING form, not the map-constructor
        // named-zone-donation path) — the zone bracket round-trips through
        // `temporal_retag::strip_zone_bracket`/`fn_temporal.rs`'s string
        // constructor, and `zone_name` on both operands is set, but
        // `normalize_pair`'s donation step never fires here since neither
        // side lacks its own offset. This exercises the same "a datetime
        // string literal with a `[Zone]` bracket parses and computes
        // correctly" surface Temporal10 scenario [1]'s Stockholm rows pin.
        //
        // Deliberately NOT run through `assert_between_and_invariant`: `a +
        // between(a, b)` lands on the exact same UTC instant as `b`
        // (03:00Z either way) but renders with the OTHER side's offset
        // (`05:00+02:00` vs. `04:00+01:00`) — re-resolving which offset a
        // `+`/`-` arithmetic result renders with after crossing a DST
        // transition is explicitly out of scope for this task (see this
        // crate's `apply_duration_to_tagged_instant`, which keeps the
        // operand's own pre-arithmetic offset unconditionally).
        let a = make_datetime(
            2017,
            10,
            28,
            23,
            0,
            0,
            0,
            7200,
            Some("Europe/Stockholm".into()),
        );
        let b = make_datetime(
            2017,
            10,
            29,
            4,
            0,
            0,
            0,
            3600,
            Some("Europe/Stockholm".into()),
        );
        assert_eq!(rendered(between(&a, &b)), "PT6H");
        assert_eq!(rendered(between(&b, &a)), "PT-6H");
    }

    #[test]
    fn in_seconds_scenario_8_daylight_saving_time_day() {
        // openCypher TCK `Temporal10.feature` scenario [8]'s full six-row
        // table — 29 October 2017 is Stockholm's DST fall-back day
        // (03:00 CEST -> 02:00 CET). Every row pairs a zoned `datetime`
        // against an offset-less operand (`localdatetime`/`localtime`/
        // `date`), each exercising `donate_offset_via_named_zone` from a
        // different direction/shape.
        let zoned_hour0 = make_datetime(
            2017,
            10,
            29,
            0,
            0,
            0,
            0,
            7200,
            Some("Europe/Stockholm".into()),
        );
        let zoned_hour4 = make_datetime(
            2017,
            10,
            29,
            4,
            0,
            0,
            0,
            3600,
            Some("Europe/Stockholm".into()),
        );

        let cases: [(Value, Value); 6] = [
            (
                zoned_hour0.clone(),
                make_localdatetime(2017, 10, 29, 4, 0, 0, 0),
            ),
            (zoned_hour0.clone(), make_localtime(4, 0, 0, 0)),
            (
                make_localdatetime(2017, 10, 29, 0, 0, 0, 0),
                zoned_hour4.clone(),
            ),
            (make_localtime(0, 0, 0, 0), zoned_hour4.clone()),
            (make_date(2017, 10, 29), zoned_hour4),
            (zoned_hour0, make_date(2017, 10, 30)),
        ];
        let expected = ["PT5H", "PT5H", "PT5H", "PT5H", "PT5H", "PT25H"];

        for ((lhs, rhs), expected) in cases.into_iter().zip(expected) {
            assert_eq!(
                rendered(in_seconds(&lhs, &rhs)),
                expected,
                "inSeconds({lhs:?}, {rhs:?})"
            );
        }
    }

    #[test]
    fn donation_is_a_no_op_when_the_donor_has_only_a_fixed_numeric_offset() {
        // A fixed-offset (unnamed) datetime paired with an offset-less
        // operand must keep comparing via bare local fields — donating a
        // CONSTANT offset always nets a zero shift (see
        // `normalize_pair`'s "Named-zone donation" doc section), so this
        // must reproduce the exact pre-donation answer.
        let fixed = make_datetime(2014, 7, 21, 21, 40, 36, 143_000_000, 7200, None);
        let bare = make_localtime(16, 30, 0, 0);
        assert_eq!(rendered(in_seconds(&fixed, &bare)), "PT-5H-10M-36.143S");
    }

    #[test]
    fn donation_degrades_gracefully_for_an_unresolvable_stored_bracket_zone() {
        // A property already sitting in storage as
        // `'2020-01-01T12:00Z[Bogus/Zone]'` retags (via `retag_instant`)
        // into a tagged `datetime` carrying an unresolvable zone name —
        // the retag path's bracket parsing does not validate it (unlike
        // the map/string CONSTRUCTOR paths — see
        // `temporal_retag::is_valid_named_zone`'s doc comment). Pairing it
        // with an offset-less operand must degrade to the pre-existing
        // "compare bare local fields" behavior, never hard-error the
        // whole query over data that was already written.
        let stored = Value::String("2020-01-01T12:00Z[Bogus/Zone]".to_string());
        let bare = make_localtime(16, 30, 0, 0);
        let result = in_seconds(&stored, &bare);
        assert!(
            result.is_ok(),
            "an unresolvable stored bracket zone must not hard-error the query: {result:?}"
        );
        assert_eq!(rendered(result), "PT4H30M");
    }
}
