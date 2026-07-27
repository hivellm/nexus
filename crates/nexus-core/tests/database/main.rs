//! Integration test harness for the `database` group.
//! One test binary per group keeps link time down; each module below is a
//! former top-level `tests/*.rs` integration file.

mod alter_database_test;
mod cross_database_queries_test;
mod multi_database_integration_test;
mod multi_database_success_metrics_test;
mod test_show_constraints;
