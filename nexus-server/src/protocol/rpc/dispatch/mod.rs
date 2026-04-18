//! RPC command dispatcher — routes a [`Request`] onto the right handler
//! and produces a [`Response`].
//!
//! Handlers are grouped into sibling modules (`admin`, and later `cypher`,
//! `graph`, `knn`, `ingest`, `schema`, `database`). Each exposes its own
//! `run(state, cmd, args) -> Result<NexusValue, String>`; the top-level
//! [`run`] here only classifies which group an uppercased command belongs
//! to. Wrong-arity and argument-type errors are caught inside handlers via
//! the [`arg_str`], [`arg_int`], ... helpers below.
//!
//! Per-connection state carried through every invocation lives in
//! [`RpcSession`]; it owns an `Arc<NexusServer>` plus the few bits the
//! accept loop shares with handlers (auth flag, connection id).

pub mod admin;
pub mod convert;
pub mod cypher;
pub mod graph;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::NexusServer;
use crate::protocol::rpc::{NexusValue, Request, Response};

/// Per-connection state carried through every command invocation.
///
/// The `authenticated` flag is an `AtomicBool` because the read loop
/// checks it on every frame while `AUTH`/`HELLO AUTH` flip it from the
/// handler task — avoiding a mutex keeps the hot path lock-free.
pub struct RpcSession {
    pub server: Arc<NexusServer>,
    pub authenticated: Arc<AtomicBool>,
    pub auth_required: bool,
    pub connection_id: u64,
}

impl RpcSession {
    pub fn is_authenticated(&self) -> bool {
        self.authenticated.load(Ordering::Relaxed)
    }

    pub fn mark_authenticated(&self) {
        self.authenticated.store(true, Ordering::Relaxed);
    }
}

/// Commands that are always accepted, even before `AUTH` has been run.
pub(crate) const PRE_AUTH_COMMANDS: &[&str] = &["PING", "HELLO", "AUTH", "QUIT"];

/// Run one request and produce the matching response. Never panics on
/// malformed input — all error paths surface as [`Response::err`].
#[tracing::instrument(
    name = "rpc.dispatch",
    skip(state, req),
    fields(cmd = %req.command, id = req.id)
)]
pub async fn dispatch(state: &RpcSession, req: Request) -> Response {
    let Request { id, command, args } = req;
    match run(state, &command, args).await {
        Ok(value) => Response::ok(id, value),
        Err(msg) => Response::err(id, msg),
    }
}

/// Route an uppercased command name onto its handler. Authentication is
/// enforced here so individual handlers do not repeat the check.
pub async fn run(
    state: &RpcSession,
    command: &str,
    args: Vec<NexusValue>,
) -> Result<NexusValue, String> {
    let cmd = command.to_ascii_uppercase();

    if state.auth_required
        && !state.is_authenticated()
        && !PRE_AUTH_COMMANDS.contains(&cmd.as_str())
    {
        return Err(format!("NOAUTH authentication required to run '{command}'"));
    }

    match cmd.as_str() {
        "PING" | "HELLO" | "AUTH" | "QUIT" => admin::run(state, cmd.as_str(), &args).await,
        "CYPHER" => cypher::run(state, cmd.as_str(), &args).await,
        "CREATE_NODE" | "CREATE_REL" | "UPDATE_NODE" | "DELETE_NODE" | "MATCH_NODES" => {
            graph::run(state, cmd.as_str(), &args).await
        }
        other => Err(format!("ERR unknown command '{other}'")),
    }
}

// ── Argument helpers ─────────────────────────────────────────────────────────
//
// Centralised so every handler reports the same error shape
// (`ERR argument N must be TYPE` / `ERR missing argument N`). The index is
// caller-supplied so handlers can refer to positional arguments by name.

/// Extract a UTF-8 string (or UTF-8 view of bytes) at `idx`.
pub fn arg_str(args: &[NexusValue], idx: usize) -> Result<String, String> {
    match args.get(idx) {
        Some(NexusValue::Str(s)) => Ok(s.clone()),
        Some(NexusValue::Bytes(b)) => std::str::from_utf8(b)
            .map(|s| s.to_owned())
            .map_err(|_| format!("ERR argument {idx} is not valid UTF-8")),
        Some(_) => Err(format!("ERR argument {idx} must be a string")),
        None => Err(format!("ERR missing argument {idx}")),
    }
}

/// Extract a byte buffer at `idx`. Accepts [`NexusValue::Str`] as UTF-8
/// bytes so clients can pass either; KNN embeddings go through here.
pub fn arg_bytes(args: &[NexusValue], idx: usize) -> Result<Vec<u8>, String> {
    match args.get(idx) {
        Some(NexusValue::Bytes(b)) => Ok(b.clone()),
        Some(NexusValue::Str(s)) => Ok(s.as_bytes().to_vec()),
        Some(_) => Err(format!("ERR argument {idx} must be bytes")),
        None => Err(format!("ERR missing argument {idx}")),
    }
}

