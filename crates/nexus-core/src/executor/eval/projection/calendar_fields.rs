//! The four ways a temporal map can name a date.
//!
//! `date({...})` and its siblings accept a calendar date, an ISO week date, a
//! quarter date, or an ordinal date:
//!
//! ```cypher
//! date({year: 1984, month: 3, day: 7})            -- calendar
//! date({year: 1984, week: 10, dayOfWeek: 3})      -- ISO week      → 1984-03-07
//! date({year: 1984, quarter: 1, dayOfQuarter: 67})-- quarter       → 1984-03-07
//! date({year: 1984, ordinalDay: 67})              -- ordinal       → 1984-03-07
//! ```
//!
//! Every constructor read `year`/`month`/`day` and nothing else, so the other
//! three notations silently produced January 1st of the given year — a wrong
//! answer reported as success, across `date`, `datetime`, `localdatetime` and
//! `localtime`.
//!
//! Field defaults follow the same "largest component given, rest default to
//! their first value" rule the calendar form already used: `{year, week}` is
//! the Monday of that ISO week, `{year, quarter}` the first day of that
//! quarter.

use chrono::{Datelike as _, NaiveDate};
use serde_json::{Map, Value};

use super::super::temporal_value;

/// Resolve a temporal map's time fields to `(hour, minute, second, nanosecond)`.
///
/// The mirror of [`date_from_map`] for the clock half: a `time` / `datetime` /
/// `localdatetime` key selects an existing value's time, and the map's own
/// `hour`/`minute`/`second` (and sub-second) keys override it. Absent both, a
/// component is zero — midnight.
///
/// Returns `None` for a field that is present but cannot be a clock component,
/// which callers answer as `null`. The sub-second keys are NOT handled here:
/// they compose additively across `millisecond`/`microsecond`/`nanosecond` and
/// have their own bounds-checked reader in `fn_temporal`, which reports a
/// typed error rather than a silent `null`.
pub(super) fn time_from_map(map: &Map<String, Value>) -> Option<(u32, u32, u32, u32)> {
    let selected = ["time", "datetime", "localdatetime"]
        .iter()
        .find_map(|key| map.get(*key).and_then(temporal_value::time_components));
    let (base_h, base_m, base_s, base_n) = selected.unwrap_or((0, 0, 0, 0));

    let hour = field(map, "hour", base_h)?;
    let minute = field(map, "minute", base_m)?;
    let second = field(map, "second", base_s)?;
    // A selected value's sub-second part carries over only when the map does
    // not state one of its own; `fn_temporal` composes those keys and passes
    // the result in, so this is the "nothing given" fallback.
    Some((hour, minute, second, base_n))
}

/// True when the map selects its time from an existing temporal value and
/// states no sub-second key of its own — the case where that value's
/// nanoseconds must survive.
pub(super) fn inherits_subsecond(map: &Map<String, Value>) -> bool {
    !map.contains_key("millisecond")
        && !map.contains_key("microsecond")
        && !map.contains_key("nanosecond")
        && ["time", "datetime", "localdatetime"]
            .iter()
            .any(|key| map.contains_key(*key))
}

/// Resolve a temporal map's date fields to a `(year, month, day)` triple.
///
/// Returns `None` when the fields name no real date (`week: 53` in a year with
/// 52, `ordinalDay: 366` in a common year, `month: 13`), which every caller
/// already treats as "not a date" and answers `null`.
///
/// Which notation is used is decided by which keys are present, checked from
/// the most specific to the least so a map carrying several — as a re-tagged
/// temporal value's map does, since it holds `year`/`month`/`day` alongside
/// everything else — resolves the same way it was built.
pub(super) fn date_from_map(map: &Map<String, Value>) -> Option<(i32, u32, u32)> {
    // A `date`/`datetime` key selects the date out of an existing temporal
    // value, which the rest of the map then overrides component by component:
    // `date({date: d})` copies it, `date({date: d, day: 1})` moves to the
    // first of its month. With no overriding key at all the selection *is*
    // the answer.
    let selected = selected_date(map);
    if let Some(base) = selected {
        if !map.keys().any(|k| is_date_component(k)) {
            return Some(base);
        }
    }

    let year = map
        .get("year")
        .and_then(Value::as_i64)
        .map(|y| y as i32)
        .or_else(|| selected.map(|(y, _, _)| y))
        .unwrap_or_else(|| chrono::Local::now().year());

    // A component the map omits falls back to the selected value's own, and to
    // the notation's first value when nothing was selected — so
    // `date({date: d, day: 1})` keeps `d`'s month while `date({year: 1984})`
    // is the 1st of January.
    let base_month = selected.map_or(1, |(_, m, _)| m);
    let base_day = selected.map_or(1, |(_, _, d)| d);

    // A map that carries an explicit month is a calendar date; the week /
    // quarter / ordinal notations are mutually exclusive with it.
    if map.contains_key("month") {
        let month = field(map, "month", base_month)?;
        let day = field(map, "day", 1)?;
        return NaiveDate::from_ymd_opt(year, month, day).map(ymd);
    }

    if map.contains_key("week") {
        let week = field(map, "week", 1)?;
        let day_of_week = field(map, "dayOfWeek", 1)?;
        let weekday =
            chrono::Weekday::try_from(u8::try_from(day_of_week).ok()?.checked_sub(1)?).ok()?;
        // The year a week number is counted within is the ISO *week-year*,
        // which differs from the calendar year at both ends of December and
        // January — that is the whole point of the notation. When the year
        // came from a selected value rather than an explicit `year` key, it
        // has to be re-read as that value's week-year:
        // `date({date: date('1816-12-30'), week: 2, dayOfWeek: 3})` is
        // 1817-01-08, because 1816-12-30 already belongs to week-year 1817.
        let week_year = match (map.contains_key("year"), selected) {
            (false, Some((y, m, d))) => NaiveDate::from_ymd_opt(y, m, d)?.iso_week().year(),
            _ => year,
        };
        return NaiveDate::from_isoywd_opt(week_year, week, weekday).map(ymd);
    }

    if map.contains_key("quarter") {
        let quarter = field(map, "quarter", 1)?;
        if !(1..=4).contains(&quarter) {
            return None;
        }
        let day_of_quarter = field(map, "dayOfQuarter", 1)?;
        let first_month = (quarter - 1) * 3 + 1;
        let first_day = NaiveDate::from_ymd_opt(year, first_month, 1)?;
        let date = first_day.checked_add_days(chrono::Days::new(u64::from(day_of_quarter) - 1))?;
        // Overshooting into the next quarter is not a longer quarter, it is a
        // bad `dayOfQuarter`.
        if quarter_of(&date) != quarter || date.year() != year {
            return None;
        }
        return Some(ymd(date));
    }

    if map.contains_key("ordinalDay") {
        let ordinal = field(map, "ordinalDay", 1)?;
        return NaiveDate::from_yo_opt(year, ordinal).map(ymd);
    }

    // Plain `{year}` / `{year, day}` — the calendar form with its defaults.
    let day = field(map, "day", base_day)?;
    NaiveDate::from_ymd_opt(year, base_month, day).map(ymd)
}

