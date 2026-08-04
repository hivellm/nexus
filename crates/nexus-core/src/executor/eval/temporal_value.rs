//! Typed temporal value representation for the executor's intermediate
//! evaluation layer.
//!
//! Cypher temporal values (`date`, `localtime`, `localdatetime`, `time`,
//! `datetime`, `duration`) evaluate to a *tagged* `serde_json::Value::Object`
//! while they flow through row evaluation: a `_nexus_temporal_type`
//! discriminant plus normalized components, so downstream extractor
//! functions and arithmetic can recover the original kind instead of
//! guessing from a rendered string. The tag is never meant to survive past
//! the projection boundary — [`canonicalize_value_in_place`] (invoked once,
//! at the executor's public `execute` entry point) collapses every tagged
//! object, including ones nested inside lists/maps, back into its canonical
//! ISO-8601 string before a row leaves the executor.
//!
//! Duration is stored the same way Neo4j's own `DurationValue` stores it —
//! `(months, days, seconds, nanos)` — rather than as the six user-facing
//! components (`years`, `months`, `weeks`, `days`, `hours`, `minutes`,
//! `seconds`) a `duration({...})` map literal supplies. `years` folds into
//! `months` (`years * 12 + months`); `hours`/`minutes`/`seconds` fold into
//! `seconds` (with any fractional remainder carried into `nanos`). This
//! matches Neo4j's canonical-rendering behaviour, verified against the
//! openCypher TCK's `Temporal2.feature`/`Temporal8.feature` expectation
//! tables (e.g. `{years: 12, months: 5, ...}` renders `P12Y5M...`, not
//! `P149M...`).

use serde_json::{Map, Value};

/// JSON object key that marks a tagged intermediate temporal value. Mirrors
/// the project's existing `_nexus_*` marker convention (see
/// `is_relationship_value` / `_nexus_rel_type`).
pub(in crate::executor) const TEMPORAL_TAG_KEY: &str = "_nexus_temporal_type";

/// The six Cypher temporal kinds this module represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::executor) enum TemporalKind {
    Date,
    LocalTime,
    LocalDateTime,
    Time,
    DateTime,
    Duration,
}

impl TemporalKind {
    fn tag(self) -> &'static str {
        match self {
            TemporalKind::Date => "date",
            TemporalKind::LocalTime => "localtime",
            TemporalKind::LocalDateTime => "localdatetime",
            TemporalKind::Time => "time",
            TemporalKind::DateTime => "datetime",
            TemporalKind::Duration => "duration",
        }
    }

    fn from_tag(tag: &str) -> Option<Self> {
        match tag {
            "date" => Some(Self::Date),
            "localtime" => Some(Self::LocalTime),
            "localdatetime" => Some(Self::LocalDateTime),
            "time" => Some(Self::Time),
            "datetime" => Some(Self::DateTime),
            "duration" => Some(Self::Duration),
            _ => None,
        }
    }
}

/// Returns the [`TemporalKind`] of a tagged intermediate value, or `None`
/// if `value` is not a tagged temporal object — including a plain
/// `Value::String`/legacy `Value::Object` shape. Callers that must keep
/// accepting those legacy shapes fall back to their own parsing.
pub(in crate::executor) fn temporal_kind(value: &Value) -> Option<TemporalKind> {
    let obj = value.as_object()?;
    let tag = obj.get(TEMPORAL_TAG_KEY)?.as_str()?;
    TemporalKind::from_tag(tag)
}

/// True when `value` is specifically a tagged `duration`.
pub(in crate::executor) fn is_duration(value: &Value) -> bool {
    temporal_kind(value) == Some(TemporalKind::Duration)
}

/// True when `value` is a tagged temporal *instant* (anything but
/// `duration`) — i.e. usable on either side of `+`/`-` against a duration.
pub(in crate::executor) fn is_temporal_instant(value: &Value) -> bool {
    matches!(
        temporal_kind(value),
        Some(k) if k != TemporalKind::Duration
    )
}

fn get_i64(map: &Map<String, Value>, key: &str) -> i64 {
    map.get(key).and_then(Value::as_i64).unwrap_or(0)
}

fn get_u32(map: &Map<String, Value>, key: &str) -> u32 {
    map.get(key).and_then(Value::as_u64).unwrap_or(0) as u32
}

fn get_i32(map: &Map<String, Value>, key: &str) -> i32 {
    map.get(key).and_then(Value::as_i64).unwrap_or(0) as i32
}

// ─────────────────────────── Constructors ────────────────────────────────

/// Builds a tagged `date` value from calendar components.
pub(in crate::executor) fn make_date(year: i32, month: u32, day: u32) -> Value {
    let mut m = Map::new();
    m.insert(
        TEMPORAL_TAG_KEY.to_string(),
        Value::String(TemporalKind::Date.tag().to_string()),
    );
    m.insert("year".to_string(), Value::Number(year.into()));
    m.insert("month".to_string(), Value::Number(month.into()));
    m.insert("day".to_string(), Value::Number(day.into()));
    Value::Object(m)
}

