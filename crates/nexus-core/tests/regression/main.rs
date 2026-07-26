//! Integration test harness for the `regression` group.
//! One test binary per group keeps link time down; each module below is a
//! former top-level `tests/*.rs` integration file.

mod create_negative_numeric_literal_test;
mod dynamic_label_read_path_test;
mod dynamic_rel_type_read_path_test;
mod incoming_traversal_large_graph_test;
mod match_create_multi_pattern_test;
mod match_merge_multi_pattern_test;
mod node_property_named_type_test;
mod regression_extended_create;
mod regression_extended_engine;
mod regression_extended_functions;
mod regression_extended_match;
mod regression_extended_relationships;
mod regression_extended_simple;
mod regression_extended_union;
mod regression_tests;
mod test_regression_fixes;
