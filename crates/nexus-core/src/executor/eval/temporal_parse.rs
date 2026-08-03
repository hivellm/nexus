//! ISO-8601 string parsing for the temporal constructors (`date('...')`,
//! `time('...')`, `localtime('...')`, `datetime('...')`,
//! `localdatetime('...')`, `duration('...')`).
//!
//! Three independent grammars live here:
//!
//! - [`parse_iso_date`] — calendar (`YYYY-MM-DD`), week (`YYYY-Www-D`), and
//!   ordinal (`YYYY-DDD`) date forms, each in both extended (hyphenated) and
//!   basic (compact) notation, plus their truncated (`YYYY-MM`, `YYYY-Www`,
//!   `YYYY`) variants. Verified against the openCypher TCK's
//!   `Temporal2.feature` "Should parse date from string" scenario table.
//! - [`parse_iso_time`] / [`parse_offset`] / [`parse_iso_time_and_offset`] /
//!   [`parse_iso_datetime`] — time-of-day (`HH[:MM[:SS[.fraction]]]`,
//!   extended or compact) plus a trailing UTC offset (`Z`, extended
//!   `±HH:MM[:SS]`, or the compact `±HHMM[SS]`/`±HH` forms), and the full
//!   `date'T'time[offset]` instant grammar built from the two. Verified
//!   against the openCypher TCK's `Temporal10.feature` fixture set, which
//!   exercises fractional seconds and the compact (colonless) offset form
//!   throughout (e.g. `'2015-07-21T21:40:32.142+0100'`).
//! - [`parse_iso_duration`] — the full `P[n Y][n M][n W][n D][T[n H][n
//!   M][n S]]` duration grammar plus the ISO-8601 "alternative format"
//!   (`P<year>-<month>-<day>T<hour>:<minute>:<second>`), with fractional
//!   values on any component carrying down into the next-smaller unit.
//!   Verified against `Temporal2.feature`'s "Should parse duration from
//!   string" scenario table.
//!
//! Every function here hand-rolls its own tokenizing (no `regex` — every
//! substring access goes through `str::get`, so a malformed or non-ASCII
//! argument returns `None` rather than panicking on a bad char-boundary
//! slice) and returns `None` on any malformed or out-of-range input; callers
//! fall back to `Value::Null`, matching every other temporal constructor's
//! existing convention for an unparseable argument.

use super::temporal_value;
use chrono::{Datelike, NaiveDate, Weekday};

// ─────────────────────────────── Date parsing ──────────────────────────────

/// Parses a `date('...')` string argument into `(year, month, day)`,
/// accepting every ISO-8601 calendar/week/ordinal date form the TCK's
/// `Temporal2.feature` "Should parse date from string" scenario exercises:
///
/// - Calendar: `YYYY-MM-DD` / `YYYYMMDD`, `YYYY-MM` / `YYYYMM`, `YYYY`.
/// - Week (ISO week date — week 01 is the week containing the year's first
///   Thursday; weekday `1`=Monday..`7`=Sunday, defaulting to `1` when
///   omitted): `YYYY-Www-D` / `YYYYWwwD`, `YYYY-Www` / `YYYYWww`.
/// - Ordinal (day-of-year): `YYYY-DDD` / `YYYYDDD`.
///
/// Extended (hyphenated) and basic (compact) notation are never mixed
/// within one input — `has_hyphen`, decided once from the byte immediately
/// after the year, gates every separator check downstream.
pub(in crate::executor) fn parse_iso_date(input: &str) -> Option<(i32, u32, u32)> {
    let (sign, rest) = if let Some(r) = input.strip_prefix('-') {
        (-1_i32, r)
    } else if let Some(r) = input.strip_prefix('+') {
        (1_i32, r)
    } else {
        (1_i32, input)
    };

    let year_str = rest.get(..4)?;
    if !year_str.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let year: i32 = year_str.parse().ok()?;
    let year = year * sign;
    let remainder = rest.get(4..)?;

    if remainder.is_empty() {
        let date = NaiveDate::from_ymd_opt(year, 1, 1)?;
        return Some((date.year(), date.month(), date.day()));
    }

    let has_hyphen = remainder.starts_with('-');
    let body = if has_hyphen {
        remainder.get(1..)?
    } else {
        remainder
    };

    if let Some(week_body) = body.strip_prefix('W') {
        return parse_iso_week_date(year, week_body, has_hyphen);
    }

    parse_calendar_or_ordinal(year, body, has_hyphen)
}

/// Resolves the week-date tail (`Www[-D]` / `WwwD`) that follows the `W`
/// designator into a calendar `(year, month, day)`, defaulting the weekday
/// to `1` (Monday) when omitted.
fn parse_iso_week_date(year: i32, week_body: &str, has_hyphen: bool) -> Option<(i32, u32, u32)> {
    let week_str = week_body.get(..2)?;
    if !week_str.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let week: u32 = week_str.parse().ok()?;
    let day_part = week_body.get(2..)?;

    let weekday_num: u32 = if day_part.is_empty() {
        1
    } else {
        let digit_str = if has_hyphen {
            day_part.strip_prefix('-')?
        } else {
            day_part
        };
        if digit_str.chars().count() != 1 || !digit_str.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        digit_str.parse().ok()?
    };

    let weekday = match weekday_num {
        1 => Weekday::Mon,
        2 => Weekday::Tue,
        3 => Weekday::Wed,
        4 => Weekday::Thu,
        5 => Weekday::Fri,
        6 => Weekday::Sat,
        7 => Weekday::Sun,
        _ => return None,
    };

    let date = NaiveDate::from_isoywd_opt(year, week, weekday)?;
    Some((date.year(), date.month(), date.day()))
}

