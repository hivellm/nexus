//! `date.truncate`/`datetime.truncate`/`localdatetime.truncate`/
//! `time.truncate`/`localtime.truncate` — the five Neo4j "truncate a
//! temporal instant to a coarser unit" builtins (openCypher TCK
//! `Temporal9.feature`).
//!
//! `truncate(unit, other, map)` zeroes every calendar/time-of-day field
//! below `unit`, then optionally overrides specific fields from the `map`
//! argument. Verified row-by-row against every scenario table in
//! `Temporal9.feature`:
//!
//! - `millennium`/`century`/`decade`/`year` truncate the calendar year down
//!   to the nearest boundary (`year.div_euclid(n) * n`) and reset
//!   month/day to `1`/`1`.
//! - `quarter` resets the day to `1` and the month to the first month of
//!   the input's quarter (`((month - 1) / 3) * 3 + 1`).
//! - `month` resets only the day to `1`.
//! - `week` truncates to the Monday of the input's own ISO week
//!   (`NaiveDate::iso_week`); `weekYear` truncates to the Monday of week 1
//!   of the input's ISO week-*year* — these are genuinely different
//!   anchors (an ISO week-year's week 1 can start in the *previous*
//!   calendar year, e.g. `datetime({year: 1984, month: 1, day: 1})`'s ISO
//!   week-year is 1983, so `weekYear` truncates to `1983-01-03`).
//! - `day` and finer (`hour`/`minute`/`second`/`millisecond`/
//!   `microsecond`) leave the calendar date untouched and truncate only
//!   the time-of-day, down to (and including) the given unit.
//!
//! Every unit coarser than `day` also zeroes the entire time-of-day
//! (`00:00:00.000000000`); `day` itself is the shared boundary between the
//! "date" family and the "time" family of units above.
//!
//! ## The `map` argument
//!
//! Only the four keys the TCK actually exercises are recognised:
//!
//! - `day` — replaces the truncated day-of-month directly (e.g.
//!   `date.truncate('decade', ..., {day: 2})` lands on the decade's first
//!   year, day 2 — not "2 days after the truncated instant").
//! - `dayOfWeek` — only meaningful after a `week`/`weekYear` truncation:
//!   re-anchors within the *same* ISO week the truncation already landed
//!   on, to the given ISO weekday (`1` = Monday … `7` = Sunday).
//!   Combining both in one call is not exercised by the TCK; this
//!   implementation resolves it deterministically (`dayOfWeek` wins) rather
//!   than treating it as an error.
//! - `nanosecond` — ADDS to the truncated nanosecond-of-second field
//!   (never replaces it): `millisecond`/`microsecond` truncation already
//!   leaves a non-zero sub-second remainder (e.g. `645_000_000` for
//!   `millisecond`), and the TCK's `{nanosecond: 2}` rows expect that
//!   remainder preserved with the override folded in underneath it
//!   (`645_000_002`, not `2`). For every coarser unit (`day` through
//!   `second`) the truncated nanosecond is already `0`, so add and replace
//!   are indistinguishable there. The override is bounded to the sub-unit
//!   precision the truncation itself just freed — strictly under
//!   `1_000_000` after a `millisecond` truncation, strictly under `1_000`
//!   after a `microsecond` one, the full `[0, 1_000_000_000)` range
//!   otherwise (see [`nanosecond_override_bound`]) — specifically so an
//!   override can never itself carry the result past the very truncation
//!   boundary it was truncated to (e.g. `{nanosecond: 999999999}` on top of
//!   a `millisecond` truncation must not silently roll the instant into the
//!   next second, minute, hour, or calendar year); an out-of-bound value is
//!   a hard error, never a silently-accepted carry.
//! - `timezone` — overrides the output's UTC offset (only meaningful for
//!   the `datetime`/`time` output kinds); resolved through
//!   [`super::temporal_retag::resolve_timezone_string`], the same helper
//!   `fn_temporal.rs::timezone_from_map` uses for the `time`/`datetime`
//!   constructors' own `timezone` key — a named IANA zone (e.g.
//!   `'Europe/Stockholm'`) is not resolvable without a timezone database
//!   and errors explicitly. Absent a `timezone` override, the output
//!   offset is the input's own offset when the input is `time`/`datetime`
//!   (a `date`/`localtime`/`localdatetime` input carries none, and the
//!   output default is UTC — rendered `Z`).
//!
//! Every map key above is validated the same way: the key being *absent*
//! leaves the corresponding field untouched, but the key being *present
//! with an invalid value* (wrong JSON type, out of range, or — for
//! `nanosecond` — out of its unit-bounded range) is always a
//! `CypherExecution` error, never silently treated as if the key had been
//! omitted.
//!
//! ## Unit × target-kind validity
//!
//! Not every unit applies to every output kind — `date.truncate` has no
//! time-of-day to truncate, so it rejects `hour` and finer; `time.truncate`/
//! `localtime.truncate` have no calendar date to truncate, so they reject
//! everything coarser than `day`. `datetime.truncate`/
//! `localdatetime.truncate` accept every unit (see
//! [`unit_rejects_target`]). A rejected combination is a `CypherExecution`
//! error, not a silent no-op.
//!
//! ## Input coercion
//!
//! `other` may be an already-tagged temporal value (the common case — a
//! nested `date(...)`/`datetime(...)`/... constructor call, still tagged
//! at this point in evaluation) or a stored canonical ISO-8601 string
//! (re-derived via [`super::temporal_retag::retag_canonical_string`], the
//! same re-derivation every other typed temporal consumer in this crate
//! uses for a value read back from storage). Any other shape — including a
//! tagged `duration` — truncates to `Null`, matching this codebase's
//! established "can't interpret the input, return Null" convention for
//! temporal builtins (see `fn_temporal.rs`'s constructors). A date-less
//! source (`time`/`localtime`) paired with a date-bearing target
//! (`date`/`datetime`/`localdatetime`) also truncates to `Null` — there is
//! no real calendar date to truncate, and fabricating `0000-01-01` would be
//! silently wrong rather than absent.

