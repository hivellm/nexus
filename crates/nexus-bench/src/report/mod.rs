//! Comparative reporting: Markdown for humans, JSON for CI.
//!
//! The emitters consume a set of [`ComparativeRow`] entries — one per
//! (scenario, engine-pair) tuple — and format them. Every row carries
//! both engines' latencies + a [`Classification`] computed from the
//! ratio, so the emitters never recompute the same logic twice.

use serde::{Deserialize, Serialize};

use crate::harness::ScenarioResult;

pub mod json;
pub mod markdown;

/// Latency ratio between the two engines, classified for the
/// ⭐/✅/⚠️/🚨 column in the Markdown report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Classification {
    /// Nexus < 0.8× Neo4j.
    Lead,
    /// 0.8× ≤ Nexus ≤ 1.2× Neo4j.
    Parity,
    /// 1.2× < Nexus ≤ 2× Neo4j.
    Behind,
    /// Nexus > 2× Neo4j.
    Gap,
}

impl Classification {
    /// Classify by ratio (nexus p50 / neo4j p50). `ratio = NaN` or
    /// non-finite falls back to [`Classification::Gap`] so a
    /// divergent or missing number doesn't accidentally read as
    /// green.
    #[must_use]
    pub fn from_ratio(ratio: f64) -> Self {
        if !ratio.is_finite() {
            return Self::Gap;
        }
        if ratio < 0.8 {
            Self::Lead
        } else if ratio <= 1.2 {
            Self::Parity
        } else if ratio <= 2.0 {
            Self::Behind
        } else {
            Self::Gap
        }
    }

    /// Short emoji banner for Markdown.
    #[must_use]
    pub fn emoji(self) -> &'static str {
        match self {
            Self::Lead => "⭐",
            Self::Parity => "✅",
            Self::Behind => "⚠️",
            Self::Gap => "🚨",
        }
    }

    /// Plain-text form for JSON / non-UTF-8 viewers.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Lead => "lead",
            Self::Parity => "parity",
            Self::Behind => "behind",
            Self::Gap => "gap",
        }
    }
}

/// One row in a comparative report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparativeRow {
    /// Scenario id the row reports on.
    pub scenario_id: String,
    /// Category the scenario falls into — derived from the id's
    /// prefix, kept explicit so reports can group without regex.
    pub category: String,
    /// Nexus-side result.
    pub nexus: ScenarioResult,
    /// Neo4j-side result. `None` when Neo4j wasn't available for
    /// this run; the Markdown emitter renders the cell as `—`.
    pub neo4j: Option<ScenarioResult>,
    /// Ratio `nexus.p50 / neo4j.p50`. `None` when Neo4j is missing.
    pub ratio_p50: Option<f64>,
    /// Classification derived from `ratio_p50`. `None` when Neo4j is
    /// missing.
    pub classification: Option<Classification>,
}

impl ComparativeRow {
    /// Build a row given both engines' results.
    pub fn new(nexus: ScenarioResult, neo4j: Option<ScenarioResult>) -> Self {
        let category = nexus
            .scenario_id
            .split('.')
            .next()
            .unwrap_or("other")
            .to_string();
        let (ratio, cls) = match &neo4j {
            Some(n) if n.p50_us > 0 => {
                let r = nexus.p50_us as f64 / n.p50_us as f64;
                (Some(r), Some(Classification::from_ratio(r)))
            }
            _ => (None, None),
        };
        Self {
            scenario_id: nexus.scenario_id.clone(),
            category,
            nexus,
            neo4j,
            ratio_p50: ratio,
            classification: cls,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_result(id: &str, p50_us: u64) -> ScenarioResult {
        ScenarioResult {
            scenario_id: id.into(),
            engine: "x".into(),
            samples_us: vec![p50_us; 10],
            p50_us,
            p95_us: p50_us,
            p99_us: p50_us,
            min_us: p50_us,
            max_us: p50_us,
            mean_us: p50_us,
            ops_per_second: 1_000_000.0 / (p50_us as f64),
            rows_returned: 1,
        }
    }

    #[test]
    fn lead_when_nexus_is_faster() {
        let c = Classification::from_ratio(0.5);
        assert_eq!(c, Classification::Lead);
        assert_eq!(c.emoji(), "⭐");
    }

    #[test]
    fn parity_tolerates_20pct_band() {
        assert_eq!(Classification::from_ratio(0.81), Classification::Parity);
        assert_eq!(Classification::from_ratio(1.0), Classification::Parity);
        assert_eq!(Classification::from_ratio(1.19), Classification::Parity);
    }

    #[test]
    fn behind_when_1_2_to_2x() {
        assert_eq!(Classification::from_ratio(1.5), Classification::Behind);
        assert_eq!(Classification::from_ratio(2.0), Classification::Behind);
    }

    #[test]
    fn gap_when_over_2x_or_nan() {
        assert_eq!(Classification::from_ratio(2.01), Classification::Gap);
        assert_eq!(Classification::from_ratio(f64::NAN), Classification::Gap);
        assert_eq!(
            Classification::from_ratio(f64::INFINITY),
            Classification::Gap
        );
    }

    #[test]
    fn comparative_row_derives_category_from_id_prefix() {
        let row = ComparativeRow::new(sample_result("scalar.abs", 100), None);
        assert_eq!(row.category, "scalar");
        assert!(row.ratio_p50.is_none());
        assert!(row.classification.is_none());
    }

    #[test]
    fn comparative_row_computes_ratio() {
        let row = ComparativeRow::new(
            sample_result("scalar.abs", 100),
            Some(sample_result("scalar.abs", 200)),
        );
        assert!((row.ratio_p50.unwrap() - 0.5).abs() < 1e-9);
        assert_eq!(row.classification, Some(Classification::Lead));
    }

    #[test]
    fn comparative_row_handles_neo4j_zero_p50() {
        let row = ComparativeRow::new(
            sample_result("scalar.abs", 100),
            Some(sample_result("scalar.abs", 0)),
        );
        assert!(row.ratio_p50.is_none());
    }
}
