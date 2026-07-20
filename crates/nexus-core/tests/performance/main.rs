//! Integration test harness for the `performance` group.
//! One test binary per group keeps link time down; each module below is a
//! former top-level `tests/*.rs` integration file.

mod benchmark_aggregation_performance;
mod benchmark_create_profiling;
mod benchmark_lock_contention;
mod benchmark_relationship_traversal;
mod benchmark_write_performance;
mod integration_performance_test;
mod performance_benchmark;
mod performance_tests;
mod phase8_relationship_optimization_test;
mod phase9_memory_optimization_test;
mod test_write_intensive;