use super::temporal_retag;
use super::temporal_value::{self, TemporalKind};
use crate::{Error, Result};
use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime, Timelike, Weekday};
use serde_json::{Map, Value};

/// One resolved input operand: calendar date, time-of-day, and (if the
/// input kind carries one) UTC offset — defaulting the calendar date to
/// `0000-01-01` and the time-of-day to midnight when the input kind
/// doesn't carry that part (e.g. a bare `time` has no calendar date), so
/// every unit's truncation math below can operate on a single uniform
/// shape. `has_date` records whether that default was actually used —
/// [`truncate`] consults it to reject (as `Null`) a date-less source
/// against a date-bearing target (`date`/`datetime`/`localdatetime`)
/// instead of silently fabricating `0000-01-01`; a time-family target
/// (`time`/`localtime`) never needs a real calendar date, so it ignores
/// `has_date` and the `(0, 1, 1)` default stays purely internal — it only
/// exists so the nanosecond-override carry step has a `NaiveDateTime` to
/// add to.
struct SourceInstant {
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    nanosecond: u32,
    has_date: bool,
    offset_seconds: Option<i32>,
}

/// Resolves `value` into a [`SourceInstant`], accepting either an
/// already-tagged temporal instant or a stored canonical-string rendering
/// (see the module doc comment's "Input coercion" section). Returns `None`
/// for anything else, including a tagged `duration` (which has no
/// calendar/time-of-day fields to truncate).
fn resolve_temporal_input(value: &Value) -> Option<SourceInstant> {
    let tagged = if temporal_value::is_temporal_instant(value) {
        value.clone()
    } else if let Value::String(s) = value {
        let candidate = temporal_retag::retag_canonical_string(s)?;
        if !temporal_value::is_temporal_instant(&candidate) {
            return None;
        }
        candidate
    } else {
        return None;
    };

    let date_components = temporal_value::date_components(&tagged);
    let (year, month, day) = date_components.unwrap_or((0, 1, 1));
    let (hour, minute, second, nanosecond) =
        temporal_value::time_components(&tagged).unwrap_or((0, 0, 0, 0));
    let offset_seconds = temporal_value::offset_seconds(&tagged);
    Some(SourceInstant {
        year,
        month,
        day,
        hour,
        minute,
        second,
        nanosecond,
        has_date: date_components.is_some(),
        offset_seconds,
    })
}

/// `year.div_euclid(n) * n` — floors `year` to the nearest multiple of `n`
/// at or below it (correct for negative years too, unlike truncating `/`).
fn floor_to(year: i32, n: i32) -> i32 {
    year.div_euclid(n) * n
}

/// The unit×target-kind validity matrix, derived from which unit/target
/// combinations `Temporal9.feature`'s five scenario tables actually
/// exercise: `date.truncate` only ever uses `millennium` through `day`
/// (never `hour` and finer — a `date` has no time-of-day to truncate),
/// while `time.truncate`/`localtime.truncate` only ever use `day` through
/// `microsecond` (never `week`/`month`/... — a bare time has no calendar
/// date to truncate). `datetime.truncate`/`localdatetime.truncate` use
/// every unit, since those targets carry both parts. `day` itself sits on
/// both lists (the shared boundary) and is valid everywhere. Returns
/// `true` only for a *recognised* unit that is invalid for `target` — an
/// unrecognised unit string is not this function's concern (it falls
/// through to `truncate_date_part`/`truncate_time_part`, which resolve it
/// to `Null` unconditionally).
fn unit_rejects_target(unit: &str, target: TemporalKind) -> bool {
    let date_only = matches!(
        unit,
        "millennium" | "century" | "decade" | "year" | "weekYear" | "quarter" | "month" | "week"
    );
    let time_only = matches!(
        unit,
        "hour" | "minute" | "second" | "millisecond" | "microsecond"
    );
    match target {
        TemporalKind::Date => time_only,
        TemporalKind::Time | TemporalKind::LocalTime => date_only,
        TemporalKind::DateTime | TemporalKind::LocalDateTime | TemporalKind::Duration => false,
    }
}

