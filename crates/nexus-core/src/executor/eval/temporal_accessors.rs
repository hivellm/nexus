//! Property-access accessor table for tagged temporal values.
//!
//! Cypher exposes a temporal instant's or duration's derived components
//! through property access (`d.year`, `d.dayOfQuarter`,
//! `d.secondsOfMinute`, …), not function calls — see the openCypher TCK's
//! `Temporal5.feature` ("Access Components of Temporal Values"). This
//! module is the single implementation both
//! `super::super::projection::core`'s `PropertyAccess` dispatch and any
//! future caller reuse; it never duplicates the component math already
//! shared with canonical rendering in [`super::temporal_value`]
//! (`date_components`, `time_components`, `duration_components`,
//! `duration_parts`).
//!
//! [`temporal_property`] is the sole entry point: it returns `Value::Null`
//! for any accessor name that doesn't apply to the value's kind (e.g.
//! `d.hour` on a `date`, or an accessor unknown to the openCypher/Neo4j
//! surface) — the openCypher TCK never expects an error for those, only a
//! `Null` (mirroring the existing behaviour of a missing key on a plain
//! map).

use super::temporal_value::{self, TemporalKind};
use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime};
use serde_json::Value;

/// Looks up a derived component name on a tagged temporal value (any of
/// `date`/`localtime`/`localdatetime`/`time`/`datetime`/`duration`).
/// Returns `Value::Null` if `value` isn't a tagged temporal at all, or if
/// `property` doesn't name a component that applies to its kind.
pub(in crate::executor) fn temporal_property(value: &Value, property: &str) -> Value {
    let Some(kind) = temporal_value::temporal_kind(value) else {
        return Value::Null;
    };
    if kind == TemporalKind::Duration {
        return duration_property(value, property);
    }
    instant_property(kind, value, property)
}

/// Dispatches a `date`/`localtime`/`localdatetime`/`time`/`datetime`
/// accessor to whichever component family(ies) that kind actually
/// carries: the date part (`date`, `localdatetime`, `datetime`), the
/// time part (everything but `date`), the offset part (`time`,
/// `datetime`), and the zone/epoch part (`datetime` only).
fn instant_property(kind: TemporalKind, value: &Value, property: &str) -> Value {
    let has_date = matches!(
        kind,
        TemporalKind::Date | TemporalKind::LocalDateTime | TemporalKind::DateTime
    );
    let has_time = kind != TemporalKind::Date;
    let has_offset = matches!(kind, TemporalKind::Time | TemporalKind::DateTime);
    let has_epoch = kind == TemporalKind::DateTime;

    if has_date {
        if let Some(v) = date_property(value, property) {
            return v;
        }
    }
    if has_time {
        if let Some(v) = time_property(value, property) {
            return v;
        }
    }
    if has_offset {
        if let Some(v) = offset_property(value, property) {
            return v;
        }
    }
    if has_epoch {
        if let Some(v) = epoch_property(value, property) {
            return v;
        }
    }
    Value::Null
}

/// The date-part accessors shared by `date`, `localdatetime`, and
/// `datetime`. Verified against `Temporal5.feature` scenarios 1, 2, 5, 6:
///
/// - `date({year: 1984, month: 10, day: 11})` -> `year: 1984, quarter: 4,
///   month: 10, week: 41, weekYear: 1984, day: 11, ordinalDay: 285,
///   weekDay: 4, dayOfQuarter: 11`
/// - `date({year: 1984, month: 1, day: 1})` -> `weekYear: 1983, week: 52,
///   weekDay: 7` (Jan 1 1984 was a Sunday, ISO week 52 of the *previous*
///   year — the reason `weekYear` and `year` can disagree).
fn date_property(value: &Value, property: &str) -> Option<Value> {
    let (year, month, day) = temporal_value::date_components(value)?;
    let date = NaiveDate::from_ymd_opt(year, month, day)?;
    let n: i64 = match property {
        "year" => i64::from(year),
        "month" => i64::from(month),
        "day" => i64::from(day),
        "quarter" => i64::from((month - 1) / 3 + 1),
        "week" => i64::from(date.iso_week().week()),
        "weekYear" => i64::from(date.iso_week().year()),
        "ordinalDay" => i64::from(date.ordinal()),
        // Neo4j's `dayOfWeek` and `weekDay` are documented synonyms (ISO
        // weekday, Monday = 1 .. Sunday = 7) — matches the top-level
        // `dayofweek()` extractor in `fn_temporal.rs`.
        "weekDay" | "dayOfWeek" => date.weekday().num_days_from_monday() as i64 + 1,
        "dayOfQuarter" => day_of_quarter(year, month, date),
        _ => return None,
    };
    Some(Value::Number(n.into()))
}

