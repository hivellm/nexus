//! Test helpers and utilities for Nexus tests
//!
//! This module provides helper macros and utilities for writing tests,
//! including support for slow tests that can be enabled via the `slow-tests` feature.

/// Marks a test as slow, only running when `slow-tests` feature is enabled
///
/// # Usage
///
/// ```rust,no_run
/// # #[test]
/// # #[slow_test]
/// fn my_slow_test() {
///     // Test code here
/// }
/// ```
///
/// To run slow tests:
/// ```bash
/// cargo test --features slow-tests
/// ```
#[cfg(feature = "slow-tests")]
#[macro_export]
macro_rules! slow_test {
    () => {};
}

#[cfg(not(feature = "slow-tests"))]
#[macro_export]
macro_rules! slow_test {
    () => {
        #[ignore = "Slow test - enable with --features slow-tests"]
    };
}

/// Helper macro to mark async tests as slow
#[cfg(feature = "slow-tests")]
#[macro_export]
macro_rules! slow_test_async {
    () => {};
}

#[cfg(not(feature = "slow-tests"))]
#[macro_export]
macro_rules! slow_test_async {
    () => {
        #[ignore = "Slow test - enable with --features slow-tests"]
    };
}
