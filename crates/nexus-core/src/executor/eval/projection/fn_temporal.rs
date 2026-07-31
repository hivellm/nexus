//! Temporal and duration built-in functions for the projection evaluator.
//!
//! Covers `date`, `datetime`, `time`, `timestamp`, `duration`, `duration.*`,
//! `toDate`, `localtime`, `localdatetime`, and temporal component extractors
//! (`year`, `month`, `day`, `hour`, `minute`, `second`, `quarter`, `week`,
//! `dayofweek`, `dayofyear`, `millisecond`, `microsecond`, `nanosecond`).
//! Duration component extractors (`years`, `months`, `weeks`, `days`,
//! `hours`, `minutes`, `seconds`) are also here.
//!
//! `date`/`time`/`localtime`/`datetime`/`localdatetime`/`duration` build a
//! *tagged* intermediate value (see
//! `super::super::super::eval::temporal_value`) rather than a rendered
//! string/flat-object — the canonical ISO-8601 rendering happens once, at
//! the projection boundary (`Executor::execute`), not here. Extractor
//! functions read the tagged shape directly (via chrono, for calendar
//! derivations like `quarter`/`week`/`dayofweek`) but keep accepting the
//! legacy `Value::String`/`Value::Object` shapes too, so a raw ISO literal
//! or a hand-built map still works.

use super::super::super::context::ExecutionContext;
use super::super::super::engine::Executor;
use super::super::temporal_parse;
use super::super::temporal_retag;
use super::super::temporal_value;
use crate::Result;
use chrono::{Datelike, Offset, TimeZone, Timelike};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// If `value` is a tagged temporal instant or duration, replaces it with
/// its canonical ISO-8601 string; otherwise returns `value` unchanged.
/// Bridges the four legacy `duration.*` static functions (which only ever
/// spoke `Value::String`) and `timestamp()` to the typed constructors
/// without rewriting their own chrono parsing.
fn coerce_temporal_arg(value: Value) -> Value {
    match temporal_value::canonicalize_temporal(&value) {
        Some(s) => Value::String(s),
        None => value,
    }
}

/// Reads and validates a map constructor's `nanosecond` key (defaults to
/// `0` when absent) — shared by every instant map constructor
/// (`localtime`/`time`/`localdatetime`/`datetime`). The openCypher TCK's
/// `Temporal4.feature`/`Temporal5.feature` map literals all use this
/// singular key (distinct from `duration({...})`'s own plural
/// `nanoseconds` convention, which `"duration"`'s branch below reads
/// separately).
///
/// Must be a non-negative integer strictly less than `1_000_000_000` (a
/// whole second is exactly `1_000_000_000` nanoseconds) — anything else
/// is rejected with an explicit error rather than silently reinterpreted
/// as a smaller in-range quantity. `Value::Number::as_u64()` already
/// returns `None` for a negative value AND for any float-backed value
/// (whole or fractional), so the single `.filter()` below covers all
/// three reject shapes (negative, fractional, `>= 1_000_000_000`) — e.g.
/// `nanosecond: 1500000000` must be a hard error, not silently rendered
/// as `.15` seconds (1.5 * 10^9 truncated into the `[0, 10^9)` window is
/// a completely different quantity than the caller wrote).
fn nanosecond_from_map(map: &Map<String, Value>) -> Result<u32> {
    let Some(value) = map.get("nanosecond") else {
        return Ok(0);
    };
    value
        .as_u64()
        .filter(|n| *n < 1_000_000_000)
        .map(|n| n as u32)
        .ok_or_else(|| {
            crate::Error::CypherExecution(format!(
                "InvalidArgumentValue: `nanosecond` must be a non-negative integer in \
                 [0, 999999999], got {value}"
            ))
        })
}

/// Resolves a map constructor's `timezone` key into `(offset_seconds,
/// zone_name)` for the `time`/`datetime` map constructors — defaulting to
/// UTC (`(0, None)`) when the key is absent, matching real Neo4j's
/// default (NOT the executing machine's local offset, a since-fixed
/// constructor gap; see `Temporal4.feature`'s `datetime({year: 1912})` ->
/// `'1912-01-01T00:00Z'`). A fixed-offset string (`'+01:00'`, `'Z'`, …) or
/// the literal `'UTC'` resolves directly to `(offset, None)`.
///
/// Anything else — a named IANA zone like `'Europe/Stockholm'` — cannot
/// be resolved to a real offset without a timezone database, which isn't
/// wired into this codebase (see `temporal_value::make_datetime`'s doc
/// comment). Previously this silently fell back to UTC while still
/// carrying the unresolved name through as `zone_name`, which rendered a
/// self-contradictory `...Z[Europe/Stockholm]` (the `Z` asserts UTC, the
/// bracket asserts it isn't). Until a real timezone database lands, an
/// unresolvable name is a hard error instead — the `Option<String>` in
/// this function's return type stays `None` on every success path today,
/// and only starts carrying a real resolved zone name once that database
/// exists.
fn timezone_from_map(map: &Map<String, Value>) -> Result<(i32, Option<String>)> {
    let Some(tz) = map.get("timezone").and_then(Value::as_str) else {
        return Ok((0, None));
    };
    if tz == "UTC" {
        return Ok((0, None));
    }
    temporal_retag::strict_parse_offset(tz)
        .map(|offset| (offset, None))
        .ok_or_else(|| {
            crate::Error::CypherExecution(format!(
                "InvalidArgumentValue: timezone '{tz}' requires a timezone database, which is \
                 not available; use a numeric UTC offset (e.g. '+02:00') or 'Z'/'UTC' instead"
            ))
        })
}

