//! Temporal and duration built-in functions for the projection evaluator.
//!
//! Covers `date`, `datetime`, `time`, `timestamp`, `duration`, `duration.*`,
//! `toDate`, `localtime`, `localdatetime`, and temporal component extractors
//! (`year`, `month`, `day`, `hour`, `minute`, `second`, `quarter`, `week`,
//! `dayofweek`, `dayofyear`, `millisecond`, `microsecond`, `nanosecond`).
//! Duration component extractors (`years`, `months`, `weeks`, `days`,
//! `hours`, `minutes`, `seconds`) are also here.

use super::super::super::context::ExecutionContext;
use super::super::super::engine::Executor;
use crate::Result;
use chrono::{Datelike, TimeZone, Timelike};
use serde_json::{Map, Value};
use std::collections::HashMap;

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
                            // Support {year, month, day} format
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
                    // Return current date in ISO format (YYYY-MM-DD)
                    let now = chrono::Local::now();
                    return Some(Ok(Value::String(now.format("%Y-%m-%d").to_string())));
                } else if let Some(arg) = args.first() {
                    // Parse date from string or map
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    match value {
                        Value::String(s) => {
                            // Try to parse ISO date format
                            if let Ok(date) = chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d") {
                                return Some(Ok(Value::String(
                                    date.format("%Y-%m-%d").to_string(),
                                )));
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
            "datetime" => {
                if args.is_empty() {
                    // Return current datetime in ISO format
                    let now = chrono::Local::now();
                    return Some(Ok(Value::String(now.to_rfc3339())));
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
                                return Some(Ok(Value::String(dt.to_rfc3339())));
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
                                return Some(Ok(Value::String(dt_local.to_rfc3339())));
                            }
                        }
                        Value::Object(map) => {
                            // Support {year, month, day, hour, minute, second} format
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

                            if let Some(date) = chrono::NaiveDate::from_ymd_opt(year, month, day) {
                                if let Some(time) =
                                    chrono::NaiveTime::from_hms_opt(hour, minute, second)
                                {
                                    let dt = chrono::NaiveDateTime::new(date, time);
                                    let local = chrono::Local::now().timezone();
                                    let dt_local = local
                                        .from_local_datetime(&dt)
                                        .earliest()
                                        .unwrap_or_else(|| local.from_utc_datetime(&dt));
                                    return Some(Ok(Value::String(dt_local.to_rfc3339())));
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Some(Ok(Value::Null))
            }
            "time" => {
                if args.is_empty() {
                    // Return current time in HH:MM:SS format
                    let now = chrono::Local::now();
                    return Some(Ok(Value::String(now.format("%H:%M:%S").to_string())));
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
                                return Some(Ok(Value::String(
                                    time.format("%H:%M:%S").to_string(),
                                )));
                            }
                            // Try HH:MM format
                            if let Ok(time) = chrono::NaiveTime::parse_from_str(&s, "%H:%M") {
                                return Some(Ok(Value::String(
                                    time.format("%H:%M:%S").to_string(),
                                )));
                            }
                        }
                        Value::Object(map) => {
                            // Support {hour, minute, second} format
                            let hour = map.get("hour").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                            let minute =
                                map.get("minute").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                            let second =
                                map.get("second").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

                            if let Some(time) =
                                chrono::NaiveTime::from_hms_opt(hour, minute, second)
                            {
                                return Some(Ok(Value::String(
                                    time.format("%H:%M:%S").to_string(),
                                )));
                            }
                        }
                        _ => {}
                    }
                }
                Some(Ok(Value::Null))
            }
            "timestamp" => {
                if args.is_empty() {
                    // Return current Unix timestamp in milliseconds
                    let now = chrono::Local::now();
                    let millis = now.timestamp_millis();
                    return Some(Ok(Value::Number(millis.into())));
                } else if let Some(arg) = args.first() {
                    // Parse timestamp from string or return existing number
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    match value {
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
                    if let Value::Object(map) = value {
                        // Support duration components: years, months, days, hours, minutes, seconds
                        let mut duration_map = Map::new();

                        if let Some(years) = map.get("years") {
                            duration_map.insert("years".to_string(), years.clone());
                        }
                        if let Some(months) = map.get("months") {
                            duration_map.insert("months".to_string(), months.clone());
                        }
                        if let Some(days) = map.get("days") {
                            duration_map.insert("days".to_string(), days.clone());
                        }
                        if let Some(hours) = map.get("hours") {
                            duration_map.insert("hours".to_string(), hours.clone());
                        }
                        if let Some(minutes) = map.get("minutes") {
                            duration_map.insert("minutes".to_string(), minutes.clone());
                        }
                        if let Some(seconds) = map.get("seconds") {
                            duration_map.insert("seconds".to_string(), seconds.clone());
                        }

                        return Some(Ok(Value::Object(duration_map)));
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

                            let mut result_map = Map::new();
                            result_map.insert("months".to_string(), Value::Number(months.into()));
                            return Some(Ok(Value::Object(result_map)));
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

                            let mut result_map = Map::new();
                            result_map.insert("days".to_string(), Value::Number(days.into()));
                            return Some(Ok(Value::Object(result_map)));
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

                    if let (Value::String(s1), Value::String(s2)) = (&dt1, &dt2) {
                        // Try parsing as datetimes
                        let d1 = chrono::DateTime::parse_from_rfc3339(s1)
                            .map(|dt| dt.with_timezone(&chrono::Utc));
                        let d2 = chrono::DateTime::parse_from_rfc3339(s2)
                            .map(|dt| dt.with_timezone(&chrono::Utc));

                        if let (Ok(dt1), Ok(dt2)) = (d1, d2) {
                            let seconds = dt1.signed_duration_since(dt2).num_seconds();

                            let mut result_map = Map::new();
                            result_map.insert("seconds".to_string(), Value::Number(seconds.into()));
                            return Some(Ok(Value::Object(result_map)));
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
                    return Some(Ok(Value::String(now.format("%H:%M:%S").to_string())));
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
                                return Some(Ok(Value::String(
                                    time.format("%H:%M:%S").to_string(),
                                )));
                            }
                            // Try HH:MM format
                            if let Ok(time) = chrono::NaiveTime::parse_from_str(&s, "%H:%M") {
                                return Some(Ok(Value::String(
                                    time.format("%H:%M:%S").to_string(),
                                )));
                            }
                        }
                        Value::Object(map) => {
                            let hour = map.get("hour").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                            let minute =
                                map.get("minute").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                            let second =
                                map.get("second").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

                            if let Some(time) =
                                chrono::NaiveTime::from_hms_opt(hour, minute, second)
                            {
                                return Some(Ok(Value::String(
                                    time.format("%H:%M:%S").to_string(),
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
                    return Some(Ok(Value::String(
                        now.format("%Y-%m-%dT%H:%M:%S").to_string(),
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
                                return Some(Ok(Value::String(
                                    dt.format("%Y-%m-%dT%H:%M:%S").to_string(),
                                )));
                            }
                            // Try with timezone and convert to naive
                            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                                return Some(Ok(Value::String(
                                    dt.naive_local().format("%Y-%m-%dT%H:%M:%S").to_string(),
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

                            if let Some(date) = chrono::NaiveDate::from_ymd_opt(year, month, day) {
                                if let Some(time) =
                                    chrono::NaiveTime::from_hms_opt(hour, minute, second)
                                {
                                    let dt = chrono::NaiveDateTime::new(date, time);
                                    return Some(Ok(Value::String(
                                        dt.format("%Y-%m-%dT%H:%M:%S").to_string(),
                                    )));
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Some(Ok(Value::Null))
            }
            // Temporal component extraction functions
            "year" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    match value {
                        Value::String(s) => {
                            // Try to parse date/datetime
                            if let Ok(date) = chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d") {
                                return Some(Ok(Value::Number((date.year() as i64).into())));
                            }
                            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                                return Some(Ok(Value::Number((dt.year() as i64).into())));
                            }
                        }
                        _ => {}
                    }
                }
                Some(Ok(Value::Null))
            }
            "month" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    match value {
                        Value::String(s) => {
                            // Try to parse date/datetime
                            if let Ok(date) = chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d") {
                                return Some(Ok(Value::Number((date.month() as i64).into())));
                            }
                            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                                return Some(Ok(Value::Number((dt.month() as i64).into())));
                            }
                        }
                        _ => {}
                    }
                }
                Some(Ok(Value::Null))
            }
            "day" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    match value {
                        Value::String(s) => {
                            // Try to parse date/datetime
                            if let Ok(date) = chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d") {
                                return Some(Ok(Value::Number((date.day() as i64).into())));
                            }
                            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                                return Some(Ok(Value::Number((dt.day() as i64).into())));
                            }
                        }
                        _ => {}
                    }
                }
                Some(Ok(Value::Null))
            }
            "hour" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    match value {
                        Value::String(s) => {
                            // Try to parse datetime or time
                            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                                return Some(Ok(Value::Number((dt.hour() as i64).into())));
                            }
                            if let Ok(time) = chrono::NaiveTime::parse_from_str(&s, "%H:%M:%S") {
                                return Some(Ok(Value::Number((time.hour() as i64).into())));
                            }
                        }
                        _ => {}
                    }
                }
                Some(Ok(Value::Null))
            }
            "minute" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    match value {
                        Value::String(s) => {
                            // Try to parse datetime or time
                            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                                return Some(Ok(Value::Number((dt.minute() as i64).into())));
                            }
                            if let Ok(time) = chrono::NaiveTime::parse_from_str(&s, "%H:%M:%S") {
                                return Some(Ok(Value::Number((time.minute() as i64).into())));
                            }
                        }
                        _ => {}
                    }
                }
                Some(Ok(Value::Null))
            }
            "second" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    match value {
                        Value::String(s) => {
                            // Try to parse datetime or time
                            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                                return Some(Ok(Value::Number((dt.second() as i64).into())));
                            }
                            if let Ok(time) = chrono::NaiveTime::parse_from_str(&s, "%H:%M:%S") {
                                return Some(Ok(Value::Number((time.second() as i64).into())));
                            }
                        }
                        _ => {}
                    }
                }
                Some(Ok(Value::Null))
            }
            "quarter" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    match value {
                        Value::String(s) => {
                            // Try to parse date/datetime
                            if let Ok(date) = chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d") {
                                let quarter = (date.month() - 1) / 3 + 1;
                                return Some(Ok(Value::Number((quarter as i64).into())));
                            }
                            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                                let quarter = (dt.month() - 1) / 3 + 1;
                                return Some(Ok(Value::Number((quarter as i64).into())));
                            }
                        }
                        _ => {}
                    }
                }
                Some(Ok(Value::Null))
            }
            "week" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    match value {
                        Value::String(s) => {
                            // Try to parse date/datetime
                            if let Ok(date) = chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d") {
                                return Some(Ok(Value::Number(
                                    (date.iso_week().week() as i64).into(),
                                )));
                            }
                            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                                return Some(Ok(Value::Number(
                                    (dt.iso_week().week() as i64).into(),
                                )));
                            }
                        }
                        _ => {}
                    }
                }
                Some(Ok(Value::Null))
            }
            "dayofweek" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    match value {
                        Value::String(s) => {
                            // Try to parse date/datetime
                            if let Ok(date) = chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d") {
                                // Neo4j returns 1-7 (Monday to Sunday)
                                return Some(Ok(Value::Number(
                                    (date.weekday().num_days_from_monday() as i64 + 1).into(),
                                )));
                            }
                            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                                return Some(Ok(Value::Number(
                                    (dt.weekday().num_days_from_monday() as i64 + 1).into(),
                                )));
                            }
                        }
                        _ => {}
                    }
                }
                Some(Ok(Value::Null))
            }
            "dayofyear" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    match value {
                        Value::String(s) => {
                            // Try to parse date/datetime
                            if let Ok(date) = chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d") {
                                return Some(Ok(Value::Number((date.ordinal() as i64).into())));
                            }
                            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                                return Some(Ok(Value::Number((dt.ordinal() as i64).into())));
                            }
                        }
                        _ => {}
                    }
                }
                Some(Ok(Value::Null))
            }
            "millisecond" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    match value {
                        Value::String(s) => {
                            // Try to parse datetime
                            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                                return Some(Ok(Value::Number(
                                    ((dt.timestamp_subsec_millis() % 1000) as i64).into(),
                                )));
                            }
                        }
                        _ => {}
                    }
                }
                Some(Ok(Value::Null))
            }
            "microsecond" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    match value {
                        Value::String(s) => {
                            // Try to parse datetime
                            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                                return Some(Ok(Value::Number(
                                    ((dt.timestamp_subsec_micros() % 1000000) as i64).into(),
                                )));
                            }
                        }
                        _ => {}
                    }
                }
                Some(Ok(Value::Null))
            }
            "nanosecond" => {
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    match value {
                        Value::String(s) => {
                            // Try to parse datetime
                            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                                return Some(Ok(Value::Number(
                                    ((dt.timestamp_subsec_nanos() % 1000000000) as i64).into(),
                                )));
                            }
                        }
                        _ => {}
                    }
                }
                Some(Ok(Value::Null))
            }
            // Duration component extraction functions
            "years" => {
                // years(duration) - extract years component from duration
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if let Value::Object(map) = value {
                        if let Some(years) = map.get("years") {
                            return Some(Ok(years.clone()));
                        }
                    }
                }
                Some(Ok(Value::Null))
            }
            "months" => {
                // months(duration) - extract months component from duration
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if let Value::Object(map) = value {
                        if let Some(months) = map.get("months") {
                            return Some(Ok(months.clone()));
                        }
                    }
                }
                Some(Ok(Value::Null))
            }
            "weeks" => {
                // weeks(duration) - extract weeks component from duration
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if let Value::Object(map) = value {
                        if let Some(weeks) = map.get("weeks") {
                            return Some(Ok(weeks.clone()));
                        }
                    }
                }
                Some(Ok(Value::Null))
            }
            "days" => {
                // days(duration) - extract days component from duration
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if let Value::Object(map) = value {
                        if let Some(days) = map.get("days") {
                            return Some(Ok(days.clone()));
                        }
                    }
                }
                Some(Ok(Value::Null))
            }
            "hours" => {
                // hours(duration) - extract hours component from duration
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if let Value::Object(map) = value {
                        if let Some(hours) = map.get("hours") {
                            return Some(Ok(hours.clone()));
                        }
                    }
                }
                Some(Ok(Value::Null))
            }
            "minutes" => {
                // minutes(duration) - extract minutes component from duration
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if let Value::Object(map) = value {
                        if let Some(minutes) = map.get("minutes") {
                            return Some(Ok(minutes.clone()));
                        }
                    }
                }
                Some(Ok(Value::Null))
            }
            "seconds" => {
                // seconds(duration) - extract seconds component from duration
                if let Some(arg) = args.first() {
                    let value = match self.evaluate_projection_expression(row, context, arg) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                    if let Value::Object(map) = value {
                        if let Some(seconds) = map.get("seconds") {
                            return Some(Ok(seconds.clone()));
                        }
                    }
                }
                Some(Ok(Value::Null))
            }
            _ => None,
        }
    }
}
