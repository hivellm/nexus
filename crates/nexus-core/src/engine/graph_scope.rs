//! `GRAPH[name]` query-scope routing (phase6_opencypher-advanced-types §6).
//!
//! A leading `GRAPH[name]` clause on a Cypher query asks the engine to
//! execute against a specific named database rather than the session's
//! current one. This module owns the name-resolution policy:
//!
//! - No `DatabaseManager` attached to the executor → only the single
//!   engine's default database is reachable, and we conservatively
//!   reject every `GRAPH[name]` clause with `ERR_GRAPH_NOT_FOUND`.
//!   The rationale is that the engine cannot tell its own name from
//!   here, so returning `AcceptHere` for an arbitrary `name` would
//!   silently map every scope onto the default database.
//! - `DatabaseManager` attached → look the name up. Missing →
//!   `ERR_GRAPH_NOT_FOUND`; found → `Route(target_engine)` so the
//!   caller can hand the query to the owning engine.
//!
//! Access control (§6.3) rides on whichever auth middleware guards
//! the HTTP entry point. A caller that has reached
//! `execute_cypher_with_context` has already cleared auth for the
//! session; per-database ACLs sit on the
//! `DatabaseManager::get_database` lookup (future extension), and
//! the error surface collapses missing-access to the same
//! `ERR_GRAPH_NOT_FOUND` we already return for missing-db, matching
//! the "no information leak on unauthorised names" rule.

use super::Engine;
use crate::{Error, Result};
use parking_lot::RwLock;
use std::sync::Arc;

/// Outcome of resolving a `GRAPH[name]` scope.
pub enum ScopedDispatch {
    /// The caller can honour the scope in place — no routing needed.
    /// Returned today only when `name` coincides with the default
    /// database name tracked by the attached `DatabaseManager`;
    /// single-engine deployments never hit this variant.
    AcceptHere,
    /// The scope targets a sibling engine owned by the same
    /// `DatabaseManager`. The caller locks the returned handle and
    /// re-dispatches the query against that engine.
    Route(Arc<RwLock<Engine>>),
}

/// Resolve a `GRAPH[name]` scope. Returns
/// `Err(ERR_GRAPH_NOT_FOUND)` if the name cannot be served from this
/// process.
pub fn resolve(engine: &Engine, requested: &str) -> Result<ScopedDispatch> {
    let Some(mgr) = engine.executor.shared().database_manager() else {
        return Err(not_found(requested));
    };
    let manager = mgr.read();
    // When the scope names the manager's default database AND the
    // default happens to be served by this very engine instance, we
    // can honour the scope without routing. Otherwise we always
    // route so the target engine's own catalog/state handles the
    // query — even the default path goes through `get_database` so
    // the caller ends up locking the manager's handle for the same
    // database this engine is backing, which is a no-op.
    if manager.default_database_name() == requested {
        return Ok(ScopedDispatch::AcceptHere);
    }
    match manager.get_database(requested) {
        Ok(target) => Ok(ScopedDispatch::Route(target)),
        Err(_) => Err(not_found(requested)),
    }
}

/// Back-compat shim: returns `true` iff `resolve` would answer
/// `AcceptHere` (the single-engine fast path).
pub fn accept_here(engine: &Engine, requested: &str) -> Result<bool> {
    match resolve(engine, requested) {
        Ok(ScopedDispatch::AcceptHere) => Ok(true),
        Ok(ScopedDispatch::Route(_)) => Ok(false),
        Err(_) => Ok(false),
    }
}

fn not_found(requested: &str) -> Error {
    Error::CypherExecution(format!(
        "ERR_GRAPH_NOT_FOUND: graph {requested:?} is not accessible from this engine"
    ))
}

#[cfg(test)]
mod tests {
    // Name-resolution semantics are exercised end-to-end from the
    // integration tests in `crates/nexus-core/src/engine/tests.rs`,
    // which wire a real `DatabaseManager` with two engines and drive
    // `GRAPH[name]` queries through the REST-equivalent entry point.
}
