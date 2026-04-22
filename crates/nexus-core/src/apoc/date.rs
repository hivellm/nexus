//! `apoc.date.*` — date / time procedures (phase6 apoc §4).

use super::{ApocResult, bad_arg, not_found};
use crate::{Error, Result};
use chrono::{DateTime, Datelike, NaiveDateTime, TimeZone, Timelike, Utc};
use serde_json::{Value, json};

pub fn list() -> &'static [&'static str] {
    &[
        "apoc.date.format",
        "apoc.date.parse",
        "apoc.date.convertFormat",
        "apoc.date.parseAsZonedDateTime",
        "apoc.date.systemTimezone",
        "apoc.date.currentTimestamp",
        "apoc.date.currentMillis",
        "apoc.date.toYears",
        "apoc.date.toMonths",
        "apoc.date.toDays",
        "apoc.date.toHours",
        "apoc.date.toMinutes",
        "apoc.date.toSeconds",
        "apoc.date.add",
        "apoc.date.subtract",
        "apoc.date.fromISO",
        "apoc.date.toISO",
        "apoc.date.yearQuarter",
        "apoc.date.week",
        "apoc.date.weekday",
        "apoc.date.dayOfYear",
        "apoc.date.endOfDay",
        "apoc.date.startOfDay",
        "apoc.date.diff",
        "apoc.date.between",
    ]
}

pub fn dispatch(proc: &str, args: Vec<Value>) -> Result<ApocResult> {
    match proc {
        "format" => format_proc(args),
        "parse" => parse_proc(args),
        "convertFormat" => convert_format(args),
        "parseAsZonedDateTime" => parse_proc(args),
        "systemTimezone" => system_timezone(),
        "currentTimestamp" | "currentMillis" => current_millis(),
        "toYears" => convert_unit(args, Unit::Years),
        "toMonths" => convert_unit(args, Unit::Months),
        "toDays" => convert_unit(args, Unit::Days),
        "toHours" => convert_unit(args, Unit::Hours),
        "toMinutes" => convert_unit(args, Unit::Minutes),
        "toSeconds" => convert_unit(args, Unit::Seconds),
        "add" => add_interval(args, 1),
        "subtract" => add_interval(args, -1),
        "fromISO" => from_iso(args),
        "toISO" => to_iso(args),
        "yearQuarter" => year_quarter(args),
        "week" => iso_week(args),
        "weekday" => weekday(args),
        "dayOfYear" => day_of_year(args),
        "endOfDay" => end_of_day(args),
        "startOfDay" => start_of_day(args),
        "diff" | "between" => diff(args),
        _ => Err(not_found(&format!("apoc.date.{proc}"))),
    }
}

#[derive(Clone, Copy)]
enum Unit {
    Years,
    Months,
    Days,
    Hours,
    Minutes,
    Seconds,
}

fn unit_seconds(u: Unit) -> i64 {
    match u {
        Unit::Seconds => 1,
        Unit::Minutes => 60,
        Unit::Hours => 3600,
        Unit::Days => 86_400,
        Unit::Months => 86_400 * 30,
        Unit::Years => 86_400 * 365,
    }
}

fn arg_i64(proc: &str, args: &[Value], idx: usize) -> Result<i64> {
    args.get(idx)
        .and_then(|v| v.as_i64())
        .ok_or_else(|| bad_arg(proc, &format!("arg {idx} must be INTEGER")))
}

fn arg_str<'a>(proc: &str, args: &'a [Value], idx: usize) -> Result<&'a str> {
    args.get(idx)
        .and_then(|v| v.as_str())
        .ok_or_else(|| bad_arg(proc, &format!("arg {idx} must be STRING")))
}

fn datetime_from_millis(ms: i64) -> DateTime<Utc> {
    let secs = ms.div_euclid(1000);
    let nanos = (ms.rem_euclid(1000) as u32) * 1_000_000;
    Utc.timestamp_opt(secs, nanos)
        .single()
        .unwrap_or_else(|| Utc.timestamp_opt(0, 0).unwrap())
}

