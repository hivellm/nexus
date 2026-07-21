//! Regression coverage for `Engine::process_merge_relationship`
//! (`crates/nexus-core/src/engine/write_exec.rs`): a MERGE pattern with a
//! `Node, Relationship, Node` shape must always create-or-match the FULL
//! pattern — both endpoints and the edge — regardless of which pattern
//! elements carry a bound variable. Anonymous relationship types
//! (`-[:KNOWS]->`) and anonymous node endpoints (`(:Person {..})`) are
//! ordinary, idiomatic Cypher, not edge cases.
//!
//! Each test below targets exactly one of the three variable-presence
//! early-returns in `process_merge_relationship` (source node variable,
//! destination node variable, relationship variable) so a future
//! regression in any one of them is caught precisely.

use nexus_core::Engine;
use nexus_core::testing::TestContext;

/// Count nodes carrying `label`.
fn count_label(engine: &mut Engine, label: &str) -> u64 {
    let q = format!("MATCH (n:{label}) RETURN count(n) AS c");
    let r = engine.execute_cypher(&q).expect("count query");
    r.rows[0].values[0].as_u64().unwrap_or(u64::MAX)
}

/// Count relationships of `rel_type` directly connecting a node with
/// `src_label` to a node with `dst_label`.
fn count_rel_between(engine: &mut Engine, src_label: &str, rel_type: &str, dst_label: &str) -> u64 {
    let q = format!("MATCH (a:{src_label})-[r:{rel_type}]->(b:{dst_label}) RETURN count(r) AS c");
    let r = engine.execute_cypher(&q).expect("count query");
    r.rows[0].values[0].as_u64().unwrap_or(u64::MAX)
}

/// §1.1 — the relationship variable is absent (`-[:KNOWS]->`, no `r:`);
/// both node endpoints carry a variable. This is the single most common
/// way to write a relationship MERGE. Hits the relationship-variable
/// early-return in `process_merge_relationship` ("Get relationship
/// variable and type" `match &rel_pattern.variable { None => return
/// Ok(None) }`).
#[test]
fn variable_less_relationship_merge_creates_both_endpoints_and_edge() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher(
            "MERGE (a:MraPerson {name: 'Alice'})-[:MraKnows]->(b:MraPerson {name: 'Bob'})",
        )
        .expect("variable-less relationship MERGE must succeed with no error");

    let bob = engine
        .execute_cypher("MATCH (b:MraPerson {name: 'Bob'}) RETURN b")
        .expect("follow-up MATCH for Bob");
    assert_eq!(
        bob.rows.len(),
        1,
        "the destination endpoint (Bob) must be created by the relationship MERGE, got {} rows",
        bob.rows.len()
    );

    let edge = engine
        .execute_cypher("MATCH (a:MraPerson {name: 'Alice'})-[r:MraKnows]->() RETURN r")
        .expect("follow-up MATCH for the edge");
    assert_eq!(
        edge.rows.len(),
        1,
        "the relationship must be created by the MERGE, got {} rows",
        edge.rows.len()
    );

    assert_eq!(
        count_label(&mut engine, "MraPerson"),
        2,
        "exactly two Person nodes expected"
    );
    assert_eq!(
        count_rel_between(&mut engine, "MraPerson", "MraKnows", "MraPerson"),
        1,
        "exactly one MraKnows relationship expected"
    );
}

/// §1.2 — the relationship variable is present but the DESTINATION node is
/// anonymous. Hits the destination-node-variable early-return in
/// `process_merge_relationship`.
#[test]
fn relationship_merge_with_anonymous_destination_creates_destination_and_edge() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher(
            "MERGE (a:MrbPerson {name: 'Alice'})-[r:MrbKnows]->(:MrbPerson {name: 'Bob'})",
        )
        .expect("relationship MERGE with anonymous destination must succeed with no error");

    let bob = engine
        .execute_cypher("MATCH (b:MrbPerson {name: 'Bob'}) RETURN b")
        .expect("follow-up MATCH for Bob");
    assert_eq!(
        bob.rows.len(),
        1,
        "the anonymous destination endpoint (Bob) must be created, got {} rows",
        bob.rows.len()
    );

    assert_eq!(
        count_label(&mut engine, "MrbPerson"),
        2,
        "exactly two Person nodes expected"
    );
    assert_eq!(
        count_rel_between(&mut engine, "MrbPerson", "MrbKnows", "MrbPerson"),
        1,
        "exactly one MrbKnows relationship expected"
    );
}