/// The lowercase Cypher constructor name for `target`, used only to name
/// the function in [`unit_rejects_target`]'s error message
/// (`TemporalKind::Duration` never reaches this — no dispatch arm
/// registers `duration.truncate`).
fn target_name(target: TemporalKind) -> &'static str {
    match target {
        TemporalKind::Date => "date",
        TemporalKind::LocalTime => "localtime",
        TemporalKind::LocalDateTime => "localdatetime",
        TemporalKind::Time => "time",
        TemporalKind::DateTime => "datetime",
        TemporalKind::Duration => "duration",
    }
}

/// Truncates the calendar-date part of an instant for `unit`. Returns
/// `None` for a unit this function doesn't recognise, or if the
/// intermediate `(year, month, day)` isn't itself a valid calendar date
/// (defensive — every caller's `(year, month, day)` already came from a
/// validated temporal value).
fn truncate_date_part(unit: &str, year: i32, month: u32, day: u32) -> Option<(i32, u32, u32)> {
    let date = NaiveDate::from_ymd_opt(year, month, day)?;
    match unit {
        "millennium" => Some((floor_to(year, 1000), 1, 1)),
        "century" => Some((floor_to(year, 100), 1, 1)),
        "decade" => Some((floor_to(year, 10), 1, 1)),
        "year" => Some((year, 1, 1)),
        "quarter" => Some((year, ((month - 1) / 3) * 3 + 1, 1)),
        "month" => Some((year, month, 1)),
        "week" => {
            let iso = date.iso_week();
            let monday = NaiveDate::from_isoywd_opt(iso.year(), iso.week(), Weekday::Mon)?;
            Some((monday.year(), monday.month(), monday.day()))
        }
        "weekYear" => {
            let iso = date.iso_week();
            let monday = NaiveDate::from_isoywd_opt(iso.year(), 1, Weekday::Mon)?;
            Some((monday.year(), monday.month(), monday.day()))
        }
        "day" | "hour" | "minute" | "second" | "millisecond" | "microsecond" => {
            Some((year, month, day))
        }
        _ => None,
    }
}

/// Truncates the time-of-day part of an instant for `unit`. Every unit
/// coarser than `day` zeroes the whole time-of-day; `day` itself also
/// zeroes it (it's the coarsest member of the "time" family, matching
/// `truncate_date_part`'s finest member of the "date" family). Returns
/// `None` for a unit this function doesn't recognise.
fn truncate_time_part(
    unit: &str,
    hour: u32,
    minute: u32,
    second: u32,
    nanosecond: u32,
) -> Option<(u32, u32, u32, u32)> {
    match unit {
        "millennium" | "century" | "decade" | "year" | "quarter" | "month" | "week"
        | "weekYear" | "day" => Some((0, 0, 0, 0)),
        "hour" => Some((hour, 0, 0, 0)),
        "minute" => Some((hour, minute, 0, 0)),
        "second" => Some((hour, minute, second, 0)),
        "millisecond" => Some((hour, minute, second, (nanosecond / 1_000_000) * 1_000_000)),
        "microsecond" => Some((hour, minute, second, (nanosecond / 1_000) * 1_000)),
        _ => None,
    }
}

/// Maps a Neo4j ISO weekday number (`1` = Monday … `7` = Sunday) to
/// `chrono::Weekday`. Returns `None` outside `1..=7`.
fn weekday_from_iso_number(n: u32) -> Option<Weekday> {
    match n {
        1 => Some(Weekday::Mon),
        2 => Some(Weekday::Tue),
        3 => Some(Weekday::Wed),
        4 => Some(Weekday::Thu),
        5 => Some(Weekday::Fri),
        6 => Some(Weekday::Sat),
        7 => Some(Weekday::Sun),
        _ => None,
    }
}

/// Reads a map key expected to hold a non-negative integer, applying the
/// same "key present but invalid is a hard error, never silently treated
/// as absent" rule [`nanosecond_override`] uses — shared by the `day`/
/// `dayOfWeek` override keys below. `None` when `key` itself is absent
/// from `map` (nothing to override); `Err` for a present-but-wrong-shape
/// value (wrong JSON type, negative, fractional, or too large for `u32`)
/// — this is what closes the "`{day: -1}`/`{day: 'x'}` silently ignored"
/// gap: those used to fall through this same `and_then` chain as if the
/// key were absent, which is indistinguishable from the caller's
/// perspective from a typo'd or genuinely-omitted override.
fn non_negative_u32_map_key(map: &Map<String, Value>, key: &str) -> Result<Option<u32>> {
    let Some(value) = map.get(key) else {
        return Ok(None);
    };
    match value.as_u64().and_then(|n| u32::try_from(n).ok()) {
        Some(n) => Ok(Some(n)),
        None => Err(Error::CypherExecution(format!(
            "InvalidArgumentValue: `{key}` must be a non-negative integer, got {value}"
        ))),
    }
}

