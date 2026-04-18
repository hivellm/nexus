//! Test helpers for creating Executor instances
//!
//! This module provides standardized functions for creating Executor instances
//! in tests, ensuring proper resource management and directory existence.

use super::context::TestContext;
use crate::catalog::Catalog;
use crate::executor::Executor;
use crate::index::{KnnIndex, LabelIndex};
use crate::storage::RecordStore;
use std::path::Path;

/// Create a test Executor with guaranteed directory existence
///
/// This function creates a new Executor instance with all required components
/// (Catalog, RecordStore, LabelIndex, KnnIndex) in a temporary directory.
///
/// The directory is guaranteed to exist before any component is initialized,
/// preventing race conditions in parallel test execution.
///
/// # Returns
///
/// A tuple of `(Executor, TestContext)` where:
/// - `Executor`: The executor instance ready for use
/// - `TestContext`: Context that manages cleanup (keep this alive for the test duration)
///
/// # Example
///
/// ```rust,no_run
/// use nexus_core::testing::create_test_executor;
///
/// #[test]
/// fn my_test() {
///     let (mut executor, _ctx) = create_test_executor();
///     // Use executor...
///     // TestContext automatically cleans up on drop
/// }
/// ```
pub fn create_test_executor() -> (Executor, TestContext) {
    let ctx = TestContext::new();
    let path = ctx.path();

    // CRITICAL: Ensure all subdirectories exist before creating components
    // This prevents "No such file or directory" errors in parallel execution
    std::fs::create_dir_all(path).expect("Failed to create executor directory");

    let catalog = Catalog::new(path).expect("Failed to create catalog");

    let store = RecordStore::new(path).expect("Failed to create record store");

    let label_index = LabelIndex::new();
    let knn_index = KnnIndex::new_default(128).expect("Failed to create KNN index");

    let executor = Executor::new(&catalog, &store, &label_index, &knn_index)
        .expect("Failed to create executor");

    (executor, ctx)
}

/// Create a test Executor with isolated catalog
///
/// This function creates a new Executor instance with an isolated catalog,
/// preventing interference from parallel tests that share the default catalog.
///
/// Use this for tests that need to verify exact counts or specific data states.
///
/// # Returns
///
/// A tuple of `(Executor, TestContext)` where:
/// - `Executor`: The executor instance with isolated catalog
/// - `TestContext`: Context that manages cleanup (keep this alive for the test duration)
///
/// # Example
///
/// ```rust,no_run
/// use nexus_core::testing::create_isolated_test_executor;
///
/// #[test]
/// fn my_isolated_test() {
///     let (mut executor, _ctx) = create_isolated_test_executor();
///     // Use executor with isolated data...
/// }
/// ```
pub fn create_isolated_test_executor() -> (Executor, TestContext) {
    let ctx = TestContext::new();
    let path = ctx.path();

    // CRITICAL: Ensure all subdirectories exist before creating components
    std::fs::create_dir_all(path).expect("Failed to create executor directory");

    // Use isolated catalog to prevent interference from parallel tests
    let catalog = Catalog::with_isolated_path(path.join("catalog.mdb"), 100 * 1024 * 1024)
        .expect("Failed to create isolated catalog");

    let store = RecordStore::new(path).expect("Failed to create record store");

    let label_index = LabelIndex::new();
    let knn_index = KnnIndex::new_default(128).expect("Failed to create KNN index");

    let executor = Executor::new(&catalog, &store, &label_index, &knn_index)
        .expect("Failed to create executor");

    (executor, ctx)
}

/// Create a test Executor with a custom path
///
/// This is useful when you need to share a directory between multiple components
/// or when testing with a specific directory structure.
///
/// # Arguments
///
/// * `path` - The path where the executor's data will be stored
///
/// # Returns
///
/// A tuple of `(Executor, TestContext)` where the TestContext manages the provided path.
pub fn create_test_executor_with_path<P: AsRef<Path>>(path: P) -> (Executor, TestContext) {
    let ctx = TestContext::new();
    let base_path = ctx.path();

    // Use the provided path relative to the test context
    let executor_path = base_path.join(path.as_ref());
    std::fs::create_dir_all(&executor_path).expect("Failed to create executor directory");

    let catalog = Catalog::new(&executor_path).expect("Failed to create catalog");

    let store = RecordStore::new(&executor_path).expect("Failed to create record store");

    let label_index = LabelIndex::new();
    let knn_index = KnnIndex::new_default(128).expect("Failed to create KNN index");

    let executor = Executor::new(&catalog, &store, &label_index, &knn_index)
        .expect("Failed to create executor");

    (executor, ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::Query;
    use std::collections::HashMap;

    #[test]
    fn test_create_executor() {
        let (mut executor, _ctx) = create_test_executor();

        // Verify executor works
        let query = Query {
            cypher: "CREATE (n:Test {name: 'Alice'}) RETURN n.name AS name".to_string(),
            params: HashMap::new(),
        };

        let result = executor.execute(&query).expect("Query should execute");
        assert_eq!(result.rows.len(), 1);
    }

    #[test]
    fn test_create_executor_with_path() {
        let (mut executor, _ctx) = create_test_executor_with_path("custom");

        // Verify executor works
        let query = Query {
            cypher: "CREATE (n:Test {name: 'Bob'}) RETURN n.name AS name".to_string(),
            params: HashMap::new(),
        };

        let result = executor.execute(&query).expect("Query should execute");
        assert_eq!(result.rows.len(), 1);
    }
}
