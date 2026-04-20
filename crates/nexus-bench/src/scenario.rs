//! Scenario description — pure data, zero I/O, zero engine.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::dataset::DatasetKind;

/// Hard upper bound on any scenario's measured iteration count. Even
/// a developer who passes `--measured-multiplier 100` on the CLI
/// will be clamped to this value; wedging is the opposite of a
/// benchmark result. Chosen so the full seed catalogue plus a
/// generous multiplier still fits comfortably in a one-minute wall
/// clock against a reasonable server.
pub const MAX_MEASURED_ITERS: u32 = 500;

/// Hard upper bound on a single scenario's per-call timeout.
pub const MAX_TIMEOUT: Duration = Duration::from_secs(30);

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
    /// Warmup iteration count — results discarded.
    pub warmup_iters: u32,
    /// Measured iteration count. Clamped to [`MAX_MEASURED_ITERS`].
    pub measured_iters: u32,
    /// Per-call soft timeout. Clamped to [`MAX_TIMEOUT`].
    #[serde(with = "duration_millis")]
    pub timeout: Duration,
    /// Expected row count. A divergence raises an explicit harness
    /// error rather than a silent pass.
    pub expected_row_count: usize,
}

/// Ergonomic builder for [`Scenario`] with conservative defaults.
#[derive(Debug, Clone)]
pub struct ScenarioBuilder {
    id: String,
    description: String,
    dataset: DatasetKind,
    query: String,
    warmup_iters: u32,
    measured_iters: u32,
    timeout: Duration,
    expected_row_count: usize,
}

impl ScenarioBuilder {
    /// Start with sane defaults: 2 warmup, 10 measured, 2 s timeout.
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
            warmup_iters: 2,
            measured_iters: 10,
            timeout: Duration::from_secs(2),
            expected_row_count: 0,
        }
    }

    /// Override warmup iterations.
    #[must_use]
    pub fn warmup(mut self, n: u32) -> Self {
        self.warmup_iters = n;
        self
    }

    /// Override measured iterations (silently clamped to
    /// [`MAX_MEASURED_ITERS`]).
    #[must_use]
    pub fn measured(mut self, n: u32) -> Self {
        self.measured_iters = n.min(MAX_MEASURED_ITERS);
        self
    }

    /// Override the per-call timeout (silently clamped to
    /// [`MAX_TIMEOUT`]).
    #[must_use]
    pub fn timeout(mut self, d: Duration) -> Self {
        self.timeout = if d > MAX_TIMEOUT { MAX_TIMEOUT } else { d };
        self
    }

    /// Expected row count — defaults to 0. Every seed scenario sets
    /// this explicitly.
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
    fn defaults_are_conservative() {
        let s =
            ScenarioBuilder::new("a.b", "", DatasetKind::Tiny, "RETURN 1").build();
        assert_eq!(s.warmup_iters, 2);
        assert_eq!(s.measured_iters, 10);
        assert_eq!(s.timeout, Duration::from_secs(2));
    }

    #[test]
    fn measured_iters_clamped() {
        let s = ScenarioBuilder::new("a.b", "", DatasetKind::Tiny, "")
            .measured(10_000)
            .build();
        assert_eq!(s.measured_iters, MAX_MEASURED_ITERS);
    }

    #[test]
    fn timeout_clamped() {
        let s = ScenarioBuilder::new("a.b", "", DatasetKind::Tiny, "")
            .timeout(Duration::from_secs(3600))
            .build();
        assert_eq!(s.timeout, MAX_TIMEOUT);
    }

    #[test]
    fn json_roundtrip() {
        let s = ScenarioBuilder::new("a.b", "d", DatasetKind::Tiny, "RETURN 1")
            .expected_rows(1)
            .build();
        let j = serde_json::to_string(&s).unwrap();
        let back: Scenario = serde_json::from_str(&j).unwrap();
        assert_eq!(s.id, back.id);
        assert_eq!(s.timeout, back.timeout);
    }
}
