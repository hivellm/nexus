//! RESP3 command dispatcher — routes an incoming array of arguments onto
//! the concrete Nexus handler and produces the `Resp3Value` to frame back.
//!
//! Design: one `mod` per command group (admin, cypher, graph, knn, schema).
//! Each handler is `async fn` (most of them need the executor or database
//! manager) and returns a single `Resp3Value`. Wrong-arity and unknown
//! commands are handled centrally in [`dispatch`]; command handlers can
//! assume their inputs have already been typed-checked via the `arg_*`
//! helpers in this module.

pub mod admin;
pub mod cypher;
pub mod graph;
pub mod knn;
pub mod schema;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::NexusServer;
use crate::protocol::resp3::parser::Resp3Value;
use crate::protocol::resp3::writer::ProtocolVersion;

/// Per-connection state carried through every command invocation.
///
/// This is the minimum slice of server state every command needs. Keeping
/// it small means individual handlers are easy to test in isolation.
pub struct SessionState {
    /// Shared server state (executor, database manager, auth, etc.).
    pub server: Arc<NexusServer>,
    /// True once the client has authenticated successfully. Guarded by an
    /// `AtomicBool` so the connection loop (which reads it) and the dispatch
    /// (which writes it via `HELLO AUTH` / `AUTH`) don't need a mutex.
    pub authenticated: Arc<AtomicBool>,
    /// Whether authentication is required for this listener. If false, any
    /// command is accepted regardless of the `authenticated` flag.
    pub auth_required: bool,
    /// Protocol version negotiated via HELLO. Shared with the writer so an
    /// `AUTH` that also flips protocol propagates immediately.
    pub protocol: Arc<std::sync::atomic::AtomicU8>,
    /// Unique connection id (used by `HELLO` replies and client logs).
    pub connection_id: u64,
}

impl SessionState {
    /// Set the protocol variant for this session.
    pub fn set_protocol(&self, version: ProtocolVersion) {
        let code = match version {
            ProtocolVersion::Resp2 => 2,
            ProtocolVersion::Resp3 => 3,
        };
        self.protocol.store(code, Ordering::Relaxed);
    }

    /// Read the protocol variant negotiated for this session.
    pub fn protocol(&self) -> ProtocolVersion {
        match self.protocol.load(Ordering::Relaxed) {
            2 => ProtocolVersion::Resp2,
            _ => ProtocolVersion::Resp3,
        }
    }

    /// True iff this session is authenticated (or auth is disabled for the
    /// listener entirely).
    pub fn is_authorised(&self) -> bool {
        !self.auth_required || self.authenticated.load(Ordering::Relaxed)
    }
}

