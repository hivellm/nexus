//! Cypher relationship isomorphism: within one `MATCH` clause, two relationship
//! slots must never bind the same relationship. The rule is scoped to the clause
//! — it spans comma-separated pattern parts, and it does NOT span separate
//! `MATCH` clauses.
//!
//! The `OPTIONAL MATCH` shapes below are regression guards, not new behavior:
//! two earlier attempts at this rule shipped an accumulator whose scope was an
//! implicit side effect of where scans happen, and both regressed openCypher TCK
//! `clauses/match-where` from 28 to 25 on exactly these three scenarios
//! (`MatchWhere6` [1]/[2]/[3], fixtures reproduced verbatim). Both attempts had
//! an `OPTIONAL MATCH` control that passed, because it used a one-edge graph and
//! a directed slot; these use an undirected slot over a node with several edges,
//! which is what actually breaks.

use nexus_core::testing::setup_isolated_test_engine;

fn count(engine: &mut nexus_core::Engine, query: &str) -> i64 {
    engine
        .execute_cypher(query)
        .expect("query should execute")
        .rows
        .first()
        .and_then(|r| r.values.first().and_then(|v| v.as_i64()))
        .expect("expected a single count row")
}

/// The core rule: on a graph with ONE relationship, a two-slot pattern has no
/// match at all, because both slots would have to bind it.
#[test]
fn two_slots_cannot_reuse_the_only_relationship() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (a:A)-[:T]->(b:B)")
        .expect("fixture should be created");

    assert_eq!(
        count(&mut engine, "MATCH (x)-[]-(y)-[]-(z) RETURN count(*)"),
        0,
        "anonymous slots must still be distinct relationships"
    );
    assert_eq!(
        count(&mut engine, "MATCH (x)-[r1]-(y)-[r2]-(z) RETURN count(*)"),
        0,
        "naming the slots changes nothing about the rule"
    );
}

/// Control against over-rejection: two DIFFERENT relationships form a genuine
/// two-hop path and must still match, in both directed and undirected form.
#[test]
fn two_slots_over_two_distinct_relationships_still_match() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (a:A)-[:T1]->(b:B)-[:T2]->(c:C)")
        .expect("fixture should be created");

    assert_eq!(
        count(
            &mut engine,
            "MATCH (x:A)-[r1]->(y)-[r2]->(z:C) RETURN count(*)"
        ),
        1,
        "the directed two-hop path is a legitimate match"
    );
    // Undirected, anchored at each end node in turn: A→B→C and C→B→A.
    assert_eq!(
        count(&mut engine, "MATCH (x)-[r1]-(y)-[r2]-(z) RETURN count(*)"),
        2,
        "each direction of the two-hop walk is one binding"
    );
}

/// openCypher TCK `useCases/countingSubgraphMatches` [10]. `deg(A)=1`,
/// `deg(l)=3` (the loop contributes one incidence), `deg(B)=1`: the third
/// binding the pre-isomorphism executor produced walked `--` back over the
/// `-->` edge it had just consumed.
#[test]
fn counting_subgraph_matches_two_hop_from_a_labelled_anchor() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    create_looper_fixture(&mut engine);

    assert_eq!(count(&mut engine, "MATCH (:A)-->()--() RETURN count(*)"), 2);
}

/// openCypher TCK `useCases/countingSubgraphMatches` [11]. Without the rule the
/// total is `1 + 9 + 1 = 11`; with it, `0 + (3 * 2) + 0 = 6`.
#[test]
fn counting_subgraph_matches_two_anonymous_slots() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    create_looper_fixture(&mut engine);

    assert_eq!(
        count(&mut engine, "MATCH ()-[]-()-[]-() RETURN count(*)"),
        6
    );
}

