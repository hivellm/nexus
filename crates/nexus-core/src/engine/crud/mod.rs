//! Engine-level CRUD (create / read / update / delete) over nodes
//! and relationships, plus the private property-indexing helpers the
//! write path relies on.
//!
//! These are the methods the REST / Cypher layers ultimately call
//! through when a write goes past the executor pipeline — things like
//! `MERGE` spawning a node outside a pattern-match plan, or the
//! server's `POST /data/nodes` endpoint creating a node directly.
//!
//! Extracted from `engine/mod.rs` during the split. Public API
//! surface is unchanged; methods still resolve as `Engine::create_node`,
//! `Engine::update_node`, etc. via Rust's multi-file `impl` blocks.
//!
//! # Sub-module layout
//!
//! | File                  | Contents                                          |
//! |-----------------------|---------------------------------------------------|
//! | `nodes.rs`            | Node CRUD + external-id rollback                  |
//! | `relationships.rs`    | Relationship CRUD                                 |
//! | `lookup.rs`           | Property / pattern helpers + write-state cache    |
//! | `index_maintenance.rs`| FTS, spatial, composite B-tree, property indexes  |

use serde_json::{Map, Value};
use std::collections::HashSet;

mod index_maintenance;
mod lookup;
mod nodes;
mod relationships;

/// Ephemeral write-state kept during a Cypher write pass — pair of
/// `properties` + `labels` that later get persisted via
/// [`Engine::persist_node_state`]. Internal to the engine write path.
///
/// Visibility is `pub(in crate::engine)` so engine-level siblings
/// (`write_exec`, `constraints`, `mod.rs`) can name the type without
/// going through the `crud` module's re-export. The previous
/// `pub(super)` bound `NodeWriteState` to the old flat `crud.rs`
/// scope; after the split the meaningful boundary is the engine
/// module, not the crud sub-module.
pub(in crate::engine) struct NodeWriteState {
    pub(in crate::engine) properties: Map<String, Value>,
    pub(in crate::engine) labels: HashSet<String>,
}
