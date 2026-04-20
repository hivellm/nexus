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

    fn reset(&mut self, timeout: Duration) -> Result<(), ClientError> {
        // Same wipe the RPC client issues — parity on reset
        // semantics so both engines of a comparative run start
        // each #[ignore] test from an identical empty state.
        self.execute("MATCH (n) DETACH DELETE n", timeout)
            .map(|_| ())
    }
}

/// Convert a Bolt row to the neutral `Vec<serde_json::Value>` shape
/// the harness + divergence guard use.
///
/// neo4rs's `Row` serialises as a sequence of `(field_name, value)`
/// pairs (not a bare value list), so we target `Vec<(String, Value)>`
/// in the serde conversion and strip the keys before returning. The
/// per-column JSON value is what §2.4 calls for — typed, not a
/// `Debug` stand-in.
///
/// Column order is **not** preserved by neo4rs across all row
/// shapes: it reflects the internal iteration order of the
/// underlying Bolt structure, which need not match the `RETURN`
/// clause. The harness's count-based divergence guard tolerates
/// this; the richer row-content divergence guard (§3.4) will
/// normalise keys + values on both sides before comparing.
fn row_to_json(row: &BoltRow) -> Result<Row, ClientError> {
    let pairs: Vec<(String, serde_json::Value)> = row
        .to()
        .map_err(|e| ClientError::BadResponse(format!("bolt row deserialisation failed: {e}")))?;
    Ok(pairs.into_iter().map(|(_, v)| v).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use neo4rs::{BoltList, BoltType, Row as BoltRow};
    use serde_json::{Value, json};

    /// Compile-time smoke that `Neo4jBoltClient` satisfies the same
    /// `Send + Sync + 'static` bounds the harness relies on.
    #[test]
    fn neo4j_client_is_send_sync_benchclient() {
        fn assert_traits<T: BenchClient + Send + Sync + 'static>() {}
        assert_traits::<Neo4jBoltClient>();
    }

    /// Build a synthetic `BoltRow` with given field names and values.
    /// Tests the crate's own wire conversion — no server involved.
    fn make_row(fields: &[&str], data: Vec<BoltType>) -> BoltRow {
        let fields_list: BoltList = BoltList::from(
            fields
                .iter()
                .map(|f| BoltType::from(*f))
                .collect::<Vec<_>>(),
        );
        let data_list: BoltList = BoltList::from(data);
        BoltRow::new(fields_list, data_list)
    }

    #[test]
    fn row_to_json_empty_row() {
        let row = make_row(&[], Vec::new());
        let out = row_to_json(&row).expect("empty row deserialises");
        assert!(out.is_empty(), "empty row should produce empty Vec");
    }

    #[test]
    fn row_to_json_integer_and_string() {
        let row = make_row(
            &["id", "name"],
            vec![BoltType::from(42_i64), BoltType::from("alice")],
        );
        let out = row_to_json(&row).expect("row deserialises");
        assert_eq!(out.len(), 2);
        // neo4rs does not preserve RETURN-clause order across its
        // Row -> serde path, so check set membership rather than
        // positional equality. The richer cross-engine comparison
        // in §3.4 will normalise ordering on both sides.
        assert!(out.contains(&json!(42)), "expected 42 in {out:?}");
        assert!(out.contains(&json!("alice")), "expected 'alice' in {out:?}");
    }

    #[test]
    fn row_to_json_float_and_bool() {
        let row = make_row(
            &["score", "ok"],
            vec![BoltType::from(0.75_f64), BoltType::from(true)],
        );
        let out = row_to_json(&row).expect("row deserialises");
        assert_eq!(out.len(), 2);
        assert!(out.contains(&json!(0.75)));
        assert!(out.contains(&json!(true)));
    }

    #[test]
    fn row_to_json_preserves_null() {
        // Null is a first-class Bolt value — it must come back as
        // JSON null, not as an absent column or the string "null".
        let row = make_row(
            &["present", "absent"],
            vec![BoltType::from(7_i64), BoltType::Null(neo4rs::BoltNull)],
        );
        let out = row_to_json(&row).expect("row deserialises");
        assert_eq!(out.len(), 2);
        assert!(out.contains(&json!(7)));
        assert!(out.contains(&Value::Null));
    }

    #[test]
    fn row_to_json_emits_one_entry_per_column() {
        // Three columns in, three values out — asserts the key
        // stripping did not merge / drop entries.
        let row = make_row(
            &["c", "a", "b"],
            vec![
                BoltType::from(3_i64),
                BoltType::from(1_i64),
                BoltType::from(2_i64),
            ],
        );
        let out = row_to_json(&row).expect("row deserialises");
        assert_eq!(out.len(), 3);
        for expected in [json!(1), json!(2), json!(3)] {
            assert!(out.contains(&expected), "expected {expected} in {out:?}");
        }
    }

    #[test]
    fn row_to_json_nested_list_column() {
        let inner = BoltList::from(vec![
            BoltType::from(1_i64),
            BoltType::from(2_i64),
            BoltType::from(3_i64),
        ]);
        let row = make_row(&["xs"], vec![BoltType::List(inner)]);
        let out = row_to_json(&row).expect("row deserialises");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0], json!([1, 2, 3]));
    }
}
