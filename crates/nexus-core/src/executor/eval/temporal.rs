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

impl Executor {
    pub(in crate::executor) fn is_duration_object(value: &Value) -> bool {
        temporal_value::is_duration(value)
    }

    /// True when `value` is usable as the instant side of datetime
    /// arithmetic: either a tagged temporal instant, or (backward
    /// compatibility for any code path that still hands this a raw
    /// literal) a plain ISO date/datetime string.
    pub(in crate::executor) fn is_datetime_string(value: &Value) -> bool {
        if temporal_value::is_temporal_instant(value) {
            return true;
        }
        if let Value::String(s) = value {
            chrono::DateTime::parse_from_rfc3339(s).is_ok()
                || chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").is_ok()
                || chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()
        } else {
            false
        }
    }

    /// Extract duration components as (years, months, days, hours, minutes, seconds).
    ///
    /// Reads a tagged `duration` value's normalized `(months, days,
    /// seconds, nanos)` fields and re-expands them into this legacy
    /// six-component shape (`years` always `0` — the tagged value already
    /// folds years into `months`) so the arithmetic below, which predates
    /// the typed-value representation, needs no further changes. Sub-second
    /// precision (`nanos`) has no slot in this shape and is truncated —
    /// full nanosecond-precision arithmetic is out of scope for the typed-
    /// value gate (see `docs/analysis/tck/03-temporal.md` F-019).
    pub(in crate::executor) fn extract_duration_components(
        value: &Value,
    ) -> (i64, i64, i64, i64, i64, i64) {
        if let Some((months, days, seconds, _nanos)) = temporal_value::duration_components(value) {
            let hours = seconds / 3600;
            let rem = seconds % 3600;
            let minutes = rem / 60;
            let secs = rem % 60;
            return (0, months, days, hours, minutes, secs);
        }
        (0, 0, 0, 0, 0, 0)
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
            temporal_value::duration_components(left),
            temporal_value::duration_components(right),
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
            temporal_value::duration_components(left),
            temporal_value::duration_components(right),
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
        let (years, months, days, hours, minutes, seconds) =
            Self::extract_duration_components(duration);

        if let Value::String(dt_str) = &datetime {
            // Try RFC3339 format first
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(dt_str) {
                let mut result = dt.with_timezone(&chrono::Utc);

                // Add years and months using checked arithmetic
                if years != 0 || months != 0 {
                    let total_months = checked_total_months(years, months)?;
                    let (final_year, final_month) =
                        checked_month_rollover(result.year(), result.month(), total_months)?;

                    if let Some(new_dt) = result
                        .with_year(final_year)
                        .and_then(|d| d.with_month(final_month))
                    {
                        result = new_dt;
                    }
                }

                // Add days, hours, minutes, seconds
                let duration_secs = checked_duration_secs(days, hours, minutes, seconds)?;
                let delta = chrono::Duration::try_seconds(duration_secs).ok_or_else(|| {
                    Error::CypherExecution(
                        "duration arithmetic overflow: seconds component is outside chrono's representable range".to_string(),
                    )
                })?;
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

                    if let Some(new_dt) = result
                        .with_year(final_year)
                        .and_then(|d| d.with_month(final_month))
                    {
                        result = new_dt;
                    }
                }

                // Add days, hours, minutes, seconds
                let duration_secs = checked_duration_secs(days, hours, minutes, seconds)?;
                let delta = chrono::Duration::try_seconds(duration_secs).ok_or_else(|| {
                    Error::CypherExecution(
                        "duration arithmetic overflow: seconds component is outside chrono's representable range".to_string(),
                    )
                })?;
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

                    if let Some(new_dt) = result
                        .with_year(final_year)
                        .and_then(|d| d.with_month(final_month))
                    {
                        result = new_dt;
                    }
                }

                // Add days
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
        let (years, months, days, hours, minutes, seconds) =
            Self::extract_duration_components(duration);

        if let Value::String(dt_str) = &datetime {
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

                    if let Some(new_dt) = result
                        .with_year(final_year)
                        .and_then(|d| d.with_month(final_month))
                    {
                        result = new_dt;
                    }
                }

                // Subtract days, hours, minutes, seconds
                let duration_secs = checked_duration_secs(days, hours, minutes, seconds)?;
                let delta = chrono::Duration::try_seconds(duration_secs).ok_or_else(|| {
                    Error::CypherExecution(
                        "duration arithmetic overflow: seconds component is outside chrono's representable range".to_string(),
                    )
                })?;
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

                    if let Some(new_dt) = result
                        .with_year(final_year)
                        .and_then(|d| d.with_month(final_month))
                    {
                        result = new_dt;
                    }
                }

                // Subtract days, hours, minutes, seconds
                let duration_secs = checked_duration_secs(days, hours, minutes, seconds)?;
                let delta = chrono::Duration::try_seconds(duration_secs).ok_or_else(|| {
                    Error::CypherExecution(
                        "duration arithmetic overflow: seconds component is outside chrono's representable range".to_string(),
                    )
                })?;
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

                    if let Some(new_dt) = result
                        .with_year(final_year)
                        .and_then(|d| d.with_month(final_month))
                    {
                        result = new_dt;
                    }
                }

                // Subtract days
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
