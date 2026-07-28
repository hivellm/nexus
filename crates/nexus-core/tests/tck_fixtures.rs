//! Unit tests for the openCypher TCK named fixture graphs (`binary-tree-N`)
//! defined in `tck_common`.
//!
//! Standalone `harness = true` target (default Cargo test discovery) so the
//! `#[test]` functions actually run — `tck_common` is otherwise only
//! included by the `harness = false` corpus runners (`tck_opencypher.rs`,
//! `tck_runner.rs`), which have no libtest to collect `#[test]` fns. Same
//! pattern `tck_cells.rs` uses for the shared TCK cell parser.

#[allow(dead_code)]
#[path = "tck_common/mod.rs"]
mod tck_common;

use nexus_core::testing::setup_isolated_test_engine;
use serde_json::json;
use tck_common::{BINARY_TREE_1_CYPHER, BINARY_TREE_2_CYPHER, binary_tree_cypher};

#[test]
fn binary_tree_1_loads_canonical_node_and_relationship_counts() {
    let (mut engine, _ctx) = setup_isolated_test_engine().expect("setup_isolated_test_engine");
    let result = engine
        .execute_cypher(BINARY_TREE_1_CYPHER)
        .expect("binary-tree-1 fixture should load");

    // Totals from upstream `tck/graphs/binary-tree-1/binary-tree-1.json`.
    assert_eq!(
        result.side_effects.nodes_created, 13,
        "binary-tree-1 node count"
    );
    assert_eq!(
        result.side_effects.relationships_created, 16,
        "binary-tree-1 relationship count"
    );

    // Label distribution: 1 `:A` + 12 `:X`.
    let a_count = engine
        .execute_cypher("MATCH (n:A) RETURN count(n) AS c")
        .expect("count :A nodes");
    assert_eq!(a_count.rows[0].values[0], json!(1));
    let x_count = engine
        .execute_cypher("MATCH (n:X) RETURN count(n) AS c")
        .expect("count :X nodes");
    assert_eq!(x_count.rows[0].values[0], json!(12));

    // Relationship-type distribution: 2 `KNOWS` + 2 `FOLLOWS` + 12 `FRIEND`.
    let knows = engine
        .execute_cypher("MATCH ()-[r:KNOWS]->() RETURN count(r) AS c")
        .expect("count KNOWS relationships");
    assert_eq!(knows.rows[0].values[0], json!(2));
    let follows = engine
        .execute_cypher("MATCH ()-[r:FOLLOWS]->() RETURN count(r) AS c")
        .expect("count FOLLOWS relationships");
    assert_eq!(follows.rows[0].values[0], json!(2));
    let friend = engine
        .execute_cypher("MATCH ()-[r:FRIEND]->() RETURN count(r) AS c")
        .expect("count FRIEND relationships");
    assert_eq!(friend.rows[0].values[0], json!(12));
}

#[test]
fn binary_tree_2_loads_canonical_node_and_relationship_counts() {
    let (mut engine, _ctx) = setup_isolated_test_engine().expect("setup_isolated_test_engine");
    let result = engine
        .execute_cypher(BINARY_TREE_2_CYPHER)
        .expect("binary-tree-2 fixture should load");

    // Totals from upstream `tck/graphs/binary-tree-2/binary-tree-2.json`.
    assert_eq!(
        result.side_effects.nodes_created, 13,
        "binary-tree-2 node count"
    );
    assert_eq!(
        result.side_effects.relationships_created, 16,
        "binary-tree-2 relationship count"
    );

    // Label distribution: 1 `:A` + 8 `:X` + 4 `:Y` — the fixture that
    // distinguishes it from binary-tree-1 (whose `c*2` leaves stay `:X`).
    let x_count = engine
        .execute_cypher("MATCH (n:X) RETURN count(n) AS c")
        .expect("count :X nodes");
    assert_eq!(x_count.rows[0].values[0], json!(8));
    let y_count = engine
        .execute_cypher("MATCH (n:Y) RETURN count(n) AS c")
        .expect("count :Y nodes");
    assert_eq!(y_count.rows[0].values[0], json!(4));
}

#[test]
fn binary_tree_cypher_resolves_known_fixture_numbers_only() {
    assert_eq!(binary_tree_cypher("1"), Some(BINARY_TREE_1_CYPHER));
    assert_eq!(binary_tree_cypher("2"), Some(BINARY_TREE_2_CYPHER));
    assert_eq!(binary_tree_cypher("3"), None);
}
