//! Markdown emitter. Human-consumable per-category tables.

use std::fmt::Write;

use super::{Classification, ComparativeRow};

/// Render a Markdown report from comparative rows. Returns a String
/// so callers can pipe it to stdout, write it to a file, or paste it
/// into a GitHub PR comment.
#[must_use]
pub fn render(rows: &[ComparativeRow]) -> String {
    let mut out = String::with_capacity(rows.len() * 200);
    out.push_str("# Nexus ↔ Neo4j Benchmark Report\n\n");
    if rows.is_empty() {
        out.push_str("_No scenarios executed._\n");
        return out;
    }

    let _ = writeln!(out, "Scenarios: **{}**", rows.len());
    if let Some(summary) = summary_counts(rows) {
        let _ = writeln!(
            out,
            "\n| Classification | Count |\n|---|---|\n| ⭐ Lead | {} |\n| ✅ Parity | {} |\n| ⚠️ Behind | {} |\n| 🚨 Gap | {} |\n| — n/a | {} |",
            summary.lead, summary.parity, summary.behind, summary.gap, summary.unknown
        );
    }

    // Group by category.
    let mut categories: Vec<&str> = rows
        .iter()
        .map(|r| r.category.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    categories.sort_unstable();

    for cat in categories {
        let _ = writeln!(out, "\n## {cat}\n");
        out.push_str("| Scenario | Nexus p50 (µs) | Nexus p95 (µs) | Neo4j p50 (µs) | Ratio | |\n");
        out.push_str("|---|---|---|---|---|---|\n");
        for row in rows.iter().filter(|r| r.category == cat) {
            let neo4j_p50 = row
                .neo4j
                .as_ref()
                .map(|r| format!("{}", r.p50_us))
                .unwrap_or_else(|| "—".into());
            let ratio = row
                .ratio_p50
                .map(|r| format!("{r:.2}×"))
                .unwrap_or_else(|| "—".into());
            let banner = row.classification.map(Classification::emoji).unwrap_or("—");
            let _ = writeln!(
                out,
                "| `{}` | {} | {} | {} | {} | {} |",
                row.scenario_id, row.nexus.p50_us, row.nexus.p95_us, neo4j_p50, ratio, banner
            );
        }
    }

    out
}

/// Count-by-classification summary for the header block.
#[derive(Debug, Default)]
struct Summary {
    lead: usize,
    parity: usize,
    behind: usize,
    gap: usize,
    unknown: usize,
}

fn summary_counts(rows: &[ComparativeRow]) -> Option<Summary> {
    if rows.is_empty() {
        return None;
    }
    let mut s = Summary::default();
    for r in rows {
        match r.classification {
            Some(Classification::Lead) => s.lead += 1,
            Some(Classification::Parity) => s.parity += 1,
            Some(Classification::Behind) => s.behind += 1,
            Some(Classification::Gap) => s.gap += 1,
            None => s.unknown += 1,
        }
    }
    Some(s)
}

/// Convenience wrapper so callers can use `MarkdownReport::new(rows).render()`.
pub struct MarkdownReport<'a> {
    rows: &'a [ComparativeRow],
}

impl<'a> MarkdownReport<'a> {
    /// Build a report view over a row slice.
    #[must_use]
    pub fn new(rows: &'a [ComparativeRow]) -> Self {
        Self { rows }
    }

    /// Render to a String.
    #[must_use]
    pub fn render(&self) -> String {
        render(self.rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::ScenarioResult;

    fn row(id: &str, nexus_p50: u64, neo4j_p50: Option<u64>) -> ComparativeRow {
        let nexus = ScenarioResult {
            scenario_id: id.into(),
            engine: "nexus".into(),
            samples_us: vec![nexus_p50; 5],
            p50_us: nexus_p50,
            p95_us: nexus_p50,
            p99_us: nexus_p50,
            min_us: nexus_p50,
            max_us: nexus_p50,
            mean_us: nexus_p50,
            ops_per_second: 1.0,
            rows_returned: 1,
        };
        let neo4j = neo4j_p50.map(|p| ScenarioResult {
            scenario_id: id.into(),
            engine: "neo4j".into(),
            samples_us: vec![p; 5],
            p50_us: p,
            p95_us: p,
            p99_us: p,
            min_us: p,
            max_us: p,
            mean_us: p,
            ops_per_second: 1.0,
            rows_returned: 1,
        });
        ComparativeRow::new(nexus, neo4j)
    }

    #[test]
    fn empty_report_renders_placeholder_line() {
        let md = render(&[]);
        assert!(md.contains("No scenarios executed"));
    }

    #[test]
    fn renders_category_tables() {
        let rows = vec![
            row("scalar.abs", 100, Some(200)),
            row("scalar.ceil", 120, Some(150)),
            row("traversals.one_hop", 300, None),
        ];
        let md = render(&rows);
        assert!(md.contains("## scalar"));
        assert!(md.contains("## traversals"));
        assert!(md.contains("| `scalar.abs` |"));
        assert!(md.contains("⭐"));
        // traversals row has no Neo4j number.
        assert!(md.contains("| `traversals.one_hop` | 300 |"));
    }

    #[test]
    fn summary_counts_each_classification() {
        let rows = vec![
            row("scalar.a", 80, Some(100)),  // 0.8 ratio → Parity
            row("scalar.b", 50, Some(100)),  // 0.5 ratio → Lead
            row("scalar.c", 150, Some(100)), // 1.5 → Behind
            row("scalar.d", 300, Some(100)), // 3.0 → Gap
            row("scalar.e", 100, None),      // unknown
        ];
        let md = render(&rows);
        assert!(md.contains("⭐ Lead | 1"));
        assert!(md.contains("✅ Parity | 1"));
        assert!(md.contains("⚠️ Behind | 1"));
        assert!(md.contains("🚨 Gap | 1"));
        assert!(md.contains("— n/a | 1"));
    }

    #[test]
    fn markdown_report_wrapper_matches_function() {
        let rows = vec![row("a.b", 10, Some(10))];
        assert_eq!(MarkdownReport::new(&rows).render(), render(&rows));
    }
}
