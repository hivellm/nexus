//! Integration test harness for the `storage` group.
//! One test binary per group keeps link time down; each module below is a
//! former top-level `tests/*.rs` integration file.

mod edge_merge_index_test;
mod graph_storage_engine_validation_test;
mod merge_index_correctness_test;
mod relationship_prop_ptr_test;
mod relationship_traversal_test;
mod test_index_consistency;
mod test_relationship_debug;
mod test_storage_init;
