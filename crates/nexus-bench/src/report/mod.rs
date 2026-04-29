//! Pure-logic reporting layer. Zero I/O, zero engine.

use serde::{Deserialize, Serialize};

use crate::harness::ScenarioResult;

pub mod concurrent_report;
pub mod json;
pub mod markdown;

/// Latency ratio classification (`nexus.p50 / neo4j.p50`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Classification {
    /// Nexus < 0.8× Neo4j latency.
    Lead,
    /// 0.8× ≤ Nexus ≤ 1.2× Neo4j.
    Parity,
    /// 1.2× < Nexus ≤ 2× Neo4j.
    Behind,
    /// Nexus > 2× Neo4j, or a NaN / missing ratio.
    Gap,
}

impl Classification {
    /// Classify by ratio.
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

    /// Emoji tag for Markdown.
    #[must_use]
    pub fn emoji(self) -> &'static str {
        match self {
            Self::Lead => "⭐",
            Self::Parity => "✅",
            Self::Behind => "⚠️",
            Self::Gap => "🚨",
        }
    }

    /// Plain-text tag for JSON / logs.
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

/// One comparative row in a report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparativeRow {
    /// Scenario id.
    pub scenario_id: String,
    /// Category prefix derived from the id (text before the first `.`).
    pub category: String,
    /// Nexus-side result.
    pub nexus: ScenarioResult,
    /// Neo4j-side result. `None` when the Neo4j side was skipped.
    pub neo4j: Option<ScenarioResult>,
    /// `nexus.p50 / neo4j.p50`. `None` when Neo4j is missing or p50=0.
    pub ratio_p50: Option<f64>,
    /// Derived from `ratio_p50`. `None` when Neo4j is missing.
    pub classification: Option<Classification>,
}

impl ComparativeRow {
    /// Build a comparative row from both engines' results.
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

    fn r(id: &str, p50: u64) -> ScenarioResult {
        ScenarioResult {
            scenario_id: id.into(),
            engine: "x".into(),
            samples_us: vec![p50; 3],
            p50_us: p50,
            p95_us: p50,
            p99_us: p50,
            min_us: p50,
            max_us: p50,
            mean_us: p50,
            ops_per_second: if p50 == 0 { 0.0 } else { 1e6 / (p50 as f64) },
            rows_returned: 1,
        }
    }

    #[test]
    fn classification_buckets() {
        assert_eq!(Classification::from_ratio(0.5), Classification::Lead);
        assert_eq!(Classification::from_ratio(1.0), Classification::Parity);
        assert_eq!(Classification::from_ratio(1.5), Classification::Behind);
        assert_eq!(Classification::from_ratio(3.0), Classification::Gap);
        assert_eq!(Classification::from_ratio(f64::NAN), Classification::Gap);
        assert_eq!(
            Classification::from_ratio(f64::INFINITY),
            Classification::Gap
        );
    }

    #[test]
    fn row_derives_category() {
        let row = ComparativeRow::new(r("scalar.abs", 100), None);
        assert_eq!(row.category, "scalar");
        assert!(row.classification.is_none());
    }

    #[test]
    fn row_computes_ratio_when_neo4j_present() {
        let row = ComparativeRow::new(r("a.b", 100), Some(r("a.b", 200)));
        assert!((row.ratio_p50.unwrap() - 0.5).abs() < 1e-9);
        assert_eq!(row.classification, Some(Classification::Lead));
    }

    #[test]
    fn row_skips_ratio_when_neo4j_p50_is_zero() {
        let row = ComparativeRow::new(r("a.b", 100), Some(r("a.b", 0)));
        assert!(row.ratio_p50.is_none());
    }
}