/// Builds a tagged `localtime` value (no offset).
pub(in crate::executor) fn make_localtime(
    hour: u32,
    minute: u32,
    second: u32,
    nanosecond: u32,
) -> Value {
    let mut m = Map::new();
    m.insert(
        TEMPORAL_TAG_KEY.to_string(),
        Value::String(TemporalKind::LocalTime.tag().to_string()),
    );
    m.insert("hour".to_string(), Value::Number(hour.into()));
    m.insert("minute".to_string(), Value::Number(minute.into()));
    m.insert("second".to_string(), Value::Number(second.into()));
    m.insert("nanosecond".to_string(), Value::Number(nanosecond.into()));
    Value::Object(m)
}

/// Builds a tagged `localdatetime` value (no offset).
#[allow(clippy::too_many_arguments)]
pub(in crate::executor) fn make_localdatetime(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    nanosecond: u32,
) -> Value {
    let mut m = Map::new();
    m.insert(
        TEMPORAL_TAG_KEY.to_string(),
        Value::String(TemporalKind::LocalDateTime.tag().to_string()),
    );
    m.insert("year".to_string(), Value::Number(year.into()));
    m.insert("month".to_string(), Value::Number(month.into()));
    m.insert("day".to_string(), Value::Number(day.into()));
    m.insert("hour".to_string(), Value::Number(hour.into()));
    m.insert("minute".to_string(), Value::Number(minute.into()));
    m.insert("second".to_string(), Value::Number(second.into()));
    m.insert("nanosecond".to_string(), Value::Number(nanosecond.into()));
    Value::Object(m)
}

/// Builds a tagged `time` value with a UTC offset (in seconds).
pub(in crate::executor) fn make_time(
    hour: u32,
    minute: u32,
    second: u32,
    nanosecond: u32,
    offset_seconds: i32,
) -> Value {
    let mut m = Map::new();
    m.insert(
        TEMPORAL_TAG_KEY.to_string(),
        Value::String(TemporalKind::Time.tag().to_string()),
    );
    m.insert("hour".to_string(), Value::Number(hour.into()));
    m.insert("minute".to_string(), Value::Number(minute.into()));
    m.insert("second".to_string(), Value::Number(second.into()));
    m.insert("nanosecond".to_string(), Value::Number(nanosecond.into()));
    m.insert(
        "offset_seconds".to_string(),
        Value::Number(offset_seconds.into()),
    );
    Value::Object(m)
}

/// Builds a tagged `datetime` value with a UTC offset (in seconds) and an
/// optional IANA zone name, rendered as a `[Zone/Name]` suffix
/// (`canonicalize_temporal`'s `DateTime` arm) whenever `tz_name` is
/// `Some` — set by a `datetime({..., timezone: 'Europe/Stockholm'})` map
/// constructor or a `datetime('...[Zone]')` string literal that names a
/// real IANA zone (resolved via `temporal_retag::resolve_timezone_string`,
/// chrono-tz's bundled tzdata); `None` for a fixed numeric offset or
/// `'UTC'`/`'Z'`.
#[allow(clippy::too_many_arguments)]
pub(in crate::executor) fn make_datetime(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    nanosecond: u32,
    offset_seconds: i32,
    tz_name: Option<String>,
) -> Value {
    let mut m = Map::new();
    m.insert(
        TEMPORAL_TAG_KEY.to_string(),
        Value::String(TemporalKind::DateTime.tag().to_string()),
    );
    m.insert("year".to_string(), Value::Number(year.into()));
    m.insert("month".to_string(), Value::Number(month.into()));
    m.insert("day".to_string(), Value::Number(day.into()));
    m.insert("hour".to_string(), Value::Number(hour.into()));
    m.insert("minute".to_string(), Value::Number(minute.into()));
    m.insert("second".to_string(), Value::Number(second.into()));
    m.insert("nanosecond".to_string(), Value::Number(nanosecond.into()));
    m.insert(
        "offset_seconds".to_string(),
        Value::Number(offset_seconds.into()),
    );
    if let Some(tz) = tz_name {
        m.insert("tz".to_string(), Value::String(tz));
    }
    Value::Object(m)
}

