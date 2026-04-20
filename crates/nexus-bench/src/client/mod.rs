//! Bench client trait + shared types. Feature-gated on `live-bench`.
//!
//! The harness is generic over [`BenchClient`], so any transport
//! backend that satisfies the narrow contract (execute Cypher, return
//! rows, honour the timeout) plugs in without touching the runner.
//!
//! Current implementations:
//!
//! * [`rpc::NexusRpcClient`] — Nexus native length-prefixed
//!   MessagePack RPC. This is the **only** Nexus-side transport
//!   the bench speaks: HTTP/JSON is intentionally not an option so
//!   comparative runs against Neo4j's Bolt side measure engine
//!   work, not JSON serialisation overhead.
//! * [`neo4j::Neo4jBoltClient`] — Neo4j over the Bolt protocol
//!   (additionally gated on the `neo4j` feature).
//!
//! Every client wraps each RPC in `tokio::time::timeout` and performs
//! a short health probe on connect so the harness fails fast when
//! the server is unreachable.

use std::time::Duration;

use thiserror::Error;

use crate::harness::{BenchExecute, ExecResult};

pub mod rpc;
pub use rpc::{NexusRpcClient, NexusRpcCredentials};

#[cfg(feature = "neo4j")]
pub mod neo4j;
#[cfg(feature = "neo4j")]
pub use neo4j::Neo4jBoltClient;

/// Row shape — one cell per column, in column order.
pub type Row = Vec<serde_json::Value>;

/// Minimal outcome a client publishes; the harness converts to
/// [`ExecResult`] internally.
#[derive(Debug, Clone, PartialEq)]
pub struct ExecOutcome {
    /// Rows returned by the engine, in order.
    pub rows: Vec<Row>,
}

/// Errors a bench client can surface.
#[derive(Debug, Error)]
pub enum ClientError {
    /// Transport-level failure: connect refused, DNS, I/O read/write.
    #[error("transport error: {0}")]
    Transport(String),
    /// Bolt-level error from Neo4j. Kept distinct from
    /// [`Self::Transport`] so the harness can tell driver bugs apart
    /// from server rejections.
    #[error("bolt error: {0}")]
    Bolt(String),
    /// The server returned data that didn't match the expected shape.
    #[error("malformed response: {0}")]
    BadResponse(String),
    /// Soft per-call timeout elapsed. The harness maps this to a
    /// scenario failure rather than silently recording a huge
    /// latency.
    #[error("timeout after {0:?}")]
    Timeout(Duration),
    /// Health probe failed at startup.
    #[error("server health probe failed: {0}")]
    HealthProbe(String),
}

/// Narrow trait every bench client must satisfy. The harness is
/// generic over this, so the RPC client + the Bolt client + any
/// future transport plug in without touching the runner.
pub trait BenchClient: Send + Sync {
    /// Label reported in the engine column of the report.
    fn engine_name(&self) -> &str;

    /// Issue a single Cypher request. Must return within `timeout`
    /// or surface [`ClientError::Timeout`].
    fn execute(&mut self, cypher: &str, timeout: Duration) -> Result<ExecOutcome, ClientError>;

    /// Wipe all nodes + relationships from the database, returning
    /// once the server confirms the delete. Used by integration
    /// tests for per-test fixture isolation — without it, running
    /// `cargo test --ignored` as a batch has the second test load
    /// TinyDataset on top of the first and trip the row-count
    /// divergence guard.
    ///
    /// Default impl returns [`ClientError::BadResponse`] so
    /// implementations that genuinely cannot reset (read-only
    /// transports) opt out explicitly rather than silently
    /// succeeding.
    fn reset(&mut self, timeout: Duration) -> Result<(), ClientError> {
        let _ = timeout;
        Err(ClientError::BadResponse(
            "reset not supported by this client".into(),
        ))
    }
}

/// Bridge from the rich [`BenchClient`] trait to the harness's
/// narrower [`BenchExecute`] contract.
impl<T: BenchClient + ?Sized> BenchExecute for &mut T {
    fn execute(
        &mut self,
        cypher: &str,
        timeout: Duration,
    ) -> Result<ExecResult, Box<dyn std::error::Error + Send + Sync>> {
        let out = BenchClient::execute(*self, cypher, timeout)
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;
        Ok(ExecResult {
            row_count: out.rows.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_outcome_row_count_matches_vec_len() {
        let out = ExecOutcome {
            rows: vec![vec![serde_json::Value::from(1)]; 4],
        };
        assert_eq!(out.rows.len(), 4);
    }

    #[test]
    fn default_reset_returns_unsupported_error() {
        // A client that doesn't override `reset` must still
        // compile against the trait — the default impl returns a
        // clear error so a silent no-op can never slip through.
        struct Dummy;
        impl BenchClient for Dummy {
            fn engine_name(&self) -> &str {
                "dummy"
            }
            fn execute(&mut self, _: &str, _: Duration) -> Result<ExecOutcome, ClientError> {
                unimplemented!()
            }
        }
        let err = Dummy.reset(Duration::from_secs(1)).unwrap_err();
        assert!(
            matches!(err, ClientError::BadResponse(ref s) if s.contains("not supported")),
            "got {err:?}"
        );
    }

    #[test]
    fn client_error_variants_display() {
        // Smoke test: every variant renders a non-empty string so a
        // future enum reshuffle doesn't silently break the harness's
        // error surfacing.
        let cases = [
            ClientError::Transport("x".into()),
            ClientError::Bolt("z".into()),
            ClientError::BadResponse("w".into()),
            ClientError::Timeout(Duration::from_secs(1)),
            ClientError::HealthProbe("v".into()),
        ];
        for e in &cases {
            assert!(!e.to_string().is_empty());
        }
    }
}
