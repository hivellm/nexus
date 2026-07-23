//! Regression tests: `shortestPath()` / `allShortestPaths()` over a
//! multi-type relationship pattern — `[:R1|R2*1..5]` — must traverse EVERY
//! named type, not just the first one parsed.
//!
//! Root cause: `find_shortest_path` / `find_all_shortest_paths` /
//! `find_paths_dfs` in `crates/nexus-core/src/executor/operators/path.rs`
//! took `type_id: Option<u32>` and narrowed it to an at-most-one-element
//! slice before calling `find_relationships`. Their only callers —
//! `shortestPath` / `allShortestPaths` in
//! `crates/nexus-core/src/executor/eval/projection/fn_graph.rs` — only
//! ever extracted the FIRST named type from the pattern. Any path that
//! required a non-first type was silently dropped with no error.
//!
//! Each test registers the DECOY first type(s) in the catalog (via an
//! unrelated edge) so the first-type-only bug actually manifests: if the
//! decoy type were unknown, `get_type_id` would return `None`, the type
//! filter would collapse to "match all", and the bug would be masked.

use nexus_core::executor::Query;
use nexus_core::testing::create_isolated_test_executor;
use serde_json::Value;
use std::collections::HashMap;

fn run(executor: &mut nexus_core::executor::Executor, cypher: &str) {
    executor
        .execute(&Query {
            cypher: cypher.to_string(),
            params: HashMap::new(),
        })
        .unwrap_or_else(|e| panic!("query `{cypher}` failed: {e}"));
}

fn eval(executor: &nexus_core::executor::Executor, cypher: &str) -> Value {
    let result = executor
        .execute(&Query {
            cypher: cypher.to_string(),
            params: HashMap::new(),
        })
        .unwrap_or_else(|e| panic!("query `{cypher}` failed: {e}"));
    assert_eq!(
        result.rows.len(),
        1,
        "expected exactly one row, got {:?}",
        result.rows
    );
    result.rows[0].values[0].clone()
}

/// Number of relationships in a `shortestPath` result (a path object with a
/// `relationships` array), or `None` if the value is not a path.
fn path_rel_count(v: &Value) -> Option<usize> {
    v.get("relationships")
        .and_then(|r| r.as_array())
        .map(|a| a.len())
}

/// `shortestPath` must find a path that requires the SECOND named type in a
/// `[:R1|R2*1..5]` union. `:R1` is registered (via a decoy edge) but `a` has
/// no outgoing `:R1`, so a first-type-only traversal finds no path.
#[test]
fn shortest_path_reaches_target_via_second_named_type() {
    let (mut executor, _ctx) = create_isolated_test_executor();
    run(
        &mut executor,
        "CREATE (a:N {name: 'a'})-[:R2]->(b:N {name: 'b'})",
    );
    // Register :R1 in the catalog on an unrelated pair so the decoy first
    // type resolves to a real type id (otherwise the bug is masked).
    run(
        &mut executor,
        "CREATE (:N {name: 'x'})-[:R1]->(:N {name: 'y'})",
    );

    let p = eval(
        &executor,
        "MATCH (a:N {name: 'a'}), (b:N {name: 'b'}) \
         RETURN shortestPath((a)-[:R1|R2*1..5]->(b)) AS p",
    );

    assert_eq!(
        path_rel_count(&p),
        Some(1),
        "path a->b via the second named type :R2 has one relationship, got {p:?}"
    );
}

/// Control: an UNQUALIFIED variable-length pattern (`[*1..5]`, no type
/// filter) must keep matching every relationship type. Guards the
/// "empty `type_ids` = match all" semantics the fix must not disturb.
#[test]
fn shortest_path_unqualified_pattern_still_matches_every_type() {
    let (mut executor, _ctx) = create_isolated_test_executor();
    run(
        &mut executor,
        "CREATE (a:N {name: 'a'})-[:R2]->(b:N {name: 'b'})",
    );

    let p = eval(
        &executor,
        "MATCH (a:N {name: 'a'}), (b:N {name: 'b'}) \
         RETURN shortestPath((a)-[*1..5]->(b)) AS p",
    );

    assert_eq!(
        path_rel_count(&p),
        Some(1),
        "unqualified pattern must still match the :R2 edge, got {p:?}"
    );
}

/// Three-way type union: the edge is `:R3`, the LAST named type in
/// `[:R1|R2|R3*1..5]`. Both decoy types `:R1` and `:R2` are registered.
#[test]
fn shortest_path_reaches_target_via_third_named_type() {
    let (mut executor, _ctx) = create_isolated_test_executor();
    run(
        &mut executor,
        "CREATE (a:N {name: 'a'})-[:R3]->(b:N {name: 'b'})",
    );
    run(
        &mut executor,
        "CREATE (:N {name: 'x'})-[:R1]->(:N {name: 'y'}), \
         (:N {name: 'u'})-[:R2]->(:N {name: 'w'})",
    );

    let p = eval(
        &executor,
        "MATCH (a:N {name: 'a'}), (b:N {name: 'b'}) \
         RETURN shortestPath((a)-[:R1|R2|R3*1..5]->(b)) AS p",
    );

    assert_eq!(
        path_rel_count(&p),
        Some(1),
        "path a->b via the third named type :R3 has one relationship, got {p:?}"
    );
}

/// `allShortestPaths` must also traverse every named type: it projects a
/// LIST of paths, so the assertion is on the array and the single path it
/// contains, reached via the second named type `:R2`.
#[test]
fn all_shortest_paths_reaches_target_via_second_named_type() {
    let (mut executor, _ctx) = create_isolated_test_executor();
    run(
        &mut executor,
        "CREATE (a:N {name: 'a'})-[:R2]->(b:N {name: 'b'})",
    );
    run(
        &mut executor,
        "CREATE (:N {name: 'x'})-[:R1]->(:N {name: 'y'})",
    );

    let ps = eval(
        &executor,
        "MATCH (a:N {name: 'a'}), (b:N {name: 'b'}) \
         RETURN allShortestPaths((a)-[:R1|R2*1..5]->(b)) AS ps",
    );

    let paths = ps
        .as_array()
        .unwrap_or_else(|| panic!("allShortestPaths must return a list, got {ps:?}"));
    assert_eq!(
        paths.len(),
        1,
        "exactly one shortest path a->b via :R2 exists, got {paths:?}"
    );
    assert_eq!(
        path_rel_count(&paths[0]),
        Some(1),
        "the shortest path a->b via :R2 has one relationship, got {:?}",
        paths[0]
    );
}