fn format_proc(args: Vec<Value>) -> Result<ApocResult> {
    // apoc.date.format(time, unit='ms', format='yyyy-MM-dd HH:mm:ss', tz='UTC')
    let t = arg_i64("apoc.date.format", &args, 0)?;
    let unit = args
        .get(1)
        .and_then(|v| v.as_str())
        .unwrap_or("ms")
        .to_string();
    let fmt = args
        .get(2)
        .and_then(|v| v.as_str())
        .unwrap_or("%Y-%m-%d %H:%M:%S")
        .to_string();
    let fmt = translate_java_format(&fmt);
    let ms = to_millis(t, &unit).map_err(|e| bad_arg("apoc.date.format", &e))?;
    let dt = datetime_from_millis(ms);
    Ok(ApocResult::scalar(Value::String(
        dt.format(&fmt).to_string(),
    )))
}

fn parse_proc(args: Vec<Value>) -> Result<ApocResult> {
    // apoc.date.parse(text, unit='ms', format='yyyy-MM-dd HH:mm:ss')
    let text = arg_str("apoc.date.parse", &args, 0)?.to_string();
    let unit = args
        .get(1)
        .and_then(|v| v.as_str())
        .unwrap_or("ms")
        .to_string();
    let fmt = args
        .get(2)
        .and_then(|v| v.as_str())
        .unwrap_or("%Y-%m-%d %H:%M:%S")
        .to_string();
    let fmt = translate_java_format(&fmt);
    let dt = NaiveDateTime::parse_from_str(&text, &fmt).map_err(|e| {
        bad_arg(
            "apoc.date.parse",
            &format!("parse {text:?} against {fmt:?}: {e}"),
        )
    })?;
    let ms = dt.and_utc().timestamp_millis();
    let value = from_millis(ms, &unit).map_err(|e| bad_arg("apoc.date.parse", &e))?;
    Ok(ApocResult::scalar(Value::Number(value.into())))
}

fn convert_format(args: Vec<Value>) -> Result<ApocResult> {
    // convertFormat(text, currentFormat, targetFormat)
    let text = arg_str("apoc.date.convertFormat", &args, 0)?.to_string();
    let src = args
        .get(1)
        .and_then(|v| v.as_str())
        .unwrap_or("%Y-%m-%d %H:%M:%S")
        .to_string();
    let dst = args
        .get(2)
        .and_then(|v| v.as_str())
        .unwrap_or("%Y-%m-%d")
        .to_string();
    let src = translate_java_format(&src);
    let dst = translate_java_format(&dst);
    let dt = NaiveDateTime::parse_from_str(&text, &src).or_else(|_| {
        chrono::NaiveDate::parse_from_str(&text, &src).map(|d| d.and_hms_opt(0, 0, 0).unwrap())
    });
    let dt = dt.map_err(|e| {
        bad_arg(
            "apoc.date.convertFormat",
            &format!("parse {text:?} against {src:?}: {e}"),
        )
    })?;
    Ok(ApocResult::scalar(Value::String(
        dt.format(&dst).to_string(),
    )))
}

fn system_timezone() -> Result<ApocResult> {
    // Nexus has no OS-timezone dependency today; UTC is the canonical
    // server-side zone and also what Neo4j reports inside Docker.
    Ok(ApocResult::scalar(Value::String("UTC".to_string())))
}

fn current_millis() -> Result<ApocResult> {
    let ms = Utc::now().timestamp_millis();
    Ok(ApocResult::scalar(Value::Number(ms.into())))
}

fn convert_unit(args: Vec<Value>, target: Unit) -> Result<ApocResult> {
    let t = arg_i64("apoc.date.to*", &args, 0)?;
    let unit = args
        .get(1)
        .and_then(|v| v.as_str())
        .unwrap_or("ms")
        .to_string();
    let ms = to_millis(t, &unit).map_err(|e| bad_arg("apoc.date.to*", &e))?;
    let seconds = ms / 1000;
    let out = seconds / unit_seconds(target);
    Ok(ApocResult::scalar(Value::Number(out.into())))
}

