//! Integration test harness for the `fulltext` group.
//! One test binary per group keeps link time down; each module below is a
//! former top-level `tests/*.rs` integration file.

mod fulltext_async_writer_ordering;
mod fulltext_crash_recovery;
mod fulltext_ranking_regression;
