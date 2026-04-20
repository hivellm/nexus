//! Scenario description.
//!
//! A [`Scenario`] pairs a Cypher query with metadata the harness
//! needs: which dataset to install first, how many warmup + measured
//! iterations to run, a soft per-call timeout, and the expected row
//! count that serves as an output-divergence guard.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::dataset::DatasetKind;

/// A single benchmark scenario.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scenario {
    /// Stable identifier used in reports (e.g. `scalar.abs`).
    pub id: String,
    /// Human-readable one-liner.
    pub description: String,
    /// Dataset the scenario targets.
    pub dataset: DatasetKind,
    /// The Cypher the engine executes.
    pub query: String,
    /// Parameters bound at execution time.
    #[serde(default)]
    pub parameters: serde_json::Map<String, serde_json::Value>,
    /// Warmup iteration count — results discarded.
    pub warmup_iters: u32,
    /// Measured iteration count.
    pub measured_iters: u32,
    /// Per-call soft timeout.
    #[serde(with = "duration_millis")]
    pub timeout: Duration,
    /// Expected row count the engine should return. A divergence
    /// raises an explicit harness error rather than a silent pass.
    pub expected_row_count: usize,
}

/// Builder for [`Scenario`]. Not strictly required — all fields are
/// public on the struct — but scenarios in [`crate::scenario_catalog`]
/// use it so they read top-down.
#[derive(Debug, Clone)]
pub struct ScenarioBuilder {
    id: String,
    description: String,
    dataset: DatasetKind,
    query: String,
    parameters: serde_json::Map<String, serde_json::Value>,
    warmup_iters: u32,
    measured_iters: u32,
    timeout: Duration,
    expected_row_count: usize,
}

impl ScenarioBuilder {
    /// Start a builder with an id, description, dataset + query. Sane
    /// iteration defaults (5 warmup / 30 measured) + 2 s timeout.
    pub fn new(
        id: impl Into<String>,
        description: impl Into<String>,
        dataset: DatasetKind,
        query: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            description: description.into(),
            dataset,
            query: query.into(),
            parameters: serde_json::Map::new(),
            warmup_iters: 5,
            measured_iters: 30,
            timeout: Duration::from_secs(2),
            expected_row_count: 0,
        }
    }

    /// Set a single parameter.
    #[must_use]
    pub fn param(mut self, key: &str, value: serde_json::Value) -> Self {
        self.parameters.insert(key.to_string(), value);
        self
    }

    /// Override warmup iterations.
    #[must_use]
    pub fn warmup(mut self, n: u32) -> Self {
        self.warmup_iters = n;
        self
    }

    /// Override measured iterations.
    #[must_use]
    pub fn measured(mut self, n: u32) -> Self {
        self.measured_iters = n;
        self
    }

    /// Override the per-call timeout.
    #[must_use]
    pub fn timeout(mut self, d: Duration) -> Self {
        self.timeout = d;
        self
    }

    /// Expected row count — defaults to 0 if not set. Scenarios that
    /// return a scalar MUST set this to 1 so a regression that drops
    /// the row gets caught.
    #[must_use]
    pub fn expected_rows(mut self, n: usize) -> Self {
        self.expected_row_count = n;
        self
    }

    /// Finalize.
    pub fn build(self) -> Scenario {
        Scenario {
            id: self.id,
            description: self.description,
            dataset: self.dataset,
            query: self.query,
            parameters: self.parameters,
            warmup_iters: self.warmup_iters,
            measured_iters: self.measured_iters,
            timeout: self.timeout,
            expected_row_count: self.expected_row_count,
        }
    }
}

mod duration_millis {
    use super::Duration;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(d: &Duration, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u128(d.as_millis())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
        let ms = u64::deserialize(d)?;
        Ok(Duration::from_millis(ms))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_defaults_are_reasonable() {
        let s = ScenarioBuilder::new("a.b", "ret 1", DatasetKind::Micro, "RETURN 1").build();
        assert_eq!(s.warmup_iters, 5);
        assert_eq!(s.measured_iters, 30);
        assert_eq!(s.timeout, Duration::from_secs(2));
        assert_eq!(s.expected_row_count, 0);
    }

    #[test]
    fn builder_overrides() {
        let s = ScenarioBuilder::new("a.b", "", DatasetKind::Micro, "RETURN 1")
            .warmup(10)
            .measured(100)
            .timeout(Duration::from_secs(5))
            .expected_rows(1)
            .param("x", serde_json::Value::from(42))
            .build();
        assert_eq!(s.warmup_iters, 10);
        assert_eq!(s.measured_iters, 100);
        assert_eq!(s.timeout, Duration::from_secs(5));
        assert_eq!(s.expected_row_count, 1);
        assert_eq!(s.parameters.get("x"), Some(&serde_json::Value::from(42)));
    }

    #[test]
    fn json_roundtrip_preserves_all_fields() {
        let s = ScenarioBuilder::new("a.b", "desc", DatasetKind::Micro, "RETURN 1")
            .expected_rows(3)
            .timeout(Duration::from_millis(750))
            .build();
        let j = serde_json::to_string(&s).unwrap();
        let back: Scenario = serde_json::from_str(&j).unwrap();
        assert_eq!(s.id, back.id);
        assert_eq!(s.timeout, back.timeout);
        assert_eq!(s.expected_row_count, back.expected_row_count);
    }
}
