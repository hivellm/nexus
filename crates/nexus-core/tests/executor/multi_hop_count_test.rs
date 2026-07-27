//! Regression: `count(*)` (and `count(var)`) over a multi-hop relationship
//! pattern must report the TRUE number of matches — 0 when the pattern has no
//! matches — not a phantom count.
//!
//! Root cause (phase0_fix-multi-hop-count-star-incorrect): a second/later
//! `Expand` running on an already-empty row set fell to the branch that called
//! `update_result_set_from_rows(&[])`, wiping `result_set.columns`. `Aggregate`
//! then could not distinguish "the pattern matched nothing" from "there was no
//! pattern at all" and minted a `count(*) = 1` virtual row. A single-hop
//! pattern never triggered it (only a later `Expand` on an emptied pipeline
//! does), which is why the bug was multi-hop-specific.

use nexus_core::Engine;

/// Execute an aggregate query and return its single integer result.
fn scalar_count(engine: &mut Engine, query: &str) -> i64 {
    let r = engine
        .execute_cypher(query)
        .expect("count query must succeed");
    assert_eq!(
        r.rows.len(),
        1,
        "an aggregate with no GROUP BY returns exactly one row; got {:?}",
        r.rows
    );
    r.rows[0].values[0]
        .as_i64()
        .unwrap_or_else(|| panic!("count must be an integer; got {:?}", r.rows[0].values[0]))
}

const TWO_HOP: &str = "MATCH (a:A)-[:R1]->(b:B)-[:R2]->(c:C) RETURN count(*) AS c";
const THREE_HOP: &str = "MATCH (a:A)-[:R1]->(b:B)-[:R2]->(c:C)-[:R3]->(d:D) RETURN count(*) AS c";

#[test]
fn two_hop_count_star_is_zero_when_no_edges_exist() {
    let mut engine = Engine::new().expect("engine");
    // Three isolated nodes: neither hop exists, so the 2-hop pattern has zero
    // matches. Before the fix this returned a phantom 1.
    engine
        .execute_cypher("CREATE (:A {k:1}), (:B {k:2}), (:C {k:3})")
        .expect("seed");
    assert_eq!(scalar_count(&mut engine, TWO_HOP), 0);
}

#[test]
fn two_hop_count_star_is_zero_when_second_hop_missing() {
    let mut engine = Engine::new().expect("engine");
    engine
        .execute_cypher("CREATE (a:A {k:1})-[:R1]->(b:B {k:2})")
        .expect("seed");
    assert_eq!(scalar_count(&mut engine, TWO_HOP), 0);
}

#[test]
fn two_hop_count_star_is_one_for_a_single_full_chain() {
    let mut engine = Engine::new().expect("engine");
    engine
        .execute_cypher("CREATE (a:A {k:1})-[:R1]->(b:B {k:2})-[:R2]->(c:C {k:3})")
        .expect("seed");
    assert_eq!(scalar_count(&mut engine, TWO_HOP), 1);
}

#[test]
fn two_hop_count_star_counts_every_match() {
    let mut engine = Engine::new().expect("engine");
    for i in 0..4 {
        engine
            .execute_cypher(&format!(
                "CREATE (a:A {{k:{i}}})-[:R1]->(b:B {{k:{i}}})-[:R2]->(c:C {{k:{i}}})"
            ))
            .expect("seed chain");
    }
    assert_eq!(scalar_count(&mut engine, TWO_HOP), 4);
}

#[test]
fn two_hop_count_var_agrees_with_count_star_at_zero_one_and_n() {
    // 0 matches — count over any variable of an empty multi-hop pattern is 0.
    let mut e0 = Engine::new().expect("engine");
    e0.execute_cypher("CREATE (:A), (:B), (:C)").expect("seed");
    assert_eq!(
        scalar_count(
            &mut e0,
            "MATCH (a:A)-[:R1]->(b:B)-[:R2]->(c:C) RETURN count(c) AS c"
        ),
        0
    );
    assert_eq!(
        scalar_count(
            &mut e0,
            "MATCH (a:A)-[:R1]->(b:B)-[:R2]->(c:C) RETURN count(a) AS c"
        ),
        0
    );

    // 1 match.
    let mut e1 = Engine::new().expect("engine");
    e1.execute_cypher("CREATE (a:A)-[:R1]->(b:B)-[:R2]->(c:C)")
        .expect("seed");
    assert_eq!(
        scalar_count(
            &mut e1,
            "MATCH (a:A)-[:R1]->(b:B)-[:R2]->(c:C) RETURN count(c) AS c"
        ),
        1
    );
}

#[test]
fn three_hop_count_star_is_zero_when_no_edges_exist() {
    let mut engine = Engine::new().expect("engine");
    engine
        .execute_cypher("CREATE (:A), (:B), (:C), (:D)")
        .expect("seed");
    assert_eq!(scalar_count(&mut engine, THREE_HOP), 0);
}

#[test]
fn three_hop_count_star_is_zero_when_a_middle_hop_missing() {
    let mut engine = Engine::new().expect("engine");
    // First hop exists, second does not — the 3-hop pattern still has 0 matches.
    engine
        .execute_cypher("CREATE (a:A)-[:R1]->(b:B), (:C), (:D)")
        .expect("seed");
    assert_eq!(scalar_count(&mut engine, THREE_HOP), 0);
}

#[test]
fn three_hop_count_star_is_one_for_a_single_full_chain() {
    let mut engine = Engine::new().expect("engine");
    engine
        .execute_cypher("CREATE (a:A)-[:R1]->(b:B)-[:R2]->(c:C)-[:R3]->(d:D)")
        .expect("seed");
    assert_eq!(scalar_count(&mut engine, THREE_HOP), 1);
}

#[test]
fn two_hop_count_star_with_shared_intermediate_counts_each_path() {
    let mut engine = Engine::new().expect("engine");
    // One `b` shared by two distinct second-hop targets => two 2-hop paths.
    engine
        .execute_cypher("CREATE (a:A {k:1})-[:R1]->(b:B {k:2})")
        .expect("seed a->b");
    engine
        .execute_cypher("MATCH (b:B {k:2}) CREATE (b)-[:R2]->(:C {k:3})")
        .expect("seed b->c1");
    engine
        .execute_cypher("MATCH (b:B {k:2}) CREATE (b)-[:R2]->(:C {k:4})")
        .expect("seed b->c2");
    assert_eq!(
        scalar_count(&mut engine, TWO_HOP),
        2,
        "one shared middle node with two outgoing R2 edges yields two 2-hop paths"
    );
}
