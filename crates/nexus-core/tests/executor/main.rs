//! Integration test harness for the `executor` group.
//! One test binary per group keeps link time down; each module below is a
//! former top-level `tests/*.rs` integration file.

mod call_subquery_test;
mod collect_subquery_test;
mod correlated_index_seek_e2e_test;
mod correlated_predicate_notification_e2e_test;
mod create_path_index_and_constraints_test;
mod cypher_oom_guard_test;
mod delete_node_dangling_relationships_test;
mod executor_comprehensive_test;
mod node_key_delete_reuse_test;
mod oom_budget_verification_test;
mod optional_match_binding_leak_test;
mod optional_match_empty_driver_test;
mod plan_binding_operator_order_test;
mod query_analysis_test;
mod relationship_delete_test;
mod side_effects;
mod statistics_driven_join_ordering_test;
mod test_metadata_count_optimization;
mod unindexed_correlated_match_test;
mod unindexed_property_notification_e2e_test;
mod update_node_index_divergence_test;
mod write_refresh_visibility_test;