/// Day count within `date`'s quarter, 1-based (`date` itself is the
/// quarter's first day when `date_of_quarter == 1`). Scenario 5's
/// `localdatetime({month: 11, day: 11, …})` (Q4, which starts Oct 1) ->
/// `dayOfQuarter: 42` (31 October days + 11 November days).
fn day_of_quarter(year: i32, month: u32, date: NaiveDate) -> i64 {
    let quarter_start_month = (month - 1) / 3 * 3 + 1;
    let quarter_start = NaiveDate::from_ymd_opt(year, quarter_start_month, 1).unwrap_or(date);
    date.signed_duration_since(quarter_start).num_days() + 1
}

/// The time-part accessors shared by `localtime`, `time`,
/// `localdatetime`, and `datetime`. Verified against `Temporal5.feature`
/// scenario 3: `localtime({hour: 12, minute: 31, second: 14, nanosecond:
/// 645876123})` -> `hour: 12, minute: 31, second: 14, millisecond: 645,
/// microsecond: 645876, nanosecond: 645876123`.
fn time_property(value: &Value, property: &str) -> Option<Value> {
    let (hour, minute, second, nanosecond) = temporal_value::time_components(value)?;
    let n: i64 = match property {
        "hour" => i64::from(hour),
        "minute" => i64::from(minute),
        "second" => i64::from(second),
        "millisecond" => i64::from(nanosecond / 1_000_000),
        "microsecond" => i64::from(nanosecond / 1_000),
        "nanosecond" => i64::from(nanosecond),
        _ => return None,
    };
    Some(Value::Number(n.into()))
}

/// The offset/zone-string accessors shared by `time` and `datetime`.
/// `timezone` renders the same way for both: the named zone if the value
/// carries one (`datetime` only — see [`temporal_value::zone_name`]),
/// otherwise the numeric offset string — matching scenario 4's `time(…
/// timezone: '+01:00')` -> `timezone: '+01:00', offset: '+01:00',
/// offsetMinutes: 60, offsetSeconds: 3600` and scenario 6's
/// `datetime(… timezone: 'Europe/Stockholm')` -> `timezone:
/// 'Europe/Stockholm', offset: '+01:00'`.
fn offset_property(value: &Value, property: &str) -> Option<Value> {
    let offset = temporal_value::offset_seconds(value)?;
    match property {
        "offset" => Some(Value::String(temporal_value::render_offset(offset))),
        "offsetMinutes" => Some(Value::Number((i64::from(offset) / 60).into())),
        "offsetSeconds" => Some(Value::Number(i64::from(offset).into())),
        "timezone" => {
            let rendered = temporal_value::zone_name(value)
                .map(str::to_string)
                .unwrap_or_else(|| temporal_value::render_offset(offset));
            Some(Value::String(rendered))
        }
        _ => None,
    }
}

/// The `datetime`-only epoch accessors. Scenario 6:
/// `datetime({year: 1984, month: 11, day: 11, hour: 12, minute: 31,
/// second: 14, nanosecond: 645876123, timezone: 'Europe/Stockholm'})` (a
/// `+01:00` offset) -> `epochSeconds: 469020674, epochMillis:
/// 469020674645`. The UTC instant is the local civil time reinterpreted
/// as UTC, then shifted back by the offset (`local_as_utc - offset`) —
/// the standard definition of "the offset a wall-clock reading is
/// relative to UTC".
fn epoch_property(value: &Value, property: &str) -> Option<Value> {
    match property {
        "epochSeconds" => epoch_seconds(value).map(|s| Value::Number(s.into())),
        "epochMillis" => epoch_millis(value).map(|m| Value::Number(m.into())),
        _ => None,
    }
}

fn epoch_seconds(value: &Value) -> Option<i64> {
    let (year, month, day) = temporal_value::date_components(value)?;
    let (hour, minute, second, _nanosecond) = temporal_value::time_components(value)?;
    let offset = temporal_value::offset_seconds(value)?;
    let date = NaiveDate::from_ymd_opt(year, month, day)?;
    let time = NaiveTime::from_hms_opt(hour, minute, second)?;
    let naive = NaiveDateTime::new(date, time);
    Some(naive.and_utc().timestamp() - i64::from(offset))
}

