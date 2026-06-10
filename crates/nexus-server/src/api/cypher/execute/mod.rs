//! `execute` — directory module for the Cypher query HTTP handler.
//!
//! Submodules:
//! - `handler` — the public `execute_cypher` Axum handler.
//! - `write_ops` — `execute_create_or_merge`: CREATE/MERGE/SET/DELETE/REMOVE
//!   clause execution.

mod handler;
mod write_ops;

pub use handler::execute_cypher;
