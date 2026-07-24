//! Loads an LDBC SNB dataset into the Neo4j baseline through its Cypher HTTP
//! transaction endpoint, producing a graph identical to the one `load.rs`
//! writes into Nexus.
//!
//! Neo4j resolves edge endpoints by matching on the LDBC `id` property, so —
//! unlike the Nexus loader — this needs no id correlation map: it creates an
//! `id` index per label up front, then `UNWIND $rows MATCH … CREATE …` finds
//! both endpoints by id. Labels and relationship types cannot be Cypher
//! parameters, so they are baked into the statement string (drawn only from the
//! static `schema`, never from CSV data, so there is nothing to inject).

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Map, Value};
use std::path::Path;
use std::time::Instant;

use crate::csv_source::{coerce, parse_id, CsvSource};
use crate::schema::{EdgeFile, FkDirection, NodeFile, EDGE_FILES, NODE_FILES};

/// Minimal client for Neo4j's transactional Cypher HTTP endpoint
/// (`POST /db/<db>/tx/commit`).
pub struct Neo4jClient {
    endpoint: String,
    agent: ureq::Agent,
}

impl Neo4jClient {
    pub fn new(base_url: &str, database: &str) -> Self {
        let base = base_url.trim_end_matches('/');
        Self {
            endpoint: format!("{base}/db/{database}/tx/commit"),
            agent: ureq::AgentBuilder::new()
                .timeout_connect(std::time::Duration::from_secs(5))
                .build(),
        }
    }

    /// Run one Cypher statement with parameters, discarding the rows and
    /// failing on any Neo4j-reported error. Used for every write here (the
    /// loader never reads rows back through this path — verification counts go
    /// through `count`).
    pub fn run(&self, statement: &str, parameters: Value) -> Result<()> {
        let body: Value = self.post(statement, parameters)?;
        Self::check_errors(&body, statement)
    }

    /// `RETURN count(...)`-shaped query → the single integer cell.
    pub fn count(&self, statement: &str) -> Result<i64> {
        let body = self.post(statement, json!({}))?;
        Self::check_errors(&body, statement)?;
        body.get("results")
            .and_then(|r| r.get(0))
            .and_then(|r| r.get("data"))
            .and_then(|d| d.get(0))
            .and_then(|row| row.get("row"))
            .and_then(|row| row.get(0))
            .and_then(Value::as_i64)
            .ok_or_else(|| anyhow!("`{statement}` did not return a count; body = {body}"))
    }

    /// Preflight: a trivial query so a wrong URL or a Neo4j that is not up
    /// fails before the first CSV is read.
    pub fn ping(&self) -> Result<()> {
        self.run("RETURN 1", json!({}))
            .context("Neo4j is not reachable / not ready")
    }

    /// Delete every node and relationship in batches. A single
    /// `MATCH (n) DETACH DELETE n` over a loaded SF0.1 graph exhausts the
    /// bench container's 512 MiB heap, so this deletes a bounded number of
    /// nodes per statement and loops until none remain. `DETACH DELETE` drops
    /// each node's relationships too, so the relationships go with them.
    pub fn wipe(&self, batch: usize) -> Result<()> {
        loop {
            let deleted = self.count(&format!(
                "MATCH (n) WITH n LIMIT {batch} DETACH DELETE n RETURN count(n)"
            ))?;
            if deleted == 0 {
                return Ok(());
            }
        }
    }

    fn post(&self, statement: &str, parameters: Value) -> Result<Value> {
        let response = self
            .agent
            .post(&self.endpoint)
            .send_json(json!({
                "statements": [{ "statement": statement, "parameters": parameters }]
            }))
            .map_err(|e| match e {
                ureq::Error::Status(code, r) => {
                    let body = r.into_string().unwrap_or_else(|e| format!("<{e}>"));
                    anyhow!("Neo4j HTTP {code}: {body}")
                }
                other => anyhow!("Neo4j request failed: {other}"),
            })?;
        response.into_json().context("decoding the Neo4j response")
    }

    /// Neo4j returns HTTP 200 even for a Cypher error — the failure is in the
    /// `errors` array, so it must be inspected explicitly.
    fn check_errors(body: &Value, statement: &str) -> Result<()> {
        if let Some(errors) = body.get("errors").and_then(Value::as_array) {
            if let Some(first) = errors.first() {
                let code = first.get("code").and_then(Value::as_str).unwrap_or("?");
                let message = first.get("message").and_then(Value::as_str).unwrap_or("?");
                bail!("Neo4j rejected `{statement}`: [{code}] {message}");
            }
        }
        Ok(())
    }
}

