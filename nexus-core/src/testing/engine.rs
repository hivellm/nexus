//! Test helpers for creating Engine instances
//!
//! This module provides standardized functions for creating Engine instances
//! in tests, ensuring proper resource management and directory existence.

use super::context::TestContext;
use crate::Result;
use crate::{Engine, Error};
use std::path::Path;

/// Setup a test Engine with guaranteed directory existence
///
/// This function creates a new Engine instance in a temporary directory.
/// The directory and all required subdirectories are guaranteed to exist
/// before the Engine is initialized.
///
/// # Returns
///
/// A `Result` containing a tuple of `(Engine, TestContext)` where:
/// - `Engine`: The engine instance ready for use
/// - `TestContext`: Context that manages cleanup (keep this alive for the test duration)
///
/// # Errors
///
/// Returns an error if:
/// - The temporary directory cannot be created
/// - The Engine cannot be initialized
///
/// # Example
///
/// ```rust,no_run
/// use nexus_core::testing::setup_test_engine;
///
/// #[test]
/// fn my_test() -> Result<(), nexus_core::Error> {
///     let (mut engine, _ctx) = setup_test_engine()?;
///     // Use engine...
///     Ok(())
/// }
/// ```
pub fn setup_test_engine() -> Result<(Engine, TestContext)> {
    let ctx = TestContext::new();
    let path = ctx.path();

    // CRITICAL: Ensure directory exists before Engine initialization
    // This prevents "No such file or directory" errors in parallel execution
    std::fs::create_dir_all(path)
        .map_err(|e| Error::Internal(format!("Failed to create engine directory: {}", e)))?;

    let engine = Engine::with_data_dir(path)
        .map_err(|e| Error::Internal(format!("Failed to create engine: {}", e)))?;

    Ok((engine, ctx))
}

/// Setup a test Engine with an isolated catalog
///
/// This is useful for tests that need to prevent interference from
/// shared catalog state in parallel tests. Each test gets its own
/// isolated catalog instance.
///
/// # Returns
///
/// A `Result` containing a tuple of `(Engine, TestContext)`.
pub fn setup_isolated_test_engine() -> Result<(Engine, TestContext)> {
    let ctx = TestContext::new();
    let path = ctx.path();

    std::fs::create_dir_all(path)
        .map_err(|e| Error::Internal(format!("Failed to create engine directory: {}", e)))?;

    let engine = Engine::with_isolated_catalog(path)
        .map_err(|e| Error::Internal(format!("Failed to create isolated engine: {}", e)))?;

    Ok((engine, ctx))
}

/// Setup a test Engine with a custom data directory path
///
/// This is useful when you need to test with a specific directory structure
/// or when sharing a directory between multiple components.
///
/// # Arguments
///
/// * `data_dir` - The path where the engine's data will be stored
///
/// # Returns
///
/// A `Result` containing a tuple of `(Engine, TestContext)`.
pub fn setup_test_engine_with_path<P: AsRef<Path>>(data_dir: P) -> Result<(Engine, TestContext)> {
    let ctx = TestContext::new();
    let base_path = ctx.path();

    // Use the provided path relative to the test context
    let engine_path = base_path.join(data_dir.as_ref());
    std::fs::create_dir_all(&engine_path)
        .map_err(|e| Error::Internal(format!("Failed to create engine directory: {}", e)))?;

    let engine = Engine::with_data_dir(&engine_path)
        .map_err(|e| Error::Internal(format!("Failed to create engine: {}", e)))?;

    Ok((engine, ctx))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_setup_engine() -> Result<()> {
        let (mut engine, _ctx) = setup_test_engine()?;

        // Verify engine works
        let result =
            engine.execute_cypher("CREATE (n:Test {name: 'Alice'}) RETURN n.name AS name")?;
        assert_eq!(result.rows.len(), 1);

        Ok(())
    }

    #[test]
    fn test_setup_engine_with_path() -> Result<()> {
        let (mut engine, _ctx) = setup_test_engine_with_path("custom_engine")?;

        // Verify engine works
        let result =
            engine.execute_cypher("CREATE (n:Test {name: 'Bob'}) RETURN n.name AS name")?;
        assert_eq!(result.rows.len(), 1);

        Ok(())
    }
}