fn add_interval(args: Vec<Value>, sign: i64) -> Result<ApocResult> {
    // add/subtract(time, unit, delta, deltaUnit)
    let t = arg_i64("apoc.date.add", &args, 0)?;
    let unit = args
        .get(1)
        .and_then(|v| v.as_str())
        .unwrap_or("ms")
        .to_string();
    let delta = arg_i64("apoc.date.add", &args, 2)?;
    let delta_unit = args
        .get(3)
        .and_then(|v| v.as_str())
        .unwrap_or("s")
        .to_string();
    let ms = to_millis(t, &unit).map_err(|e| bad_arg("apoc.date.add", &e))?;
    let delta_ms = to_millis(delta, &delta_unit).map_err(|e| bad_arg("apoc.date.add", &e))?;
    let out_ms = ms + sign * delta_ms;
    let out = from_millis(out_ms, &unit).map_err(|e| bad_arg("apoc.date.add", &e))?;
    Ok(ApocResult::scalar(Value::Number(out.into())))
}

fn from_iso(args: Vec<Value>) -> Result<ApocResult> {
    let text = arg_str("apoc.date.fromISO", &args, 0)?;
    let dt = DateTime::parse_from_rfc3339(text)
        .map_err(|e| bad_arg("apoc.date.fromISO", &format!("invalid RFC3339: {e}")))?;
    Ok(ApocResult::scalar(Value::Number(
        dt.timestamp_millis().into(),
    )))
}

fn to_iso(args: Vec<Value>) -> Result<ApocResult> {
    let ms = arg_i64("apoc.date.toISO", &args, 0)?;
    let dt = datetime_from_millis(ms);
    Ok(ApocResult::scalar(Value::String(
        dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
    )))
}

fn year_quarter(args: Vec<Value>) -> Result<ApocResult> {
    let ms = arg_i64("apoc.date.yearQuarter", &args, 0)?;
    let dt = datetime_from_millis(ms);
    let quarter = ((dt.month() - 1) / 3) + 1;
    Ok(ApocResult::scalar(Value::Number((quarter as i64).into())))
}

fn iso_week(args: Vec<Value>) -> Result<ApocResult> {
    let ms = arg_i64("apoc.date.week", &args, 0)?;
    let dt = datetime_from_millis(ms);
    let week = dt.iso_week().week();
    Ok(ApocResult::scalar(Value::Number((week as i64).into())))
}

fn weekday(args: Vec<Value>) -> Result<ApocResult> {
    let ms = arg_i64("apoc.date.weekday", &args, 0)?;
    let dt = datetime_from_millis(ms);
    // Neo4j uses Monday=1..Sunday=7.
    let wd = dt.weekday().number_from_monday();
    Ok(ApocResult::scalar(Value::Number((wd as i64).into())))
}

fn day_of_year(args: Vec<Value>) -> Result<ApocResult> {
    let ms = arg_i64("apoc.date.dayOfYear", &args, 0)?;
    let dt = datetime_from_millis(ms);
    Ok(ApocResult::scalar(Value::Number(
        (dt.ordinal() as i64).into(),
    )))
}

fn end_of_day(args: Vec<Value>) -> Result<ApocResult> {
    let ms = arg_i64("apoc.date.endOfDay", &args, 0)?;
    let dt = datetime_from_millis(ms);
    let eod = dt
        .with_hour(23)
        .and_then(|d| d.with_minute(59))
        .and_then(|d| d.with_second(59))
        .and_then(|d| d.with_nanosecond(999_000_000))
        .unwrap_or(dt);
    Ok(ApocResult::scalar(Value::Number(
        eod.timestamp_millis().into(),
    )))
}

fn start_of_day(args: Vec<Value>) -> Result<ApocResult> {
    let ms = arg_i64("apoc.date.startOfDay", &args, 0)?;
    let dt = datetime_from_millis(ms);
    let sod = dt
        .with_hour(0)
        .and_then(|d| d.with_minute(0))
        .and_then(|d| d.with_second(0))
        .and_then(|d| d.with_nanosecond(0))
        .unwrap_or(dt);
    Ok(ApocResult::scalar(Value::Number(
        sod.timestamp_millis().into(),
    )))
}

