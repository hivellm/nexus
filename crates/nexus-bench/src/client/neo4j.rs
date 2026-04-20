//! Neo4j Bolt bench client. Feature-gated on `neo4j` (composable
//! with `live-bench`).
//!
//! Targets a Neo4j server over the Bolt protocol via `neo4rs`.
//! Wraps every call in `tokio::time::timeout` so a hung server
//! cannot wedge the harness — the same guard-rail discipline the
//! HTTP client applies to reqwest — and performs a `RETURN 1`
//! health probe on connect, matching [`super::HttpClient`]'s
//! contract.
//!
//! Authentication: pass the Neo4j user + password. For a compose
//! container running with `NEO4J_AUTH=none` the server accepts any
//! credentials at HELLO time, so the default `neo4j` / `neo4j`
//! pair works in both the `AUTH=none` and the stock-password
//! modes the bench docker-compose may run in.

use std::time::Duration;

use neo4rs::{ConfigBuilder, Graph, Row as BoltRow, query};
use tokio::runtime::Handle;

use super::{BenchClient, ClientError, ExecOutcome, Row};

/// Bench client that talks to a Neo4j server over Bolt.
pub struct Neo4jBoltClient {
    graph: Graph,
    engine_label: String,
    runtime: Handle,
}

impl Neo4jBoltClient {
    /// Connect to a Neo4j server and run a `RETURN 1` health probe
    /// within 2 seconds — same cap the HTTP client enforces on its
    /// `/health` probe so both sides of a comparative run fail
    /// fast on identical timing criteria.
    pub async fn connect(
        uri: impl Into<String>,
        user: impl Into<String>,
        password: impl Into<String>,
        engine_label: impl Into<String>,
        runtime: Handle,
    ) -> Result<Self, ClientError> {
        let uri = uri.into();
        let user = user.into();
        let password = password.into();
        let engine_label = engine_label.into();

        let config = ConfigBuilder::default()
            .uri(uri.as_str())
            .user(user.as_str())
            .password(password.as_str())
            .build()
            .map_err(|e| ClientError::Bolt(e.to_string()))?;

        // 5 s ceiling on the HELLO round-trip — if the server is not
        // answering bolt within that window, something is wrong with
        // the container / port-forward, and waiting longer only
        // buries the signal.
        let graph = tokio::time::timeout(Duration::from_secs(5), Graph::connect(config))
            .await
            .map_err(|_| ClientError::HealthProbe("bolt connect timed out after 5 s".into()))?
            .map_err(|e| ClientError::Bolt(e.to_string()))?;

        // Health probe: RETURN 1 must come back within 2 s, parity
        // with the HTTP client's /health probe window.
        let probe = async {
            let mut stream = graph
                .execute(query("RETURN 1 AS n"))
                .await
                .map_err(|e| ClientError::HealthProbe(e.to_string()))?;
            // Drain one row so the server actually executes, not
            // just parses. Any further rows are discarded — RETURN 1
            // only produces one.
            let _ = stream
                .next()
                .await
                .map_err(|e| ClientError::HealthProbe(e.to_string()))?;
            Ok::<(), ClientError>(())
        };
        tokio::time::timeout(Duration::from_secs(2), probe)
            .await
            .map_err(|_| ClientError::HealthProbe("RETURN 1 timed out after 2 s".into()))??;

        Ok(Self {
            graph,
            engine_label,
            runtime,
        })
    }

    /// Runtime handle the sync [`BenchClient`] impl uses to bridge
    /// into neo4rs's async API.
    pub fn runtime(&self) -> &Handle {
        &self.runtime
    }

    /// Shared execute path — called from the sync [`BenchClient`]
    /// impl. Bounded by `timeout` end-to-end: any neo4rs call that
    /// blocks longer than the caller-supplied budget gets cancelled
    /// and surfaced as [`ClientError::Timeout`].
    async fn execute_async(
        &self,
        cypher: &str,
        timeout: Duration,
    ) -> Result<ExecOutcome, ClientError> {
        let run = async {
            let mut stream = self
                .graph
                .execute(query(cypher))
                .await
                .map_err(|e| ClientError::Bolt(e.to_string()))?;
            let mut rows: Vec<Row> = Vec::new();
            while let Some(bolt_row) = stream
                .next()
                .await
                .map_err(|e| ClientError::Bolt(e.to_string()))?
            {
                rows.push(row_to_json(&bolt_row)?);
            }
            Ok::<_, ClientError>(ExecOutcome { rows })
        };
        tokio::time::timeout(timeout, run)
            .await
            .map_err(|_| ClientError::Timeout(timeout))?
    }
}

impl BenchClient for Neo4jBoltClient {
    fn engine_name(&self) -> &str {
        &self.engine_label
    }

    fn execute(&mut self, cypher: &str, timeout: Duration) -> Result<ExecOutcome, ClientError> {
        let cypher = cypher.to_string();
        tokio::task::block_in_place(|| {
            self.runtime
                .block_on(async { self.execute_async(&cypher, timeout).await })
        })
    }
}

/// Convert a Bolt row to the neutral `Vec<serde_json::Value>` shape
/// the harness + divergence guard use. Relies on neo4rs's serde
/// support: a row's data list deserialises directly into a JSON
/// sequence, one entry per column in the order the `RETURN` clause
/// declared. This is what §2.4 of the parent task means by "typed
/// per-column row extraction, no Debug stand-in".
fn row_to_json(row: &BoltRow) -> Result<Row, ClientError> {
    row.to::<Vec<serde_json::Value>>()
        .map_err(|e| ClientError::BadResponse(format!("bolt row deserialisation failed: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compile-time smoke that `Neo4jBoltClient` satisfies the same
    /// `Send + Sync + 'static` bounds the harness relies on.
    #[test]
    fn neo4j_client_is_send_sync_benchclient() {
        fn assert_traits<T: BenchClient + Send + Sync + 'static>() {}
        assert_traits::<Neo4jBoltClient>();
    }
}