/// Resolves the `hours`/`minutes`/`seconds` fields of a `duration({...})`
/// map into a single normalized `(whole_seconds, nanos)` pair.
///
/// Takes an exact-integer fast path via `checked_mul`/`checked_add`
/// whenever every field present is JSON-integer-typed (`is_i64`/`is_u64` —
/// i.e. the Cypher literal had no decimal point): `f64` cannot represent
/// every `i64` exactly (a 52-bit mantissa), so combining large integer
/// inputs through `f64` silently loses precision, and `as i64` on an
/// out-of-range `f64` *saturates* rather than erroring — hiding a genuine
/// overflow behind a plausible-looking wrong answer instead of surfacing
/// it, unlike the `years`/`months`/`weeks`/`days` fields above (which
/// already use checked integer arithmetic throughout). Only a field the
/// caller wrote as a genuinely fractional literal (e.g.
/// `duration({minutes: 1.5, seconds: 1})`) falls back to the `f64`
/// combination, which is unavoidable for splitting a fractional value into
/// whole seconds + a nanosecond remainder.
fn seconds_from_hms(map: &Map<String, Value>) -> Result<(i64, i32)> {
    let hours_v = map.get("hours");
    let minutes_v = map.get("minutes");
    let seconds_v = map.get("seconds");

    let is_int_or_absent = |v: Option<&Value>| v.map(|n| n.is_i64() || n.is_u64()).unwrap_or(true);

    if is_int_or_absent(hours_v) && is_int_or_absent(minutes_v) && is_int_or_absent(seconds_v) {
        let hours = hours_v.and_then(Value::as_i64).unwrap_or(0);
        let minutes = minutes_v.and_then(Value::as_i64).unwrap_or(0);
        let seconds = seconds_v.and_then(Value::as_i64).unwrap_or(0);
        let overflow = || {
            crate::Error::CypherExecution(
                "duration arithmetic overflow: hour/minute/second component exceeds i64 range"
                    .to_string(),
            )
        };
        let total = hours
            .checked_mul(3600)
            .and_then(|h| minutes.checked_mul(60).map(|m| (h, m)))
            .and_then(|(h, m)| h.checked_add(m))
            .and_then(|hm| hm.checked_add(seconds))
            .ok_or_else(overflow)?;
        return Ok((total, 0));
    }

    // At least one of hours/minutes/seconds is a genuinely fractional
    // literal — combine via `f64` and split into whole seconds + a
    // nanosecond remainder, accepting `f64`'s precision limits (there is
    // no exact-integer representation of a fraction) rather than erroring.
    let hours = hours_v.and_then(Value::as_f64).unwrap_or(0.0);
    let minutes = minutes_v.and_then(Value::as_f64).unwrap_or(0.0);
    let seconds = seconds_v.and_then(Value::as_f64).unwrap_or(0.0);
    let total_seconds_f64 = hours * 3600.0 + minutes * 60.0 + seconds;
    let whole_seconds = total_seconds_f64.trunc() as i64;
    let nanos = ((total_seconds_f64 - total_seconds_f64.trunc()) * 1_000_000_000.0).round() as i32;
    Ok((whole_seconds, nanos))
}