/// Resolves the non-week date tail into a calendar `(year, month, day)`:
/// a 2-digit body is a month-only truncation (`YYYY-MM`/`YYYYMM`, day
/// defaults to `1`), a 3-digit body is an ordinal day-of-year
/// (`YYYY-DDD`/`YYYYDDD`), and a 4-/5-digit body (depending on
/// `has_hyphen`) is a full calendar month+day.
fn parse_calendar_or_ordinal(year: i32, body: &str, has_hyphen: bool) -> Option<(i32, u32, u32)> {
    let all_ascii_digits = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());

    match (has_hyphen, body.len()) {
        (_, 2) if all_ascii_digits(body) => {
            let month: u32 = body.parse().ok()?;
            let date = NaiveDate::from_ymd_opt(year, month, 1)?;
            Some((date.year(), date.month(), date.day()))
        }
        (_, 3) if all_ascii_digits(body) => {
            let ordinal: u32 = body.parse().ok()?;
            let date = NaiveDate::from_yo_opt(year, ordinal)?;
            Some((date.year(), date.month(), date.day()))
        }
        (true, 5) => {
            let month_str = body.get(..2)?;
            let sep = body.get(2..3)?;
            let day_str = body.get(3..5)?;
            if sep != "-" || !all_ascii_digits(month_str) || !all_ascii_digits(day_str) {
                return None;
            }
            let month: u32 = month_str.parse().ok()?;
            let day: u32 = day_str.parse().ok()?;
            let date = NaiveDate::from_ymd_opt(year, month, day)?;
            Some((date.year(), date.month(), date.day()))
        }
        (false, 4) if all_ascii_digits(body) => {
            let month: u32 = body.get(..2)?.parse().ok()?;
            let day: u32 = body.get(2..4)?.parse().ok()?;
            let date = NaiveDate::from_ymd_opt(year, month, day)?;
            Some((date.year(), date.month(), date.day()))
        }
        _ => None,
    }
}

// ─────────────────────── Time-of-day + offset parsing ──────────────────────

/// Parses an ISO-8601 time-of-day (`HH[:MM[:SS[.fraction]]]`, extended or
/// the compact `HHMM[SS[.fraction]]`/`HH` notation — never mixed within one
/// input) into `(hour, minute, second, nanosecond)`. Lenient
/// constructor-input parsing (the same role [`parse_iso_date`] plays for
/// the date grammar) — deliberately NOT the strict round-trip check
/// `temporal_retag::strict_parse_time` performs against this codebase's own
/// canonical renderings, which rejects any shape that renderer itself would
/// never produce (e.g. `12:00:00` — zero seconds are always omitted on
/// output, but a caller-supplied literal is free to spell them out).
pub(in crate::executor) fn parse_iso_time(input: &str) -> Option<(u32, u32, u32, u32)> {
    let (main, frac) = match input.split_once('.') {
        Some((m, f)) => (m, Some(f)),
        None => (input, None),
    };

    // Extended (colon-separated, `HH`/`HH:MM`/`HH:MM:SS`) or compact
    // (`HH`/`HHMM`/`HHMMSS`) notation only — never mixed, and never a
    // stray/doubled colon. A blind "strip every colon" would accept
    // `'12::34:54'` (an empty field between two colons collapses away)
    // or `'1230:54'` (compact hour+minute, extended seconds) as if they
    // were well-formed; splitting on `:` first and requiring every
    // resulting field to be exactly 2 digits rejects both.
    let digits: String = if main.contains(':') {
        let fields: Vec<&str> = main.split(':').collect();
        if !matches!(fields.len(), 2 | 3) || fields.iter().any(|f| f.len() != 2) {
            return None;
        }
        fields.concat()
    } else {
        main.to_string()
    };
    if !matches!(digits.len(), 2 | 4 | 6) || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }

    // A fractional component only ever attaches to a present SECONDS
    // group (`HH:MM:SS.fraction` / `HHMMSS.fraction`) — Neo4j rejects it
    // anywhere else. Without this check, `'16:30.5'` (no seconds group
    // at all) would silently reinterpret the `.5` as a fractional SECOND
    // with an implicit whole-second value of `0`, rather than the
    // fractional MINUTE (or outright invalid literal) it actually reads
    // as.
    if frac.is_some() && digits.len() != 6 {
        return None;
    }

    let hour: u32 = digits.get(0..2)?.parse().ok()?;
    let minute: u32 = if digits.len() >= 4 {
        digits.get(2..4)?.parse().ok()?
    } else {
        0
    };
    let second: u32 = if digits.len() == 6 {
        digits.get(4..6)?.parse().ok()?
    } else {
        0
    };
    let nanosecond = match frac {
        None => 0u32,
        Some(f) if !f.is_empty() && f.len() <= 9 && f.chars().all(|c| c.is_ascii_digit()) => {
            format!("{f:0<9}").parse().ok()?
        }
        Some(_) => return None,
    };
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    Some((hour, minute, second, nanosecond))
}