/// The date carried by a `date` / `datetime` / `localdatetime` selector key.
fn selected_date(map: &Map<String, Value>) -> Option<(i32, u32, u32)> {
    ["date", "datetime", "localdatetime"]
        .iter()
        .find_map(|key| map.get(*key).and_then(temporal_value::date_components))
}

/// True for a key that overrides part of a selected date.
fn is_date_component(key: &str) -> bool {
    matches!(
        key,
        "year" | "month" | "day" | "week" | "dayOfWeek" | "quarter" | "dayOfQuarter" | "ordinalDay"
    )
}

/// A `u32` field, or `default` when absent. `None` when present but negative
/// or not a number — a date cannot be built from it either way.
fn field(map: &Map<String, Value>, key: &str, default: u32) -> Option<u32> {
    match map.get(key) {
        None | Some(Value::Null) => Some(default),
        Some(v) => u32::try_from(v.as_i64()?).ok(),
    }
}

fn ymd(date: NaiveDate) -> (i32, u32, u32) {
    (date.year(), date.month(), date.day())
}

fn quarter_of(date: &NaiveDate) -> u32 {
    (date.month() - 1) / 3 + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, i64)]) -> Map<String, Value> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), Value::from(*v)))
            .collect()
    }

    #[test]
    fn calendar_fields_resolve_directly() {
        assert_eq!(
            date_from_map(&map(&[("year", 1984), ("month", 3), ("day", 7)])),
            Some((1984, 3, 7))
        );
    }

    #[test]
    fn an_iso_week_date_resolves_to_its_calendar_day() {
        assert_eq!(
            date_from_map(&map(&[("year", 1984), ("week", 10), ("dayOfWeek", 3)])),
            Some((1984, 3, 7))
        );
        // Bare `{year, week}` is that week's Monday.
        assert_eq!(
            date_from_map(&map(&[("year", 1816), ("week", 52)])),
            Some((1816, 12, 23))
        );
    }

    #[test]
    fn an_iso_week_year_may_differ_from_the_calendar_year() {
        // ISO week 1 of 2019 starts on 2018-12-31 — the notation counts weeks
        // in the week-year, not the calendar year.
        assert_eq!(
            date_from_map(&map(&[("year", 2019), ("week", 1), ("dayOfWeek", 1)])),
            Some((2018, 12, 31))
        );
    }

    #[test]
    fn a_quarter_date_resolves_to_its_calendar_day() {
        assert_eq!(
            date_from_map(&map(&[
                ("year", 1984),
                ("quarter", 1),
                ("dayOfQuarter", 67)
            ])),
            Some((1984, 3, 7))
        );
        assert_eq!(
            date_from_map(&map(&[("year", 1984), ("quarter", 3)])),
            Some((1984, 7, 1))
        );
    }

    #[test]
    fn an_ordinal_date_resolves_to_its_calendar_day() {
        // 1984 is a leap year, so day 67 is 7 March.
        assert_eq!(
            date_from_map(&map(&[("year", 1984), ("ordinalDay", 67)])),
            Some((1984, 3, 7))
        );
        assert_eq!(
            date_from_map(&map(&[("year", 1983), ("ordinalDay", 67)])),
            Some((1983, 3, 8))
        );
    }

    #[test]
    fn fields_naming_no_real_date_resolve_to_nothing() {
        assert_eq!(date_from_map(&map(&[("year", 1984), ("month", 13)])), None);
        assert_eq!(date_from_map(&map(&[("year", 1984), ("week", 54)])), None);
        assert_eq!(date_from_map(&map(&[("year", 1984), ("quarter", 5)])), None);
        // 1983 is not a leap year, so it has no day 366.
        assert_eq!(
            date_from_map(&map(&[("year", 1983), ("ordinalDay", 366)])),
            None
        );
        // Day 92 overshoots a 91-day first quarter.
        assert_eq!(
            date_from_map(&map(&[
                ("year", 1983),
                ("quarter", 1),
                ("dayOfQuarter", 92)
            ])),
            None
        );
    }

    #[test]
    fn a_year_alone_is_the_first_of_january() {
        assert_eq!(date_from_map(&map(&[("year", 1984)])), Some((1984, 1, 1)));
    }
}
