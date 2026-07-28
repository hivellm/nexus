//! Operational methods for [`RecordStore`]: CRUD operations for nodes and
//! relationships, property management, adjacency-list helpers, and
//! store-level utilities (`clear_all`, `repair_corrupt_node_prop_ptrs`).
//!
//! All methods are implemented on `RecordStore` and live in a separate file
//! purely to keep `record_store.rs` (struct definition + lifecycle methods)
//! under the 1 500-line budget.

mod clear;
mod create;
mod delete;
mod node;
mod properties;
mod query;
mod relationship;