impl Executor {
    /// Evaluate temporal and duration built-in functions.
    ///
    /// Returns `None` if the function name is not handled here.
    pub(super) fn eval_builtin_temporal(
        &self,
        row: &HashMap<String, Value>,
        context: &ExecutionContext,
        name: &str,
        args: &[super::super::super::parser::Expression],
    ) -> Option<Result<Value>> {
        match name {
            "todate" => {
                // toDate(value) - Convert to date string (YYYY-MM-DD)
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    match value {
                        Value::String(s) => {
                            // Try to parse date string
                            if let Ok(date) = chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d") {
                                return Some(Ok(Value::String(
                                    date.format("%Y-%m-%d").to_string(),
                                )));
                            }
                            // Try datetime format
                            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                                return Some(Ok(Value::String(
                                    dt.date_naive().format("%Y-%m-%d").to_string(),
                                )));
                            }
                        }
                        Value::Object(map) => {
                            // Support {year, month, day} format — also covers a
                            // tagged date/datetime argument, whose map carries
                            // the same `year`/`month`/`day` keys alongside the
                            // `_nexus_temporal_type` tag (harmlessly ignored).
                            let year = map
                                .get("year")
                                .and_then(|v| v.as_i64())
                                .unwrap_or_else(|| chrono::Local::now().year() as i64)
                                as i32;
                            let month =
                                map.get("month").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
                            let day = map.get("day").and_then(|v| v.as_u64()).unwrap_or(1) as u32;

                            if let Some(date) = chrono::NaiveDate::from_ymd_opt(year, month, day) {
                                return Some(Ok(Value::String(
                                    date.format("%Y-%m-%d").to_string(),
                                )));
                            }
                        }
                        _ => {}
                    }
                }
                Some(Ok(Value::Null))
            }
            // Temporal functions
            "date" => {
                if args.is_empty() {
                    let now = chrono::Local::now();
                    return Some(Ok(temporal_value::make_date(
                        now.year(),
                        now.month(),
                        now.day(),
                    )));
                } else if let Some(arg) = args.first() {
                    // Parse date from string or map
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    match value {
                        Value::String(s) => {
                            // Full ISO-8601 calendar/week/ordinal date
                            // parsing (extended and compact notation) — see
                            // `temporal_parse::parse_iso_date`.
                            if let Some((year, month, day)) = temporal_parse::parse_iso_date(&s) {
                                return Some(Ok(temporal_value::make_date(year, month, day)));
                            }
                        }
                        Value::Object(map) => {
                            // Support {year, month, day} format
                            let year = map
                                .get("year")
                                .and_then(|v| v.as_i64())
                                .unwrap_or_else(|| chrono::Local::now().year() as i64)
                                as i32;
                            let month =
                                map.get("month").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
                            let day = map.get("day").and_then(|v| v.as_u64()).unwrap_or(1) as u32;

                            if chrono::NaiveDate::from_ymd_opt(year, month, day).is_some() {
                                return Some(Ok(temporal_value::make_date(year, month, day)));
                            }
                        }
                        _ => {}
                    }
                }
                Some(Ok(Value::Null))
            }
            "datetime" => {
                if args.is_empty() {
                    let now = chrono::Local::now();
                    let offset_seconds = now.offset().fix().local_minus_utc();
                    return Some(Ok(temporal_value::make_datetime(
                        now.year(),
                        now.month(),
                        now.day(),
                        now.hour(),
                        now.minute(),
                        now.second(),
                        now.nanosecond(),
                        offset_seconds,
                        None,
                    )));
                } else if let Some(arg) = args.first() {
                    // Parse datetime from string or map
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    match value {
                        Value::String(s) => {
                            // Try to parse RFC3339/ISO8601 datetime
                            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                                return Some(Ok(temporal_value::make_datetime(
                                    dt.year(),
                                    dt.month(),
                                    dt.day(),
                                    dt.hour(),
                                    dt.minute(),
                                    dt.second(),
                                    dt.nanosecond(),
                                    dt.offset().local_minus_utc(),
                                    None,
                                )));
                            }
                            // Try to parse without timezone
                            if let Ok(dt) =
                                chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S")
                            {
                                let local = chrono::Local::now().timezone();
                                let dt_local = local
                                    .from_local_datetime(&dt)
                                    .earliest()
                                    .unwrap_or_else(|| local.from_utc_datetime(&dt));
                                let offset_seconds = dt_local.offset().fix().local_minus_utc();
                                return Some(Ok(temporal_value::make_datetime(
                                    dt.year(),
                                    dt.month(),
                                    dt.day(),
                                    dt.hour(),
                                    dt.minute(),
                                    dt.second(),
                                    dt.nanosecond(),
                                    offset_seconds,
                                    None,
                                )));
                            }
                        }
                        Value::Object(map) => {
                            // Support {year, month, day, hour, minute, second, nanosecond, timezone} format
                            let year = map
                                .get("year")
                                .and_then(|v| v.as_i64())
                                .unwrap_or_else(|| chrono::Local::now().year() as i64)
                                as i32;
                            let month =
                                map.get("month").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
                            let day = map.get("day").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
                            let hour = map.get("hour").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                            let minute =
                                map.get("minute").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                            let second =
                                map.get("second").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                            let nanosecond = match nanosecond_from_map(&map) {
                                Ok(n) => n,
                                Err(e) => return Some(Err(e)),
                            };
                            let (offset_seconds, zone_name) = match timezone_from_map(&map) {
                                Ok(pair) => pair,
                                Err(e) => return Some(Err(e)),
                            };

                            if chrono::NaiveDate::from_ymd_opt(year, month, day).is_some()
                                && chrono::NaiveTime::from_hms_opt(hour, minute, second).is_some()
                            {
                                return Some(Ok(temporal_value::make_datetime(
                                    year,
                                    month,
                                    day,
                                    hour,
                                    minute,
                                    second,
                                    nanosecond,
                                    offset_seconds,
                                    zone_name,
                                )));
                            }
                        }
                        _ => {}
                    }
                }
                Some(Ok(Value::Null))
            }
            "time" => {
                if args.is_empty() {
                    let now = chrono::Local::now();
                    let offset_seconds = now.offset().fix().local_minus_utc();
                    return Some(Ok(temporal_value::make_time(
                        now.hour(),
                        now.minute(),
                        now.second(),
                        now.nanosecond(),
                        offset_seconds,
                    )));
                } else if let Some(arg) = args.first() {
                    // Parse time from string or map
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    match value {
                        Value::String(s) => {
                            // Try to parse time format HH:MM:SS
                            if let Ok(time) = chrono::NaiveTime::parse_from_str(&s, "%H:%M:%S") {
                                return Some(Ok(temporal_value::make_time(
                                    time.hour(),
                                    time.minute(),
                                    time.second(),
                                    time.nanosecond(),
                                    0,
                                )));
                            }
                            // Try HH:MM format
                            if let Ok(time) = chrono::NaiveTime::parse_from_str(&s, "%H:%M") {
                                return Some(Ok(temporal_value::make_time(
                                    time.hour(),
                                    time.minute(),
                                    time.second(),
                                    time.nanosecond(),
                                    0,
                                )));
                            }
                        }
                        Value::Object(map) => {
                            // Support {hour, minute, second, nanosecond, timezone} format
                            let hour = map.get("hour").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                            let minute =
                                map.get("minute").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                            let second =
                                map.get("second").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                            let nanosecond = match nanosecond_from_map(&map) {
                                Ok(n) => n,
                                Err(e) => return Some(Err(e)),
                            };
                            let (offset_seconds, _zone_name) = match timezone_from_map(&map) {
                                Ok(pair) => pair,
                                Err(e) => return Some(Err(e)),
                            };

                            if chrono::NaiveTime::from_hms_opt(hour, minute, second).is_some() {
                                return Some(Ok(temporal_value::make_time(
                                    hour,
                                    minute,
                                    second,
                                    nanosecond,
                                    offset_seconds,
                                )));
                            }
                        }
                        _ => {}
                    }
                }
                Some(Ok(Value::Null))
            }
            "timestamp" => {
                // timestamp() - current or coerced Unix timestamp in milliseconds
                if args.is_empty() {
                    let now = chrono::Local::now();
                    let millis = now.timestamp_millis();
                    return Some(Ok(Value::Number(millis.into())));
                } else if let Some(arg) = args.first() {
                    // Parse timestamp from string or return existing number
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    // A tagged datetime/date argument coerces to its ISO
                    // string first, so the RFC3339 parse below still fires.
                    match coerce_temporal_arg(value) {
                        Value::Number(n) => {
                            // Return as-is if already a number
                            return Some(Ok(Value::Number(n)));
                        }
                        Value::String(s) => {
                            // Try to parse datetime and convert to timestamp
                            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                                let millis = dt.timestamp_millis();
                                return Some(Ok(Value::Number(millis.into())));
                            }
                        }
                        _ => {}
                    }
                }
                Some(Ok(Value::Null))
            }
            "duration" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if let Value::String(s) = &value {
                        // Full ISO-8601 duration string parsing (standard
                        // component form + the alternative
                        // `P<date>T<time>` count form), including
                        // fractional-component carry — see
                        // `temporal_parse::parse_iso_duration`.
                        if let Some((months, days, seconds, nanos)) =
                            temporal_parse::parse_iso_duration(s)
                        {
                            return Some(Ok(temporal_value::make_duration(
                                months, days, seconds, nanos,
                            )));
                        }
                        return Some(Ok(Value::Null));
                    }
                    if let Value::Object(map) = value {
                        // Fold years into months, weeks into days, and
                        // hours/minutes/seconds into a single (whole-seconds,
                        // nanos) pair — see `temporal_value`'s module doc for
                        // why this (months, days, seconds, nanos) shape
                        // (Neo4j's own internal Duration layout) is what
                        // canonical rendering needs, rather than the raw
                        // components a `duration({...})` literal supplies.
                        let years = map.get("years").and_then(|v| v.as_i64()).unwrap_or(0);
                        let months_in = map.get("months").and_then(|v| v.as_i64()).unwrap_or(0);
                        let weeks = map.get("weeks").and_then(|v| v.as_i64()).unwrap_or(0);
                        let days_in = map.get("days").and_then(|v| v.as_i64()).unwrap_or(0);

                        let months = match years
                            .checked_mul(12)
                            .and_then(|y12| y12.checked_add(months_in))
                        {
                            Some(m) => m,
                            None => {
                                return Some(Err(crate::Error::CypherExecution(
                                    "duration arithmetic overflow: year/month component exceeds i64 range"
                                        .to_string(),
                                )));
                            }
                        };
                        let days = match weeks.checked_mul(7).and_then(|w7| w7.checked_add(days_in))
                        {
                            Some(d) => d,
                            None => {
                                return Some(Err(crate::Error::CypherExecution(
                                    "duration arithmetic overflow: week/day component exceeds i64 range"
                                        .to_string(),
                                )));
                            }
                        };

                        let (hms_seconds, hms_nanos) = match seconds_from_hms(&map) {
                            Ok(pair) => pair,
                            Err(e) => return Some(Err(e)),
                        };

                        // A `nanoseconds` map key folds in alongside (not
                        // instead of) whatever fractional-seconds
                        // remainder `seconds_from_hms` already produced —
                        // `duration({seconds: 70, nanoseconds: 1})` sums
                        // both, matching the openCypher TCK's
                        // `Temporal8.feature` scenario 7 fixture.
                        let extra_nanos =
                            map.get("nanoseconds").and_then(Value::as_i64).unwrap_or(0);
                        let overflow = || {
                            crate::Error::CypherExecution(
                                "duration arithmetic overflow: nanosecond component exceeds i64 range"
                                    .to_string(),
                            )
                        };
                        let combined_nanos = match i64::from(hms_nanos).checked_add(extra_nanos) {
                            Some(n) => n,
                            None => return Some(Err(overflow())),
                        };
                        let carry_seconds = combined_nanos.div_euclid(1_000_000_000);
                        let remainder_nanos = combined_nanos.rem_euclid(1_000_000_000) as i32;
                        let whole_seconds = match hms_seconds.checked_add(carry_seconds) {
                            Some(s) => s,
                            None => return Some(Err(overflow())),
                        };

                        return Some(Ok(temporal_value::make_duration(
                            months,
                            days,
                            whole_seconds,
                            remainder_nanos,
                        )));
                    }
                }
                Some(Ok(Value::Null))
            }
            "duration.between" => {
                // duration.between(datetime1, datetime2) - computes the duration between two datetimes
                if args.len() >= 2 {
                    let dt1 = match self.evaluate_projection_expression(row, context, &args[0]) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    let dt2 = match self.evaluate_projection_expression(row, context, &args[1]) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    let dt1 = coerce_temporal_arg(dt1);
                    let dt2 = coerce_temporal_arg(dt2);

                    if Self::is_datetime_string(&dt1) && Self::is_datetime_string(&dt2) {
                        return Some(self.datetime_difference(&dt1, &dt2));
                    }
                }
                Some(Ok(Value::Null))
            }
            "duration.inMonths" => {
                // duration.inMonths(datetime1, datetime2) - duration in months
                if args.len() >= 2 {
                    let dt1 = match self.evaluate_projection_expression(row, context, &args[0]) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    let dt2 = match self.evaluate_projection_expression(row, context, &args[1]) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    let dt1 = coerce_temporal_arg(dt1);
                    let dt2 = coerce_temporal_arg(dt2);

                    if let (Value::String(s1), Value::String(s2)) = (&dt1, &dt2) {
                        // Try parsing as dates
                        let d1 = chrono::NaiveDate::parse_from_str(s1, "%Y-%m-%d").or_else(|_| {
                            chrono::DateTime::parse_from_rfc3339(s1).map(|dt| dt.date_naive())
                        });
                        let d2 = chrono::NaiveDate::parse_from_str(s2, "%Y-%m-%d").or_else(|_| {
                            chrono::DateTime::parse_from_rfc3339(s2).map(|dt| dt.date_naive())
                        });

                        if let (Ok(date1), Ok(date2)) = (d1, d2) {
                            let months = (date1.year() - date2.year()) * 12
                                + (date1.month() as i32 - date2.month() as i32);

                            return Some(Ok(temporal_value::make_duration(
                                i64::from(months),
                                0,
                                0,
                                0,
                            )));
                        }
                    }
                }
                Some(Ok(Value::Null))
            }
            "duration.inDays" => {
                // duration.inDays(datetime1, datetime2) - duration in days
                if args.len() >= 2 {
                    let dt1 = match self.evaluate_projection_expression(row, context, &args[0]) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    let dt2 = match self.evaluate_projection_expression(row, context, &args[1]) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    let dt1 = coerce_temporal_arg(dt1);
                    let dt2 = coerce_temporal_arg(dt2);

                    if let (Value::String(s1), Value::String(s2)) = (&dt1, &dt2) {
                        // Try parsing as dates
                        let d1 = chrono::NaiveDate::parse_from_str(s1, "%Y-%m-%d").or_else(|_| {
                            chrono::DateTime::parse_from_rfc3339(s1).map(|dt| dt.date_naive())
                        });
                        let d2 = chrono::NaiveDate::parse_from_str(s2, "%Y-%m-%d").or_else(|_| {
                            chrono::DateTime::parse_from_rfc3339(s2).map(|dt| dt.date_naive())
                        });

                        if let (Ok(date1), Ok(date2)) = (d1, d2) {
                            let days = date1.signed_duration_since(date2).num_days();

                            return Some(Ok(temporal_value::make_duration(0, days, 0, 0)));
                        }
                    }
                }
                Some(Ok(Value::Null))
            }
            "duration.inSeconds" => {
                // duration.inSeconds(datetime1, datetime2) - duration in seconds
                if args.len() >= 2 {
                    let dt1 = match self.evaluate_projection_expression(row, context, &args[0]) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    let dt2 = match self.evaluate_projection_expression(row, context, &args[1]) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    let dt1 = coerce_temporal_arg(dt1);
                    let dt2 = coerce_temporal_arg(dt2);

                    if let (Value::String(s1), Value::String(s2)) = (&dt1, &dt2) {
                        // Try parsing as datetimes
                        let d1 = chrono::DateTime::parse_from_rfc3339(s1)
                            .map(|dt| dt.with_timezone(&chrono::Utc));
                        let d2 = chrono::DateTime::parse_from_rfc3339(s2)
                            .map(|dt| dt.with_timezone(&chrono::Utc));

                        if let (Ok(dt1), Ok(dt2)) = (d1, d2) {
                            let seconds = dt1.signed_duration_since(dt2).num_seconds();

                            return Some(Ok(temporal_value::make_duration(0, 0, seconds, 0)));
                        }
                    }
                }
                Some(Ok(Value::Null))
            }
            // Advanced temporal functions
            "localtime" => {
                // localtime() - returns current local time without timezone
                if args.is_empty() {
                    let now = chrono::Local::now();
                    return Some(Ok(temporal_value::make_localtime(
                        now.hour(),
                        now.minute(),
                        now.second(),
                        now.nanosecond(),
                    )));
                } else if let Some(arg) = args.first() {
                    // Parse time from string or map
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    match value {
                        Value::String(s) => {
                            // Try to parse time format
                            if let Ok(time) = chrono::NaiveTime::parse_from_str(&s, "%H:%M:%S") {
                                return Some(Ok(temporal_value::make_localtime(
                                    time.hour(),
                                    time.minute(),
                                    time.second(),
                                    time.nanosecond(),
                                )));
                            }
                            // Try HH:MM format
                            if let Ok(time) = chrono::NaiveTime::parse_from_str(&s, "%H:%M") {
                                return Some(Ok(temporal_value::make_localtime(
                                    time.hour(),
                                    time.minute(),
                                    time.second(),
                                    time.nanosecond(),
                                )));
                            }
                        }
                        Value::Object(map) => {
                            let hour = map.get("hour").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                            let minute =
                                map.get("minute").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                            let second =
                                map.get("second").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                            let nanosecond = match nanosecond_from_map(&map) {
                                Ok(n) => n,
                                Err(e) => return Some(Err(e)),
                            };

                            if chrono::NaiveTime::from_hms_opt(hour, minute, second).is_some() {
                                return Some(Ok(temporal_value::make_localtime(
                                    hour, minute, second, nanosecond,
                                )));
                            }
                        }
                        _ => {}
                    }
                }
                Some(Ok(Value::Null))
            }
            "localdatetime" => {
                // localdatetime() - returns current local datetime without timezone
                if args.is_empty() {
                    let now = chrono::Local::now();
                    return Some(Ok(temporal_value::make_localdatetime(
                        now.year(),
                        now.month(),
                        now.day(),
                        now.hour(),
                        now.minute(),
                        now.second(),
                        now.nanosecond(),
                    )));
                } else if let Some(arg) = args.first() {
                    // Parse datetime from string or map
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    match value {
                        Value::String(s) => {
                            // Try to parse datetime format
                            if let Ok(dt) =
                                chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S")
                            {
                                return Some(Ok(temporal_value::make_localdatetime(
                                    dt.year(),
                                    dt.month(),
                                    dt.day(),
                                    dt.hour(),
                                    dt.minute(),
                                    dt.second(),
                                    dt.nanosecond(),
                                )));
                            }
                            // Try with timezone and convert to naive
                            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                                let naive = dt.naive_local();
                                return Some(Ok(temporal_value::make_localdatetime(
                                    naive.year(),
                                    naive.month(),
                                    naive.day(),
                                    naive.hour(),
                                    naive.minute(),
                                    naive.second(),
                                    naive.nanosecond(),
                                )));
                            }
                        }
                        Value::Object(map) => {
                            let year = map
                                .get("year")
                                .and_then(|v| v.as_i64())
                                .unwrap_or_else(|| chrono::Local::now().year() as i64)
                                as i32;
                            let month =
                                map.get("month").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
                            let day = map.get("day").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
                            let hour = map.get("hour").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                            let minute =
                                map.get("minute").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                            let second =
                                map.get("second").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                            let nanosecond = match nanosecond_from_map(&map) {
                                Ok(n) => n,
                                Err(e) => return Some(Err(e)),
                            };

                            if chrono::NaiveDate::from_ymd_opt(year, month, day).is_some()
                                && chrono::NaiveTime::from_hms_opt(hour, minute, second).is_some()
                            {
                                return Some(Ok(temporal_value::make_localdatetime(
                                    year, month, day, hour, minute, second, nanosecond,
                                )));
                            }
                        }
                        _ => {}
                    }
                }
                Some(Ok(Value::Null))
            }
            // Temporal component extraction functions
            "year" => Some(self.date_component_from(row, context, args, |date| date.year() as i64)),
            "month" => {
                Some(self.date_component_from(row, context, args, |date| date.month() as i64))
            }
            "day" => Some(self.date_component_from(row, context, args, |date| date.day() as i64)),
            "hour" => Some(self.time_component_from(row, context, args, |h, _m, _s, _ns| h as i64)),
            "minute" => {
                Some(self.time_component_from(row, context, args, |_h, m, _s, _ns| m as i64))
            }
            "second" => {
                Some(self.time_component_from(row, context, args, |_h, _m, s, _ns| s as i64))
            }
            "quarter" => Some(self.date_component_from(row, context, args, |date| {
                ((date.month() - 1) / 3 + 1) as i64
            })),
            "week" => Some(
                self.date_component_from(row, context, args, |date| date.iso_week().week() as i64),
            ),
            "dayofweek" => Some(self.date_component_from(row, context, args, |date| {
                // Neo4j returns 1-7 (Monday to Sunday)
                date.weekday().num_days_from_monday() as i64 + 1
            })),
            "dayofyear" => {
                Some(self.date_component_from(row, context, args, |date| date.ordinal() as i64))
            }
            "millisecond" => Some(self.time_component_from(
                row,
                context,
                args,
                |_h, _m, _s, ns| (ns / 1_000_000) as i64,
            )),
            "microsecond" => Some(self.time_component_from(
                row,
                context,
                args,
                |_h, _m, _s, ns| (ns / 1_000) as i64,
            )),
            "nanosecond" => {
                Some(self.time_component_from(row, context, args, |_h, _m, _s, ns| ns as i64))
            }
            // Duration component extraction functions — see
            // `temporal_value::duration_parts` for the years/monthsOfYear/
            // weeks/hours/minutesOfHour/secondsOfMinute decomposition
            // these share with canonical rendering. `weeks` is `days / 7`
            // (Neo4j's `duration.weeks` semantics) — `days` itself stays
            // the untouched total, not a `daysOfWeek` remainder.
            "years" => Some(self.duration_component_from(row, context, args, |parts| parts.years)),
            "months" => {
                Some(self.duration_component_from(row, context, args, |parts| parts.months_of_year))
            }
            "weeks" => Some(self.duration_component_from(row, context, args, |parts| parts.weeks)),
            "days" => Some(self.duration_component_from(row, context, args, |parts| parts.days)),
            "hours" => Some(self.duration_component_from(row, context, args, |parts| parts.hours)),
            "minutes" => Some(
                self.duration_component_from(row, context, args, |parts| parts.minutes_of_hour),
            ),
            "seconds" => Some(
                self.duration_component_from(row, context, args, |parts| parts.seconds_of_minute),
            ),
            _ => None,
        }
    }

    /// Shared body for `year`/`month`/`day`/`quarter`/`week`/`dayofweek`/
    /// `dayofyear`: evaluate the sole argument, resolve it to a
    /// `chrono::NaiveDate` (from a tagged date/localdatetime/datetime value
    /// or a legacy ISO string), and apply `f`. Returns `Null` when the
    /// argument isn't date-like.
    fn date_component_from(
        &self,
        row: &HashMap<String, Value>,
        context: &ExecutionContext,
        args: &[super::super::super::parser::Expression],
        f: impl FnOnce(chrono::NaiveDate) -> i64,
    ) -> Result<Value> {
        let Some(arg) = args.first() else {
            return Ok(Value::Null);
        };
        let value = self.evaluate_projection_expression(row, context, arg)?;

        if let Some((year, month, day)) = temporal_value::date_components(&value) {
            if let Some(date) = chrono::NaiveDate::from_ymd_opt(year, month, day) {
                return Ok(Value::Number(f(date).into()));
            }
        }
        if let Value::String(s) = &value {
            if let Ok(date) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
                return Ok(Value::Number(f(date).into()));
            }
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
                return Ok(Value::Number(f(dt.date_naive()).into()));
            }
        }
        Ok(Value::Null)
    }

    /// Shared body for `hour`/`minute`/`second`/`millisecond`/
    /// `microsecond`/`nanosecond`: evaluate the sole argument, resolve its
    /// `(hour, minute, second, nanosecond)` components from a tagged
    /// time-bearing value or a legacy ISO string, and apply `f`.
    fn time_component_from(
        &self,
        row: &HashMap<String, Value>,
        context: &ExecutionContext,
        args: &[super::super::super::parser::Expression],
        f: impl FnOnce(u32, u32, u32, u32) -> i64,
    ) -> Result<Value> {
        let Some(arg) = args.first() else {
            return Ok(Value::Null);
        };
        let value = self.evaluate_projection_expression(row, context, arg)?;

        if let Some((h, m, s, ns)) = temporal_value::time_components(&value) {
            return Ok(Value::Number(f(h, m, s, ns).into()));
        }
        if let Value::String(s) = &value {
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
                return Ok(Value::Number(
                    f(dt.hour(), dt.minute(), dt.second(), dt.nanosecond()).into(),
                ));
            }
            if let Ok(time) = chrono::NaiveTime::parse_from_str(s, "%H:%M:%S") {
                return Ok(Value::Number(
                    f(time.hour(), time.minute(), time.second(), time.nanosecond()).into(),
                ));
            }
        }
        Ok(Value::Null)
    }

    /// Shared body for the duration component extractors
    /// (`years`/`months`/`days`/`hours`/`minutes`/`seconds`): evaluate the
    /// sole argument, decompose a tagged `duration` via
    /// `temporal_value::duration_parts`, and apply `f`.
    fn duration_component_from(
        &self,
        row: &HashMap<String, Value>,
        context: &ExecutionContext,
        args: &[super::super::super::parser::Expression],
        f: impl FnOnce(temporal_value::DurationParts) -> i64,
    ) -> Result<Value> {
        let Some(arg) = args.first() else {
            return Ok(Value::Null);
        };
        let value = self.evaluate_projection_expression(row, context, arg)?;
        if let Some((months, days, seconds, nanos)) = temporal_value::duration_components(&value) {
            let parts = temporal_value::duration_parts(months, days, seconds, nanos);
            return Ok(Value::Number(f(parts).into()));
        }
        Ok(Value::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(entries: &[(&str, Value)]) -> Map<String, Value> {
        entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn nanosecond_from_map_defaults_to_zero_when_absent() {
        assert_eq!(nanosecond_from_map(&map(&[])).unwrap(), 0);
    }

    #[test]
    fn nanosecond_from_map_accepts_an_in_range_value() {
        assert_eq!(
            nanosecond_from_map(&map(&[("nanosecond", Value::from(645_876_123u64))])).unwrap(),
            645_876_123
        );
    }

    #[test]
    fn nanosecond_from_map_rejects_a_value_of_exactly_one_billion() {
        // BLOCKER 2 probe case: 1_500_000_000 must not silently become
        // "1.5 seconds" (0.15s after a naive mod-1e9/zero-pad) — it is a
        // flatly invalid nanosecond-of-second value.
        let err = nanosecond_from_map(&map(&[("nanosecond", Value::from(1_500_000_000u64))]))
            .unwrap_err();
        assert!(matches!(err, crate::Error::CypherExecution(_)));
    }

    #[test]
    fn nanosecond_from_map_rejects_a_value_far_beyond_u32() {
        // BLOCKER 2 probe case: 4_294_967_297 (u32::MAX + 2) must error,
        // not silently wrap into a small in-range u32.
        let err = nanosecond_from_map(&map(&[("nanosecond", Value::from(4_294_967_297u64))]))
            .unwrap_err();
        assert!(matches!(err, crate::Error::CypherExecution(_)));
    }

    #[test]
    fn nanosecond_from_map_rejects_a_negative_value() {
        // BLOCKER 2 probe case: -1 must error, not silently become 0 (or
        // wrap into a huge positive value).
        let err = nanosecond_from_map(&map(&[("nanosecond", Value::from(-1i64))])).unwrap_err();
        assert!(matches!(err, crate::Error::CypherExecution(_)));
    }

    #[test]
    fn nanosecond_from_map_rejects_a_fractional_value() {
        let err = nanosecond_from_map(&map(&[("nanosecond", Value::from(1.5))])).unwrap_err();
        assert!(matches!(err, crate::Error::CypherExecution(_)));
    }

    #[test]
    fn timezone_from_map_defaults_to_utc_when_absent() {
        assert_eq!(timezone_from_map(&map(&[])).unwrap(), (0, None));
    }

    #[test]
    fn timezone_from_map_accepts_a_numeric_offset() {
        assert_eq!(
            timezone_from_map(&map(&[("timezone", Value::from("+02:00"))])).unwrap(),
            (7200, None)
        );
    }

    #[test]
    fn timezone_from_map_accepts_utc_and_z() {
        assert_eq!(
            timezone_from_map(&map(&[("timezone", Value::from("UTC"))])).unwrap(),
            (0, None)
        );
        assert_eq!(
            timezone_from_map(&map(&[("timezone", Value::from("Z"))])).unwrap(),
            (0, None)
        );
    }

    #[test]
    fn timezone_from_map_rejects_an_unresolvable_named_zone() {
        // MAJOR 1: a named IANA zone can't be resolved to a real offset
        // without a timezone database — must error explicitly instead of
        // silently falling back to UTC while still carrying the
        // unresolved name (the old behaviour rendered the
        // self-contradictory `...Z[Europe/Stockholm]`).
        let err =
            timezone_from_map(&map(&[("timezone", Value::from("Europe/Stockholm"))])).unwrap_err();
        assert!(matches!(err, crate::Error::CypherExecution(_)));
    }
}
