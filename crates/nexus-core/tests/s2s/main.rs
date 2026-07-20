//! Integration test harness for the `s2s` group.
//! One test binary per group keeps link time down; each module below is a
//! former top-level `tests/*.rs` integration file.

mod advanced_features_s2s_test;
mod api_keys_s2s_test;
mod auth_integration_s2s_test;
mod auth_s2s_test;
mod performance_monitoring_s2s_test;
mod schema_admin_s2s_test;
mod string_operations_s2s_test;
mod write_operations_s2s_test;
