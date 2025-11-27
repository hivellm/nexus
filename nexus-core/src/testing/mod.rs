//! Testing infrastructure for Nexus Core
//!
//! This module provides centralized test helpers and infrastructure to ensure
//! proper isolation and resource management when running tests in parallel.
//!
//! # Key Features
//!
//! - **TestContext**: Manages test lifecycle and automatic cleanup
//! - **Guaranteed Directory Existence**: All paths are created before use
//! - **Resource Pooling**: Reuses LMDB environments to avoid TlsFull errors
//! - **Serial Test Support**: Integration with `serial_test` for exclusive access
//!
//! # Usage
//!
//! ```rust,no_run
//! use nexus_core::testing::{create_test_executor, setup_test_engine};
//!
//! #[test]
//! fn my_test() {
//!     let (mut executor, _ctx) = create_test_executor();
//!     // Use executor...
//!     // TestContext automatically cleans up on drop
//! }
//!
//! #[test]
//! fn my_engine_test() -> Result<(), nexus_core::Error> {
//!     let (mut engine, _ctx) = setup_test_engine()?;
//!     // Use engine...
//!     Ok(())
//! }
//! ```

mod context;
mod engine;
mod executor;
mod graph;
mod loader;

pub use context::TestContext;
pub use engine::{setup_isolated_test_engine, setup_test_engine};
pub use executor::{create_isolated_test_executor, create_test_executor};
pub use graph::{create_isolated_test_graph, create_test_graph};
pub use loader::create_test_loader;
