//! Test helpers for creating Graph instances
//!
//! This module provides standardized functions for creating Graph instances
//! in tests, ensuring proper resource management and directory existence.

use super::context::TestContext;
use crate::catalog::Catalog;
use crate::storage::RecordStore;
use crate::{Graph, Result};
use std::sync::Arc;

/// Create a test Graph with guaranteed directory existence
///
/// This function creates a new Graph instance in a temporary directory.
/// The directory and all required subdirectories are guaranteed to exist
/// before the Graph is initialized.
///
/// # Returns
///
/// A tuple of `(Graph, TestContext)` where:
/// - `Graph`: The graph instance ready for use
/// - `TestContext`: Context that manages cleanup (keep this alive for the test duration)
///
/// # Example
///
/// ```rust,no_run
/// use nexus_core::testing::create_test_graph;
///
/// #[test]
/// fn my_test() {
///     let (graph, _ctx) = create_test_graph();
///     // Use graph...
/// }
/// ```
pub fn create_test_graph() -> (Graph, TestContext) {
    let ctx = TestContext::new();
    let path = ctx.path();

    // CRITICAL: Ensure directory exists before component initialization
    std::fs::create_dir_all(path).expect("Failed to create test directory");

    let catalog = Arc::new(Catalog::new(path).expect("Failed to create catalog"));
    let store = RecordStore::new(path).expect("Failed to create record store");
    let graph = Graph::new(store, catalog);

    (graph, ctx)
}

/// Create a test Graph with isolated catalog
///
/// This function creates a new Graph instance with an isolated catalog,
/// preventing interference from parallel tests that share the default catalog.
///
/// Use this for tests that need to verify exact counts or specific data states.
///
/// # Returns
///
/// A tuple of `(Graph, TestContext)` where:
/// - `Graph`: The graph instance with isolated catalog
/// - `TestContext`: Context that manages cleanup (keep this alive for the test duration)
pub fn create_isolated_test_graph() -> (Graph, TestContext) {
    let ctx = TestContext::new();
    let path = ctx.path();

    // CRITICAL: Ensure directory exists before component initialization
    std::fs::create_dir_all(path).expect("Failed to create test directory");

    // Use isolated catalog to prevent interference from parallel tests
    let catalog = Arc::new(
        Catalog::with_isolated_path(path.join("catalog.mdb"), 100 * 1024 * 1024)
            .expect("Failed to create isolated catalog"),
    );
    let store = RecordStore::new(path).expect("Failed to create record store");
    let graph = Graph::new(store, catalog);

    (graph, ctx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_graph() {
        let (graph, _ctx) = create_test_graph();

        // Verify graph works
        let node_id = graph.create_node(vec!["Test".to_string()]).unwrap();
        let node = graph.get_node(node_id).unwrap();
        assert!(node.is_some());
    }
}
