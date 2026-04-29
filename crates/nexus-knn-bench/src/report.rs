//! JSON + CSV emitters for [`crate::sweep::SweepCell`] results.

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::sweep::SweepCell;

#[derive(Debug, thiserror::Error)]
pub enum ReportError {
    #[error("io error writing {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("serde_json error writing {path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
}

/// Top-level JSON envelope. Keeps the corpus + sweep config alongside
/// the result rows so consumers can audit the run without a separate
/// log file.
#[derive(Debug, serde::Serialize)]
pub struct Report<'a> {
    pub generated_at: String,
    pub host: String,
    pub corpus_kind: crate::corpus::CorpusKind,
    pub cells: &'a [SweepCell],
}

/// Emit the sweep result as pretty-printed JSON.
pub fn write_json(path: &Path, cells: &[SweepCell]) -> Result<(), ReportError> {
    let report = Report {
        generated_at: now_iso8601(),
        host: hostname_or_unknown(),
        corpus_kind: cells
            .first()
            .map(|c| c.corpus_kind)
            .unwrap_or(crate::corpus::CorpusKind::Synthetic),
        cells,
    };
    let file = File::create(path).map_err(|e| ReportError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, &report).map_err(|e| ReportError::Json {
        path: path.to_path_buf(),
        source: e,
    })?;
    writer.flush().map_err(|e| ReportError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    Ok(())
}

/// Emit the sweep result as a CSV. The column set is picked to drop
/// directly into a Pareto-frontier scatter plot.
pub fn write_csv(path: &Path, cells: &[SweepCell]) -> Result<(), ReportError> {
    let file = File::create(path).map_err(|e| ReportError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    let mut writer = BufWriter::new(file);
    writeln!(
        writer,
        "corpus,m,ef_construction,ef_search,k,base_count,query_count,dim,\
         build_time_seconds,recall_at_1,recall_at_10,recall_at_100,\
         latency_mean_us,latency_p50_us,latency_p95_us,latency_p99_us,latency_min_us,latency_max_us,latency_samples"
    )
    .map_err(|e| ReportError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    for c in cells {
        writeln!(
            writer,
            "{:?},{},{},{},{},{},{},{},{:.6},{:.6},{:.6},{:.6},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{}",
            c.corpus_kind,
            c.m,
            c.ef_construction,
            c.ef_search,
            c.k,
            c.base_count,
            c.query_count,
            c.dim,
            c.build_time_seconds,
            c.recall.recall_at_1,
            c.recall.recall_at_10,
            c.recall.recall_at_100,
            c.latency.mean_us,
            c.latency.p50_us,
            c.latency.p95_us,
            c.latency.p99_us,
            c.latency.min_us,
            c.latency.max_us,
            c.latency.samples,
        )
        .map_err(|e| ReportError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;
    }
    writer.flush().map_err(|e| ReportError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    Ok(())
}

fn now_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    // Avoid pulling in `chrono` for one timestamp — emit a plain
    // RFC-3339 derived from the unix epoch.
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    format_unix_seconds(secs)
}

fn format_unix_seconds(secs: i64) -> String {
    // Days from epoch (1970-01-01).
    let days = secs.div_euclid(86_400);
    let mut rem = secs.rem_euclid(86_400);
    let hours = rem / 3_600;
    rem %= 3_600;
    let minutes = rem / 60;
    let seconds = rem % 60;

    let (year, month, day) = days_to_ymd(days);
    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

fn days_to_ymd(days: i64) -> (i64, u32, u32) {
    // Howard Hinnant's date algorithm — exact for the proleptic
    // Gregorian calendar over the entire range of `i64`.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    (year, m as u32, d as u32)
}

fn hostname_or_unknown() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus::CorpusKind;
    use crate::metrics::{LatencyStats, Recall};
    use tempfile::TempDir;

    fn sample_cells() -> Vec<SweepCell> {
        vec![SweepCell {
            m: 16,
            ef_construction: 200,
            ef_search: 50,
            k: 10,
            corpus_kind: CorpusKind::Synthetic,
            base_count: 100,
            query_count: 5,
            dim: 4,
            build_time_seconds: 0.123,
            recall: Recall {
                recall_at_1: 0.9,
                recall_at_10: 0.95,
                recall_at_100: 1.0,
                query_count: 5,
            },
            latency: LatencyStats {
                mean_us: 12.5,
                p50_us: 10.0,
                p95_us: 22.0,
                p99_us: 30.0,
                min_us: 5.0,
                max_us: 31.0,
                samples: 5,
            },
        }]
    }

    #[test]
    fn json_writer_emits_envelope_and_cells() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("report.json");
        write_json(&path, &sample_cells()).expect("write");
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(text.contains("\"corpus_kind\""));
        assert!(text.contains("\"recall_at_10\": 0.95"));
        assert!(text.contains("\"ef_construction\": 200"));
    }

    #[test]
    fn csv_writer_emits_header_and_rows() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("report.csv");
        write_csv(&path, &sample_cells()).expect("write");
        let text = std::fs::read_to_string(&path).expect("read");
        let mut lines = text.lines();
        let header = lines.next().expect("header");
        assert!(header.starts_with("corpus,m,ef_construction"));
        let row = lines.next().expect("row");
        assert!(row.contains("Synthetic,16,200,50,10"));
        assert!(lines.next().is_none(), "exactly one data row expected");
    }

    #[test]
    fn unix_seconds_format_rounds_through_known_dates() {
        // 1970-01-01T00:00:00Z
        assert_eq!(format_unix_seconds(0), "1970-01-01T00:00:00Z");
        // 2000-01-01T00:00:00Z = 946_684_800
        assert_eq!(format_unix_seconds(946_684_800), "2000-01-01T00:00:00Z");
        // 2026-04-29T12:34:56Z
        // = 20572 days * 86400 + 12*3600 + 34*60 + 56
        // = 1_777_420_800 + 45_296
        assert_eq!(format_unix_seconds(1_777_466_096), "2026-04-29T12:34:56Z");
        // 2024-02-29T00:00:00Z (leap day)
        // 1970..2024 = 54 yrs * 365 + 13 leap days = 19_723 days,
        // plus 59 days into 2024 = 19_782 days = 1_709_164_800 s.
        assert_eq!(format_unix_seconds(1_709_164_800), "2024-02-29T00:00:00Z");
    }
}
