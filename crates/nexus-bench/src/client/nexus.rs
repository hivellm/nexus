//! In-process Nexus client — driven directly by [`nexus_core::Engine`].
//!
//! Runs the query inside the same process, against an engine rooted at
//! a throw-away tempdir. Startup cost is a ~millisecond or two so a
//! benchmark can `reset` between iterations without worrying about
//! cross-run contamination.
//!
//! # Parameters
//!
//! The engine's `execute_cypher` does not currently accept parameters
//! — it parses + plans the query string as-is. The client interpolates
//! `$name` sites by literal substitution for the small set of types
//! the benchmark suite uses (numbers, strings, booleans); complex
//! parameter values (maps, arrays of structs) should be inlined by
//! the scenario generator instead. Production code paths stay on the
//! typed parameter map used by the REST / RPC surfaces.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::Value;
use tempfile::TempDir;

use nexus_core::Engine;

use super::{BenchClient, ClientError, ExecOutcome, Row};

/// In-process benchmark client backed by a fresh `nexus_core::Engine`.
pub struct NexusClient {
    engine: Engine,
    /// Owns the tempdir so the engine's data files live for the
    /// client's lifetime.
    _data_dir: TempDir,
}

impl NexusClient {
    /// Build a client with a default-configuration engine. The engine
    /// lives under a throwaway tempdir that is wiped when the client
    /// drops.
    pub fn new() -> Result<Self, ClientError> {
        let dir = tempfile::tempdir().map_err(|e| ClientError::Setup(e.to_string()))?;
        let engine =
            Engine::with_data_dir(dir.path()).map_err(|e| ClientError::Setup(e.to_string()))?;
        Ok(Self {
            engine,
            _data_dir: dir,
        })
    }

    /// Build a client rooted at an explicit directory. Useful when a
    /// scenario wants to share a pre-loaded dataset across iterations
    /// without paying the reset cost.
    pub fn with_data_dir(path: PathBuf) -> Result<Self, ClientError> {
        std::fs::create_dir_all(&path).map_err(|e| ClientError::Setup(e.to_string()))?;
        // We still hold a tempdir — it just isn't the one we use. The
        // dir field is a lifetime anchor; callers that pick their own
        // path manage cleanup externally.
        let anchor = tempfile::tempdir().map_err(|e| ClientError::Setup(e.to_string()))?;
        let engine = Engine::with_data_dir(&path).map_err(|e| ClientError::Setup(e.to_string()))?;
        Ok(Self {
            engine,
            _data_dir: anchor,
        })
    }
}

impl BenchClient for NexusClient {
    fn engine_name(&self) -> &'static str {
        "nexus"
    }

    fn execute(
        &mut self,
        cypher: &str,
        parameters: &serde_json::Map<String, Value>,
        timeout: Duration,
    ) -> Result<ExecOutcome, ClientError> {
        let query = interpolate_parameters(cypher, parameters);
        let start = Instant::now();
        // `execute_cypher` is sync; we enforce the soft timeout by
        // measuring elapsed time afterwards. Interrupting a sync
        // call requires engine cooperation (future work).
        let rs = self
            .engine
            .execute_cypher(&query)
            .map_err(|e| ClientError::Engine(e.to_string()))?;
        let elapsed = start.elapsed();
        if elapsed > timeout {
            return Err(ClientError::Timeout(elapsed));
        }
        let rows: Vec<Row> = rs.rows.into_iter().map(|r| r.values).collect();
        Ok(ExecOutcome {
            rows,
            engine_reported: Some(elapsed),
        })
    }

    fn reset(&mut self) -> Result<(), ClientError> {
        // Rebuild the engine against a fresh tempdir — cheaper than
        // replaying a DELETE for a wiped benchmark state.
        let dir = tempfile::tempdir().map_err(|e| ClientError::Setup(e.to_string()))?;
        let engine =
            Engine::with_data_dir(dir.path()).map_err(|e| ClientError::Setup(e.to_string()))?;
        self.engine = engine;
        self._data_dir = dir;
        Ok(())
    }
}

/// Minimal `$name` → JSON-literal interpolation. Enough for the
/// benchmark suite's scalar-function scenarios which pass numbers and
/// strings; complex parameter types (maps, arrays of structs) should
/// be inlined by the scenario generator.
fn interpolate_parameters(cypher: &str, params: &serde_json::Map<String, Value>) -> String {
    let mut out = String::with_capacity(cypher.len());
    let mut chars = cypher.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '$' {
            let mut name = String::new();
            while let Some(&n) = chars.peek() {
                if n.is_alphanumeric() || n == '_' {
                    name.push(n);
                    chars.next();
                } else {
                    break;
                }
            }
            if let Some(v) = params.get(&name) {
                out.push_str(&render_value(v));
                continue;
            }
            out.push('$');
            out.push_str(&name);
        } else {
            out.push(c);
        }
    }
    out
}

fn render_value(v: &Value) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => format!("'{}'", s.replace('\'', "\\'")),
        Value::Array(_) | Value::Object(_) => v.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_new_client_on_fresh_tempdir() {
        let client = NexusClient::new().expect("new");
        assert_eq!(client.engine_name(), "nexus");
    }

    #[test]
    fn executes_scalar_return() {
        let mut client = NexusClient::new().unwrap();
        let out = client
            .execute(
                "RETURN 1 + 2 AS sum",
                &serde_json::Map::new(),
                Duration::from_secs(5),
            )
            .expect("execute");
        assert_eq!(out.row_count(), 1);
        // Nexus promotes integer arithmetic to f64 on the RETURN
        // path; assert by numeric equality rather than type-and-
        // value to avoid the 3 vs 3.0 distinction.
        assert_eq!(out.rows[0][0].as_f64(), Some(3.0));
    }

    #[test]
    fn interpolates_numeric_parameter() {
        let mut params = serde_json::Map::new();
        params.insert("x".into(), Value::from(42));
        let q = interpolate_parameters("RETURN $x AS n", &params);
        assert_eq!(q, "RETURN 42 AS n");
    }

    #[test]
    fn interpolates_string_parameter_with_escape() {
        let mut params = serde_json::Map::new();
        params.insert("s".into(), Value::from("o'brien"));
        let q = interpolate_parameters("RETURN $s AS s", &params);
        assert_eq!(q, "RETURN 'o\\'brien' AS s");
    }

    #[test]
    fn unresolved_parameter_passes_through() {
        let q = interpolate_parameters("RETURN $missing AS n", &serde_json::Map::new());
        assert_eq!(q, "RETURN $missing AS n");
    }

    #[test]
    fn reset_clears_state() {
        let mut client = NexusClient::new().unwrap();
        client
            .execute(
                "CREATE (n:Foo {id: 1})",
                &serde_json::Map::new(),
                Duration::from_secs(5),
            )
            .unwrap();
        client.reset().unwrap();
        let out = client
            .execute(
                "MATCH (n:Foo) RETURN count(n) AS c",
                &serde_json::Map::new(),
                Duration::from_secs(5),
            )
            .unwrap();
        assert_eq!(out.rows[0][0], Value::from(0));
    }
}
