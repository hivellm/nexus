//! Integration test harness for the `spatial` group.
//! One test binary per group keeps link time down; each module below is a
//! former top-level `tests/*.rs` integration file.

mod geospatial_integration_test;
mod geospatial_predicates_test;
mod rtree_crash_recovery;
mod spatial_crash_recovery;
mod spatial_planner_test;
mod test_point_return;
