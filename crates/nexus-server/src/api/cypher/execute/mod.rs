//! `execute` — directory module for the Cypher query HTTP handler.
//!
//! Submodules:
//! - `handler` — the public `execute_cypher` Axum handler. Routes
//!   CREATE/MERGE/SET/DELETE/REMOVE/FOREACH through
//!   `Engine::execute_cypher_with_params` directly (write-path
//!   unification, `docs/nexus/04-write-path-unification.md` Steps 2-4);
//!   the hand-rolled `write_ops.rs` fork that used to reimplement this
//!   dispatch has been deleted.

mod handler;

pub use handler::execute_cypher;