fn diff(args: Vec<Value>) -> Result<ApocResult> {
    // diff(from, to, unit) — returns (to - from) in `unit`.
    let from = arg_i64("apoc.date.diff", &args, 0)?;
    let to = arg_i64("apoc.date.diff", &args, 1)?;
    let unit = args
        .get(2)
        .and_then(|v| v.as_str())
        .unwrap_or("ms")
        .to_string();
    let delta_ms = to - from;
    let out = from_millis(delta_ms, &unit).map_err(|e| bad_arg("apoc.date.diff", &e))?;
    Ok(ApocResult::scalar(Value::Number(out.into())))
}

// ─────────────────── unit conversions ──────────────────────

fn to_millis(value: i64, unit: &str) -> std::result::Result<i64, String> {
    let factor: i64 = match unit {
        "ms" | "milliseconds" => 1,
        "s" | "seconds" => 1000,
        "m" | "minutes" => 60 * 1000,
        "h" | "hours" => 3600 * 1000,
        "d" | "days" => 86_400 * 1000,
        other => return Err(format!("unknown unit {other:?}")),
    };
    Ok(value.saturating_mul(factor))
}

fn from_millis(ms: i64, unit: &str) -> std::result::Result<i64, String> {
    let divisor: i64 = match unit {
        "ms" | "milliseconds" => 1,
        "s" | "seconds" => 1000,
        "m" | "minutes" => 60 * 1000,
        "h" | "hours" => 3600 * 1000,
        "d" | "days" => 86_400 * 1000,
        other => return Err(format!("unknown unit {other:?}")),
    };
    Ok(ms / divisor)
}

