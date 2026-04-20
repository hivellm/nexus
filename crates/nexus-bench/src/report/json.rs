//! JSON emitter. Pure; never touches the filesystem.

use std::io::Write;

use serde::{Deserialize, Serialize};

use super::ComparativeRow;

/// Top-level JSON report shape. Versioned so baseline comparisons
/// across releases stay unambiguous.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonReport {
    /// Report schema version.
    pub schema_version: u32,
    /// Report-generation timestamp, ISO-8601 UTC.
    pub timestamp: String,
    /// `CARGO_PKG_VERSION` at build time.
    pub nexus_version: String,
    /// Number of rows in the report.
    pub scenario_count: usize,
    /// Per-scenario comparative rows.
    pub rows: Vec<ComparativeRow>,
}

impl JsonReport {
    /// Build a report from comparative rows. Uses the current
    /// `SystemTime` for the timestamp; no wall-clock dependency.
    #[must_use]
    pub fn new(rows: Vec<ComparativeRow>) -> Self {
        Self {
            schema_version: 1,
            timestamp: iso8601_now(),
            nexus_version: env!("CARGO_PKG_VERSION").to_string(),
            scenario_count: rows.len(),
            rows,
        }
    }

    /// Serialize to pretty-printed JSON.
    pub fn to_pretty_string(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Stream to any writer.
    pub fn write<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        let s = self.to_pretty_string().map_err(std::io::Error::other)?;
        writer.write_all(s.as_bytes())
    }
}

fn iso8601_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = d.as_secs() as i64;
    let (year, month, day, hour, min, sec) = epoch_to_utc(secs);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{min:02}:{sec:02}Z")
}

fn epoch_to_utc(mut secs: i64) -> (i32, u32, u32, u32, u32, u32) {
    let sec = (secs % 60) as u32;
    secs /= 60;
    let min = (secs % 60) as u32;
    secs /= 60;
    let hour = (secs % 24) as u32;
    secs /= 24;
    let mut year = 1970i32;
    loop {
        let days = if is_leap(year) { 366 } else { 365 };
        if secs < days {
            break;
        }
        secs -= days;
        year += 1;
    }
    let months: [i64; 12] = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 1u32;
    for &m in &months {
        if secs < m {
            break;
        }
        secs -= m;
        month += 1;
    }
    let day = (secs as u32) + 1;
    (year, month, day, hour, min, sec)
}

fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::ScenarioResult;

    fn row(id: &str) -> ComparativeRow {
        ComparativeRow::new(
            ScenarioResult {
                scenario_id: id.into(),
                engine: "nexus".into(),
                samples_us: vec![100; 5],
                p50_us: 100,
                p95_us: 100,
                p99_us: 100,
                min_us: 100,
                max_us: 100,
                mean_us: 100,
                ops_per_second: 10_000.0,
                rows_returned: 1,
            },
            None,
        )
    }

    #[test]
    fn empty_serializes() {
        let r = JsonReport::new(vec![]);
        assert_eq!(r.scenario_count, 0);
        let s = r.to_pretty_string().unwrap();
        assert!(s.contains("\"schema_version\": 1"));
    }

    #[test]
    fn roundtrips() {
        let r = JsonReport::new(vec![row("a.b"), row("a.c")]);
        let s = r.to_pretty_string().unwrap();
        let back: JsonReport = serde_json::from_str(&s).unwrap();
        assert_eq!(back.scenario_count, 2);
        assert_eq!(back.rows[0].scenario_id, "a.b");
    }

    #[test]
    fn timestamp_is_iso8601() {
        let t = iso8601_now();
        assert_eq!(t.len(), 20);
        assert!(t.ends_with('Z'));
    }

    #[test]
    fn epoch_fixture_roundtrip() {
        // 2025-01-01T00:00:00Z → 1735689600
        assert_eq!(epoch_to_utc(1_735_689_600), (2025, 1, 1, 0, 0, 0));
    }

    #[test]
    fn leap_year_gregorian() {
        assert!(is_leap(2000));
        assert!(!is_leap(1900));
        assert!(is_leap(2024));
        assert!(!is_leap(2023));
    }
}
