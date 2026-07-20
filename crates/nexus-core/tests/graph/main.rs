//! Integration test harness for the `graph` group.
//! One test binary per group keeps link time down; each module below is a
//! former top-level `tests/*.rs` integration file.

mod graph_comparison_test;
mod graph_correlation_complete_test;
mod graph_correlation_complex_dependency_test;
mod graph_correlation_integration_test;
mod graph_correlation_mock_integration_test;
mod graph_correlation_real_codebase_test;
mod graph_correlation_visualization_integration_test;
mod recursive_call_detection_test;
