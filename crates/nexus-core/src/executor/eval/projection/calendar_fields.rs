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
    let year = map
        .get("year")
        .and_then(Value::as_i64)
        .unwrap_or_else(|| i64::from(chrono::Local::now().year())) as i32;

    // A map that carries an explicit month is a calendar date; the week /
    // quarter / ordinal notations are mutually exclusive with it.
    if map.contains_key("month") {
        let month = field(map, "month", 1)?;
        let day = field(map, "day", 1)?;
        return NaiveDate::from_ymd_opt(year, month, day).map(ymd);
    }

    if map.contains_key("week") {
        let week = field(map, "week", 1)?;
        let day_of_week = field(map, "dayOfWeek", 1)?;
        let weekday =
            chrono::Weekday::try_from(u8::try_from(day_of_week).ok()?.checked_sub(1)?).ok()?;
        // `year` here is the ISO week-year, which is what the week number is
        // counted within — it can differ from the calendar year at both ends
        // of the year, and that is precisely the point of the notation.
        return NaiveDate::from_isoywd_opt(year, week, weekday).map(ymd);
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
    let day = field(map, "day", 1)?;
    NaiveDate::from_ymd_opt(year, 1, day).map(ymd)
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