/// Renormalizes an unnormalized `(seconds, nanos)` split so `nanos` sits in
/// `(-1_000_000_000, 1_000_000_000)` and its sign matches `seconds` (unless
/// `seconds` is zero, in which case `nanos` keeps its own sign) — the
/// standard "carrying divmod" shape, so a caller can pass a raw
/// `whole_seconds + leftover_nanos` split (e.g. from splitting a fractional
/// `f64` of seconds) without pre-normalizing it.
fn normalize_seconds_nanos(mut seconds: i64, mut nanos: i32) -> (i64, i32) {
    const NANOS_PER_SEC: i32 = 1_000_000_000;
    // `nanos <= -NANOS_PER_SEC || nanos >= NANOS_PER_SEC` rather than
    // `nanos.abs() >= NANOS_PER_SEC`: `i32::MIN.abs()` overflows (panics in
    // a debug build, silently wraps back to `i32::MIN` in release), and
    // `nanos == i32::MIN` is a real caller-reachable input (any duration
    // subtraction can produce it).
    if nanos <= -NANOS_PER_SEC || nanos >= NANOS_PER_SEC {
        let carry = nanos / NANOS_PER_SEC;
        seconds = seconds.saturating_add(i64::from(carry));
        nanos -= carry * NANOS_PER_SEC;
    }
    if seconds > 0 && nanos < 0 {
        seconds -= 1;
        nanos += NANOS_PER_SEC;
    } else if seconds < 0 && nanos > 0 {
        seconds += 1;
        nanos -= NANOS_PER_SEC;
    }
    (seconds, nanos)
}

/// Builds a tagged `duration` value from Neo4j's own internal
/// representation: total months (`years * 12 + months`), total days, total
/// whole seconds, and a nanosecond remainder. `seconds`/`nanos` are
/// renormalized via [`normalize_seconds_nanos`].
pub(in crate::executor) fn make_duration(
    months: i64,
    days: i64,
    seconds: i64,
    nanos: i32,
) -> Value {
    let (seconds, nanos) = normalize_seconds_nanos(seconds, nanos);
    let mut m = Map::new();
    m.insert(
        TEMPORAL_TAG_KEY.to_string(),
        Value::String(TemporalKind::Duration.tag().to_string()),
    );
    m.insert("months".to_string(), Value::Number(months.into()));
    m.insert("days".to_string(), Value::Number(days.into()));
    m.insert("seconds".to_string(), Value::Number(seconds.into()));
    m.insert("nanos".to_string(), Value::Number(nanos.into()));
    Value::Object(m)
}

// ──────────────────────────── Accessors ───────────────────────────────────

/// Extracts `(months, days, seconds, nanos)` from a tagged `duration`
/// value, or `None` if `value` is not one.
pub(in crate::executor) fn duration_components(value: &Value) -> Option<(i64, i64, i64, i32)> {
    if temporal_kind(value) != Some(TemporalKind::Duration) {
        return None;
    }
    let obj = value.as_object()?;
    Some((
        get_i64(obj, "months"),
        get_i64(obj, "days"),
        get_i64(obj, "seconds"),
        get_i32(obj, "nanos"),
    ))
}

/// Extracts `(year, month, day)` from a tagged `date`/`localdatetime`/
/// `datetime` value, or `None` if `value` doesn't carry a date part.
pub(in crate::executor) fn date_components(value: &Value) -> Option<(i32, u32, u32)> {
    let kind = temporal_kind(value)?;
    if !matches!(
        kind,
        TemporalKind::Date | TemporalKind::LocalDateTime | TemporalKind::DateTime
    ) {
        return None;
    }
    let obj = value.as_object()?;
    Some((
        get_i32(obj, "year"),
        get_u32(obj, "month"),
        get_u32(obj, "day"),
    ))
}

/// Extracts `(hour, minute, second, nanosecond)` from a tagged
/// `localtime`/`time`/`localdatetime`/`datetime` value, or `None` if
/// `value` doesn't carry a time part.
pub(in crate::executor) fn time_components(value: &Value) -> Option<(u32, u32, u32, u32)> {
    let kind = temporal_kind(value)?;
    if kind == TemporalKind::Date {
        return None;
    }
    let obj = value.as_object()?;
    Some((
        get_u32(obj, "hour"),
        get_u32(obj, "minute"),
        get_u32(obj, "second"),
        get_u32(obj, "nanosecond"),
    ))
}

/// Extracts the UTC offset (in seconds) from a tagged `time`/`datetime`
/// value, or `None` for a value that doesn't carry one — every other
/// kind (`date`, `localtime`, `localdatetime`, `duration`) or a value
/// that isn't a tagged temporal at all.
pub(in crate::executor) fn offset_seconds(value: &Value) -> Option<i32> {
    let kind = temporal_kind(value)?;
    if !matches!(kind, TemporalKind::Time | TemporalKind::DateTime) {
        return None;
    }
    let raw = value.as_object()?.get("offset_seconds")?.as_i64()?;
    Some(raw as i32)
}

/// Extracts the IANA zone name from a tagged `datetime` value
/// constructed with one (see [`make_datetime`]'s `tz_name` argument), or
/// `None` for a fixed-offset `datetime` (no `tz` field) or any other
/// kind.
pub(in crate::executor) fn zone_name(value: &Value) -> Option<&str> {
    if temporal_kind(value) != Some(TemporalKind::DateTime) {
        return None;
    }
    value.as_object()?.get("tz")?.as_str()
}