/// Parses a lenient ISO-8601 UTC offset into signed seconds: `Z`/`z`, the
/// extended `±HH:MM[:SS]`, or the compact `±HHMM[SS]`/bare `±HH` forms —
/// unlike `temporal_retag::strict_parse_offset` (which only accepts the
/// exact colon-delimited shape this codebase's own renderer produces),
/// this accepts every offset shape a `time('...')`/`datetime('...')`
/// argument may legally spell, e.g. `'+0100'` (no colon), the form the
/// openCypher TCK's `Temporal10.feature` fixtures use throughout.
pub(in crate::executor) fn parse_offset(input: &str) -> Option<i32> {
    if input.eq_ignore_ascii_case("z") {
        return Some(0);
    }
    let (sign, rest) = match input.strip_prefix('+') {
        Some(r) => (1_i32, r),
        None => (-1_i32, input.strip_prefix('-')?),
    };
    let digits: String = rest.chars().filter(|c| *c != ':').collect();
    if !matches!(digits.len(), 2 | 4 | 6) || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let hours: i32 = digits.get(0..2)?.parse().ok()?;
    let minutes: i32 = if digits.len() >= 4 {
        digits.get(2..4)?.parse().ok()?
    } else {
        0
    };
    let seconds: i32 = if digits.len() == 6 {
        digits.get(4..6)?.parse().ok()?
    } else {
        0
    };
    if hours > 23 || minutes > 59 || seconds > 59 {
        return None;
    }
    Some(sign * (hours * 3600 + minutes * 60 + seconds))
}

/// Splits a trailing UTC offset (`Z` or the first `+`/`-`, which can only
/// ever start an offset in this grammar — the time-of-day portion is
/// digits/`:`/`.` only) off a time-of-day argument, parsing whichever
/// pieces are present. Returns `(time, None)` when no offset suffix is
/// present at all (a `localtime('...')`-shaped argument), or `(time,
/// Some(offset))` when one is (a `time('...')`-shaped argument).
pub(in crate::executor) fn parse_iso_time_and_offset(
    input: &str,
) -> Option<((u32, u32, u32, u32), Option<i32>)> {
    if let Some(stripped) = input.strip_suffix('Z').or_else(|| input.strip_suffix('z')) {
        return Some((parse_iso_time(stripped)?, Some(0)));
    }
    for (idx, ch) in input.char_indices() {
        if ch == '+' || ch == '-' {
            let time = parse_iso_time(input.get(..idx)?)?;
            let offset = parse_offset(input.get(idx..)?)?;
            return Some((time, Some(offset)));
        }
    }
    Some((parse_iso_time(input)?, None))
}

/// Parses a full `date'T'time[offset]` ISO-8601 instant (a
/// `datetime('...')`/`localdatetime('...')` string argument) into its
/// `(year, month, day)`, `(hour, minute, second, nanosecond)`, and
/// optional offset parts, reusing [`parse_iso_date`] and
/// [`parse_iso_time_and_offset`] for the two halves either side of the
/// `T`/`t` separator.
pub(in crate::executor) fn parse_iso_datetime(
    input: &str,
) -> Option<((i32, u32, u32), (u32, u32, u32, u32), Option<i32>)> {
    let (date_part, time_part) = input.split_once('T').or_else(|| input.split_once('t'))?;
    let date = parse_iso_date(date_part)?;
    let (time, offset) = parse_iso_time_and_offset(time_part)?;
    Some((date, time, offset))
}

// ────────────────────────────── Duration parsing ───────────────────────────

/// Average seconds per month (`365.2425 days/year * 86400 / 12`, the
/// Gregorian mean year), used only to carry a *fractional month* remainder
/// down into days/seconds — an exact ISO duration has no fixed
/// month-length, so this approximation is unavoidable, and it is the same
/// constant that reproduces the TCK's `'P0.75M'` -> `'P22DT19H51M49.5S'`
/// expectation exactly.
///
/// `pub(in crate::executor)`: `temporal`'s `scale_duration` (backing
/// `duration * number`/`duration / number`) reuses the exact same
/// fractional-month carry the string parser below performs, verified
/// against `Temporal8.feature` scenario 7's `* 0.5` row.
pub(in crate::executor) const AVG_SECONDS_PER_MONTH: f64 = 2_629_746.0;
pub(in crate::executor) const SECONDS_PER_DAY: f64 = 86_400.0;

/// Converts an already-integer-valued `f64` (the caller has called
/// `.trunc()`/`.round()` on it) into an `i64`, rejecting anything outside
/// `i64`'s range instead of silently saturating. `as i64` on an
/// out-of-range float *saturates* rather than erroring — that would let a
/// wildly out-of-range component (e.g. `duration('P99999999999999999999D')`)
/// through as a plausible-looking `i64::MAX`/`i64::MIN`, and worse, a
/// saturated value can still overflow if something downstream negates it
/// (`-i64::MIN` panics/wraps). Checking the `f64` magnitude before the cast
/// avoids both.
///
/// `pub(in crate::executor)`: shared with `temporal::scale_duration`'s
/// float-to-i64 narrowing for the same overflow-safety reason.
pub(in crate::executor) fn to_i64(v: f64) -> Option<i64> {
    (v.is_finite() && v >= -(2f64.powi(63)) && v < 2f64.powi(63)).then_some(v as i64)
}

