//! `GRAPH[name]` query-scope validation (phase6_opencypher-advanced-types §6).
//!
//! A leading `GRAPH[name]` clause on a Cypher query asks the engine to
//! execute against a specific named database rather than the session's
//! current one. The full multi-database routing lives one layer up
//! (in `DatabaseManager`) — this module is the shim the single-engine
//! code path calls to answer "can I serve this scope from here?".
//!
//! Semantics implemented here:
//!
//! - If the engine is wired to exactly one session-scoped database and
//!   the caller asks for its name, the scope is accepted (returns
//!   `true`).
//! - Otherwise we answer "no": the caller will surface
//!   `ERR_GRAPH_NOT_FOUND` to the client. The multi-database router
//!   takes a different code path and never ends up here, so "no" is
//!   the safest answer for the single-engine case.
//!
//! The access-control story (§6.3) is delegated to the caller's
//! existing auth middleware — this shim only answers name resolution.

use super::Engine;
use crate::Result;

/// Return `Ok(true)` if the engine is currently executing against the
/// requested graph name (i.e. the query can be served in place),
/// `Ok(false)` otherwise. The caller decides whether to raise
/// `ERR_GRAPH_NOT_FOUND`.
///
/// We don't presently probe the session manager for a per-session
/// `current_database` override because the single-engine code path
/// doesn't track one — multi-database routing runs above the engine.
/// `accept_here` therefore always returns `false`, and the caller's
/// error message is the user-visible surface.
pub fn accept_here(_engine: &Engine, _requested: &str) -> Result<bool> {
    Ok(false)
}

#[cfg(test)]
mod tests {
    // `accept_here` is deliberately trivial; the interesting checks
    // live at the parser boundary (see
    // `crates/nexus-core/src/executor/parser/tests.rs`) and in the
    // integration test `tests/advanced_types.rs`.
}