/// The `years` / `monthsOfYear` / `weeks` / `days` / `hours` /
/// `minutesOfHour` / `secondsOfMinute` / `nanos` decomposition shared by
/// canonical duration rendering ([`render_duration`]) and the
/// `years`/`months`/`weeks`/`days`/`hours`/`minutes`/`seconds` top-level
/// extractor functions in `fn_temporal.rs` — both must agree on what "the
/// hours component" of a duration means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::executor) struct DurationParts {
    pub years: i64,
    pub months_of_year: i64,
    /// Total whole weeks in `days` (`days / 7`, Neo4j's `duration.weeks`
    /// semantics) — `days` itself is left as the full count, not a
    /// `daysOfWeek` remainder, since canonical rendering and the `days()`
    /// accessor both need the untouched total.
    pub weeks: i64,
    pub days: i64,
    pub hours: i64,
    pub minutes_of_hour: i64,
    pub seconds_of_minute: i64,
    pub nanos: i32,
}

/// Decomposes normalized `(months, days, seconds, nanos)` into the
/// human-facing units used both for canonical rendering and for the
/// component-extractor functions. Division is Rust's default (truncating,
/// remainder keeps the dividend's sign) — the same convention the TCK's
/// negative-duration rendering expectations (`Temporal6.feature`,
/// `Temporal10.feature`) assume.
pub(in crate::executor) fn duration_parts(
    months: i64,
    days: i64,
    seconds: i64,
    nanos: i32,
) -> DurationParts {
    // Re-normalize defensively: this is the shared decomposition boundary
    // for both canonical rendering (`render_duration`) and the
    // `years`/`months`/`days`/`hours`/`minutes`/`seconds` accessor
    // functions in `fn_temporal.rs`. `make_duration` already normalizes on
    // construction, but `duration_parts` is `pub(in crate::executor)` and
    // callers must not have to re-derive the `(-1e9, 1e9)` nanos invariant
    // themselves — an un-normalized `nanos` (e.g. `1_999_999_999`) would
    // otherwise render a malformed 10-digit ISO fraction.
    let (seconds, nanos) = normalize_seconds_nanos(seconds, nanos);
    let years = months / 12;
    let months_of_year = months % 12;
    let weeks = days / 7;
    let hours = seconds / 3600;
    let rem = seconds % 3600;
    let minutes_of_hour = rem / 60;
    let seconds_of_minute = rem % 60;
    DurationParts {
        years,
        months_of_year,
        weeks,
        days,
        hours,
        minutes_of_hour,
        seconds_of_minute,
        nanos,
    }
}

// ────────────────────────── Canonical rendering ────────────────────────────

/// Formats a nanosecond remainder (`0..1_000_000_000`) as a fractional
/// digit string with trailing zeros trimmed (`645_000_000` -> `"645"`,
/// `3` -> `"000000003"`), or `None` when it is exactly zero (no fraction to
/// render at all — matches the TCK's `12:31:14` vs `12:31:14.645` forms).
fn format_nanos_fraction(nanos: u32) -> Option<String> {
    if nanos == 0 {
        return None;
    }
    let padded = format!("{nanos:09}");
    let trimmed = padded.trim_end_matches('0');
    Some(trimmed.to_string())
}

fn render_date_ymd(year: i32, month: u32, day: u32) -> String {
    if year >= 0 {
        format!("{year:04}-{month:02}-{day:02}")
    } else {
        format!("-{:04}-{month:02}-{day:02}", -year)
    }
}

/// Renders `HH:MM[:SS[.fraction]]`, omitting the seconds field entirely
/// when both `second` and `nanosecond` are zero — matches the openCypher
/// TCK's `Temporal4.feature` store-round-trip table (`localtime({hour:
/// 12})` -> `'12:00'`, not `'12:00:00'`) and `Temporal9.feature`'s
/// `truncate` expectation table (`'1900-01-01T00:00'`, never
/// `'...T00:00:00'`), which independently pins the same rule dozens of
/// times. A non-zero `nanosecond` always forces the seconds field to show
/// (even when `second` itself is `0`), since the fraction has nowhere
/// else to attach — `Temporal9.feature`'s `{nanosecond: 2}` override rows
/// render `'...T00:00:00.000000002'`, not `'...T00:00.000000002'`.
fn render_time_of_day(hour: u32, minute: u32, second: u32, nanosecond: u32) -> String {
    let mut s = format!("{hour:02}:{minute:02}");
    if second != 0 || nanosecond != 0 {
        s.push_str(&format!(":{second:02}"));
        if let Some(frac) = format_nanos_fraction(nanosecond) {
            s.push('.');
            s.push_str(&frac);
        }
    }
    s
}