fn epoch_millis(value: &Value) -> Option<i64> {
    let seconds = epoch_seconds(value)?;
    let (_, _, _, nanosecond) = temporal_value::time_components(value)?;
    let millis_of_second = i64::from(nanosecond / 1_000_000);
    Some(
        seconds
            .saturating_mul(1_000)
            .saturating_add(millis_of_second),
    )
}

/// The full `duration` accessor table: eleven "total in this unit"
/// accessors (`years` .. `nanoseconds`) plus their "remainder within the
/// parent unit" (`…OfYear`/`…OfQuarter`/`…OfWeek`/`…OfHour`/`…OfMinute`/
/// `…OfSecond`) counterparts. Verified against `Temporal5.feature`
/// scenario 7 and `Temporal10.feature`'s `duration.between(...).days` /
/// `.seconds` / `.nanosecondsOfSecond` reads:
///
/// `duration({years: 1, months: 4, days: 10, hours: 1, minutes: 1,
/// seconds: 1, nanoseconds: 111111111})` — internally `(months: 16,
/// days: 10, seconds: 3661, nanos: 111111111)` — yields `years: 1,
/// quarters: 5, months: 16, weeks: 1, days: 10, hours: 1, minutes: 61,
/// seconds: 3661, milliseconds: 3661111, microseconds: 3661111111,
/// nanoseconds: 3661111111111, quartersOfYear: 1, monthsOfQuarter: 1,
/// monthsOfYear: 4, daysOfWeek: 3, minutesOfHour: 1, secondsOfMinute: 1,
/// millisecondsOfSecond: 111, microsecondsOfSecond: 111111,
/// nanosecondsOfSecond: 111111111`.
///
/// Note the total/remainder split is *not* uniform across units:
/// `months`/`days`/`seconds` are themselves already Neo4j's internal
/// duration fields (`temporal_value::duration_components`), so the
/// "total" reading is the raw field — but `minutes` (61) and `hours` (1,
/// via `duration_parts.hours`) are *derived* totals computed from the
/// total `seconds`, distinct from `minutesOfHour`/`secondsOfMinute`
/// (the within-the-hour/within-the-minute remainders).
fn duration_property(value: &Value, property: &str) -> Value {
    let Some((months, days, seconds, nanos)) = temporal_value::duration_components(value) else {
        return Value::Null;
    };
    let parts = temporal_value::duration_parts(months, days, seconds, nanos);
    // Reconstructing `seconds_total`/`nanos` from the normalized `parts`
    // (rather than trusting the raw `seconds`/`nanos` arguments) keeps
    // every total in agreement with `duration_parts`'s own defensive
    // re-normalization — see its doc comment for why a caller must not
    // assume its inputs are already normalized.
    let seconds_total = parts.hours * 3_600 + parts.minutes_of_hour * 60 + parts.seconds_of_minute;
    let nanos = i64::from(parts.nanos);

    let n: i64 = match property {
        "years" => parts.years,
        "quarters" => months / 3,
        "months" => months,
        "weeks" => parts.weeks,
        "days" => days,
        "hours" => parts.hours,
        "minutes" => seconds_total / 60,
        "seconds" => seconds_total,
        "milliseconds" => seconds_total
            .saturating_mul(1_000)
            .saturating_add(nanos / 1_000_000),
        "microseconds" => seconds_total
            .saturating_mul(1_000_000)
            .saturating_add(nanos / 1_000),
        "nanoseconds" => seconds_total
            .saturating_mul(1_000_000_000)
            .saturating_add(nanos),
        "quartersOfYear" => parts.months_of_year / 3,
        "monthsOfQuarter" => parts.months_of_year % 3,
        "monthsOfYear" => parts.months_of_year,
        "daysOfWeek" => days % 7,
        "minutesOfHour" => parts.minutes_of_hour,
        "secondsOfMinute" => parts.seconds_of_minute,
        "millisecondsOfSecond" => nanos / 1_000_000,
        "microsecondsOfSecond" => nanos / 1_000,
        "nanosecondsOfSecond" => nanos,
        _ => return Value::Null,
    };
    Value::Number(n.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::eval::temporal_value::{
        make_date, make_datetime, make_duration, make_localdatetime, make_localtime, make_time,
    };

    #[test]
    fn non_temporal_value_yields_null() {
        let v = serde_json::json!({"year": 1984});
        assert_eq!(temporal_property(&v, "year"), Value::Null);
    }

    #[test]
    fn unknown_accessor_on_a_temporal_yields_null() {
        let d = make_date(2020, 1, 1);
        assert_eq!(temporal_property(&d, "notARealAccessor"), Value::Null);
    }

    #[test]
    fn date_accessors_match_tck_scenario_1() {
        let d = make_date(1984, 10, 11);
        assert_eq!(temporal_property(&d, "year"), Value::Number(1984.into()));
        assert_eq!(temporal_property(&d, "quarter"), Value::Number(4.into()));
        assert_eq!(temporal_property(&d, "month"), Value::Number(10.into()));
        assert_eq!(temporal_property(&d, "week"), Value::Number(41.into()));
        assert_eq!(
            temporal_property(&d, "weekYear"),
            Value::Number(1984.into())
        );
        assert_eq!(temporal_property(&d, "day"), Value::Number(11.into()));
        assert_eq!(
            temporal_property(&d, "ordinalDay"),
            Value::Number(285.into())
        );
        assert_eq!(temporal_property(&d, "weekDay"), Value::Number(4.into()));
        assert_eq!(temporal_property(&d, "dayOfWeek"), Value::Number(4.into()));
        assert_eq!(
            temporal_property(&d, "dayOfQuarter"),
            Value::Number(11.into())
        );
        // Time-part accessors don't apply to a bare `date`.
        assert_eq!(temporal_property(&d, "hour"), Value::Null);
    }

    #[test]
    fn date_accessors_match_tck_scenario_2_week_year_boundary() {
        let d = make_date(1984, 1, 1);
        assert_eq!(temporal_property(&d, "year"), Value::Number(1984.into()));
        assert_eq!(
            temporal_property(&d, "weekYear"),
            Value::Number(1983.into())
        );
        assert_eq!(temporal_property(&d, "week"), Value::Number(52.into()));
        assert_eq!(temporal_property(&d, "weekDay"), Value::Number(7.into()));
    }

    #[test]
    fn localtime_accessors_match_tck_scenario_3() {
        let t = make_localtime(12, 31, 14, 645_876_123);
        assert_eq!(temporal_property(&t, "hour"), Value::Number(12.into()));
        assert_eq!(temporal_property(&t, "minute"), Value::Number(31.into()));
        assert_eq!(temporal_property(&t, "second"), Value::Number(14.into()));
        assert_eq!(
            temporal_property(&t, "millisecond"),
            Value::Number(645.into())
        );
        assert_eq!(
            temporal_property(&t, "microsecond"),
            Value::Number(645_876.into())
        );
        assert_eq!(
            temporal_property(&t, "nanosecond"),
            Value::Number(645_876_123.into())
        );
        // Date-part accessors don't apply to a bare `localtime`.
        assert_eq!(temporal_property(&t, "year"), Value::Null);
    }

    #[test]
    fn time_accessors_match_tck_scenario_4() {
        let t = make_time(12, 31, 14, 645_876_123, 3600);
        assert_eq!(
            temporal_property(&t, "timezone"),
            Value::String("+01:00".to_string())
        );
        assert_eq!(
            temporal_property(&t, "offset"),
            Value::String("+01:00".to_string())
        );
        assert_eq!(
            temporal_property(&t, "offsetMinutes"),
            Value::Number(60.into())
        );
        assert_eq!(
            temporal_property(&t, "offsetSeconds"),
            Value::Number(3600.into())
        );
    }

    #[test]
    fn localdatetime_accessors_match_tck_scenario_5() {
        let d = make_localdatetime(1984, 11, 11, 12, 31, 14, 645_876_123);
        assert_eq!(temporal_property(&d, "week"), Value::Number(45.into()));
        assert_eq!(
            temporal_property(&d, "dayOfQuarter"),
            Value::Number(42.into())
        );
        assert_eq!(
            temporal_property(&d, "ordinalDay"),
            Value::Number(316.into())
        );
        assert_eq!(temporal_property(&d, "weekDay"), Value::Number(7.into()));
        assert_eq!(
            temporal_property(&d, "millisecond"),
            Value::Number(645.into())
        );
    }

    #[test]
    fn datetime_accessors_match_tck_scenario_6() {
        let dt = make_datetime(
            1984,
            11,
            11,
            12,
            31,
            14,
            645_876_123,
            3600,
            Some("Europe/Stockholm".to_string()),
        );
        assert_eq!(
            temporal_property(&dt, "timezone"),
            Value::String("Europe/Stockholm".to_string())
        );
        assert_eq!(
            temporal_property(&dt, "offset"),
            Value::String("+01:00".to_string())
        );
        assert_eq!(
            temporal_property(&dt, "offsetMinutes"),
            Value::Number(60.into())
        );
        assert_eq!(
            temporal_property(&dt, "offsetSeconds"),
            Value::Number(3600.into())
        );
        assert_eq!(
            temporal_property(&dt, "epochSeconds"),
            Value::Number(469_020_674i64.into())
        );
        assert_eq!(
            temporal_property(&dt, "epochMillis"),
            Value::Number(469_020_674_645i64.into())
        );
    }

    #[test]
    fn duration_accessors_match_tck_scenario_7() {
        // years: 1, months: 4, days: 10, hours: 1, minutes: 1, seconds: 1,
        // nanoseconds: 111111111 -> internal (months: 16, days: 10,
        // seconds: 3661, nanos: 111111111).
        let d = make_duration(12 + 4, 10, 3600 + 60 + 1, 111_111_111);
        assert_eq!(temporal_property(&d, "years"), Value::Number(1.into()));
        assert_eq!(temporal_property(&d, "quarters"), Value::Number(5.into()));
        assert_eq!(temporal_property(&d, "months"), Value::Number(16.into()));
        assert_eq!(temporal_property(&d, "weeks"), Value::Number(1.into()));
        assert_eq!(temporal_property(&d, "days"), Value::Number(10.into()));
        assert_eq!(temporal_property(&d, "hours"), Value::Number(1.into()));
        assert_eq!(temporal_property(&d, "minutes"), Value::Number(61.into()));
        assert_eq!(temporal_property(&d, "seconds"), Value::Number(3661.into()));
        assert_eq!(
            temporal_property(&d, "milliseconds"),
            Value::Number(3_661_111i64.into())
        );
        assert_eq!(
            temporal_property(&d, "microseconds"),
            Value::Number(3_661_111_111i64.into())
        );
        assert_eq!(
            temporal_property(&d, "nanoseconds"),
            Value::Number(3_661_111_111_111i64.into())
        );
        assert_eq!(
            temporal_property(&d, "quartersOfYear"),
            Value::Number(1.into())
        );
        assert_eq!(
            temporal_property(&d, "monthsOfQuarter"),
            Value::Number(1.into())
        );
        assert_eq!(
            temporal_property(&d, "monthsOfYear"),
            Value::Number(4.into())
        );
        assert_eq!(temporal_property(&d, "daysOfWeek"), Value::Number(3.into()));
        assert_eq!(
            temporal_property(&d, "minutesOfHour"),
            Value::Number(1.into())
        );
        assert_eq!(
            temporal_property(&d, "secondsOfMinute"),
            Value::Number(1.into())
        );
        assert_eq!(
            temporal_property(&d, "millisecondsOfSecond"),
            Value::Number(111.into())
        );
        assert_eq!(
            temporal_property(&d, "microsecondsOfSecond"),
            Value::Number(111_111.into())
        );
        assert_eq!(
            temporal_property(&d, "nanosecondsOfSecond"),
            Value::Number(111_111_111.into())
        );
        // Instant-only accessors don't apply to a `duration`.
        assert_eq!(temporal_property(&d, "year"), Value::Null);
    }

    #[test]
    fn duration_accessors_truncate_negative_seconds_toward_zero() {
        // No TCK row pins the total/remainder split's sign convention for
        // a negative duration — this future-proofs it against
        // `duration_parts`'s own documented convention (Rust's default
        // division: truncating toward zero, remainder keeps the
        // dividend's sign). `duration({seconds: -90})` -> `hours: 0`
        // (`-90 / 3600` truncates to `0`), `minutes: -1` (total minutes,
        // `-90 / 60` truncates to `-1`), `seconds: -90` (the raw total),
        // `minutesOfHour: -1` and `secondsOfMinute: -30` (`-90 % 3600 ==
        // -90`, then `-90 / 60 == -1` and `-90 % 60 == -30`).
        let d = make_duration(0, 0, -90, 0);
        assert_eq!(temporal_property(&d, "hours"), Value::Number(0.into()));
        assert_eq!(temporal_property(&d, "minutes"), Value::Number((-1).into()));
        assert_eq!(
            temporal_property(&d, "seconds"),
            Value::Number((-90).into())
        );
        assert_eq!(
            temporal_property(&d, "minutesOfHour"),
            Value::Number((-1).into())
        );
        assert_eq!(
            temporal_property(&d, "secondsOfMinute"),
            Value::Number((-30).into())
        );
    }
}
