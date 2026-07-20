//! Integration test harness for the `cluster` group.
//! One test binary per group keeps link time down; each module below is a
//! former top-level `tests/*.rs` integration file.

mod cluster_isolation_tests;
mod replication_integration_test;
mod v2_sharding_e2e;
mod v2_tcp_cluster_integration;