/// Parses a `duration('...')` string argument into `(months, days,
/// seconds, nanos)` — the same shape [`temporal_value::make_duration`]
/// expects, so a caller only has to feed the result straight through.
/// `nanos` is an *un-normalized* fractional-seconds remainder (rounding can
/// land it exactly on `±1_000_000_000`); `make_duration` owns normalizing
/// it back into range alongside `seconds`.
///
/// Accepts both ISO-8601 duration notations:
///
/// - The standard component form, `P[nY][nM][nW][nD][T[nH][nM][nS]]` —
///   components must appear in that order (each at most once), and any
///   component's value may carry a fractional part (e.g. `P5M1.5D`),
///   which is carried down into the next-smaller unit (year -> month ->
///   [week/day] -> hour -> minute -> second -> nanosecond) all the way to
///   whole seconds + a nanosecond remainder, regardless of which unit
///   letters the fraction has to pass through unmentioned.
/// - The "alternative format", `P<year>-<month>-<day>T<hour>:<minute>:
///   <second>[.fraction]` (and its compact `PYYYYMMDDTHHMMSS[.fraction]`
///   counterpart) — every field is a plain count, not a calendar date/
///   time-of-day (no range validation against real calendar rules).
///
/// An optional leading `-` negates every component of the result. Any
/// component (or the combination of a per-component minus and the leading
/// `-`) that would overflow `i64` once resolved returns `None` rather than
/// wrapping or panicking.
pub(in crate::executor) fn parse_iso_duration(input: &str) -> Option<(i64, i64, i64, i32)> {
    let (negative, body) = match input.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, input),
    };
    let rest = body.strip_prefix('P')?;
    if rest.is_empty() {
        return None;
    }

    let (years, months, weeks, days, hours, minutes, seconds) =
        try_parse_alternative_duration(rest).or_else(|| parse_standard_duration(rest))?;

    // Apply the overall sign to every raw component *before* any further
    // arithmetic or the final float->i64 casts. Negating an already-cast
    // i64 at the end (the old approach) can itself overflow: a
    // per-component minus (`P-9223372036854775808S`) combined with a
    // leading `-P...` would cast to exactly `i64::MIN` and then negate it,
    // which panics in an overflow-checked build and silently wraps in
    // release. Negating the `f64` first never hits that — the resulting
    // out-of-range magnitude is instead caught by `to_i64` below.
    let sign = if negative { -1.0 } else { 1.0 };
    let years = years * sign;
    let months = months * sign;
    let weeks = weeks * sign;
    let days = days * sign;
    let hours = hours * sign;
    let minutes = minutes * sign;
    let seconds = seconds * sign;

    carry_duration_components(years, months, weeks, days, hours, minutes, seconds)
}

/// Folds raw (possibly fractional) `years`/`months`/`weeks`/`days`/
/// `hours`/`minutes`/`seconds` magnitudes down into the normalized
/// `(months, days, seconds, nanos)` shape [`temporal_value::make_duration`]
/// expects — the exact carry chain [`parse_iso_duration`] performs on a
/// string's fractional-component values (year -> month exact ×12, month
/// remainder -> days/seconds via [`AVG_SECONDS_PER_MONTH`], week/day exact
/// ×[`SECONDS_PER_DAY`], down to whole seconds + a nanosecond remainder),
/// extracted here so any other caller with the same seven raw components
/// (fractional or not) — namely the `duration({...})` map constructor's
/// fractional `years`/`months`/`weeks`/`days` fields in
/// `projection/fn_temporal.rs` — reuses this math byte-for-byte instead of
/// re-deriving an approximation of it.
///
/// `pub(in crate::executor)`: shared with `fn_temporal`'s map-constructor
/// fractional-component carry path.
pub(in crate::executor) fn carry_duration_components(
    years: f64,
    months: f64,
    weeks: f64,
    days: f64,
    hours: f64,
    minutes: f64,
    seconds: f64,
) -> Option<(i64, i64, i64, i32)> {
    // Fold years into a single (possibly fractional) total-months value —
    // years never need the lossy avg-month conversion since 1 year is
    // always exactly 12 months.
    let months_total = years * 12.0 + months;
    let months_final = to_i64(months_total.trunc())?;
    let months_frac = months_total - months_total.trunc();

    // Weeks/days are exact (1 week = 7 days, always); a fractional month
    // remainder converts through the average-month-in-seconds constant.
    // Combining both into one "days-equivalent seconds" value before
    // splitting back into (days, remainder seconds) avoids a lossy
    // intermediate rounding step that splitting them separately would
    // introduce.
    let days_equivalent_seconds =
        (weeks * 7.0 + days) * SECONDS_PER_DAY + months_frac * AVG_SECONDS_PER_MONTH;
    let days_final_f64 = (days_equivalent_seconds / SECONDS_PER_DAY).trunc();
    let days_final = to_i64(days_final_f64)?;
    let day_remainder_seconds = days_equivalent_seconds - days_final_f64 * SECONDS_PER_DAY;

    let total_seconds = day_remainder_seconds + hours * 3600.0 + minutes * 60.0 + seconds;
    let whole_seconds = to_i64(total_seconds.trunc())?;
    let nanos = ((total_seconds - total_seconds.trunc()) * 1_000_000_000.0).round() as i32;

    Some((months_final, days_final, whole_seconds, nanos))
}