/// Route an already-parsed RESP3 command array onto its handler.
///
/// Uppercases `args[0]` for case-insensitive matching (Redis convention).
/// Unknown commands yield `-ERR unknown command '<name>' (Nexus is a graph
/// DB, see HELP)`. Wrong arity yields `-ERR wrong number of arguments for
/// '<name>' command` (also Redis convention) — callers always get an
/// `Resp3Value` back, never a panic.
pub async fn dispatch(state: &SessionState, args: Vec<Resp3Value>) -> Resp3Value {
    if args.is_empty() {
        return err("ERR empty command");
    }

    let name = match args[0].as_str() {
        Some(s) => s.to_ascii_uppercase(),
        None => return err("ERR first argument must be a command name"),
    };

    // Pre-auth commands: always allowed even when `authenticated == false`.
    let pre_auth = matches!(
        name.as_str(),
        "PING" | "HELLO" | "AUTH" | "QUIT" | "HELP" | "COMMAND"
    );
    if !state.is_authorised() && !pre_auth {
        return Resp3Value::Error("NOAUTH Authentication required.".into());
    }

    match name.as_str() {
        // ---- Admin ------------------------------------------------------
        "PING" => admin::ping(state, &args).await,
        "HELLO" => admin::hello(state, &args).await,
        "AUTH" => admin::auth(state, &args).await,
        "QUIT" => admin::quit(state, &args).await,
        "HELP" => admin::help(state, &args).await,
        "COMMAND" => admin::command(state, &args).await,
        // ---- Cypher -----------------------------------------------------
        "CYPHER" => cypher::cypher(state, &args).await,
        "CYPHER.WITH" => cypher::cypher_with(state, &args).await,
        "CYPHER.EXPLAIN" => cypher::cypher_explain(state, &args).await,
        // ---- Graph CRUD -------------------------------------------------
        "NODE.CREATE" => graph::node_create(state, &args).await,
        "NODE.GET" => graph::node_get(state, &args).await,
        "NODE.UPDATE" => graph::node_update(state, &args).await,
        "NODE.DELETE" => graph::node_delete(state, &args).await,
        "NODE.MATCH" => graph::node_match(state, &args).await,
        "REL.CREATE" => graph::rel_create(state, &args).await,
        "REL.GET" => graph::rel_get(state, &args).await,
        "REL.DELETE" => graph::rel_delete(state, &args).await,
        // ---- KNN / ingest ----------------------------------------------
        "KNN.SEARCH" => knn::knn_search(state, &args).await,
        "KNN.TRAVERSE" => knn::knn_traverse(state, &args).await,
        "INGEST.NODES" => knn::ingest_nodes(state, &args).await,
        "INGEST.RELS" => knn::ingest_rels(state, &args).await,
        // ---- Schema, indexes, databases --------------------------------
        "INDEX.CREATE" => schema::index_create(state, &args).await,
        "INDEX.DROP" => schema::index_drop(state, &args).await,
        "INDEX.LIST" => schema::index_list(state, &args).await,
        "DB.LIST" => schema::db_list(state, &args).await,
        "DB.CREATE" => schema::db_create(state, &args).await,
        "DB.DROP" => schema::db_drop(state, &args).await,
        "DB.USE" => schema::db_use(state, &args).await,
        "LABELS" => schema::labels(state, &args).await,
        "REL_TYPES" => schema::rel_types(state, &args).await,
        "PROPERTY_KEYS" => schema::property_keys(state, &args).await,
        "STATS" => schema::stats(state, &args).await,
        "HEALTH" => schema::health(state, &args).await,
        // ---- Unknown ----------------------------------------------------
        other => err(format!(
            "ERR unknown command '{}' (Nexus is a graph DB, see HELP)",
            other
        )),
    }
}

// --------------------------------------------------------------------------
// Argument helpers — uniform error messages + early return on bad input.
// --------------------------------------------------------------------------

/// Build a `-ERR <msg>` value. Helpers in subcommand modules use this for
/// every validation failure so the wire text stays consistent.
pub fn err<S: Into<String>>(msg: S) -> Resp3Value {
    Resp3Value::Error(msg.into())
}

/// Require a UTF-8 string at `args[idx]`. Returns `Err(resp_value)` ready
/// to be forwarded to the caller if the index is out of bounds or the
/// value is not a string.
pub fn arg_str_required<'a>(
    args: &'a [Resp3Value],
    idx: usize,
    cmd: &str,
) -> Result<&'a str, Resp3Value> {
    match args.get(idx).and_then(Resp3Value::as_str) {
        Some(s) => Ok(s),
        None => Err(err(format!(
            "ERR wrong number of arguments for '{}' command",
            cmd
        ))),
    }
}

/// Require an integer at `args[idx]`.
pub fn arg_int_required(args: &[Resp3Value], idx: usize, cmd: &str) -> Result<i64, Resp3Value> {
    match args.get(idx).and_then(Resp3Value::as_int) {
        Some(n) => Ok(n),
        None => Err(err(format!(
            "ERR argument {} of '{}' must be an integer",
            idx, cmd
        ))),
    }
}

/// Require a bytes payload at `args[idx]` (either BulkString or coercible).
pub fn arg_bytes_required<'a>(
    args: &'a [Resp3Value],
    idx: usize,
    cmd: &str,
) -> Result<&'a [u8], Resp3Value> {
    match args.get(idx) {
        Some(Resp3Value::BulkString(b)) => Ok(b.as_slice()),
        Some(other) => match other.as_str() {
            Some(s) => Ok(s.as_bytes()),
            None => Err(err(format!(
                "ERR argument {} of '{}' must be a bulk string",
                idx, cmd
            ))),
        },
        None => Err(err(format!(
            "ERR wrong number of arguments for '{}' command",
            cmd
        ))),
    }
}