/// The scope is the clause: SEPARATE `MATCH` clauses may each bind the same
/// relationship. This is the boundary an implicit, scan-derived scope cannot
/// hold — and getting it wrong is what regressed the corpus twice.
#[test]
fn isomorphism_does_not_span_separate_match_clauses() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (a:A)-[:T]->(b:B)")
        .expect("fixture should be created");

    assert_eq!(
        count(
            &mut engine,
            "MATCH (x)-[r1]->(y) MATCH (p)-[r2]->(q) RETURN count(*)"
        ),
        1,
        "two clauses may both bind the one relationship"
    );
    assert_eq!(
        count(&mut engine, "MATCH (x)-[r1]->(y)-[r2]->(z) RETURN count(*)"),
        0,
        "one clause may not, on the same fixture"
    );
}

/// The scope spans comma-separated parts of ONE clause — the parser flattens
/// them into a single `Pattern`, and Cypher scopes the rule to the clause.
#[test]
fn isomorphism_spans_comma_separated_parts_of_one_clause() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (a:A)-[:T]->(b:B)")
        .expect("fixture should be created");

    assert_eq!(
        count(
            &mut engine,
            "MATCH (x)-[r1]->(y), (p)-[r2]->(q) RETURN count(*)"
        ),
        0,
        "the two parts would have to share the only relationship"
    );
}

/// Control for the comma case: with two relationships available, the comma
/// pattern has the two ordered pairs of DISTINCT relationships.
#[test]
fn comma_separated_parts_still_pair_distinct_relationships() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (a:A)-[:T1]->(b:B), (c:C)-[:T2]->(d:D)")
        .expect("fixture should be created");

    assert_eq!(
        count(
            &mut engine,
            "MATCH (x)-[r1]->(y), (p)-[r2]->(q) RETURN count(*)"
        ),
        2
    );
}

/// Repeating ONE relationship variable across two slots of a pattern is
/// unsatisfiable, and openCypher rejects it rather than returning zero rows.
/// Source: openCypher TCK `clauses/match/Match3.feature` [29] "Fail when
/// re-using a relationship in the same pattern", which requires a compile-time
/// `SyntaxError` carrying `RelationshipUniquenessViolation`.
#[test]
fn repeating_one_relationship_variable_is_rejected_not_silently_empty() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (a:A)-[:T]->(b:B)")
        .expect("fixture should be created");

    let err = engine
        .execute_cypher("MATCH (x)-[r]-(y)-[r]-(z) RETURN r")
        .expect_err("a repeated relationship variable must be rejected");
    assert!(
        err.to_string().contains("RelationshipUniquenessViolation"),
        "expected the openCypher detail token, got: {err}"
    );
}

/// openCypher TCK `clauses/match-where` `MatchWhere6` [7]: an `OPTIONAL MATCH`
/// whose pattern has TWO hops, so isomorphism bookkeeping IS active on rows
/// that then reach `execute_optional_filter`. This is the shape that proves the
/// bookkeeping key is excluded from that operator's group key — otherwise each
/// candidate becomes its own group and pads its own row.
#[test]
fn multi_hop_optional_match_with_a_where_pads_once_per_anchor() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher(
            "CREATE (:X {val: 1})-[:E1]->(:Y {val: 2})-[:E2]->(:Z {val: 3}),
                    (:X {val: 4})-[:E1]->(:Y {val: 5}),
                    (:X {val: 6})",
        )
        .expect("fixture should be created");

    let result = engine
        .execute_cypher(
            "MATCH (x:X)
             OPTIONAL MATCH (x)-[:E1]->(y:Y)-[:E2]->(z:Z)
             WHERE x.val < z.val
             RETURN x.val, z.val",
        )
        .expect("query should execute");

    assert_eq!(
        result.rows.len(),
        3,
        "one row per :X anchor — matched or padded, got {:?}",
        result.rows
    );
}