/// §1.3 — the relationship variable and destination variable are present,
/// but the SOURCE node is anonymous. Hits the source-node-variable
/// early-return in `process_merge_relationship` (the FIRST of the three
/// checks, so it fires before the destination or relationship variables
/// are even inspected).
#[test]
fn relationship_merge_with_anonymous_source_creates_source_and_edge() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher(
            "MERGE (:MrcPerson {name: 'Alice'})-[r:MrcKnows]->(b:MrcPerson {name: 'Bob'})",
        )
        .expect("relationship MERGE with anonymous source must succeed with no error");

    let alice = engine
        .execute_cypher("MATCH (a:MrcPerson {name: 'Alice'}) RETURN a")
        .expect("follow-up MATCH for Alice");
    assert_eq!(
        alice.rows.len(),
        1,
        "the anonymous source endpoint (Alice) must be created, got {} rows",
        alice.rows.len()
    );

    assert_eq!(
        count_label(&mut engine, "MrcPerson"),
        2,
        "exactly two Person nodes expected"
    );
    assert_eq!(
        count_rel_between(&mut engine, "MrcPerson", "MrcKnows", "MrcPerson"),
        1,
        "exactly one MrcKnows relationship expected"
    );
}

/// §4.2 — the fully-anonymous form: no variable anywhere in the pattern
/// (source node, relationship, AND destination node are all anonymous).
/// The most extreme case of the same bug: all three variable-presence
/// checks are bypassed, so both endpoints and the edge must still be
/// created atomically.
#[test]
fn fully_anonymous_relationship_merge_creates_both_endpoints_and_edge() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher(
            "MERGE (:MrdPerson {name: 'Alice'})-[:MrdKnows]->(:MrdPerson {name: 'Bob'})",
        )
        .expect("fully-anonymous relationship MERGE must succeed with no error");

    assert_eq!(
        count_label(&mut engine, "MrdPerson"),
        2,
        "both endpoints of a fully-anonymous relationship MERGE must be created"
    );
    assert_eq!(
        count_rel_between(&mut engine, "MrdPerson", "MrdKnows", "MrdPerson"),
        1,
        "the edge of a fully-anonymous relationship MERGE must be created"
    );

    // Re-running the identical fully-anonymous MERGE must not duplicate
    // anything: anonymous endpoints are still resolved by label+property
    // match-or-create, same as named ones.
    engine
        .execute_cypher(
            "MERGE (:MrdPerson {name: 'Alice'})-[:MrdKnows]->(:MrdPerson {name: 'Bob'})",
        )
        .expect("second identical fully-anonymous MERGE must not error");
    assert_eq!(
        count_label(&mut engine, "MrdPerson"),
        2,
        "re-running the fully-anonymous MERGE must not create duplicate nodes"
    );
    assert_eq!(
        count_rel_between(&mut engine, "MrdPerson", "MrdKnows", "MrdPerson"),
        1,
        "re-running the fully-anonymous MERGE must not create a duplicate edge"
    );
}

/// §4.2 — `ON CREATE SET` targeting a NODE variable (not the absent
/// relationship variable) must still apply when the relationship itself is
/// anonymous. The pattern's own `a` remains a real, user-visible binding
/// even though the relationship has none.
#[test]
fn on_create_set_targeting_node_applies_when_relationship_variable_is_absent() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher(
            "MERGE (a:MrePerson {name: 'Alice'})-[:MreKnows]->(b:MrePerson {name: 'Bob'}) \
             ON CREATE SET a.since = 'today'",
        )
        .expect("relationship MERGE with ON CREATE SET on a node var must succeed");

    let alice = engine
        .execute_cypher("MATCH (a:MrePerson {name: 'Alice'}) RETURN a.since AS since")
        .expect("follow-up MATCH for Alice");
    assert_eq!(alice.rows.len(), 1, "Alice must have been created");
    assert_eq!(
        alice.rows[0].values[0].as_str(),
        Some("today"),
        "ON CREATE SET a.since must apply even though the relationship variable is absent"
    );
}

/// §4.2 — `ON CREATE SET` referencing the (absent) relationship alias must
/// behave sanely: since no relationship variable was ever declared in the
/// pattern, no SET item could legitimately target it, so this must not
/// panic and must not corrupt the pattern's real bindings.
#[test]
fn on_create_set_referencing_a_nonexistent_relationship_alias_behaves_sanely() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    let res = engine.execute_cypher(
        "MERGE (a:MrfPerson {name: 'Alice'})-[:MrfKnows]->(b:MrfPerson {name: 'Bob'}) \
         ON CREATE SET r.touched = true",
    );
    // `r` was never declared anywhere in this statement, so this must
    // either be rejected with a clear error, or (at minimum) must not
    // panic and must not corrupt the pattern's own bindings.
    match res {
        Err(_) => {
            // Rejected outright — acceptable, and arguably the most
            // correct behaviour (Neo4j itself rejects `SET` on an
            // undeclared variable).
        }
        Ok(_) => {
            let both = count_label(&mut engine, "MrfPerson");
            assert_eq!(
                both, 2,
                "if the statement is accepted, both endpoints must still be created correctly"
            );
        }
    }
}