/// Applies the map's `day`/`dayOfWeek` override to an already-truncated
/// calendar date (see the module doc comment's "The `map` argument"
/// section for the exact semantics of each — note `dayOfWeek` takes
/// precedence when a caller supplies both, since it's the more specific of
/// the two: it re-anchors within the ISO week `dayOfWeek` itself already
/// depends on, whereas plain `day` is a context-free day-of-month
/// replacement). Neither key present leaves the truncated date untouched.
/// An out-of-range override (an invalid day-of-month, or a `dayOfWeek`
/// outside `1..=7`) is now a hard `CypherExecution` error — same rule as a
/// wrong-shape value (see [`non_negative_u32_map_key`]'s doc comment) —
/// rather than silently collapsing the whole `truncate()` call to `Null`.
fn apply_date_override(
    map: &Map<String, Value>,
    year: i32,
    month: u32,
    day: u32,
) -> Result<Option<(i32, u32, u32)>> {
    if let Some(day_of_week) = non_negative_u32_map_key(map, "dayOfWeek")? {
        let Some(weekday) = weekday_from_iso_number(day_of_week) else {
            return Err(Error::CypherExecution(format!(
                "InvalidArgumentValue: `dayOfWeek` must be an integer in [1, 7], got {day_of_week}"
            )));
        };
        let Some(base) = NaiveDate::from_ymd_opt(year, month, day) else {
            return Ok(None);
        };
        let iso = base.iso_week();
        let Some(adjusted) = NaiveDate::from_isoywd_opt(iso.year(), iso.week(), weekday) else {
            return Ok(None);
        };
        return Ok(Some((adjusted.year(), adjusted.month(), adjusted.day())));
    }
    if let Some(day_override) = non_negative_u32_map_key(map, "day")? {
        if NaiveDate::from_ymd_opt(year, month, day_override).is_none() {
            return Err(Error::CypherExecution(format!(
                "InvalidArgumentValue: `day` {day_override} is not a valid day-of-month for \
                 {year:04}-{month:02}"
            )));
        }
        return Ok(Some((year, month, day_override)));
    }
    Ok(Some((year, month, day)))
}

/// The exclusive upper bound a `nanosecond` override must stay under for
/// `unit`: exactly the sub-unit precision that unit's own truncation just
/// freed. `millisecond` truncation leaves a remainder that is always a
/// multiple of `1_000_000`, so an override of `1_000_000` or more would
/// carry the result past the very millisecond boundary `millisecond.
/// truncate` just landed on — same reasoning for `microsecond` and
/// `1_000`. Every other unit's freed remainder is either already `0`
/// (`day` through `second`) or — for `day` specifically — up to a whole
/// day, but the map's `nanosecond` key can only ever contribute a
/// sub-second amount in this codebase's TCK-derived grammar (see the
/// module doc comment), so bounding those to a full second
/// (`1_000_000_000`, [`nanosecond_override`]'s pre-existing type-range
/// check) already keeps every one of them safely short of its own next
/// coarser boundary.
fn nanosecond_override_bound(unit: &str) -> i64 {
    match unit {
        "millisecond" => 1_000_000,
        "microsecond" => 1_000,
        _ => 1_000_000_000,
    }
}

/// Reads the map's `nanosecond` override, bounded to
/// [`nanosecond_override_bound`]'s sub-unit range for `unit` — `None` when
/// the key is absent (nothing to add), an explicit error for a value
/// outside that range (including a negative or fractional one) rather than
/// silently reinterpreting it or letting it carry the result past the
/// unit's own truncation boundary (e.g. `{nanosecond: 999999999}` on top
/// of a `millisecond` truncation must not silently roll the instant into
/// the next second, minute, hour, or even calendar year).
fn nanosecond_override(map: &Map<String, Value>, unit: &str) -> Result<Option<i64>> {
    let Some(value) = map.get("nanosecond") else {
        return Ok(None);
    };
    let bound = nanosecond_override_bound(unit);
    match value.as_i64().filter(|n| (0..bound).contains(n)) {
        Some(n) => Ok(Some(n)),
        None => Err(Error::CypherExecution(format!(
            "InvalidArgumentValue: `nanosecond` must be a non-negative integer in [0, {bound}) \
             for unit '{unit}', got {value}"
        ))),
    }
}

/// Applies both the date override (`day`/`dayOfWeek`) and the time
/// override (`nanosecond`, added rather than replacing — see the module
/// doc comment) to an already unit-truncated instant, returning the final
/// `(year, month, day, hour, minute, second, nanosecond)`. An invalid
/// override value (wrong shape, out of range, or a nanosecond addition
/// bounded by [`nanosecond_override_bound`]) is now always a
/// `Result::Err`, never a silent `Null` — the only remaining `Ok(None)`
/// paths are the two genuinely defensive "the truncated instant plus a
/// structurally in-range override still isn't a real calendar
/// date/time/`NaiveDateTime` add" cases, which no known TCK-derived input
/// reaches.
#[allow(clippy::too_many_arguments)]
fn apply_overrides(
    map: &Map<String, Value>,
    unit: &str,
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    nanosecond: u32,
) -> Result<Option<(i32, u32, u32, u32, u32, u32, u32)>> {
    let Some((year, month, day)) = apply_date_override(map, year, month, day)? else {
        return Ok(None);
    };
    let Some(base_date) = NaiveDate::from_ymd_opt(year, month, day) else {
        return Ok(None);
    };
    let Some(base_time) = NaiveTime::from_hms_nano_opt(hour, minute, second, nanosecond) else {
        return Ok(None);
    };
    let base_dt = NaiveDateTime::new(base_date, base_time);

    let final_dt = match nanosecond_override(map, unit)? {
        Some(extra_nanos) => {
            match base_dt.checked_add_signed(chrono::Duration::nanoseconds(extra_nanos)) {
                Some(dt) => dt,
                None => return Ok(None),
            }
        }
        None => base_dt,
    };

    Ok(Some((
        final_dt.year(),
        final_dt.month(),
        final_dt.day(),
        final_dt.hour(),
        final_dt.minute(),
        final_dt.second(),
        final_dt.nanosecond(),
    )))
}

