//! Phase 3: Adjacency List Storage for Relationship Traversal Optimization
//!
//! This module implements an adjacency list structure to optimize relationship traversal
//! by co-locating relationship information with nodes, improving cache locality and
//! reducing random access patterns.
//!
//! ## Design
//!
//! - **Outgoing relationships**: Stored in `adjacency.store` with node ID as key
//! - **Incoming relationships**: Stored separately for efficient reverse traversal
//! - **Format**: Variable-length records with type-filtered lists
//! - **Cache-friendly**: Adjacency lists are stored contiguously for better cache performance

mod store;
mod tests;
mod types;

pub use store::AdjacencyListStore;
pub use types::{AdjacencyEntry, AdjacencyListHeader};