pub struct Neo4jLoader<'a> {
    pub client: &'a Neo4jClient,
    pub dataset: &'a Path,
    pub batch_rows: usize,
}

impl Neo4jLoader<'_> {
    /// Create a range index on `id` for every node label before loading, so
    /// the per-row edge `MATCH (a:L {id: …})` is a seek, not an O(N) scan. A
    /// 1.5 M-edge load with unindexed lookups would be quadratic.
    pub fn create_indexes(&self) -> Result<()> {
        // A range index on `id` per PRIMARY label, plus one on the `:Message`
        // superlabel so IS4–IS7 (`MATCH (m:Message {id: …})`) seek instead of
        // scanning all 286k messages.
        let mut labels: Vec<&str> = NODE_FILES.iter().map(|f| f.label).collect();
        labels.push("Message");
        for label in labels {
            let index = format!("ldbc_{}_id", label.to_lowercase());
            self.client.run(
                &format!("CREATE INDEX {index} IF NOT EXISTS FOR (n:{label}) ON (n.id)"),
                json!({}),
            )?;
        }
        Ok(())
    }

    /// Pass 1 — nodes, static labels first (so pass 2's foreign keys resolve).
    pub fn load_nodes(&self) -> Result<()> {
        for file in NODE_FILES {
            let started = Instant::now();
            let count = self.load_node_file(file)?;
            report(file.path, count, started);
        }
        Ok(())
    }

    fn load_node_file(&self, file: &'static NodeFile) -> Result<usize> {
        let mut source = CsvSource::open(&self.dataset.join(file.path))?;
        let id_column = source.column(file.id_column)?;
        let property_columns = file
            .properties
            .iter()
            .map(|p| Ok((source.column(p.column)?, p)))
            .collect::<Result<Vec<_>>>()?;
        // `:Post:Message` — labels are baked in, never parameterized.
        let labels = file
            .all_labels()
            .iter()
            .map(|l| format!(":{l}"))
            .collect::<String>();
        let statement = format!("UNWIND $rows AS props CREATE (n{labels}) SET n = props");

        let mut batch: Vec<Value> = Vec::new();
        let mut total = 0usize;
        for row in source.rows() {
            let row = row?;
            // Neo4j resolves edges by this id later, so it must be stored.
            let _ = parse_id(&row[id_column], file.label)?
                .with_context(|| format!("{}: a node row has an empty id", file.path))?;
            let mut props = Map::new();
            for (index, property) in &property_columns {
                if let Some(value) = coerce(&row[*index], property.coerce)
                    .with_context(|| format!("{} column `{}`", file.path, property.column))?
                {
                    props.insert(property.name.to_string(), value);
                }
            }
            batch.push(Value::Object(props));
            total += 1;
            if batch.len() >= self.batch_rows {
                self.flush(&statement, &mut batch)?;
            }
        }
        self.flush(&statement, &mut batch)?;
        Ok(total)
    }

    /// Pass 2 — merge-foreign relationships folded into the node CSVs.
    pub fn load_foreign_key_edges(&self) -> Result<()> {
        for file in NODE_FILES {
            if file.foreign_keys.is_empty() {
                continue;
            }
            let started = Instant::now();
            let mut total = 0usize;
            for fk in file.foreign_keys {
                total += self.load_one_foreign_key(file, fk)?;
            }
            report(&format!("{} (foreign keys)", file.path), total, started);
        }
        Ok(())
    }

    fn load_one_foreign_key(
        &self,
        file: &'static NodeFile,
        fk: &'static crate::schema::ForeignKey,
    ) -> Result<usize> {
        let mut source = CsvSource::open(&self.dataset.join(file.path))?;
        let id_column = source.column(file.id_column)?;
        let fk_column = source.column(fk.column)?;
        // The owner row is `file.label`; the referenced node is
        // `fk.target_label`. Direction decides which is src/dst.
        let (src_label, dst_label) = match fk.direction {
            FkDirection::Outgoing => (file.label, fk.target_label),
            FkDirection::Incoming => (fk.target_label, file.label),
        };
        let statement = edge_statement(src_label, dst_label, fk.rel_type);

        let mut batch: Vec<Value> = Vec::new();
        let mut total = 0usize;
        for row in source.rows() {
            let row = row?;
            let owner = parse_id(&row[id_column], file.label)?
                .with_context(|| format!("{}: a node row has an empty id", file.path))?;
            let Some(target) = parse_id(&row[fk_column], fk.column)? else {
                continue; // empty foreign key — no edge
            };
            let (src, dst) = match fk.direction {
                FkDirection::Outgoing => (owner, target),
                FkDirection::Incoming => (target, owner),
            };
            batch.push(json!({ "src": src, "dst": dst, "props": {} }));
            total += 1;
            if batch.len() >= self.batch_rows {
                self.flush(&statement, &mut batch)?;
            }
        }
        self.flush(&statement, &mut batch)?;
        Ok(total)
    }

    /// Pass 3 — the explicit edge files.
    pub fn load_edge_files(&self) -> Result<()> {
        for file in EDGE_FILES {
            let started = Instant::now();
            let count = self.load_edge_file(file)?;
            report(file.path, count, started);
        }
        Ok(())
    }

    fn load_edge_file(&self, file: &'static EdgeFile) -> Result<usize> {
        let mut source = CsvSource::open(&self.dataset.join(file.path))?;
        let src_column = source.nth_column(file.src.0, 0)?;
        let dst_column = if file.src.0 == file.dst.0 {
            source.nth_column(file.dst.0, 1)?
        } else {
            source.column(file.dst.0)?
        };
        let property_columns = file
            .properties
            .iter()
            .map(|p| Ok((source.column(p.column)?, p)))
            .collect::<Result<Vec<_>>>()?;
        let path = source.path().to_string();
        // One edge per CSV row, in the recorded direction. `person_knows_person`
        // is stored once per friendship and is NOT mirrored — the reference
        // queries traverse it undirected (`-[:KNOWS]-`), so a second stored
        // direction would double every friendship.
        let statement = edge_statement(file.src.1, file.dst.1, file.rel_type);

        let mut batch: Vec<Value> = Vec::new();
        let mut total = 0usize;
        for row in source.rows() {
            let row = row?;
            let src = parse_id(&row[src_column], file.src.0)?
                .with_context(|| format!("{path}: empty source id"))?;
            let dst = parse_id(&row[dst_column], file.dst.0)?
                .with_context(|| format!("{path}: empty destination id"))?;
            let mut props = Map::new();
            for (index, property) in &property_columns {
                if let Some(value) = coerce(&row[*index], property.coerce)
                    .with_context(|| format!("{path} column `{}`", property.column))?
                {
                    props.insert(property.name.to_string(), value);
                }
            }
            batch.push(json!({ "src": src, "dst": dst, "props": props }));
            total += 1;
            if batch.len() >= self.batch_rows {
                self.flush(&statement, &mut batch)?;
            }
        }
        self.flush(&statement, &mut batch)?;
        Ok(total)
    }

    fn flush(&self, statement: &str, batch: &mut Vec<Value>) -> Result<()> {
        if batch.is_empty() {
            return Ok(());
        }
        self.client
            .run(statement, json!({ "rows": std::mem::take(batch) }))?;
        batch.clear();
        Ok(())
    }
}

/// `UNWIND $rows AS r MATCH (a:Src {id:r.src}), (b:Dst {id:r.dst}) CREATE
/// (a)-[e:TYPE]->(b) SET e = r.props`. When `props` is always empty (the
/// merge-foreign edges) the `SET` is harmless — an empty map sets nothing.
fn edge_statement(src_label: &str, dst_label: &str, rel_type: &str) -> String {
    format!(
        "UNWIND $rows AS r \
         MATCH (a:{src_label} {{id: r.src}}), (b:{dst_label} {{id: r.dst}}) \
         CREATE (a)-[e:{rel_type}]->(b) SET e = r.props"
    )
}

fn report(what: &str, rows: usize, started: Instant) {
    let seconds = started.elapsed().as_secs_f64();
    let rate = if seconds > 0.0 {
        rows as f64 / seconds
    } else {
        0.0
    };
    println!("  {what:<52} {rows:>9} rows  {seconds:>7.1}s  {rate:>9.0} rows/s");
}