/// Resolves the output's UTC offset: the map's `timezone` override if
/// present, otherwise `fallback` (the input's own offset, or `0`/UTC when
/// the input carries none — resolved by the caller). A `timezone` key that
/// isn't a string (e.g. `{timezone: 5}`) is now a hard error — same
/// "key present but wrong shape is never silently treated as absent" rule
/// [`non_negative_u32_map_key`] applies to `day`/`dayOfWeek` — rather than
/// silently falling back to `fallback` as if the key had never been
/// supplied. A well-formed string resolves through
/// [`temporal_retag::resolve_timezone_string`], the same helper
/// `fn_temporal.rs::timezone_from_map` uses for the `time`/`datetime`
/// constructors' own `timezone` key, so both report identical error text
/// for the one case they share: an unresolvable named IANA zone.
fn resolve_offset(map: &Map<String, Value>, fallback: i32) -> Result<i32> {
    let Some(raw) = map.get("timezone") else {
        return Ok(fallback);
    };
    let Some(tz) = raw.as_str() else {
        return Err(Error::CypherExecution(format!(
            "InvalidArgumentValue: `timezone` must be a string, got {raw}"
        )));
    };
    temporal_retag::resolve_timezone_string(tz)
}

/// Evaluates `<kind>.truncate(unit, other, map)` for the already-evaluated
/// `unit`/`other`/`map` argument values — `map` is `None` for the
/// (Neo4j-legal) two-argument call form. `target` selects which of the
/// five temporal constructors builds the result (`date`, `datetime`,
/// `localdatetime`, `time`, `localtime` — a `duration` target never
/// reaches this function, since no dispatch arm registers
/// `duration.truncate`).
///
/// Returns `Value::Null` for a shape this function genuinely can't
/// interpret (an unrecognised unit, a non-temporal `other`, a non-object
/// `map`) or a date-bearing target (`date`/`datetime`/`localdatetime`)
/// paired with a date-less source (`time`/`localtime` — truncating those
/// to a calendar unit is meaningless, not `0000-01-01`), matching every
/// other temporal builtin in this crate's "can't construct a real result"
/// convention. Everything else this function can positively identify as
/// invalid — a unit that doesn't apply to `target` (see
/// [`unit_rejects_target`]), or a malformed/out-of-range `day`/
/// `dayOfWeek`/`nanosecond`/`timezone` map override — is a hard
/// `Result::Err` instead: a caller-visible mistake should surface as an
/// error, not sink into a hard-to-debug `Null`.
pub(in crate::executor) fn truncate(
    target: TemporalKind,
    unit_value: &Value,
    source_value: &Value,
    map_value: Option<&Value>,
) -> Result<Value> {
    let Some(unit) = unit_value.as_str() else {
        return Ok(Value::Null);
    };
    if unit_rejects_target(unit, target) {
        return Err(Error::CypherExecution(format!(
            "InvalidArgumentValue: unit '{unit}' is not valid for {}.truncate()",
            target_name(target)
        )));
    }
    let Some(source) = resolve_temporal_input(source_value) else {
        return Ok(Value::Null);
    };
    if !source.has_date
        && matches!(
            target,
            TemporalKind::Date | TemporalKind::DateTime | TemporalKind::LocalDateTime
        )
    {
        return Ok(Value::Null);
    }
    let empty_map = Map::new();
    let map: &Map<String, Value> = match map_value {
        Some(Value::Object(m)) => m,
        Some(Value::Null) | None => &empty_map,
        _ => return Ok(Value::Null),
    };

    let Some((trunc_year, trunc_month, trunc_day)) =
        truncate_date_part(unit, source.year, source.month, source.day)
    else {
        return Ok(Value::Null);
    };
    let Some((trunc_hour, trunc_minute, trunc_second, trunc_nanosecond)) = truncate_time_part(
        unit,
        source.hour,
        source.minute,
        source.second,
        source.nanosecond,
    ) else {
        return Ok(Value::Null);
    };

    let Some((year, month, day, hour, minute, second, nanosecond)) = apply_overrides(
        map,
        unit,
        trunc_year,
        trunc_month,
        trunc_day,
        trunc_hour,
        trunc_minute,
        trunc_second,
        trunc_nanosecond,
    )?
    else {
        return Ok(Value::Null);
    };

    match target {
        TemporalKind::Date => Ok(temporal_value::make_date(year, month, day)),
        TemporalKind::LocalDateTime => Ok(temporal_value::make_localdatetime(
            year, month, day, hour, minute, second, nanosecond,
        )),
        TemporalKind::DateTime => {
            let offset = resolve_offset(map, source.offset_seconds.unwrap_or(0))?;
            Ok(temporal_value::make_datetime(
                year, month, day, hour, minute, second, nanosecond, offset, None,
            ))
        }
        TemporalKind::LocalTime => Ok(temporal_value::make_localtime(
            hour, minute, second, nanosecond,
        )),
        TemporalKind::Time => {
            let offset = resolve_offset(map, source.offset_seconds.unwrap_or(0))?;
            Ok(temporal_value::make_time(
                hour, minute, second, nanosecond, offset,
            ))
        }
        TemporalKind::Duration => Ok(Value::Null),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use temporal_value::{make_date, make_datetime, make_localdatetime, make_localtime, make_time};

    fn obj(entries: &[(&str, Value)]) -> Value {
        Value::Object(
            entries
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        )
    }

    #[test]
    fn truncates_date_to_millennium_with_day_override() {
        let source = make_date(2017, 10, 11);
        let unit = Value::String("millennium".to_string());
        let map = obj(&[("day", Value::from(2))]);
        let result = truncate(TemporalKind::Date, &unit, &source, Some(&map)).unwrap();
        assert_eq!(result, make_date(2000, 1, 2));
    }

    #[test]
    fn truncates_date_to_weekyear_from_a_datetime_source() {
        // datetime({year: 1984, month: 1, day: 1, ...}) — ISO week-year is
        // 1983 (Jan 1 1984 is a Sunday), so `weekYear` truncates to the
        // Monday of week 1 of 1983, not 1984.
        let source = make_datetime(1984, 1, 1, 12, 31, 14, 645_876_123, 3600, None);
        let unit = Value::String("weekYear".to_string());
        let result = truncate(TemporalKind::Date, &unit, &source, None).unwrap();
        assert_eq!(result, make_date(1983, 1, 3));
    }

    #[test]
    fn truncates_date_to_week_with_dayofweek_override() {
        let source = make_date(1984, 10, 11);
        let unit = Value::String("week".to_string());
        let map = obj(&[("dayOfWeek", Value::from(2))]);
        let result = truncate(TemporalKind::Date, &unit, &source, Some(&map)).unwrap();
        assert_eq!(result, make_date(1984, 10, 9));
    }

    #[test]
    fn truncates_datetime_to_millennium_keeping_source_offset() {
        let source = make_datetime(2017, 10, 11, 12, 31, 14, 645_876_123, 3600, None);
        let unit = Value::String("millennium".to_string());
        let map = obj(&[("day", Value::from(2))]);
        let result = truncate(TemporalKind::DateTime, &unit, &source, Some(&map)).unwrap();
        assert_eq!(result, make_datetime(2000, 1, 2, 0, 0, 0, 0, 3600, None));
    }

    #[test]
    fn truncates_datetime_from_a_date_source_defaults_offset_to_utc() {
        let source = make_date(2017, 10, 11);
        let unit = Value::String("millennium".to_string());
        let result = truncate(TemporalKind::DateTime, &unit, &source, None).unwrap();
        assert_eq!(result, make_datetime(2000, 1, 1, 0, 0, 0, 0, 0, None));
    }

    #[test]
    fn nanosecond_override_adds_to_the_truncated_remainder_not_replaces_it() {
        // millisecond truncation of 645_876_123 leaves 645_000_000; the
        // {nanosecond: 2} override must land on 645_000_002, not 2.
        let source = make_datetime(1984, 10, 11, 12, 31, 14, 645_876_123, 3600, None);
        let unit = Value::String("millisecond".to_string());
        let map = obj(&[("nanosecond", Value::from(2))]);
        let result = truncate(TemporalKind::DateTime, &unit, &source, Some(&map)).unwrap();
        assert_eq!(
            result,
            make_datetime(1984, 10, 11, 12, 31, 14, 645_000_002, 3600, None)
        );
    }

    #[test]
    fn truncates_localdatetime_dropping_any_source_offset() {
        let source = make_datetime(1984, 10, 11, 12, 31, 14, 645_876_123, 3600, None);
        let unit = Value::String("hour".to_string());
        let result = truncate(TemporalKind::LocalDateTime, &unit, &source, None).unwrap();
        assert_eq!(result, make_localdatetime(1984, 10, 11, 12, 0, 0, 0));
    }

    #[test]
    fn truncates_localtime_to_day_zeroing_the_whole_time_of_day() {
        let source = make_datetime(1984, 10, 11, 12, 31, 14, 645_876_123, 3600, None);
        let unit = Value::String("day".to_string());
        let map = obj(&[("nanosecond", Value::from(2))]);
        let result = truncate(TemporalKind::LocalTime, &unit, &source, Some(&map)).unwrap();
        assert_eq!(result, make_localtime(0, 0, 0, 2));
    }

    #[test]
    fn truncates_time_with_a_numeric_timezone_override() {
        let source = make_time(12, 31, 14, 645_876_123, -3600);
        let unit = Value::String("hour".to_string());
        let map = obj(&[("timezone", Value::from("+01:00"))]);
        let result = truncate(TemporalKind::Time, &unit, &source, Some(&map)).unwrap();
        assert_eq!(result, make_time(12, 0, 0, 0, 3600));
    }

    #[test]
    fn truncates_time_from_a_localtime_source_defaults_offset_to_utc() {
        let source = make_localtime(12, 31, 14, 645_876_123);
        let unit = Value::String("minute".to_string());
        let result = truncate(TemporalKind::Time, &unit, &source, None).unwrap();
        assert_eq!(result, make_time(12, 31, 0, 0, 0));
    }

    #[test]
    fn named_timezone_override_errors_explicitly() {
        let source = make_date(2017, 10, 11);
        let unit = Value::String("millennium".to_string());
        let map = obj(&[("timezone", Value::from("Europe/Stockholm"))]);
        let err = truncate(TemporalKind::DateTime, &unit, &source, Some(&map)).unwrap_err();
        assert!(matches!(err, Error::CypherExecution(_)));
    }

    #[test]
    fn out_of_range_nanosecond_override_errors_explicitly() {
        let source = make_date(2017, 10, 11);
        let unit = Value::String("day".to_string());
        let map = obj(&[("nanosecond", Value::from(-1))]);
        let err = truncate(TemporalKind::LocalTime, &unit, &source, Some(&map)).unwrap_err();
        assert!(matches!(err, Error::CypherExecution(_)));
    }

    #[test]
    fn unrecognised_unit_truncates_to_null() {
        let source = make_date(2017, 10, 11);
        let unit = Value::String("fortnight".to_string());
        let result = truncate(TemporalKind::Date, &unit, &source, None).unwrap();
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn a_duration_source_truncates_to_null() {
        let source = temporal_value::make_duration(0, 14, 0, 0);
        let unit = Value::String("day".to_string());
        let result = truncate(TemporalKind::Date, &unit, &source, None).unwrap();
        assert_eq!(result, Value::Null);
    }

    // ── Item 1: date-less source vs. a date-bearing target ──────────────

    #[test]
    fn date_less_source_against_a_date_bearing_target_truncates_to_null() {
        let source = make_localtime(12, 31, 14, 0);
        let unit = Value::String("year".to_string());
        assert_eq!(
            truncate(TemporalKind::Date, &unit, &source, None).unwrap(),
            Value::Null
        );
        assert_eq!(
            truncate(TemporalKind::DateTime, &unit, &source, None).unwrap(),
            Value::Null
        );
        assert_eq!(
            truncate(TemporalKind::LocalDateTime, &unit, &source, None).unwrap(),
            Value::Null
        );
    }

    #[test]
    fn date_less_source_against_a_time_family_target_still_truncates() {
        // `time`/`localtime` targets never need a real calendar date — the
        // internal `(0, 1, 1)` default stays purely internal plumbing for
        // the nanosecond-carry `NaiveDateTime`, never surfaced.
        let source = make_localtime(12, 31, 14, 645_876_123);
        let unit = Value::String("hour".to_string());
        assert_eq!(
            truncate(TemporalKind::LocalTime, &unit, &source, None).unwrap(),
            make_localtime(12, 0, 0, 0)
        );
        assert_eq!(
            truncate(TemporalKind::Time, &unit, &source, None).unwrap(),
            make_time(12, 0, 0, 0, 0)
        );
    }

    // ── Item 2: unit × target-kind validity matrix ───────────────────────

    #[test]
    fn a_sub_day_unit_against_a_date_target_is_a_hard_error() {
        let source = make_date(1984, 10, 11);
        let unit = Value::String("hour".to_string());
        let err = truncate(TemporalKind::Date, &unit, &source, None).unwrap_err();
        assert!(matches!(err, Error::CypherExecution(_)));
    }

    #[test]
    fn a_coarser_than_day_unit_against_a_time_target_is_a_hard_error() {
        let source = make_time(12, 31, 14, 0, 0);
        let unit = Value::String("year".to_string());
        let err = truncate(TemporalKind::Time, &unit, &source, None).unwrap_err();
        assert!(matches!(err, Error::CypherExecution(_)));

        let source = make_localtime(12, 31, 14, 0);
        let err = truncate(TemporalKind::LocalTime, &unit, &source, None).unwrap_err();
        assert!(matches!(err, Error::CypherExecution(_)));
    }

    #[test]
    fn every_unit_is_valid_against_datetime_and_localdatetime_targets() {
        let source = make_datetime(1984, 10, 11, 12, 31, 14, 0, 0, None);
        for unit in [
            "millennium",
            "century",
            "decade",
            "year",
            "weekYear",
            "quarter",
            "month",
            "week",
            "day",
            "hour",
            "minute",
            "second",
            "millisecond",
            "microsecond",
        ] {
            let unit_value = Value::String(unit.to_string());
            assert!(
                truncate(TemporalKind::DateTime, &unit_value, &source, None).is_ok(),
                "unit {unit} should be valid for datetime.truncate"
            );
            assert!(
                truncate(TemporalKind::LocalDateTime, &unit_value, &source, None).is_ok(),
                "unit {unit} should be valid for localdatetime.truncate"
            );
        }
    }

    // ── Item 3: map override validation is uniform (present-but-invalid
    // is always an error, never silently treated as absent) ─────────────

    #[test]
    fn a_non_numeric_day_override_is_a_hard_error() {
        let source = make_date(1984, 10, 11);
        let unit = Value::String("month".to_string());
        let map = obj(&[("day", Value::from("x"))]);
        let err = truncate(TemporalKind::Date, &unit, &source, Some(&map)).unwrap_err();
        assert!(matches!(err, Error::CypherExecution(_)));
    }

    #[test]
    fn a_negative_day_override_is_a_hard_error() {
        let source = make_date(1984, 10, 11);
        let unit = Value::String("month".to_string());
        let map = obj(&[("day", Value::from(-1))]);
        let err = truncate(TemporalKind::Date, &unit, &source, Some(&map)).unwrap_err();
        assert!(matches!(err, Error::CypherExecution(_)));
    }

    #[test]
    fn an_out_of_range_day_override_is_a_hard_error() {
        // October only has 31 days.
        let source = make_date(1984, 10, 11);
        let unit = Value::String("month".to_string());
        let map = obj(&[("day", Value::from(35))]);
        let err = truncate(TemporalKind::Date, &unit, &source, Some(&map)).unwrap_err();
        assert!(matches!(err, Error::CypherExecution(_)));
    }

    #[test]
    fn a_negative_day_of_week_override_is_a_hard_error() {
        let source = make_date(1984, 10, 11);
        let unit = Value::String("week".to_string());
        let map = obj(&[("dayOfWeek", Value::from(-1))]);
        let err = truncate(TemporalKind::Date, &unit, &source, Some(&map)).unwrap_err();
        assert!(matches!(err, Error::CypherExecution(_)));
    }

    #[test]
    fn an_out_of_range_day_of_week_override_is_a_hard_error() {
        let source = make_date(1984, 10, 11);
        let unit = Value::String("week".to_string());
        let map = obj(&[("dayOfWeek", Value::from(8))]);
        let err = truncate(TemporalKind::Date, &unit, &source, Some(&map)).unwrap_err();
        assert!(matches!(err, Error::CypherExecution(_)));
    }

    #[test]
    fn a_non_string_timezone_override_is_a_hard_error() {
        let source = make_date(1984, 10, 11);
        let unit = Value::String("year".to_string());
        let map = obj(&[("timezone", Value::from(5))]);
        let err = truncate(TemporalKind::DateTime, &unit, &source, Some(&map)).unwrap_err();
        assert!(matches!(err, Error::CypherExecution(_)));
    }

    // ── Item 4: the nanosecond override is bounded to the sub-unit
    // precision the truncation itself freed ─────────────────────────────

    #[test]
    fn nanosecond_override_within_the_millisecond_bound_still_works() {
        let source = make_datetime(1984, 10, 11, 12, 31, 14, 645_876_123, 3600, None);
        let unit = Value::String("millisecond".to_string());
        let map = obj(&[("nanosecond", Value::from(999_999))]);
        let result = truncate(TemporalKind::DateTime, &unit, &source, Some(&map)).unwrap();
        assert_eq!(
            result,
            make_datetime(1984, 10, 11, 12, 31, 14, 645_999_999, 3600, None)
        );
    }

    #[test]
    fn nanosecond_override_at_or_past_the_millisecond_bound_is_a_hard_error() {
        let source = make_datetime(1984, 10, 11, 12, 31, 14, 645_876_123, 3600, None);
        let unit = Value::String("millisecond".to_string());
        let map = obj(&[("nanosecond", Value::from(1_000_000))]);
        let err = truncate(TemporalKind::DateTime, &unit, &source, Some(&map)).unwrap_err();
        assert!(matches!(err, Error::CypherExecution(_)));
    }

    #[test]
    fn nanosecond_override_at_or_past_the_microsecond_bound_is_a_hard_error() {
        let source = make_datetime(1984, 10, 11, 12, 31, 14, 645_876_123, 3600, None);
        let unit = Value::String("microsecond".to_string());
        let map = obj(&[("nanosecond", Value::from(1_000))]);
        let err = truncate(TemporalKind::DateTime, &unit, &source, Some(&map)).unwrap_err();
        assert!(matches!(err, Error::CypherExecution(_)));
    }

    #[test]
    fn an_unbounded_nanosecond_override_can_no_longer_push_the_instant_into_the_next_year() {
        // Regression repro: `{nanosecond: 999999999}` on top of a
        // `millisecond` truncation of `.999876123` used to carry ~2 seconds
        // past midnight on New Year's Eve, rolling the result into the next
        // year. The bound now rejects it outright instead.
        let source = make_datetime(1984, 12, 31, 23, 59, 59, 999_876_123, 0, None);
        let unit = Value::String("millisecond".to_string());
        let map = obj(&[("nanosecond", Value::from(999_999_999))]);
        let err = truncate(TemporalKind::DateTime, &unit, &source, Some(&map)).unwrap_err();
        assert!(matches!(err, Error::CypherExecution(_)));
    }
}
