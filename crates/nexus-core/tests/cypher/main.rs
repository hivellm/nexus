//! Integration test harness for the `cypher` group.
//! One test binary per group keeps link time down; each module below is a
//! former top-level `tests/*.rs` integration file.

mod builtin_functions_test;
mod count_distinct_tests;
mod cypher_external_id;
mod cypher_external_id_rel_merge;
mod cypher_external_id_write_paths;
mod cypher_groupby_expression_key_test;
mod cypher_non_ascii_test;
mod in_operator_tests;
mod logical_operators_tests;
mod mathematical_operators_test;
mod merge_relationship_anonymous_variable_test;
mod merge_relationship_arrow_direction_test;
mod new_functions_test;
mod null_comparison_tests;
mod phase4_cypher_parity_quick_wins_test;
mod return_where_tests;
mod skip_pattern_queries_test;
mod test_aggregation_virtual_row;
mod test_array_concatenation;
mod test_array_indexing;
mod test_array_slicing;
mod test_call_procedures;
mod test_collect_aggregation;
mod test_create_arrow_direction;
mod test_create_with_return;
mod test_create_without_return;
mod test_filter_function;
mod test_multiple_relationship_types;
mod test_regex_functions;
mod test_relationship_counting;
mod test_size_function;
mod test_string_concatenation;
mod test_substring_negative;
mod test_sum_empty_match;
mod test_temporal_arithmetic;
mod test_where_in;
mod unbounded_alloc_guard_test;
mod unwind_tests;