/// Require a JSON-shaped map at `args[idx]` (supplied as a string and
/// parsed eagerly so downstream handlers get a typed `serde_json::Value`).
pub fn arg_json_required(
    args: &[Resp3Value],
    idx: usize,
    cmd: &str,
) -> Result<serde_json::Value, Resp3Value> {
    let raw = arg_str_required(args, idx, cmd)?;
    serde_json::from_str(raw).map_err(|e| {
        err(format!(
            "ERR argument {} of '{}' is not valid JSON: {}",
            idx, cmd, e
        ))
    })
}

/// Check that the command has exactly `expected` arguments (the command
/// name counts as one). Emits the Redis-style wrong-arity error.
pub fn expect_arity(args: &[Resp3Value], expected: usize, cmd: &str) -> Option<Resp3Value> {
    if args.len() != expected {
        Some(err(format!(
            "ERR wrong number of arguments for '{}' command",
            cmd
        )))
    } else {
        None
    }
}

/// Check that the command has between `min` and `max` arguments, inclusive.
pub fn expect_arity_range(
    args: &[Resp3Value],
    min: usize,
    max: usize,
    cmd: &str,
) -> Option<Resp3Value> {
    if args.len() < min || args.len() > max {
        Some(err(format!(
            "ERR wrong number of arguments for '{}' command",
            cmd
        )))
    } else {
        None
    }
}

/// Check that the command has at least `min` arguments.
pub fn expect_arity_min(args: &[Resp3Value], min: usize, cmd: &str) -> Option<Resp3Value> {
    if args.len() < min {
        Some(err(format!(
            "ERR wrong number of arguments for '{}' command",
            cmd
        )))
    } else {
        None
    }
}

// --------------------------------------------------------------------------
// Tests.
// --------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> Vec<Resp3Value> {
        items.iter().map(|s| Resp3Value::bulk(*s)).collect()
    }

    #[test]
    fn arg_str_required_returns_string_or_wrong_arity_error() {
        let a = args(&["PING"]);
        assert_eq!(arg_str_required(&a, 0, "PING").unwrap(), "PING");
        let err = arg_str_required(&a, 1, "PING").unwrap_err();
        match err {
            Resp3Value::Error(s) => assert!(s.contains("wrong number of arguments")),
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[test]
    fn arg_int_required_parses_numeric_bulk() {
        let a = args(&["X", "42"]);
        assert_eq!(arg_int_required(&a, 1, "X").unwrap(), 42);
    }

    #[test]
    fn arg_int_required_rejects_non_numeric() {
        let a = args(&["X", "abc"]);
        assert!(arg_int_required(&a, 1, "X").is_err());
    }

    #[test]
    fn arg_json_required_parses_object() {
        let a = args(&["X", r#"{"a":1}"#]);
        let v = arg_json_required(&a, 1, "X").unwrap();
        assert_eq!(v["a"], serde_json::json!(1));
    }

    #[test]
    fn arg_json_required_rejects_bad_json() {
        let a = args(&["X", "{not-json"]);
        assert!(arg_json_required(&a, 1, "X").is_err());
    }

    #[test]
    fn expect_arity_checks_exact_count() {
        let a = args(&["PING", "extra"]);
        assert!(expect_arity(&a, 1, "PING").is_some());
        assert!(expect_arity(&a, 2, "PING").is_none());
    }

    #[test]
    fn expect_arity_range_accepts_optional_args() {
        let a = args(&["X", "y"]);
        assert!(expect_arity_range(&a, 1, 3, "X").is_none());
        let a2 = args(&["X"]);
        assert!(expect_arity_range(&a2, 2, 3, "X").is_some());
    }

    #[test]
    fn expect_arity_min_enforces_lower_bound() {
        let a = args(&["X"]);
        assert!(expect_arity_min(&a, 2, "X").is_some());
        let a2 = args(&["X", "y", "z"]);
        assert!(expect_arity_min(&a2, 2, "X").is_none());
    }
}