/// Renders a UTC offset in seconds as `Z` (exactly zero — matches
/// `java.time.ZoneOffset.UTC`'s own `toString()`, which is what Neo4j's
/// temporal rendering is built on, and the openCypher TCK's
/// `Temporal4.feature` store-round-trip table: `time({hour: 12})` ->
/// `'12:00Z'`, never `'12:00+00:00'`), `+HH:MM` (or `+HH:MM:SS` when the
/// offset carries a sub-minute remainder) otherwise, matching the TCK's
/// `12:31:14+01:00` form.
///
/// `pub(in crate::executor)`, not private: `temporal_accessors` reuses
/// this for the `offset`/`timezone` property accessors on `time`/
/// `datetime` values, so the offset renders identically whether it
/// reaches the caller through canonical rendering or a `d.offset`
/// property read.
pub(in crate::executor) fn render_offset(offset_seconds: i32) -> String {
    if offset_seconds == 0 {
        return "Z".to_string();
    }
    let sign = if offset_seconds < 0 { '-' } else { '+' };
    let abs = offset_seconds.unsigned_abs();
    let hours = abs / 3600;
    let minutes = (abs % 3600) / 60;
    let seconds = abs % 60;
    if seconds != 0 {
        format!("{sign}{hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!("{sign}{hours:02}:{minutes:02}")
    }
}

/// Renders the seconds-of-minute + nanos pair (e.g. `-1`, `-1_000_000` ->
/// `"-1.001"`) shared by [`render_duration`]'s `T` section.
fn render_duration_seconds(seconds_of_minute: i64, nanos: i32) -> String {
    let negative = seconds_of_minute < 0 || nanos < 0;
    let abs_seconds = seconds_of_minute.unsigned_abs();
    let abs_nanos = nanos.unsigned_abs();
    let mut s = String::new();
    if negative {
        s.push('-');
    }
    s.push_str(&abs_seconds.to_string());
    if let Some(frac) = format_nanos_fraction(abs_nanos) {
        s.push('.');
        s.push_str(&frac);
    }
    s
}

/// Renders a normalized `(months, days, seconds, nanos)` duration as its
/// canonical ISO-8601 string. Verified against the openCypher TCK's
/// `Temporal2.feature`/`Temporal6.feature`/`Temporal8.feature` expectation
/// tables:
///
/// - `(0, 14, 58320, 0)` -> `"P14DT16H12M"` (the F-015 gate case)
/// - `(149, 14, 58390, 0)` -> `"P12Y5M14DT16H13M10S"` (total months split
///   into years + monthsOfYear, never left as a bare `149M`)
/// - `(0, 0, -60, -1_000_000)` -> `"PT-1M-0.001S"` (each non-zero unit
///   carries its own sign, not one leading sign before `P`)
/// - all-zero (including a raw, un-normalized zero like `(0, 0, -1,
///   1_000_000_000)`, which `duration_parts` normalizes to all-zero) ->
///   `"PT0S"`
pub(in crate::executor) fn render_duration(
    months: i64,
    days: i64,
    seconds: i64,
    nanos: i32,
) -> String {
    // Testing the raw `(seconds, nanos)` inputs for zero here (before
    // `duration_parts` normalizes them) is wrong: a raw, un-normalized
    // zero like `(seconds: -1, nanos: 1_000_000_000)` normalizes to
    // `(seconds: 0, nanos: 0)` (an actual zero duration) but would fail a
    // pre-normalization `== 0` check, falling through to the date/time
    // rendering below and emitting a bare, invalid `"P"` (no `T` section
    // ever gets built when every decomposed part is zero). Guard the
    // *output* instead: if nothing ended up in either section, the
    // duration is zero regardless of which raw shape it arrived in.
    let parts = duration_parts(months, days, seconds, nanos);

    let mut date_part = String::new();
    if parts.years != 0 {
        date_part.push_str(&format!("{}Y", parts.years));
    }
    if parts.months_of_year != 0 {
        date_part.push_str(&format!("{}M", parts.months_of_year));
    }
    if parts.days != 0 {
        date_part.push_str(&format!("{}D", parts.days));
    }

    let mut time_part = String::new();
    if parts.hours != 0 {
        time_part.push_str(&format!("{}H", parts.hours));
    }
    if parts.minutes_of_hour != 0 {
        time_part.push_str(&format!("{}M", parts.minutes_of_hour));
    }
    if parts.seconds_of_minute != 0 || parts.nanos != 0 {
        time_part.push_str(&render_duration_seconds(
            parts.seconds_of_minute,
            parts.nanos,
        ));
        time_part.push('S');
    }

    if date_part.is_empty() && time_part.is_empty() {
        return "PT0S".to_string();
    }

    let mut out = String::from("P");
    out.push_str(&date_part);
    if !time_part.is_empty() {
        out.push('T');
        out.push_str(&time_part);
    }
    out
}

/// Renders a tagged temporal value to its canonical ISO-8601 string, or
/// `None` if `value` is not a tagged temporal.
///
/// `pub(crate)`, not `pub(in crate::executor)`: `crate::engine`'s write
/// path (`engine::write_exec::return_builder`) reads node/relationship
/// properties straight out of storage for its own inline `RETURN` — a
/// separate canonicalization point from the executor's, see
/// [`canonicalize_value_in_place`]'s doc comment.
pub(crate) fn canonicalize_temporal(value: &Value) -> Option<String> {
    let kind = temporal_kind(value)?;
    let obj = value.as_object()?;
    Some(match kind {
        TemporalKind::Date => render_date_ymd(
            get_i32(obj, "year"),
            get_u32(obj, "month"),
            get_u32(obj, "day"),
        ),
        TemporalKind::LocalTime => render_time_of_day(
            get_u32(obj, "hour"),
            get_u32(obj, "minute"),
            get_u32(obj, "second"),
            get_u32(obj, "nanosecond"),
        ),
        TemporalKind::LocalDateTime => format!(
            "{}T{}",
            render_date_ymd(
                get_i32(obj, "year"),
                get_u32(obj, "month"),
                get_u32(obj, "day")
            ),
            render_time_of_day(
                get_u32(obj, "hour"),
                get_u32(obj, "minute"),
                get_u32(obj, "second"),
                get_u32(obj, "nanosecond"),
            )
        ),
        TemporalKind::Time => format!(
            "{}{}",
            render_time_of_day(
                get_u32(obj, "hour"),
                get_u32(obj, "minute"),
                get_u32(obj, "second"),
                get_u32(obj, "nanosecond"),
            ),
            render_offset(get_i32(obj, "offset_seconds"))
        ),
        TemporalKind::DateTime => {
            let base = format!(
                "{}T{}{}",
                render_date_ymd(
                    get_i32(obj, "year"),
                    get_u32(obj, "month"),
                    get_u32(obj, "day")
                ),
                render_time_of_day(
                    get_u32(obj, "hour"),
                    get_u32(obj, "minute"),
                    get_u32(obj, "second"),
                    get_u32(obj, "nanosecond"),
                ),
                render_offset(get_i32(obj, "offset_seconds"))
            );
            match obj.get("tz").and_then(Value::as_str) {
                Some(zone) => format!("{base}[{zone}]"),
                None => base,
            }
        }
        TemporalKind::Duration => render_duration(
            get_i64(obj, "months"),
            get_i64(obj, "days"),
            get_i64(obj, "seconds"),
            get_i32(obj, "nanos"),
        ),
    })
}

/// Recursively collapses every tagged temporal value in `value` (including
/// ones nested inside `Value::Array`/`Value::Object`, e.g. a list or map of
/// durations) into its canonical ISO-8601 `Value::String`, in place.
/// Non-temporal values are left structurally untouched (only descended
/// into).
///
/// `pub(crate)`: this is called from **three** independent boundary points,
/// not one — there is no single funnel a tagged value is guaranteed to
/// cross before reaching a caller:
///
/// 1. [`super::super::dispatch::execute::Executor::execute`] — the
///    projection boundary for read queries and the plain executor's write
///    fallback (standalone `CREATE`, the HTTP layer's read-only lock-free
///    fast path, and `Engine::dispatch`'s generic fallback all route
///    through here).
/// 2. [`super::super::operators::create::Executor::resolve_property_expr_for_create`]
///    — the *storage* boundary: a `CREATE (n {d: duration(...)})` property
///    value must canonicalize before it is written to a node/relationship
///    record, or the tagged JSON object persists on disk and is fed to
///    indexes.
/// 3. `crate::engine::write_exec::return_builder::{build_return_result,
///    build_return_result_with_rels}` — the `MERGE`/`SET`/`REMOVE`/
///    `FOREACH` write path's own inline `RETURN`. This path reads node/
///    relationship properties directly out of storage and builds its
///    `ResultSet` without ever calling `Executor::execute`, so point 1
///    does not cover it — defense in depth alongside point 2, since a
///    property written before point 2 existed (or by a write path that
///    does not resolve through `resolve_property_expr_for_create`, e.g.
///    `SET`) could still carry a stale tag.
pub(crate) fn canonicalize_value_in_place(value: &mut Value) {
    if let Some(rendered) = canonicalize_temporal(value) {
        *value = Value::String(rendered);
        return;
    }
    match value {
        Value::Array(items) => {
            for item in items.iter_mut() {
                canonicalize_value_in_place(item);
            }
        }
        Value::Object(map) => {
            for v in map.values_mut() {
                canonicalize_value_in_place(v);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_duration_map_renders_canonical_iso() {
        // The F-015 gate case: duration({days: 14, hours: 16, minutes: 12}).
        let seconds = 16 * 3600 + 12 * 60;
        let d = make_duration(0, 14, seconds, 0);
        assert_eq!(canonicalize_temporal(&d).as_deref(), Some("P14DT16H12M"));
    }

    #[test]
    fn gate_date_map_renders_canonical_iso() {
        let d = make_date(2020, 1, 1);
        assert_eq!(canonicalize_temporal(&d).as_deref(), Some("2020-01-01"));
    }

    #[test]
    fn duration_zero_renders_pt0s() {
        let d = make_duration(0, 0, 0, 0);
        assert_eq!(canonicalize_temporal(&d).as_deref(), Some("PT0S"));
    }

    #[test]
    fn duration_render_normalizes_a_raw_unnormalized_zero_to_pt0s() {
        // (seconds: -1, nanos: 1_000_000_000) is zero once normalized
        // (carries to seconds: 0, nanos: 0) but is NOT zero as raw input —
        // `render_duration` must test the normalized output, not the raw
        // `(seconds, nanos)` parameters, or this renders the invalid bare
        // "P" instead of "PT0S".
        assert_eq!(render_duration(0, 0, -1, 1_000_000_000), "PT0S");
    }

    #[test]
    fn duration_splits_total_months_into_years_and_months_of_year() {
        // years:12, months:5 -> months_total = 149; days:14;
        // hours:16, minutes:12, seconds:70 -> seconds_total = 58390.
        let d = make_duration(12 * 12 + 5, 14, 16 * 3600 + 12 * 60 + 70, 0);
        assert_eq!(
            canonicalize_temporal(&d).as_deref(),
            Some("P12Y5M14DT16H13M10S")
        );
    }

    #[test]
    fn duration_fractional_seconds_are_trimmed_not_padded() {
        let d = make_duration(0, 0, -1, -999_000_000);
        assert_eq!(canonicalize_temporal(&d).as_deref(), Some("PT-1.999S"));
    }

    #[test]
    fn duration_negative_seconds_of_minute_carries_into_minutes() {
        // seconds: -60, milliseconds: -1 -> raw -60.001s.
        let d = make_duration(0, 0, -60, -1_000_000);
        assert_eq!(canonicalize_temporal(&d).as_deref(), Some("PT-1M-0.001S"));
    }

    #[test]
    fn duration_render_normalizes_out_of_range_nanos_input() {
        // MAJOR-1 regression: `render_duration`/`duration_parts` must not
        // trust an already-normalized `nanos` — a caller passing a raw,
        // un-normalized magnitude (e.g. `1_999_999_999`, which
        // `normalize_seconds_nanos` alone would never produce, but a
        // future/foreign caller of `render_duration` might) must still
        // carry cleanly into `seconds` rather than rendering a malformed
        // 10-digit fraction like `"PT0.1999999999S"`.
        assert_eq!(render_duration(0, 0, 0, 1_999_999_999), "PT1.999999999S");
        assert_eq!(render_duration(0, 0, 0, -1_999_999_999), "PT-1.999999999S");
        // i32::MIN specifically must not panic (`.abs()` on `i32::MIN`
        // overflows) and must still normalize into range.
        assert_eq!(render_duration(0, 0, 0, i32::MIN), "PT-2.147483648S");
    }

    #[test]
    fn duration_negative_years_and_months_render_per_component_sign() {
        let d = make_duration(-1 * 12 + -8, 0, 0, 0);
        assert_eq!(canonicalize_temporal(&d).as_deref(), Some("P-1Y-8M"));
    }

    #[test]
    fn localtime_trims_trailing_zero_nanoseconds() {
        assert_eq!(
            canonicalize_temporal(&make_localtime(12, 31, 14, 645_000_000)).as_deref(),
            Some("12:31:14.645")
        );
        assert_eq!(
            canonicalize_temporal(&make_localtime(12, 31, 14, 645_876_000)).as_deref(),
            Some("12:31:14.645876")
        );
        assert_eq!(
            canonicalize_temporal(&make_localtime(12, 31, 14, 645_876_123)).as_deref(),
            Some("12:31:14.645876123")
        );
        assert_eq!(
            canonicalize_temporal(&make_localtime(12, 31, 14, 3)).as_deref(),
            Some("12:31:14.000000003")
        );
        assert_eq!(
            canonicalize_temporal(&make_localtime(12, 31, 14, 0)).as_deref(),
            Some("12:31:14")
        );
    }

    #[test]
    fn localdatetime_renders_date_t_time() {
        let d = make_localdatetime(1984, 10, 11, 12, 31, 14, 0);
        assert_eq!(
            canonicalize_temporal(&d).as_deref(),
            Some("1984-10-11T12:31:14")
        );
    }

    #[test]
    fn time_renders_with_offset() {
        let t = make_time(12, 31, 14, 0, 3600);
        assert_eq!(canonicalize_temporal(&t).as_deref(), Some("12:31:14+01:00"));
        let t_neg = make_time(12, 31, 14, 0, -3600);
        assert_eq!(
            canonicalize_temporal(&t_neg).as_deref(),
            Some("12:31:14-01:00")
        );
    }

    #[test]
    fn datetime_renders_with_offset_and_no_zone_suffix_by_default() {
        let dt = make_datetime(2020, 1, 1, 12, 31, 14, 0, 3600, None);
        assert_eq!(
            canonicalize_temporal(&dt).as_deref(),
            Some("2020-01-01T12:31:14+01:00")
        );
    }

    #[test]
    fn datetime_renders_zone_suffix_when_tz_name_present() {
        let dt = make_datetime(
            2020,
            1,
            1,
            12,
            31,
            14,
            0,
            3600,
            Some("Europe/Stockholm".to_string()),
        );
        assert_eq!(
            canonicalize_temporal(&dt).as_deref(),
            Some("2020-01-01T12:31:14+01:00[Europe/Stockholm]")
        );
    }

    #[test]
    fn canonicalize_value_in_place_leaves_non_temporal_values_alone() {
        let mut v = serde_json::json!({"a": 1, "b": [1, 2, "x"], "c": null});
        let before = v.clone();
        canonicalize_value_in_place(&mut v);
        assert_eq!(v, before);
    }

    #[test]
    fn canonicalize_value_in_place_rewrites_nested_temporals_in_list_and_map() {
        let mut v = Value::Array(vec![
            make_date(2020, 1, 1),
            Value::Object(Map::from_iter([(
                "d".to_string(),
                make_duration(0, 14, 16 * 3600 + 12 * 60, 0),
            )])),
        ]);
        canonicalize_value_in_place(&mut v);
        assert_eq!(
            v,
            Value::Array(vec![
                Value::String("2020-01-01".to_string()),
                Value::Object(Map::from_iter([(
                    "d".to_string(),
                    Value::String("P14DT16H12M".to_string())
                )])),
            ])
        );
    }

    #[test]
    fn canonicalize_value_in_place_never_leaves_the_tag_key_in_output() {
        let mut v = make_datetime(2020, 1, 1, 0, 0, 0, 0, 0, None);
        canonicalize_value_in_place(&mut v);
        let rendered = v.as_str().expect("temporal must canonicalize to a string");
        assert!(!rendered.contains(TEMPORAL_TAG_KEY));
        assert!(matches!(v, Value::String(_)));
    }

    #[test]
    fn duration_components_reads_back_normalized_fields() {
        let d = make_duration(14, 2, 90, 500_000_000);
        assert_eq!(duration_components(&d), Some((14, 2, 90, 500_000_000)));
        assert_eq!(duration_components(&make_date(2020, 1, 1)), None);
    }

    #[test]
    fn offset_seconds_reads_time_and_datetime_only() {
        let t = make_time(12, 31, 14, 0, 3600);
        assert_eq!(offset_seconds(&t), Some(3600));
        let dt = make_datetime(2020, 1, 1, 0, 0, 0, 0, -1800, None);
        assert_eq!(offset_seconds(&dt), Some(-1800));
        assert_eq!(offset_seconds(&make_localtime(12, 31, 14, 0)), None);
        assert_eq!(offset_seconds(&make_date(2020, 1, 1)), None);
    }

    #[test]
    fn zone_name_reads_datetime_tz_field_only() {
        let dt = make_datetime(
            2020,
            1,
            1,
            0,
            0,
            0,
            0,
            3600,
            Some("Europe/Stockholm".to_string()),
        );
        assert_eq!(zone_name(&dt), Some("Europe/Stockholm"));
        let dt_no_zone = make_datetime(2020, 1, 1, 0, 0, 0, 0, 3600, None);
        assert_eq!(zone_name(&dt_no_zone), None);
        let t = make_time(12, 31, 14, 0, 3600);
        assert_eq!(zone_name(&t), None);
    }

    #[test]
    fn duration_parts_matches_accessor_semantics() {
        // years(duration({years: 5, months: 3})) == 5,
        // months(duration({years: 5, months: 3})) == 3.
        let parts = duration_parts(5 * 12 + 3, 0, 0, 0);
        assert_eq!(parts.years, 5);
        assert_eq!(parts.months_of_year, 3);

        // hours(duration({hours: 12, minutes: 30})) == 12,
        // minutes(duration({hours: 12, minutes: 30})) == 30.
        let parts = duration_parts(0, 0, 12 * 3600 + 30 * 60, 0);
        assert_eq!(parts.hours, 12);
        assert_eq!(parts.minutes_of_hour, 30);

        // seconds(duration({minutes: 5, seconds: 45})) == 45.
        let parts = duration_parts(0, 0, 5 * 60 + 45, 0);
        assert_eq!(parts.seconds_of_minute, 45);
    }
}
