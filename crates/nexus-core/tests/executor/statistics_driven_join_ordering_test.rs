//! phase6_traversal-aggregation-perf §4 — plan-quality tests for the
//! statistics-driven scan/join ordering fix in
//! `crates/nexus-core/src/executor/planner/queries/cost.rs`.
//!
//! Before this fix, `estimate_operator_cost`'s `Operator::NodeByLabel` arm
//! priced every label scan using `avg_nodes_per_label` (the average across
//! *every* registered label), so two different-cardinality labels always
//! cost identically and `optimize_operator_order`'s cost-sort could never
//! actually reorder them. These tests assert on the real, executed plan
//! (`Engine::executor::parse_and_plan`, which — unlike EXPLAIN — runs
//! `optimize_operator_order`) that a comma-separated `MATCH` pattern drives
//! the lower-cardinality label first regardless of written order.

use nexus_core::Engine;
use nexus_core::executor::Operator;
use nexus_core::testing::setup_isolated_test_engine;
use std::sync::atomic::{AtomicU32, Ordering};

static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

fn execute_cypher(engine: &mut Engine, query: &str) -> nexus_core::executor::ResultSet {
    engine.execute_cypher(query).unwrap()
}

/// Extract the `variable` of every `NodeByLabel` operator, in plan order.
fn node_by_label_scan_order(operators: &[Operator]) -> Vec<String> {
    operators
        .iter()
        .filter_map(|op| match op {
            Operator::NodeByLabel { variable, .. } => Some(variable.clone()),
            _ => None,
        })
        .collect()
}

/// §4.2 — a 2-label comma-separated MATCH with a large cardinality skew
/// (|Big| = 500, |Small| = 5) must scan the smaller label first in the
/// final (post-optimizer) operator list, even though the query text names
/// the larger label first.
#[test]
fn join_ordering_drives_lower_cardinality_label_first() {
    let test_id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let big_label = format!("Big{}", test_id);
    let small_label = format!("Small{}", test_id);
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    for i in 0..500 {
        execute_cypher(
            &mut engine,
            &format!("CREATE (n:{} {{id: {}}})", big_label, i),
        );
    }
    for i in 0..5 {
        execute_cypher(
            &mut engine,
            &format!("CREATE (n:{} {{id: {}}})", small_label, i),
        );
    }

    // `b` (Big, 500 nodes) is written FIRST; `a` (Small, 5 nodes) second.
    let cypher = format!(
        "MATCH (b:{}), (a:{}) RETURN count(*) as total",
        big_label, small_label
    );
    let operators = engine.executor.parse_and_plan(&cypher).unwrap();

    let scan_order = node_by_label_scan_order(&operators);
    assert_eq!(
        scan_order,
        vec!["a".to_string(), "b".to_string()],
        "the 5-node label (a) must be scanned before the 500-node label (b) \
         even though b was written first in the pattern; got operators: {:?}",
        operators
    );

    // Correctness: the reordering must not change the answer.
    let result = execute_cypher(&mut engine, &cypher);
    assert_eq!(
        result.rows[0].values[0].as_u64(),
        Some(500 * 5),
        "reordering scans must not change the cross-product count"
    );
}

/// §4 conservative fallback — on a cold catalog (label registered but zero
/// live nodes for either side) the cost model must not panic or produce a
/// nonsensical order; both scans price at 0 and the (stable) sort leaves
/// the written order unchanged, matching pre-fix behaviour exactly.
#[test]
fn join_ordering_cold_labels_keep_written_order() {
    let test_id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let label_x = format!("ColdX{}", test_id);
    let label_y = format!("ColdY{}", test_id);
    let (engine, _ctx) = setup_isolated_test_engine().unwrap();

    // Neither label has been created yet — the catalog/label index has no
    // entries for them (equivalent to a cold-catalog cardinality of 0).
    let cypher = format!(
        "MATCH (x:{}), (y:{}) RETURN count(*) as total",
        label_x, label_y
    );
    let operators = engine.executor.parse_and_plan(&cypher).unwrap();

    let scan_order = node_by_label_scan_order(&operators);
    assert_eq!(
        scan_order,
        vec!["x".to_string(), "y".to_string()],
        "equal (zero) cost on both cold labels must preserve written order"
    );
}