/// Extract a signed integer at `idx`. Parses [`NexusValue::Str`] so
/// clients in weakly-typed languages do not have to coerce.
pub fn arg_int(args: &[NexusValue], idx: usize) -> Result<i64, String> {
    match args.get(idx) {
        Some(NexusValue::Int(n)) => Ok(*n),
        Some(NexusValue::Str(s)) => s
            .parse::<i64>()
            .map_err(|_| format!("ERR argument {idx} is not an integer")),
        Some(_) => Err(format!("ERR argument {idx} must be an integer")),
        None => Err(format!("ERR missing argument {idx}")),
    }
}

/// Extract a float at `idx`. Widens [`NexusValue::Int`] and parses
/// [`NexusValue::Str`] for the same reasons as [`arg_int`].
pub fn arg_float(args: &[NexusValue], idx: usize) -> Result<f64, String> {
    match args.get(idx) {
        Some(NexusValue::Float(f)) => Ok(*f),
        Some(NexusValue::Int(n)) => Ok(*n as f64),
        Some(NexusValue::Str(s)) => s
            .parse::<f64>()
            .map_err(|_| format!("ERR argument {idx} is not a float")),
        Some(_) => Err(format!("ERR argument {idx} must be a float")),
        None => Err(format!("ERR missing argument {idx}")),
    }
}

/// Extract a `Map` at `idx` as a borrowed slice of (key, value) pairs.
pub fn arg_map<'a>(
    args: &'a [NexusValue],
    idx: usize,
) -> Result<&'a [(NexusValue, NexusValue)], String> {
    match args.get(idx) {
        Some(NexusValue::Map(pairs)) => Ok(pairs.as_slice()),
        Some(_) => Err(format!("ERR argument {idx} must be a map")),
        None => Err(format!("ERR missing argument {idx}")),
    }
}

/// Extract an `Array` at `idx` as a borrowed slice.
pub fn arg_array<'a>(args: &'a [NexusValue], idx: usize) -> Result<&'a [NexusValue], String> {
    match args.get(idx) {
        Some(NexusValue::Array(items)) => Ok(items.as_slice()),
        Some(_) => Err(format!("ERR argument {idx} must be an array")),
        None => Err(format!("ERR missing argument {idx}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arg_str_accepts_str_and_utf8_bytes() {
        let a = vec![
            NexusValue::Str("hello".into()),
            NexusValue::Bytes(b"world".to_vec()),
        ];
        assert_eq!(arg_str(&a, 0).unwrap(), "hello");
        assert_eq!(arg_str(&a, 1).unwrap(), "world");
    }

    #[test]
    fn arg_str_rejects_non_utf8_bytes() {
        let a = vec![NexusValue::Bytes(vec![0xFF, 0xFE])];
        let err = arg_str(&a, 0).unwrap_err();
        assert!(err.contains("not valid UTF-8"));
    }

    #[test]
    fn arg_str_rejects_wrong_type_and_missing() {
        let a = vec![NexusValue::Int(1)];
        assert!(arg_str(&a, 0).unwrap_err().contains("must be a string"));
        assert!(arg_str(&a, 9).unwrap_err().contains("missing argument 9"));
    }

    #[test]
    fn arg_bytes_accepts_bytes_and_string() {
        let a = vec![
            NexusValue::Bytes(vec![1, 2, 3]),
            NexusValue::Str("abc".into()),
        ];
        assert_eq!(arg_bytes(&a, 0).unwrap(), vec![1, 2, 3]);
        assert_eq!(arg_bytes(&a, 1).unwrap(), b"abc".to_vec());
    }

    #[test]
    fn arg_int_parses_strings() {
        let a = vec![NexusValue::Str("42".into()), NexusValue::Int(-7)];
        assert_eq!(arg_int(&a, 0).unwrap(), 42);
        assert_eq!(arg_int(&a, 1).unwrap(), -7);
    }

    #[test]
    fn arg_int_rejects_unparseable_strings() {
        let a = vec![NexusValue::Str("not-an-int".into())];
        assert!(arg_int(&a, 0).unwrap_err().contains("not an integer"));
    }

    #[test]
    fn arg_float_widens_ints_and_parses_strings() {
        let a = vec![
            NexusValue::Int(3),
            NexusValue::Float(2.5),
            NexusValue::Str("1e3".into()),
        ];
        assert_eq!(arg_float(&a, 0).unwrap(), 3.0);
        assert_eq!(arg_float(&a, 1).unwrap(), 2.5);
        assert_eq!(arg_float(&a, 2).unwrap(), 1000.0);
    }

    #[test]
    fn arg_map_returns_borrowed_slice() {
        let a = vec![NexusValue::Map(vec![(
            NexusValue::Str("k".into()),
            NexusValue::Int(1),
        )])];
        let pairs = arg_map(&a, 0).unwrap();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0.as_str(), Some("k"));
    }

    #[test]
    fn arg_map_rejects_non_map() {
        let a = vec![NexusValue::Int(1)];
        assert!(arg_map(&a, 0).unwrap_err().contains("must be a map"));
    }

    #[test]
    fn arg_array_returns_borrowed_slice() {
        let a = vec![NexusValue::Array(vec![
            NexusValue::Int(1),
            NexusValue::Int(2),
        ])];
        let items = arg_array(&a, 0).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].as_int(), Some(1));
    }

    #[test]
    fn arg_array_rejects_non_array() {
        let a = vec![NexusValue::Str("x".into())];
        assert!(arg_array(&a, 0).unwrap_err().contains("must be an array"));
    }
}