/// Translate Java-style date format tokens (as APOC accepts them)
/// into chrono's strftime-style tokens.
///
/// Mapping:
///
/// - `yyyy` → `%Y`
/// - `yy`   → `%y`
/// - `MM`   → `%m`
/// - `dd`   → `%d`
/// - `HH`   → `%H`
/// - `mm`   → `%M`
/// - `ss`   → `%S`
fn translate_java_format(fmt: &str) -> String {
    let mut out = String::with_capacity(fmt.len());
    let mut rest = fmt;
    while !rest.is_empty() {
        if let Some(stripped) = rest.strip_prefix("yyyy") {
            out.push_str("%Y");
            rest = stripped;
        } else if let Some(stripped) = rest.strip_prefix("yy") {
            out.push_str("%y");
            rest = stripped;
        } else if let Some(stripped) = rest.strip_prefix("MM") {
            out.push_str("%m");
            rest = stripped;
        } else if let Some(stripped) = rest.strip_prefix("dd") {
            out.push_str("%d");
            rest = stripped;
        } else if let Some(stripped) = rest.strip_prefix("HH") {
            out.push_str("%H");
            rest = stripped;
        } else if let Some(stripped) = rest.strip_prefix("mm") {
            out.push_str("%M");
            rest = stripped;
        } else if let Some(stripped) = rest.strip_prefix("ss") {
            out.push_str("%S");
            rest = stripped;
        } else {
            let mut chars = rest.chars();
            if let Some(c) = chars.next() {
                out.push(c);
                rest = chars.as_str();
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(proc: &str, args: Vec<Value>) -> Value {
        dispatch(proc, args).unwrap().rows[0][0].clone()
    }

    #[test]
    fn format_yyyy_mm_dd() {
        // 2021-01-15 00:00:00 UTC = 1610668800000 ms
        assert_eq!(
            call(
                "format",
                vec![
                    json!(1_610_668_800_000i64),
                    json!("ms"),
                    json!("yyyy-MM-dd")
                ]
            ),
            json!("2021-01-15")
        );
    }

    #[test]
    fn parse_roundtrips_through_format() {
        let parsed = call(
            "parse",
            vec![
                json!("2021-01-15 12:30:45"),
                json!("ms"),
                json!("yyyy-MM-dd HH:mm:ss"),
            ],
        );
        let back = call(
            "format",
            vec![parsed.clone(), json!("ms"), json!("yyyy-MM-dd HH:mm:ss")],
        );
        assert_eq!(back, json!("2021-01-15 12:30:45"));
    }

    #[test]
    fn current_millis_is_positive_and_recent() {
        let now = call("currentMillis", vec![]).as_i64().unwrap();
        // Well past Jan 1 2020.
        assert!(now > 1_577_836_800_000);
    }

    #[test]
    fn system_timezone_is_utc() {
        assert_eq!(call("systemTimezone", vec![]), json!("UTC"));
    }

    #[test]
    fn to_days_from_ms() {
        // 2 days worth of ms.
        assert_eq!(
            call("toDays", vec![json!(2 * 86_400_000i64), json!("ms")]),
            json!(2)
        );
    }

    #[test]
    fn add_hours() {
        // 0 ms + 2 h = 2h in ms = 7_200_000.
        assert_eq!(
            call("add", vec![json!(0), json!("ms"), json!(2), json!("h")]),
            json!(7_200_000)
        );
    }

    #[test]
    fn subtract_days() {
        assert_eq!(
            call(
                "subtract",
                vec![json!(86_400_000), json!("ms"), json!(1), json!("d")]
            ),
            json!(0)
        );
    }

    #[test]
    fn from_iso_to_millis() {
        let ms = call("fromISO", vec![json!("2021-01-15T00:00:00Z")]);
        assert_eq!(ms, json!(1_610_668_800_000i64));
    }

    #[test]
    fn to_iso_formats_rfc3339() {
        let s = call("toISO", vec![json!(1_610_668_800_000i64)]);
        assert_eq!(s, json!("2021-01-15T00:00:00.000Z"));
    }

    #[test]
    fn year_quarter_classification() {
        // Jan → Q1, Apr → Q2, Jul → Q3, Oct → Q4.
        assert_eq!(
            call("yearQuarter", vec![json!(1_610_668_800_000i64)]),
            json!(1)
        );
        let apr = call(
            "parse",
            vec![
                json!("2021-04-01 00:00:00"),
                json!("ms"),
                json!("yyyy-MM-dd HH:mm:ss"),
            ],
        );
        assert_eq!(call("yearQuarter", vec![apr]), json!(2));
    }

    #[test]
    fn weekday_is_monday_based() {
        // 2021-01-15 is a Friday → 5.
        assert_eq!(call("weekday", vec![json!(1_610_668_800_000i64)]), json!(5));
    }

    #[test]
    fn day_of_year_is_ordinal() {
        // 2021-01-15 → day 15.
        assert_eq!(
            call("dayOfYear", vec![json!(1_610_668_800_000i64)]),
            json!(15)
        );
    }

    #[test]
    fn start_and_end_of_day_bracket_midnight() {
        let start = call("startOfDay", vec![json!(1_610_668_800_000i64 + 12_345)]);
        let end = call("endOfDay", vec![json!(1_610_668_800_000i64 + 12_345)]);
        assert_eq!(start, json!(1_610_668_800_000i64));
        // End-of-day is 23:59:59.999 → start + 86_400_000 - 1.
        assert_eq!(end, json!(1_610_668_800_000i64 + 86_400_000 - 1));
    }

    #[test]
    fn diff_between_two_millis_values() {
        let from = 1_610_668_800_000i64;
        let to = from + 86_400_000 * 3;
        assert_eq!(
            call("diff", vec![json!(from), json!(to), json!("d")]),
            json!(3)
        );
    }

    #[test]
    fn convert_format_reshapes_date_string() {
        assert_eq!(
            call(
                "convertFormat",
                vec![
                    json!("2021-01-15"),
                    json!("yyyy-MM-dd"),
                    json!("dd/MM/yyyy")
                ]
            ),
            json!("15/01/2021")
        );
    }
}
