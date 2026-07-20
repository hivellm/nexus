//! Integration test harness for the `integration` group.
//! One test binary per group keeps link time down; each module below is a
//! former top-level `tests/*.rs` integration file.

mod integration;
mod integration_extended;
mod tracing_volume;
mod validation_comprehensive_test;
