//! Reproducible dataset catalogue.
//!
//! A [`Dataset`] is a deterministic generator — same seed, same rows
//! every time — plus a loader that translates the generated records
//! into Cypher `CREATE` statements a [`BenchClient`] can replay.
//! Scenarios declare which dataset they need; the harness installs
//! it once per iteration (or once per run, when `reset_between =
//! false`) before measuring.

use thiserror::Error;

use crate::client::{BenchClient, ClientError};

pub mod micro;

/// Kind discriminator used in reports + JSON output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatasetKind {
    /// `micro` — 10k nodes, 50k relationships, 5 labels. Ideal for
    /// scalar functions + point reads.
    Micro,
    /// Reserved for the LDBC SNB integration. The kind is
    /// pre-allocated so report schemas stay stable when the
    /// generator lands.
    Social,
    /// Reserved for the 100k-node vector dataset. Same rationale as
    /// [`Self::Social`].
    Vector,
}

/// Errors surfaced while generating / loading a dataset.
#[derive(Debug, Error)]
pub enum DatasetLoadError {
    /// Client refused one of the setup statements.
    #[error("load failed on statement #{index}: {source}")]
    Statement {
        index: usize,
        #[source]
        source: ClientError,
    },
    /// The generator produced no statements — misconfiguration.
    #[error("empty dataset — generator returned no statements")]
    Empty,
}

/// Any type that can describe itself and emit a sequence of Cypher
/// setup statements. Keeping the trait boring (no async, no streams)
/// lets scenarios compose datasets easily; the runner loops over
/// `statements()` and calls `client.execute` one at a time.
pub trait Dataset {
    /// Kind tag for reports.
    fn kind(&self) -> DatasetKind;

    /// Human-friendly short name (e.g. `"micro"`).
    fn name(&self) -> &'static str;

    /// Cypher statements that, when executed in order, bring a fresh
    /// engine to the dataset's canonical shape.
    fn statements(&self) -> Vec<String>;

    /// Number of node rows the dataset produces. Used for smoke-test
    /// assertions.
    fn expected_node_count(&self) -> usize;

    /// Load the dataset into `client`. Stops at the first error.
    fn load(&self, client: &mut dyn BenchClient) -> Result<(), DatasetLoadError> {
        let stmts = self.statements();
        if stmts.is_empty() {
            return Err(DatasetLoadError::Empty);
        }
        for (i, s) in stmts.iter().enumerate() {
            client
                .execute(
                    s,
                    &serde_json::Map::new(),
                    std::time::Duration::from_secs(30),
                )
                .map_err(|e| DatasetLoadError::Statement {
                    index: i,
                    source: e,
                })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EmptyDataset;
    impl Dataset for EmptyDataset {
        fn kind(&self) -> DatasetKind {
            DatasetKind::Micro
        }
        fn name(&self) -> &'static str {
            "empty"
        }
        fn statements(&self) -> Vec<String> {
            vec![]
        }
        fn expected_node_count(&self) -> usize {
            0
        }
    }

    #[test]
    fn empty_dataset_load_errors() {
        let mut client = crate::client::NexusClient::new().unwrap();
        let err = EmptyDataset.load(&mut client).unwrap_err();
        assert!(matches!(err, DatasetLoadError::Empty));
    }

    #[test]
    fn kind_serializes_snake_case() {
        let s = serde_json::to_string(&DatasetKind::Micro).unwrap();
        assert_eq!(s, "\"micro\"");
    }
}
