//! Integration test harness for the `security` group.
//! One test binary per group keeps link time down; each module below is a
//! former top-level `tests/*.rs` integration file.

mod kms_aws_integration;
mod kms_gcp_integration;
mod kms_vault_integration;
mod security_tests;