/// Parses the standard `[nY][nM][nW][nD][T[nH][nM][nS]]` component form
/// (the text after the leading `P`), returning `(years, months, weeks,
/// days, hours, minutes, seconds)` as raw (possibly fractional) values.
/// Rejects out-of-order or repeated unit letters and requires at least one
/// component overall.
fn parse_standard_duration(rest: &str) -> Option<(f64, f64, f64, f64, f64, f64, f64)> {
    let (date_part, time_part) = match rest.find('T') {
        Some(idx) => (&rest[..idx], Some(&rest[idx + 1..])),
        None => (rest, None),
    };

    let mut years = 0.0_f64;
    let mut months = 0.0_f64;
    let mut weeks = 0.0_f64;
    let mut days = 0.0_f64;
    let mut any_component = false;

    let mut cursor = 0usize;
    let mut stage = 0u8; // Y=0, M=1, W=2, D=3 — strictly increasing.
    while cursor < date_part.len() {
        let (value, next, unit) = parse_number_and_unit(date_part, cursor)?;
        let unit_stage = match unit {
            'Y' => 0,
            'M' => 1,
            'W' => 2,
            'D' => 3,
            _ => return None,
        };
        if unit_stage < stage {
            return None;
        }
        stage = unit_stage + 1;
        match unit {
            'Y' => years = value,
            'M' => months = value,
            'W' => weeks = value,
            _ => days = value,
        }
        any_component = true;
        cursor = next;
    }

    let mut hours = 0.0_f64;
    let mut minutes = 0.0_f64;
    let mut seconds = 0.0_f64;

    if let Some(tp) = time_part {
        if tp.is_empty() {
            return None;
        }
        let mut cursor = 0usize;
        let mut stage = 0u8; // H=0, M=1, S=2 — strictly increasing.
        while cursor < tp.len() {
            let (value, next, unit) = parse_number_and_unit(tp, cursor)?;
            let unit_stage = match unit {
                'H' => 0,
                'M' => 1,
                'S' => 2,
                _ => return None,
            };
            if unit_stage < stage {
                return None;
            }
            stage = unit_stage + 1;
            match unit {
                'H' => hours = value,
                'M' => minutes = value,
                _ => seconds = value,
            }
            any_component = true;
            cursor = next;
        }
    }

    if !any_component {
        return None;
    }

    Some((years, months, weeks, days, hours, minutes, seconds))
}

/// Parses one `<number><unit-letter>` token starting at byte offset `start`
/// of `s` — the number is an optionally-signed run of digits with an
/// optional `.`-fractional part, the unit is the single uppercase ASCII
/// letter immediately following it. Returns the parsed value, the byte
/// offset just past the unit letter, and the unit letter itself.
fn parse_number_and_unit(s: &str, start: usize) -> Option<(f64, usize, char)> {
    let bytes = s.as_bytes();
    if start >= bytes.len() {
        return None;
    }
    let mut i = start;
    if bytes[i] == b'-' {
        i += 1;
    }
    let digits_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == digits_start {
        return None;
    }
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        let frac_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == frac_start {
            return None;
        }
    }
    if i >= bytes.len() || !bytes[i].is_ascii_uppercase() {
        return None;
    }
    let unit = bytes[i] as char;
    let number_str = s.get(start..i)?;
    let value: f64 = number_str.parse().ok()?;
    Some((value, i + 1, unit))
}

/// Tries the ISO-8601 "alternative format"
/// (`<year>-<month>-<day>T<hour>:<minute>:<second>` or its compact
/// counterpart) on the text after `P`. Returns `None` immediately — falling
/// back to [`parse_standard_duration`] — for anything that isn't a `T`-
/// separated pure numeric date/time pair, so it never misfires on a
/// standard-form string (which always carries at least one unit letter in
/// its date part).
fn try_parse_alternative_duration(rest: &str) -> Option<(f64, f64, f64, f64, f64, f64, f64)> {
    let idx = rest.find('T')?;
    let date_part = &rest[..idx];
    let time_part = &rest[idx + 1..];
    let (year, month, day) = parse_alt_date(date_part)?;
    let (hour, minute, second) = parse_alt_time(time_part)?;
    Some((
        year as f64,
        month as f64,
        0.0,
        day as f64,
        hour as f64,
        minute as f64,
        second,
    ))
}

/// Parses the alternative form's date segment: extended `YYYY-MM-DD` (10
/// bytes, hyphens at fixed positions) or basic `YYYYMMDD` (8 all-digit
/// bytes). Each field is a plain count, not calendar-validated.
fn parse_alt_date(s: &str) -> Option<(i64, i64, i64)> {
    if s.len() == 10 {
        let year = s.get(0..4)?;
        let sep1 = s.get(4..5)?;
        let month = s.get(5..7)?;
        let sep2 = s.get(7..8)?;
        let day = s.get(8..10)?;
        if sep1 != "-" || sep2 != "-" {
            return None;
        }
        return Some((
            parse_ascii_digits(year)?,
            parse_ascii_digits(month)?,
            parse_ascii_digits(day)?,
        ));
    }
    if s.len() == 8 {
        let year = s.get(0..4)?;
        let month = s.get(4..6)?;
        let day = s.get(6..8)?;
        return Some((
            parse_ascii_digits(year)?,
            parse_ascii_digits(month)?,
            parse_ascii_digits(day)?,
        ));
    }
    None
}