/// openCypher TCK `clauses/match-where` `MatchWhere6` [1]. The filtered first
/// clause leaves one row; the optional clause's own filter leaves one binding.
#[test]
fn optional_match_after_a_filtered_match_keeps_one_row() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher(
            "CREATE (a {name: 'A'}), (b:B {name: 'B'}), (c:C {name: 'C'}), (d:D {name: 'C'})",
        )
        .expect("fixture nodes should be created");
    engine
        .execute_cypher(
            "MATCH (a {name: 'A'}), (b:B), (c:C), (d:D)
             CREATE (a)-[:T]->(b), (a)-[:T]->(c), (a)-[:T]->(d)",
        )
        .expect("fixture relationships should be created");

    let result = engine
        .execute_cypher(
            "MATCH (a)-->(b)
             WHERE b:B
             OPTIONAL MATCH (a)-->(c)
             WHERE c:C
             RETURN a.name",
        )
        .expect("query should execute");

    assert_eq!(result.rows.len(), 1, "expected exactly one row");
    assert_eq!(
        result.rows[0].values[0].as_str(),
        Some("A"),
        "expected a.name = 'A'"
    );
}

/// openCypher TCK `clauses/match-where` `MatchWhere6` [2]. Every candidate of
/// the optional undirected expand is rejected by the predicate, so the LEFT
/// OUTER contract asks for exactly one NULL-padded row — not one per rejected
/// candidate.
#[test]
fn optional_match_rejecting_every_candidate_pads_exactly_one_row() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    create_single_hub_fixture(&mut engine);

    let result = engine
        .execute_cypher(
            "MATCH (n:Single)
             OPTIONAL MATCH (n)-[r]-(m)
             WHERE m:NonExistent
             RETURN r",
        )
        .expect("query should execute");

    assert_eq!(
        result.rows.len(),
        1,
        "expected exactly one NULL-padded row, got {:?}",
        result.rows
    );
    assert!(
        result.rows[0].values[0].is_null(),
        "expected r = null, got {:?}",
        result.rows[0].values[0]
    );
}

/// openCypher TCK `clauses/match-where` `MatchWhere6` [3]. Same shape as [2]
/// with a property predicate that ONE candidate satisfies: one row, bound (not
/// padded).
#[test]
fn optional_match_with_a_property_predicate_keeps_the_single_match() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    create_single_hub_fixture(&mut engine);

    let result = engine
        .execute_cypher(
            "MATCH (n:Single)
             OPTIONAL MATCH (n)-[r]-(m)
             WHERE m.num = 42
             RETURN m.num",
        )
        .expect("query should execute");

    assert_eq!(
        result.rows.len(),
        1,
        "expected exactly one row, got {:?}",
        result.rows
    );
    assert_eq!(
        result.rows[0].values[0].as_i64(),
        Some(42),
        "expected the :A candidate to survive"
    );
}

/// `(:A)-[:T1]->(l:Looper)`, `(l)-[:LOOP]->(l)`, `(l)-[:T2]->(:B)` — the
/// openCypher TCK `useCases/countingSubgraphMatches` [10]/[11] fixture.
fn create_looper_fixture(engine: &mut nexus_core::Engine) {
    engine
        .execute_cypher("CREATE (:A)-[:T1]->(l:Looper), (l)-[:LOOP]->(l), (l)-[:T2]->(:B)")
        .expect("fixture should be created");
}

/// `(s:Single)` with two outgoing edges, one of which continues, plus a
/// self-loop elsewhere — the `MatchWhere6` [2]/[3] fixture.
fn create_single_hub_fixture(engine: &mut nexus_core::Engine) {
    engine
        .execute_cypher("CREATE (s:Single), (a:A {num: 42}), (b:B {num: 46}), (c:C)")
        .expect("fixture nodes should be created");
    engine
        .execute_cypher(
            "MATCH (s:Single), (a:A), (b:B), (c:C)
             CREATE (s)-[:REL]->(a), (s)-[:REL]->(b), (a)-[:REL]->(c), (b)-[:LOOP]->(b)",
        )
        .expect("fixture relationships should be created");
}
