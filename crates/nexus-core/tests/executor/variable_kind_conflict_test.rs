//! A variable is bound to one kind of thing, and stays that kind.
//!
//! openCypher gives a variable exactly one of four kinds — node,
//! relationship, path, or plain value — and rebinding a name to a different
//! kind within a scope is a `VariableTypeConflict`. Only the node-vs-
//! relationship half of that was enforced; a path or a value colliding with
//! either ran happily and answered.
//!
//! The scope rule matters as much as the conflict rule: a `WITH` ends the
//! previous scope, so a name reused *after* one is a legitimate rename, not a
//! conflict. The "still legal" section below is what stops this check from
//! rejecting ordinary Cypher.

use nexus_core::testing::setup_isolated_test_engine;

fn engine() -> nexus_core::Engine {
    let (mut engine, ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (:VarKind {name: 'a'})-[:LINKS]->(:VarKind {name: 'b'})")
        .unwrap();
    std::mem::forget(ctx);
    engine
}

#[track_caller]
fn assert_conflict(query: &str) {
    let mut engine = engine();
    match engine.execute_cypher(query) {
        Ok(rs) => panic!(
            "expected `{query}` to be rejected, got {} rows",
            rs.rows.len()
        ),
        Err(e) => {
            let msg = e.to_string();
            assert!(
                msg.contains("VariableTypeConflict"),
                "`{query}` raised the wrong error: {msg}"
            );
        }
    }
}

#[track_caller]
fn assert_accepted(query: &str) {
    let mut engine = engine();
    if let Err(e) = engine.execute_cypher(query) {
        panic!("`{query}` should be legal but was rejected: {e}");
    }
}

// ── path vs node / relationship ────────────────────────────────────────

#[test]
fn a_path_variable_reused_as_a_node_in_the_same_pattern_is_rejected() {
    assert_conflict("MATCH p = (p)-[]-() RETURN p");
}

#[test]
fn a_path_variable_reused_as_a_relationship_in_the_same_pattern_is_rejected() {
    assert_conflict("MATCH p = ()-[p]-() RETURN p");
}

#[test]
fn a_path_variable_reused_as_a_node_in_a_later_match_is_rejected() {
    assert_conflict("MATCH r = ()-[]-() MATCH (r) RETURN r");
}

#[test]
fn a_path_variable_reused_as_a_relationship_in_a_later_match_is_rejected() {
    assert_conflict("MATCH r = ()-[]-() MATCH ()-[r]-() RETURN r");
}

#[test]
fn a_node_variable_reused_as_a_path_in_a_later_match_is_rejected() {
    assert_conflict("MATCH (p)-[]-() MATCH p = ()-[]-() RETURN p");
}

#[test]
fn a_relationship_variable_reused_as_a_path_in_a_later_match_is_rejected() {
    assert_conflict("MATCH ()-[p]-() MATCH p = ()-[]-() RETURN p");
}

#[test]
fn a_path_variable_reused_across_comma_separated_parts_is_rejected() {
    assert_conflict("MATCH r = ()-[]-(), (r) RETURN r");
    assert_conflict("MATCH r = ()-[]-(), ()-[r]-() RETURN r");
}

// ── value vs graph entity ──────────────────────────────────────────────

#[test]
fn a_value_bound_by_with_cannot_be_matched_as_a_node() {
    assert_conflict("WITH true AS n MATCH (n) RETURN n");
}

#[test]
fn a_value_bound_by_with_cannot_be_matched_as_a_relationship() {
    assert_conflict("WITH true AS r MATCH ()-[r]-() RETURN r");
}

#[test]
fn a_value_bound_by_with_cannot_be_matched_as_a_path() {
    assert_conflict("WITH true AS p MATCH p = ()-[]-() RETURN p");
}

#[test]
fn a_null_alias_does_not_pin_the_kind() {
    // `null` is a member of every type, so binding it says nothing about what
    // the name may later be matched as. Narrowing the rule to non-null
    // literals is what keeps this legal.
    assert_accepted("WITH null AS a OPTIONAL MATCH p = (a)-[r]->() RETURN nodes(p)");
}

#[test]
fn an_alias_of_an_unreadable_type_does_not_pin_the_kind() {
    assert_accepted("MATCH (n:VarKind) WITH head(collect(n)) AS m MATCH (m) RETURN m");
}

// ── node vs relationship (the half that already worked) ────────────────

#[test]
fn a_node_variable_reused_as_a_relationship_is_still_rejected() {
    assert_conflict("MATCH (a) MATCH ()-[a]-() RETURN a");
}

// ── still legal — the rule must not overreach ──────────────────────────

#[test]
fn passing_a_node_through_with_keeps_it_a_node() {
    assert_accepted("MATCH (n:VarKind) WITH n MATCH (n)-[:LINKS]->(m) RETURN m");
}

#[test]
fn rebinding_a_name_after_a_with_is_a_rename_not_a_conflict() {
    assert_accepted("MATCH (n:VarKind) WITH n.name AS n RETURN n");
}

#[test]
fn a_name_freed_by_a_with_may_be_rebound_as_a_node() {
    assert_accepted("MATCH (x:VarKind) WITH 1 AS other MATCH (x:VarKind) RETURN x");
}

#[test]
fn the_same_node_variable_may_repeat_within_one_pattern() {
    assert_accepted("MATCH (a:VarKind)-[:LINKS]->(b:VarKind) RETURN a, b");
    assert_accepted("MATCH (a:VarKind) MATCH (a)-[r:LINKS]->() RETURN a, r");
}

#[test]
fn a_named_path_over_distinct_variables_is_legal() {
    assert_accepted("MATCH p = (a:VarKind)-[r:LINKS]->(b:VarKind) RETURN p, a, r, b");
}

#[test]
fn two_named_paths_in_one_pattern_are_legal() {
    assert_accepted("MATCH p = (a:VarKind)-[:LINKS]->(b), q = (c:VarKind) RETURN p, q");
}