/// Parses the alternative form's time segment: extended `HH:MM:SS[.fff]`
/// (colons at fixed positions) or basic `HHMMSS[.fff]` (6+ all-digit
/// leading bytes). `seconds` keeps any fractional remainder.
fn parse_alt_time(s: &str) -> Option<(i64, i64, f64)> {
    if s.len() >= 8 {
        if let (Some(hour), Some(sep1), Some(minute), Some(sep2), Some(seconds)) = (
            s.get(0..2),
            s.get(2..3),
            s.get(3..5),
            s.get(5..6),
            s.get(6..),
        ) {
            if sep1 == ":" && sep2 == ":" {
                return Some((
                    parse_ascii_digits(hour)?,
                    parse_ascii_digits(minute)?,
                    parse_plain_decimal(seconds)?,
                ));
            }
        }
    }
    if s.len() >= 6 {
        let hour = s.get(0..2)?;
        let minute = s.get(2..4)?;
        let seconds = s.get(4..)?;
        return Some((
            parse_ascii_digits(hour)?,
            parse_ascii_digits(minute)?,
            parse_plain_decimal(seconds)?,
        ));
    }
    None
}

/// Strict "every character is an ASCII digit" integer parse — rejects the
/// `i64::from_str`/`f64::from_str` quirks (leading `+`/`-`, whitespace)
/// that would otherwise let a malformed field like `"-0"` or `" 12"` slip
/// through a fixed-width slice unnoticed.
fn parse_ascii_digits(s: &str) -> Option<i64> {
    if s.is_empty() || !s.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

/// Strict "digits with at most one `.`" decimal parse for the alternative
/// format's fractional-seconds field — rejects `f64::from_str`'s `"nan"` /
/// `"inf"` / scientific-notation acceptances, which a raw `.parse()` on an
/// unconstrained trailing slice would otherwise let through as a silently
/// bogus (but non-panicking) duration.
fn parse_plain_decimal(s: &str) -> Option<f64> {
    if s.is_empty() {
        return None;
    }
    let mut seen_dot = false;
    let mut seen_digit = false;
    for c in s.chars() {
        if c == '.' {
            if seen_dot {
                return None;
            }
            seen_dot = true;
        } else if c.is_ascii_digit() {
            seen_digit = true;
        } else {
            return None;
        }
    }
    if !seen_digit {
        return None;
    }
    s.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─────────────────────────── date parse ladder ─────────────────────

    #[test]
    fn date_calendar_forms() {
        assert_eq!(parse_iso_date("2015-07-21"), Some((2015, 7, 21)));
        assert_eq!(parse_iso_date("20150721"), Some((2015, 7, 21)));
        assert_eq!(parse_iso_date("2015-07"), Some((2015, 7, 1)));
        assert_eq!(parse_iso_date("201507"), Some((2015, 7, 1)));
        assert_eq!(parse_iso_date("2015"), Some((2015, 1, 1)));
    }

    #[test]
    fn date_week_forms() {
        assert_eq!(parse_iso_date("2015-W30-2"), Some((2015, 7, 21)));
        assert_eq!(parse_iso_date("2015W302"), Some((2015, 7, 21)));
        assert_eq!(parse_iso_date("2015-W30"), Some((2015, 7, 20)));
        assert_eq!(parse_iso_date("2015W30"), Some((2015, 7, 20)));
    }

    #[test]
    fn date_ordinal_forms() {
        assert_eq!(parse_iso_date("2015-202"), Some((2015, 7, 21)));
        assert_eq!(parse_iso_date("2015202"), Some((2015, 7, 21)));
    }

    #[test]
    fn date_invalid_forms_return_none_not_panic() {
        assert_eq!(parse_iso_date(""), None);
        assert_eq!(parse_iso_date("abcd"), None);
        assert_eq!(parse_iso_date("2015-13-01"), None); // month 13
        assert_eq!(parse_iso_date("2015-02-30"), None); // Feb 30
        assert_eq!(parse_iso_date("2015-W54"), None); // week 54 never exists
        assert_eq!(parse_iso_date("2015-999"), None); // ordinal out of range
        assert_eq!(parse_iso_date("日本語"), None); // non-ASCII, must not panic
        assert_eq!(parse_iso_date("2015-07-21-extra"), None);
    }

    #[test]
    fn date_mixed_notation_is_rejected() {
        // Extended year-hyphen prefix with no separating hyphen before the
        // weekday digit (valid extended form is `2015-W30-2`).
        assert_eq!(parse_iso_date("2015-W302"), None);
        // Compact (no year-hyphen) prefix with a stray hyphen before the
        // weekday digit (valid compact form is `2015W302`).
        assert_eq!(parse_iso_date("2015W30-2"), None);
        // Extended year-hyphen prefix with a compact (no inner hyphen)
        // month+day body (valid extended form is `2015-07-21`).
        assert_eq!(parse_iso_date("2015-0721"), None);
    }

    #[test]
    fn date_leap_year_ordinal_and_week_boundaries() {
        // 2015 is not a leap year; day 366 doesn't exist.
        assert_eq!(parse_iso_date("2015-366"), None);
        // 2016 is a leap year; day 366 is Dec 31.
        assert_eq!(parse_iso_date("2016-366"), Some((2016, 12, 31)));
    }

    // ───────────────── time-of-day / offset / datetime parse ────────────

    #[test]
    fn time_extended_and_compact_forms() {
        assert_eq!(parse_iso_time("16:30"), Some((16, 30, 0, 0)));
        assert_eq!(parse_iso_time("1630"), Some((16, 30, 0, 0)));
        assert_eq!(parse_iso_time("14:30"), Some((14, 30, 0, 0)));
        assert_eq!(parse_iso_time("12:34:54"), Some((12, 34, 54, 0)));
        assert_eq!(parse_iso_time("123454"), Some((12, 34, 54, 0)));
    }

    #[test]
    fn time_fractional_seconds() {
        // openCypher TCK `Temporal10.feature` scenario [2]: fractional
        // seconds on a bare time-of-day.
        assert_eq!(
            parse_iso_time("12:34:54.7"),
            Some((12, 34, 54, 700_000_000))
        );
        assert_eq!(
            parse_iso_time("21:40:32.142"),
            Some((21, 40, 32, 142_000_000))
        );
    }

    #[test]
    fn time_invalid_forms_return_none_not_panic() {
        assert_eq!(parse_iso_time(""), None);
        assert_eq!(parse_iso_time("24:00"), None); // hour out of range
        assert_eq!(parse_iso_time("12:60"), None); // minute out of range
        assert_eq!(parse_iso_time("日本語"), None); // non-ASCII, must not panic
    }

    #[test]
    fn time_fraction_requires_a_seconds_group() {
        // A fraction with no seconds group present must be rejected, not
        // silently reinterpreted as a fractional second with an implicit
        // whole-second value of 0 (`'16:30.5'` is NOT `16:30:00.5`).
        assert_eq!(parse_iso_time("16:30.5"), None);
        assert_eq!(parse_iso_time("16.5"), None);
        assert_eq!(parse_iso_time("1630.5"), None);
        // A fraction WITH a seconds group present is still accepted.
        assert_eq!(
            parse_iso_time("16:30:30.5"),
            Some((16, 30, 30, 500_000_000))
        );
        assert_eq!(parse_iso_time("163030.5"), Some((16, 30, 30, 500_000_000)));
    }

    #[test]
    fn time_rejects_doubled_or_misplaced_colons() {
        // A doubled colon collapses to an empty field, not a valid one.
        assert_eq!(parse_iso_time("12::34:54"), None);
        // Extended-vs-compact must not be mixed within one input: compact
        // hour+minute with an extended (colon-separated) seconds field.
        assert_eq!(parse_iso_time("1230:54"), None);
        // A trailing stray colon.
        assert_eq!(parse_iso_time("16:30:"), None);
        // A colon-separated field that isn't exactly 2 digits.
        assert_eq!(parse_iso_time("1:30"), None);
        assert_eq!(parse_iso_time("16:3"), None);
    }

    #[test]
    fn offset_extended_and_compact_forms() {
        assert_eq!(parse_offset("Z"), Some(0));
        assert_eq!(parse_offset("+01:00"), Some(3600));
        // The TCK's `Temporal10.feature` fixtures use the compact
        // (colonless) offset form throughout, e.g. `datetime('...+0100')`.
        assert_eq!(parse_offset("+0100"), Some(3600));
        assert_eq!(parse_offset("+0200"), Some(7200));
        assert_eq!(parse_offset("-0100"), Some(-3600));
        assert_eq!(parse_offset("+01"), Some(3600));
    }

    #[test]
    fn offset_invalid_forms_return_none_not_panic() {
        assert_eq!(parse_offset(""), None);
        assert_eq!(parse_offset("+25:00"), None); // hour out of range
        assert_eq!(parse_offset("nope"), None);
    }

    #[test]
    fn time_and_offset_splits_correctly() {
        assert_eq!(
            parse_iso_time_and_offset("16:30+0100"),
            Some(((16, 30, 0, 0), Some(3600)))
        );
        assert_eq!(
            parse_iso_time_and_offset("14:30"),
            Some(((14, 30, 0, 0), None))
        );
        assert_eq!(
            parse_iso_time_and_offset("12:00Z"),
            Some(((12, 0, 0, 0), Some(0)))
        );
    }

    #[test]
    fn datetime_fractional_seconds_and_compact_offset() {
        // openCypher TCK `Temporal10.feature` scenario [2] fixture shapes.
        assert_eq!(
            parse_iso_datetime("2015-07-21T21:40:32.142+0100"),
            Some(((2015, 7, 21), (21, 40, 32, 142_000_000), Some(3600)))
        );
        assert_eq!(
            parse_iso_datetime("2015-07-21T21:40:32.142"),
            Some(((2015, 7, 21), (21, 40, 32, 142_000_000), None))
        );
        assert_eq!(
            parse_iso_datetime("2016-07-21T21:45:22.142"),
            Some(((2016, 7, 21), (21, 45, 22, 142_000_000), None))
        );
    }

    // ───────────────────────── duration parse + carry ───────────────────

    fn render(months: i64, days: i64, seconds: i64, nanos: i32) -> String {
        temporal_value::render_duration(months, days, seconds, nanos)
    }

    #[test]
    fn duration_no_carry_round_trips() {
        let (m, d, s, n) = parse_iso_duration("P14DT16H12M").unwrap();
        assert_eq!(render(m, d, s, n), "P14DT16H12M");
    }

    #[test]
    fn duration_fractional_day_carries_into_hours() {
        let (m, d, s, n) = parse_iso_duration("P5M1.5D").unwrap();
        assert_eq!(render(m, d, s, n), "P5M1DT12H");
    }

    #[test]
    fn duration_fractional_month_carries_through_days_and_time() {
        let (m, d, s, n) = parse_iso_duration("P0.75M").unwrap();
        assert_eq!(render(m, d, s, n), "P22DT19H51M49.5S");
    }

    #[test]
    fn duration_fractional_minute_carries_into_seconds() {
        let (m, d, s, n) = parse_iso_duration("PT0.75M").unwrap();
        assert_eq!(render(m, d, s, n), "PT45S");
    }

    #[test]
    fn duration_fractional_week_carries_into_days_and_hours() {
        let (m, d, s, n) = parse_iso_duration("P2.5W").unwrap();
        assert_eq!(render(m, d, s, n), "P17DT12H");
    }

    #[test]
    fn duration_overflowing_seconds_normalize_into_minutes() {
        let (m, d, s, n) = parse_iso_duration("P12Y5M14DT16H12M70S").unwrap();
        assert_eq!(render(m, d, s, n), "P12Y5M14DT16H13M10S");
    }

    #[test]
    fn duration_alternative_format_round_trips() {
        let (m, d, s, n) = parse_iso_duration("P2012-02-02T14:37:21.545").unwrap();
        assert_eq!(render(m, d, s, n), "P2012Y2M2DT14H37M21.545S");
    }

    #[test]
    fn duration_alternative_format_compact_matches_extended() {
        let extended = parse_iso_duration("P2012-02-02T14:37:21.545").unwrap();
        let compact = parse_iso_duration("P20120202T143721.545").unwrap();
        assert_eq!(extended, compact);
    }

    #[test]
    fn duration_leading_minus_negates_every_component() {
        let (m, d, s, n) = parse_iso_duration("-P14DT16H12M").unwrap();
        assert_eq!(render(m, d, s, n), "P-14DT-16H-12M");
    }

    #[test]
    fn duration_invalid_forms_return_none_not_panic() {
        assert_eq!(parse_iso_duration(""), None);
        assert_eq!(parse_iso_duration("14DT16H12M"), None); // missing P
        assert_eq!(parse_iso_duration("P"), None); // no components at all
        assert_eq!(parse_iso_duration("PT"), None); // T with nothing after
        assert_eq!(parse_iso_duration("P1D1Y"), None); // out-of-order units
        assert_eq!(parse_iso_duration("P1Y2Y"), None); // duplicate unit
        assert_eq!(parse_iso_duration("P1X"), None); // unknown unit letter
        assert_eq!(parse_iso_duration("PnanS"), None); // no digits at all
        assert_eq!(parse_iso_duration("日本語"), None); // non-ASCII, must not panic
    }

    #[test]
    fn duration_alt_format_seconds_field_rejects_nan_injection() {
        // A malformed "seconds" tail that happens to spell a float
        // special-case must not silently become a NaN/garbage duration.
        assert_eq!(parse_iso_duration("P2012-02-02T14:37:nan"), None);
    }

    #[test]
    fn duration_combined_negation_overflow_returns_none_not_panic() {
        // The exact BLOCKER repro: a per-component minus on `i64::MIN`
        // (`-9223372036854775808`) combined with an overall leading `-`
        // used to cast to `i64::MIN` and then negate it — `-i64::MIN`
        // overflows `i64`, panicking in an overflow-checked build. The
        // literal string as given (no `T`) is already rejected for an
        // unrelated reason (`S` is only a valid unit inside the time
        // section), so it never even reaches the arithmetic below —
        // asserted here too so the exact reported repro is covered.
        assert_eq!(parse_iso_duration("-P-9223372036854775808S"), None);
        // The form that actually reaches the sign-then-cast arithmetic:
        // seconds = -i64::MIN negated twice (once by the component's own
        // `-`, once by the leading `-P`) lands exactly on `2^63`, one past
        // `i64::MAX` — must return `None`, not overflow.
        assert_eq!(parse_iso_duration("-PT-9223372036854775808S"), None);
    }

    #[test]
    fn duration_wildly_out_of_range_magnitude_returns_none_not_saturated_garbage() {
        // Previously: the saturating `f64 as i64` cast silently produced
        // `i64::MAX`-ish nonsense (the magnitude effectively double-counted
        // across `days_final` and the leftover remainder, both saturating
        // independently) instead of signaling "this doesn't fit".
        assert_eq!(parse_iso_duration("P99999999999999999999D"), None);
    }
}
