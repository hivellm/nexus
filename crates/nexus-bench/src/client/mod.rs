//! Engine-agnostic client abstraction.
//!
//! The [`BenchClient`] trait is the narrow API the harness uses to
//! drive both Nexus (in-process) and Neo4j (Bolt). A scenario is
//! defined once against the trait; the runner swaps implementations
//! without touching the scenario description.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod nexus;
pub use nexus::NexusClient;

#[cfg(feature = "neo4j")]
pub mod neo4j;
#[cfg(feature = "neo4j")]
pub use neo4j::Neo4jClient;

/// One row of a result set, matching the Neo4j-compatible array form
/// the REST surface already returns (`[[value1, value2, ...]]`).
pub type Row = Vec<serde_json::Value>;

/// Outcome of a single `execute` call.
#[derive(Debug, Clone, PartialEq)]
pub struct ExecOutcome {
    /// Rows the engine returned, in engine order. Used by the
    /// output-divergence guard.
    pub rows: Vec<Row>,
    /// Wall-clock duration the engine reported (if any). `None` when
    /// the client measures the wall-clock outside.
    pub engine_reported: Option<Duration>,
}

impl ExecOutcome {
    /// Number of rows returned.
    #[inline]
    #[must_use]
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }
}

/// Errors the trait can surface.
#[derive(Debug, Error)]
pub enum ClientError {
    /// Engine rejected the Cypher / returned a runtime error.
    #[error("engine error: {0}")]
    Engine(String),
    /// Client wasn't connected / session died mid-query.
    #[error("transport error: {0}")]
    Transport(String),
    /// Query timed out at the client boundary.
    #[error("timeout after {0:?}")]
    Timeout(Duration),
    /// Setup / teardown failed (dataset load, reset, …).
    #[error("setup error: {0}")]
    Setup(String),
}

/// Narrow trait every engine client implements.
///
/// Kept synchronous-looking for clarity; implementations may block
/// on an async runtime internally (the [`NexusClient`] bridges
/// through `Engine::execute_cypher`, which is sync in the first
/// place). The Neo4j client uses `tokio::runtime::Handle::block_on`
/// internally — see its module for the multi-thread-runtime
/// requirement.
pub trait BenchClient: Send + Sync {
    /// Human-friendly engine label for report columns.
    fn engine_name(&self) -> &'static str;

    /// Execute a Cypher statement with parameters. `timeout` is a
    /// soft client-side ceiling — implementations SHOULD honour it
    /// but MAY rely on the engine's own timeout if the runtime
    /// cannot interrupt cleanly.
    fn execute(
        &mut self,
        cypher: &str,
        parameters: &serde_json::Map<String, serde_json::Value>,
        timeout: Duration,
    ) -> Result<ExecOutcome, ClientError>;

    /// Wipe all data so a scenario starts from a known state. MUST
    /// leave the engine usable (no schema / index drift between calls).
    fn reset(&mut self) -> Result<(), ClientError>;
}

// Compile-time assertion that `Row` stays `Serialize + Deserialize`
// via its `Vec<Value>` alias — the JSON report relies on that for
// verbatim row serialization.
#[allow(dead_code)]
fn _assert_row_is_serde() {
    fn _impls<T: Serialize + for<'de> Deserialize<'de>>() {}
    _impls::<Row>();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_outcome_row_count_counts_rows() {
        let out = ExecOutcome {
            rows: vec![vec![serde_json::Value::from(1)]; 42],
            engine_reported: None,
        };
        assert_eq!(out.row_count(), 42);
    }

    #[test]
    fn exec_outcome_preserves_inner_values() {
        let out = ExecOutcome {
            rows: vec![vec![serde_json::Value::from("hi")]],
            engine_reported: Some(Duration::from_millis(5)),
        };
        assert_eq!(out.rows[0][0], serde_json::Value::from("hi"));
    }
}
