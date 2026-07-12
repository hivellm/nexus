//! `execute` — directory module for the Cypher query HTTP handler.
//!
//! Submodules:
//! - `handler` — the public `execute_cypher` Axum handler.
//! - `write_ops` — `execute_create_or_merge`: CREATE/MERGE/SET/DELETE/REMOVE
//!   clause execution.

mod handler;
// `handler.rs` no longer dispatches CREATE/MERGE queries here — it routes
// through the engine's `execute_cypher_with_params` instead
// (phase1_http-merge-rel-and-set-rel-parity §3, item 2.2). The module is
// kept (not deleted) until §5 item 4.1 removes the fork entirely; until
// then `execute_create_or_merge` has no callers.
#[allow(dead_code)]
mod write_ops;

pub use handler::execute_cypher;
