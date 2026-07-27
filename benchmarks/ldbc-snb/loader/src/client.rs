//! Thin blocking HTTP client for the two Nexus endpoints the loader needs:
//! `POST /ingest` (bulk writes) and `POST /cypher` (count verification).

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};

/// Subset of `IngestResponse` the loader acts on.
#[derive(Debug, Deserialize)]
pub struct IngestResponse {
    pub nodes_ingested: usize,
    pub relationships_ingested: usize,
    /// Internal ids of the created nodes, in input order. A row that FAILED is
    /// skipped rather than padded, which is why the loader treats a short
    /// vector as a fatal desync instead of trying to realign.
    #[serde(default)]
    pub node_ids: Vec<u64>,
    #[serde(default)]
    pub error: Option<String>,
}

pub struct NexusClient {
    base_url: String,
    agent: ureq::Agent,
}

impl NexusClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            // No global timeout: a single ingest batch legitimately takes
            // seconds, and a mid-load timeout would leave the graph in a
            // half-written state that the count check could only report, not
            // repair. Connect timeout still applies so a wrong `--url` fails
            // fast instead of hanging.
            agent: ureq::AgentBuilder::new()
                .timeout_connect(std::time::Duration::from_secs(5))
                .build(),
        }
    }

    /// `GET /health` — used as a preflight so a wrong URL or a server that is
    /// not up fails before the first CSV is read.
    pub fn health(&self) -> Result<()> {
        self.agent
            .get(&format!("{}/health", self.base_url))
            .call()
            .with_context(|| format!("Nexus is not reachable at {}", self.base_url))?;
        Ok(())
    }

    /// `POST /ingest` with only nodes. Returns the created internal ids in
    /// input order, verified to line up 1:1 with what was sent.
    pub fn ingest_nodes(&self, nodes: Vec<Value>) -> Result<Vec<u64>> {
        let sent = nodes.len();
        let response = self.ingest(json!({
            "nodes": nodes,
            "relationships": [],
            // One lock acquisition for the whole request: the loader already
            // sizes its own batches, so a second layer of chunking inside the
            // server would only add lock churn.
            "use_batching": false,
        }))?;

        if let Some(error) = response.error {
            bail!("ingest reported errors for a node batch: {error}");
        }
        if response.nodes_ingested != sent || response.node_ids.len() != sent {
            bail!(
                "ingest created {} nodes and returned {} ids for {sent} sent — the \
                 positional id correlation is broken, refusing to continue with a \
                 graph whose edges would point at the wrong nodes",
                response.nodes_ingested,
                response.node_ids.len()
            );
        }
        Ok(response.node_ids)
    }

    /// `POST /ingest` with only relationships, whose `src`/`dst` are literal
    /// internal node ids (no `nodes` in the request means the server's
    /// request-scoped correlation map is empty, so endpoints resolve directly).
    pub fn ingest_relationships(&self, relationships: Vec<Value>) -> Result<()> {
        let sent = relationships.len();
        let response = self.ingest(json!({
            "nodes": [],
            "relationships": relationships,
            "use_batching": false,
        }))?;

        if let Some(error) = response.error {
            bail!("ingest reported errors for a relationship batch: {error}");
        }
        if response.relationships_ingested != sent {
            bail!(
                "ingest created {} relationships out of {sent} sent",
                response.relationships_ingested
            );
        }
        Ok(())
    }

    fn ingest(&self, body: Value) -> Result<IngestResponse> {
        let response = self
            .agent
            .post(&format!("{}/ingest", self.base_url))
            .send_json(body)
            .map_err(|e| describe_transport_error("POST /ingest", e))?;
        response
            .into_json::<IngestResponse>()
            .context("decoding the /ingest response")
    }

    /// `GET /stats` → `catalog.rel_count`, the engine's own count of created
    /// relationships.
    ///
    /// This is the authoritative check that the writes landed: it is
    /// incremented inside the write path, one per created relationship, so it
    /// cannot be affected by the traversal read bug that makes
    /// `MATCH ()-[r:T]->() RETURN count(r)` non-deterministic
    /// (`phase7_opencypher-gap-closure` 4.8). It only holds for a load that
    /// started from an empty database — the counter never decrements.
    pub fn catalog_relationship_count(&self) -> Result<i64> {
        let body: Value = self
            .agent
            .get(&format!("{}/stats", self.base_url))
            .call()
            .map_err(|e| describe_transport_error("GET /stats", e))?
            .into_json()
            .context("decoding the /stats response")?;
        body.get("catalog")
            .and_then(|c| c.get("rel_count"))
            .and_then(Value::as_i64)
            .ok_or_else(|| anyhow!("/stats has no catalog.rel_count; body = {body}"))
    }

    /// Run a Cypher query and return the first column of the first row as an
    /// integer — the shape every count query in the verification pass has.
    pub fn count(&self, cypher: &str) -> Result<i64> {
        let response = self
            .agent
            .post(&format!("{}/cypher", self.base_url))
            .send_json(json!({ "query": cypher }))
            .map_err(|e| describe_transport_error("POST /cypher", e))?;
        let body: Value = response
            .into_json()
            .context("decoding the /cypher response")?;

        body.get("rows")
            .and_then(|rows| rows.get(0))
            .and_then(|row| row.get(0))
            .and_then(Value::as_i64)
            .ok_or_else(|| anyhow!("`{cypher}` did not return a count; body = {body}"))
    }
}

/// ureq folds an HTTP error status into `Error::Status`, whose `Display` is
/// just the code — the server's own message (which says WHICH row failed) is
/// in the body, so pull it out or the operator gets "400 Bad Request" and
/// nothing else.
fn describe_transport_error(what: &str, error: ureq::Error) -> anyhow::Error {
    match error {
        ureq::Error::Status(code, response) => {
            let body = response
                .into_string()
                .unwrap_or_else(|e| format!("<unreadable body: {e}>"));
            anyhow!("{what} failed with HTTP {code}: {body}")
        }
        other => anyhow!("{what} failed: {other}"),
    }
}
