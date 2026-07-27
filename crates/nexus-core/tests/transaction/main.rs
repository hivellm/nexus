//! Integration test harness for the `transaction` group.
//! One test binary per group keeps link time down; each module below is a
//! former top-level `tests/*.rs` integration file.

mod row_lock_concurrent_test;
mod transaction_session_test;
