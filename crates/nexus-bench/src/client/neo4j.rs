//! Neo4j Bolt client (feature-gated on `neo4j`).
//!
//! Wraps `neo4rs::Graph` in a [`BenchClient`]. Requires a running
//! Neo4j server (typically via the Docker harness described in
//! `docs/benchmarks/README.md`). The client internally bridges sync
//! trait calls to async Bolt via `tokio::runtime::Handle::block_on`.

use std::time::{Duration, Instant};

use neo4rs::{ConfigBuilder, Graph};
use serde_json::Value;
use tokio::runtime::Handle;

use super::{BenchClient, ClientError, ExecOutcome, Row};

/// Neo4j Bolt client. Construct via [`Neo4jClient::connect`]; the
/// handle manages a pooled `neo4rs::Graph`.
pub struct Neo4jClient {
    graph: Graph,
    runtime: Handle,
}

impl Neo4jClient {
    /// Connect to a Neo4j server. `uri` is a Bolt URL
    /// (e.g. `neo4j://localhost:7687`); `user` / `password` are the
    /// server's credentials. `runtime` is the tokio handle the sync
    /// trait call bridges through.
    pub async fn connect(
        uri: impl Into<String>,
        user: impl Into<String>,
        password: impl Into<String>,
        runtime: Handle,
    ) -> Result<Self, ClientError> {
        let cfg = ConfigBuilder::new()
            .uri(uri)
            .user(user)
            .password(password)
            .build()
            .map_err(|e| ClientError::Setup(e.to_string()))?;
        let graph = Graph::connect(cfg)
            .await
            .map_err(|e| ClientError::Setup(e.to_string()))?;
        Ok(Self { graph, runtime })
    }
}

impl BenchClient for Neo4jClient {
    fn engine_name(&self) -> &'static str {
        "neo4j"
    }

    fn execute(
        &mut self,
        cypher: &str,
        parameters: &serde_json::Map<String, Value>,
        timeout: Duration,
    ) -> Result<ExecOutcome, ClientError> {
        let graph = self.graph.clone();
        let cypher_owned = cypher.to_string();
        let params = parameters.clone();
        tokio::task::block_in_place(|| {
            self.runtime.block_on(async move {
                let start = Instant::now();
                let mut q = neo4rs::query(&cypher_owned);
                for (k, v) in params.iter() {
                    q = attach_param(q, k, v);
                }
                let mut result = graph
                    .execute(q)
                    .await
                    .map_err(|e| ClientError::Engine(e.to_string()))?;
                let mut rows = Vec::new();
                while let Some(row) = result
                    .next()
                    .await
                    .map_err(|e| ClientError::Engine(e.to_string()))?
                {
                    rows.push(neo4j_row_to_json(row));
                }
                let elapsed = start.elapsed();
                if elapsed > timeout {
                    return Err(ClientError::Timeout(elapsed));
                }
                Ok(ExecOutcome {
                    rows,
                    engine_reported: Some(elapsed),
                })
            })
        })
    }

    fn reset(&mut self) -> Result<(), ClientError> {
        let graph = self.graph.clone();
        tokio::task::block_in_place(|| {
            self.runtime.block_on(async move {
                // MATCH (n) DETACH DELETE n wipes data without
                // touching indexes / constraints. Scenarios re-create
                // schema from their own setup.
                graph
                    .run(neo4rs::query("MATCH (n) DETACH DELETE n"))
                    .await
                    .map_err(|e| ClientError::Engine(e.to_string()))
            })
        })?;
        Ok(())
    }
}

fn attach_param(q: neo4rs::Query, key: &str, value: &Value) -> neo4rs::Query {
    match value {
        Value::Null => q.param(key, neo4rs::BoltType::Null(neo4rs::BoltNull)),
        Value::Bool(b) => q.param(key, *b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                q.param(key, i)
            } else if let Some(f) = n.as_f64() {
                q.param(key, f)
            } else {
                // Spec-illegal number (like very large u64) — pass
                // as string so the server surfaces a clear error
                // rather than the client silently dropping it.
                q.param(key, n.to_string())
            }
        }
        Value::String(s) => q.param(key, s.as_str()),
        Value::Array(_) | Value::Object(_) => q.param(key, value.to_string()),
    }
}

/// Convert a neo4rs row into the Neo4j-compatible JSON array form
/// the benchmark harness expects.
///
/// neo4rs 0.8's `Row` does not implement `Serialize` and does not
/// expose the underlying column list as a public API, so a faithful
/// per-column extraction requires knowing the scenario's RETURN
/// column names ahead of time. For the initial harness release we
/// return a single-element row carrying the row's `Debug` form —
/// enough for the output-divergence guard (row-count equality) and
/// good enough for the first wave of scalar / aggregation
/// scenarios. A typed-extraction pass is tracked in the
/// phase6 benchmark suite's follow-up items.
fn neo4j_row_to_json(row: neo4rs::Row) -> Row {
    vec![Value::String(format!("{row:?}"))]
}

#[cfg(test)]
mod tests {
    // neo4rs construction requires a live server; integration tests
    // for this client live under `tests/` behind an env-var gate so
    // `cargo test --all-features` without Neo4j still passes.
    //
    // Unit tests here exercise the pure-value helpers.
    use super::*;

    #[test]
    fn attach_param_bool_shape() {
        let q = neo4rs::query("RETURN $x");
        let _ = attach_param(q, "x", &Value::from(true));
    }

    #[test]
    fn attach_param_string_shape() {
        let q = neo4rs::query("RETURN $s");
        let _ = attach_param(q, "s", &Value::from("hi"));
    }
}
