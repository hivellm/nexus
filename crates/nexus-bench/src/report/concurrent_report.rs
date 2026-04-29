//! Concurrent-load report shape — JSON + Markdown emitters paired
//! with the [`crate::concurrent::ConcurrentResult`] rows.
//!
//! Reuses the same versioning + timestamp story as the serial
//! [`super::json::JsonReport`] so a regression detector that watches
//! both report families only needs one parser.

use std::io::Write;

use serde::{Deserialize, Serialize};

use crate::concurrent::ConcurrentResult;

/// Top-level JSON report shape for the concurrent harness.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcurrentJsonReport {
    /// Report schema version. Independent from the serial schema —
    /// bumping one does not require bumping the other.
    pub schema_version: u32,
    /// ISO-8601 UTC timestamp of report generation.
    pub timestamp: String,
    /// `CARGO_PKG_VERSION` at build time.
    pub nexus_version: String,
    /// Sweep label — typically the engine name + machine identifier
    /// the orchestrator passed in.
    pub sweep_label: String,
    /// Number of rows in the report.
    pub row_count: usize,
    /// Each row pairs one scenario × one concurrency level × one engine.
    pub rows: Vec<ConcurrentResult>,
}

impl ConcurrentJsonReport {
    /// Build a report from concurrent results.
    #[must_use]
    pub fn new(sweep_label: impl Into<String>, rows: Vec<ConcurrentResult>) -> Self {
        Self {
            schema_version: 1,
            timestamp: super::json::iso8601_now(),
            nexus_version: env!("CARGO_PKG_VERSION").to_string(),
            sweep_label: sweep_label.into(),
            row_count: rows.len(),
            rows,
        }
    }

    /// Serialise to pretty-printed JSON.
    pub fn to_pretty_string(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Stream to any writer.
    pub fn write<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        let s = self.to_pretty_string().map_err(std::io::Error::other)?;
        writer.write_all(s.as_bytes())
    }
}

/// Render concurrent results as a compact Markdown table.
///
/// Columns: scenario | engine | workers | qps | p50 µs | p95 µs |
/// p99 µs | iters | cpu %.
///
/// Rows are sorted by `(scenario_id, engine, workers)` for stable
/// diffs across runs.
pub fn render_markdown(rows: &[ConcurrentResult]) -> String {
    let mut sorted: Vec<&ConcurrentResult> = rows.iter().collect();
    sorted.sort_by(|a, b| {
        a.scenario_id
            .cmp(&b.scenario_id)
            .then_with(|| a.engine.cmp(&b.engine))
            .then_with(|| a.workers.cmp(&b.workers))
    });

    let mut out = String::new();
    out.push_str(
        "| scenario | engine | workers | qps | p50 µs | p95 µs | p99 µs | iters | CPU % |\n",
    );
    out.push_str("|---|---|---:|---:|---:|---:|---:|---:|---:|\n");
    for r in sorted {
        let cpu = r
            .cpu_util_estimate_pct
            .map(|v| format!("{v:.1}"))
            .unwrap_or_else(|| "—".to_string());
        out.push_str(&format!(
            "| {} | {} | {} | {:.1} | {} | {} | {} | {} | {} |\n",
            r.scenario_id,
            r.engine,
            r.workers,
            r.qps,
            r.p50_us,
            r.p95_us,
            r.p99_us,
            r.iterations,
            cpu,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(id: &str, engine: &str, workers: usize, qps: f64) -> ConcurrentResult {
        ConcurrentResult {
            scenario_id: id.into(),
            engine: engine.into(),
            workers,
            wall_ms: 1_000,
            iterations: (qps as u64).max(1),
            qps,
            p50_us: 100,
            p95_us: 250,
            p99_us: 500,
            min_us: 50,
            max_us: 800,
            mean_us: 150,
            rows_returned: 1,
            cpu_util_estimate_pct: None,
        }
    }

    #[test]
    fn json_report_round_trips_through_serde() {
        let rows = vec![sample("scalar.abs", "nexus", 4, 10_000.0)];
        let report = ConcurrentJsonReport::new("nexus-v1.15.0-vs-neo4j-2025.09.0", rows);
        let json = report.to_pretty_string().expect("serialize");
        let parsed: ConcurrentJsonReport = serde_json::from_str(&json).expect("parse");
        assert_eq!(parsed.row_count, 1);
        assert_eq!(parsed.rows[0].engine, "nexus");
        assert!(parsed.timestamp.ends_with('Z'));
    }

    #[test]
    fn markdown_renderer_is_sorted_and_stable() {
        let rows = vec![
            sample("b.scenario", "neo4j", 4, 100.0),
            sample("a.scenario", "nexus", 4, 200.0),
            sample("a.scenario", "nexus", 1, 80.0),
            sample("a.scenario", "neo4j", 1, 60.0),
        ];
        let md = render_markdown(&rows);
        // Header on first two lines, then data rows in sorted order.
        let mut lines = md.lines();
        assert!(lines.next().unwrap().starts_with("| scenario"));
        assert!(lines.next().unwrap().starts_with("|---"));
        let mut data: Vec<&str> = lines.collect();
        // Drop trailing empty line if present.
        if data.last().is_some_and(|s| s.is_empty()) {
            data.pop();
        }
        assert_eq!(data.len(), 4);
        // a.scenario rows come first, sorted by (engine, workers).
        assert!(data[0].contains("a.scenario | neo4j | 1"));
        assert!(data[1].contains("a.scenario | nexus | 1"));
        assert!(data[2].contains("a.scenario | nexus | 4"));
        assert!(data[3].contains("b.scenario"));
    }

    #[test]
    fn markdown_handles_missing_cpu_estimate() {
        let mut row = sample("x.y", "nexus", 1, 1.0);
        row.cpu_util_estimate_pct = None;
        let md = render_markdown(&[row]);
        assert!(md.contains("| — |"));
    }
}
