//! phase0_fix-cypher-oom-process-abort §4.2 — unit coverage for the
//! byte budget check in [`Executor::apply_cartesian_product`]. The
//! integration-level regression test (the §1.1 minimal repro shape
//! surviving end-to-end instead of aborting the process) lives in
//! `crates/nexus-core/tests/cypher_oom_guard_test.rs`; these tests
//! pin the ceiling itself: it fires deterministically, is
//! configurable via `ExecutorConfig::cartesian_product_max_bytes`,
//! and does not reject legitimate small products under the default
//! budget.

use super::super::super::context::ExecutionContext;
use crate::Error;
use crate::executor::Query;
use crate::testing::{create_isolated_test_executor, create_test_executor};
use serde_json::Value;
use serde_json::json;
use std::collections::HashMap;

#[test]
fn exists_true_when_matching_relationship_present() {
    let (mut executor, _ctx) = create_test_executor();

    let create = Query {
        cypher: "CREATE (a:ExistsProbeA {name: 'a'})-[:EXISTS_PROBE_REL]->\
                 (b:ExistsProbeA {name: 'b'})"
            .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&create).expect("create should succeed");

    let query = Query {
        cypher: "MATCH (a:ExistsProbeA {name: 'a'}) \
                 WHERE EXISTS { (a)-[:EXISTS_PROBE_REL]->() } \
                 RETURN a.name AS name"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).expect("match should succeed");

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!("a"));
}

#[test]
fn exists_false_when_no_matching_relationship() {
    let (mut executor, _ctx) = create_test_executor();

    let create = Query {
        cypher: "CREATE (a:ExistsProbeB {name: 'a'})".to_string(),
        params: HashMap::new(),
    };
    executor.execute(&create).expect("create should succeed");

    let query = Query {
        cypher: "MATCH (a:ExistsProbeB {name: 'a'}) \
                 WHERE EXISTS { (a)-[:EXISTS_PROBE_REL_B]->() } \
                 RETURN a.name AS name"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).expect("match should succeed");

    assert_eq!(result.rows.len(), 0);
}

#[test]
fn not_exists_negates_the_pattern_probe() {
    let (mut executor, _ctx) = create_test_executor();

    let create = Query {
        cypher: "CREATE (a:ExistsProbeC {name: 'a'})-[:EXISTS_PROBE_REL_C]->\
                 (b:ExistsProbeC {name: 'b'}), (c:ExistsProbeC {name: 'c'})"
            .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&create).expect("create should succeed");

    let query = Query {
        cypher: "MATCH (n:ExistsProbeC) \
                 WHERE NOT EXISTS { (n)-[:EXISTS_PROBE_REL_C]->() } \
                 RETURN n.name AS name ORDER BY name"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).expect("match should succeed");

    let names: Vec<&str> = result
        .rows
        .iter()
        .map(|row| row.values[0].as_str().expect("name is a string"))
        .collect();
    assert_eq!(names, vec!["b", "c"]);
}

#[test]
fn exists_probes_multi_hop_chains() {
    let (mut executor, _ctx) = create_test_executor();

    let create = Query {
        cypher: "CREATE (a:ExistsProbeD {name: 'a'})-[:STEP1]->\
                 (b:ExistsProbeD {name: 'b'})-[:STEP2]->(c:ExistsProbeD {name: 'c'}), \
                 (x:ExistsProbeD {name: 'x'})-[:STEP1]->(y:ExistsProbeD {name: 'y'})"
            .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&create).expect("create should succeed");

    let query = Query {
        cypher: "MATCH (n:ExistsProbeD) \
                 WHERE EXISTS { (n)-[:STEP1]->()-[:STEP2]->() } \
                 RETURN n.name AS name"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).expect("match should succeed");

    // `x` has a STEP1 hop but its target `y` has no outgoing STEP2,
    // so only the full two-hop chain from `a` is a witness.
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!("a"));
}

#[test]
fn exists_inner_where_filters_candidate_bindings() {
    let (mut executor, _ctx) = create_test_executor();

    let create = Query {
        cypher: "CREATE (a:ExistsProbeE {name: 'a'})-[:KNOWS]->\
                 (b:ExistsProbeE {name: 'b', age: 10}), \
                 (a)-[:KNOWS]->(c:ExistsProbeE {name: 'c', age: 30})"
            .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&create).expect("create should succeed");

    let matches = Query {
        cypher: "MATCH (a:ExistsProbeE {name: 'a'}) \
                 WHERE EXISTS { (a)-[:KNOWS]->(x) WHERE x.age > 20 } \
                 RETURN a.name AS name"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&matches).expect("match should succeed");
    assert_eq!(result.rows.len(), 1, "one KNOWS target has age > 20");
    assert_eq!(result.rows[0].values[0], json!("a"));

    let no_matches = Query {
        cypher: "MATCH (a:ExistsProbeE {name: 'a'}) \
                 WHERE EXISTS { (a)-[:KNOWS]->(x) WHERE x.age > 100 } \
                 RETURN a.name AS name"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&no_matches).expect("match should succeed");
    assert_eq!(
        result.rows.len(),
        0,
        "no KNOWS target satisfies the inner WHERE"
    );
}

#[test]
fn exists_respects_the_correlated_outer_variable() {
    let (mut executor, _ctx) = create_test_executor();

    let create = Query {
        cypher: "CREATE (a:ExistsProbeF {name: 'a'})-[:REL_F]->\
                 (t:ExistsProbeF {name: 'target'}), (b:ExistsProbeF {name: 'b'})"
            .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&create).expect("create should succeed");

    let query = Query {
        cypher: "MATCH (n:ExistsProbeF) \
                 WHERE EXISTS { (n)-[:REL_F]->() } \
                 RETURN n.name AS name"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).expect("match should succeed");

    // Only `a` (bound to the outer `n`) has an outgoing REL_F; `b`
    // and `target` do not. This proves the probe is evaluated
    // against each row's own correlated binding, not a single
    // global existence check.
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!("a"));
}

#[test]
fn exists_returns_null_when_correlated_variable_is_null() {
    let (mut executor, _ctx) = create_test_executor();

    let create = Query {
        cypher: "CREATE (a:ExistsProbeG {name: 'a'})".to_string(),
        params: HashMap::new(),
    };
    executor.execute(&create).expect("create should succeed");

    let query = Query {
        cypher: "MATCH (a:ExistsProbeG {name: 'a'}) \
                 OPTIONAL MATCH (a)-[:NEVER_CREATED]->(m:ExistsProbeG) \
                 RETURN EXISTS { (m)-[:ALSO_NEVER_CREATED]->() } AS result"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).expect("match should succeed");

    // `m` is bound but NULL (the OPTIONAL MATCH found nothing), so
    // the pattern correlated to it cannot be probed: the predicate
    // is unknown (NULL), not false — and evaluation must not crash.
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], Value::Null);
}

#[test]
fn exists_matches_comma_separated_pattern_parts() {
    let (mut executor, _ctx) = create_test_executor();

    let create = Query {
        cypher: "CREATE (a:ExistsProbeH {name: 'a'})-[:H1]->(b:ExistsProbeH {name: 'b'}), \
                 (c:ExistsProbeH {name: 'c'})-[:H2]->(d:ExistsProbeH {name: 'd'})"
            .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&create).expect("create should succeed");

    // The first pattern part is correlated to the outer `n`; the
    // second is an independent, freshly-anchored comma-separated
    // part (label + inline property, no shared variable). Both
    // must be satisfied simultaneously.
    let matches = Query {
        cypher: "MATCH (n:ExistsProbeH {name: 'a'}) \
                 WHERE EXISTS { (n)-[:H1]->(), (:ExistsProbeH {name: 'c'})-[:H2]->() } \
                 RETURN n.name AS name"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&matches).expect("match should succeed");
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!("a"));

    let no_match = Query {
        cypher: "MATCH (n:ExistsProbeH {name: 'a'}) \
                 WHERE EXISTS { \
                     (n)-[:H1]->(), \
                     (:ExistsProbeH {name: 'c'})-[:NEVER_CREATED_H2]->() \
                 } \
                 RETURN n.name AS name"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&no_match).expect("match should succeed");
    assert_eq!(
        result.rows.len(),
        0,
        "second comma-separated part has no matching relationship"
    );
}

#[test]
fn exists_probes_incoming_direction() {
    let (mut executor, _ctx) = create_test_executor();

    let create = Query {
        cypher: "CREATE (a:ExistsProbeI {name: 'a'})-[:INTO]->(b:ExistsProbeI {name: 'b'})"
            .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&create).expect("create should succeed");

    let query = Query {
        cypher: "MATCH (b:ExistsProbeI {name: 'b'}) \
                 WHERE EXISTS { (b)<-[:INTO]-() } \
                 RETURN b.name AS name"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).expect("match should succeed");
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!("b"));
}

#[test]
fn exists_probes_both_direction_from_either_endpoint() {
    let (mut executor, _ctx) = create_test_executor();

    let create = Query {
        cypher: "CREATE (a:ExistsProbeJ {name: 'a'})-[:LINK]->(b:ExistsProbeJ {name: 'b'})"
            .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&create).expect("create should succeed");

    // Undirected `--` must find the edge starting from EITHER
    // endpoint: `a` takes the `source_id == anchor_id` branch of
    // the Both target-selection logic, `b` takes the other.
    let query = Query {
        cypher: "MATCH (n:ExistsProbeJ) \
                 WHERE EXISTS { (n)-[:LINK]-() } \
                 RETURN n.name AS name ORDER BY name"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).expect("match should succeed");

    let names: Vec<&str> = result
        .rows
        .iter()
        .map(|row| row.values[0].as_str().expect("name is a string"))
        .collect();
    assert_eq!(names, vec!["a", "b"]);
}

#[test]
fn exists_enforces_relationship_isomorphism_on_a_single_edge() {
    let (mut executor, _ctx) = create_test_executor();

    // Exactly one edge in the whole graph: a two-hop undirected
    // probe from `a` must NOT be satisfiable by re-traversing that
    // same edge from the other side.
    let create = Query {
        cypher: "CREATE (a:ExistsProbeK {name: 'a'})-[:ONLY_EDGE]->(b:ExistsProbeK {name: 'b'})"
            .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&create).expect("create should succeed");

    let query = Query {
        cypher: "MATCH (n:ExistsProbeK {name: 'a'}) \
                 WHERE EXISTS { (n)--()--() } \
                 RETURN n.name AS name"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).expect("match should succeed");
    assert_eq!(
        result.rows.len(),
        0,
        "the only edge cannot satisfy both hops of a two-hop probe"
    );
}

#[test]
fn exists_enforces_relationship_isomorphism_on_a_self_loop() {
    let (mut executor, _ctx) = create_test_executor();

    let create = Query {
        cypher: "CREATE (a:ExistsProbeL {name: 'a'})".to_string(),
        params: HashMap::new(),
    };
    executor.execute(&create).expect("create should succeed");
    let create_loop = Query {
        cypher: "MATCH (a:ExistsProbeL {name: 'a'}) CREATE (a)-[:SELF_LOOP]->(a)".to_string(),
        params: HashMap::new(),
    };
    executor
        .execute(&create_loop)
        .expect("create should succeed");

    // The self-loop is the only :SELF_LOOP edge from `a`; a
    // two-hop probe must not be able to traverse it twice.
    let query = Query {
        cypher: "MATCH (n:ExistsProbeL {name: 'a'}) \
                 WHERE EXISTS { (n)-[:SELF_LOOP]->()-[:SELF_LOOP]->() } \
                 RETURN n.name AS name"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).expect("match should succeed");
    assert_eq!(
        result.rows.len(),
        0,
        "the self-loop cannot satisfy both hops of a two-hop probe"
    );
}

#[test]
fn exists_inline_node_property_map_filters_targets() {
    let (mut executor, _ctx) = create_test_executor();

    let create = Query {
        cypher: "CREATE (a:ExistsProbeM {name: 'a'})-[:KNOWS]->\
                 (b:ExistsProbeM {name: 'b', age: 10}), \
                 (a)-[:KNOWS]->(c:ExistsProbeM {name: 'c', age: 30})"
            .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&create).expect("create should succeed");

    let matches = Query {
        cypher: "MATCH (a:ExistsProbeM {name: 'a'}) \
                 WHERE EXISTS { (a)-[:KNOWS]->({age: 30}) } \
                 RETURN a.name AS name"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&matches).expect("match should succeed");
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!("a"));

    let no_match = Query {
        cypher: "MATCH (a:ExistsProbeM {name: 'a'}) \
                 WHERE EXISTS { (a)-[:KNOWS]->({age: 99}) } \
                 RETURN a.name AS name"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&no_match).expect("match should succeed");
    assert_eq!(result.rows.len(), 0);
}

#[test]
fn exists_correlated_inline_property_map_matches_outer_variable() {
    let (mut executor, _ctx) = create_test_executor();

    let create = Query {
        cypher: "CREATE (a:ExistsProbeN {name: 'a', id: 1})-[:SELF_REF]->\
                 (b:ExistsProbeN {name: 'b', id: 1}), \
                 (a)-[:SELF_REF]->(c:ExistsProbeN {name: 'c', id: 2})"
            .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&create).expect("create should succeed");

    // The inline property expression `a.id` references the outer,
    // correlated `a` — only the target that shares `a`'s id
    // qualifies.
    let query = Query {
        cypher: "MATCH (a:ExistsProbeN {name: 'a'}) \
                 WHERE EXISTS { (a)-[:SELF_REF]->(x {id: a.id}) } \
                 RETURN a.name AS name"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).expect("match should succeed");
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!("a"));
}

#[test]
fn exists_null_property_expression_never_matches() {
    let (mut executor, _ctx) = create_test_executor();

    let create = Query {
        cypher: "CREATE (a:ExistsProbeO {name: 'a'})-[:REL_O]->\
                 (b:ExistsProbeO {name: 'b', tag: 'x'})"
            .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&create).expect("create should succeed");

    // `a.missing_prop` evaluates to NULL (the property doesn't
    // exist on `a`); a NULL expected value must never match, even
    // though `b.tag` is a concrete non-null value.
    let query = Query {
        cypher: "MATCH (a:ExistsProbeO {name: 'a'}) \
                 WHERE EXISTS { (a)-[:REL_O]->({tag: a.missing_prop}) } \
                 RETURN a.name AS name"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).expect("match should succeed");
    assert_eq!(result.rows.len(), 0);
}

#[test]
fn exists_label_constraint_on_hop_target() {
    let (mut executor, _ctx) = create_test_executor();

    let create = Query {
        cypher: "CREATE (a:ExistsProbeP {name: 'a'})-[:REL_P]->(c:ExistsProbeP {name: 'c'})"
            .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&create).expect("create should succeed");

    let matches = Query {
        cypher: "MATCH (a:ExistsProbeP {name: 'a'}) \
                 WHERE EXISTS { (a)-[:REL_P]->(:ExistsProbeP) } \
                 RETURN a.name AS name"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&matches).expect("match should succeed");
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!("a"));

    let create_wrong_label = Query {
        cypher: "CREATE (a:ExistsProbeP2 {name: 'a'})-[:REL_P2]->(b:ExistsProbeQ2 {name: 'b'})"
            .to_string(),
        params: HashMap::new(),
    };
    executor
        .execute(&create_wrong_label)
        .expect("create should succeed");

    let no_match = Query {
        cypher: "MATCH (a:ExistsProbeP2 {name: 'a'}) \
                 WHERE EXISTS { (a)-[:REL_P2]->(:ExistsProbeP2) } \
                 RETURN a.name AS name"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&no_match).expect("match should succeed");
    assert_eq!(
        result.rows.len(),
        0,
        "the only REL_P2 target carries the wrong label"
    );
}

#[test]
fn exists_relationship_variable_reuse_requires_same_edge() {
    let (mut executor, _ctx) = create_test_executor();

    let create = Query {
        cypher: "CREATE (a:ExistsProbeR {name: 'a'})-[:REL_R {weight: 10}]->\
                 (b:ExistsProbeR {name: 'b'})"
            .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&create).expect("create should succeed");

    // The relationship variable materialises into the binding and
    // is queryable from the inner WHERE.
    let binds = Query {
        cypher: "MATCH (a:ExistsProbeR {name: 'a'}) \
                 WHERE EXISTS { (a)-[r:REL_R]->() WHERE r.weight > 5 } \
                 RETURN a.name AS name"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&binds).expect("match should succeed");
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!("a"));

    let create_two_edges = Query {
        cypher: "CREATE (a:ExistsProbeS {name: 'a'})-[:REL_S]->(b:ExistsProbeS {name: 'b'}), \
                 (a)-[:REL_S]->(c:ExistsProbeS {name: 'c'})"
            .to_string(),
        params: HashMap::new(),
    };
    executor
        .execute(&create_two_edges)
        .expect("create should succeed");

    // `a` has two distinct :REL_S edges (to `b` and to `c`).
    // Reusing the same relationship variable `r` across both
    // comma-separated parts demands the SAME edge id for both —
    // impossible here, since the edge to `b` and the edge to `c`
    // are different relationships.
    let reuse_mismatch = Query {
        cypher: "MATCH (a:ExistsProbeS {name: 'a'}) \
                 WHERE EXISTS { \
                     (a)-[r:REL_S]->(:ExistsProbeS {name: 'b'}), \
                     (a)-[r:REL_S]->(:ExistsProbeS {name: 'c'}) \
                 } \
                 RETURN a.name AS name"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor
        .execute(&reuse_mismatch)
        .expect("match should succeed");
    assert_eq!(
        result.rows.len(),
        0,
        "reusing `r` across parts requires literally the same edge"
    );
}

#[test]
fn exists_triangle_correlation_closes_back_to_anchor() {
    let (mut executor, _ctx) = create_test_executor();

    let create = Query {
        cypher: "CREATE (a:ExistsProbeT {name: 'a'})-[:TRI]->(b:ExistsProbeT {name: 'b'})"
            .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&create).expect("create should succeed");
    let close_triangle = Query {
        cypher: "MATCH (a:ExistsProbeT {name: 'a'}), (b:ExistsProbeT {name: 'b'}) \
                 CREATE (b)-[:TRI]->(a)"
            .to_string(),
        params: HashMap::new(),
    };
    executor
        .execute(&close_triangle)
        .expect("create should succeed");

    let closes = Query {
        cypher: "MATCH (a:ExistsProbeT {name: 'a'}) \
                 WHERE EXISTS { (a)-[:TRI]->()-[:TRI]->(a) } \
                 RETURN a.name AS name"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&closes).expect("match should succeed");
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!("a"));

    let create_open_chain = Query {
        cypher: "CREATE (x:ExistsProbeU {name: 'x'})-[:TRI]->\
                 (y:ExistsProbeU {name: 'y'})-[:TRI]->(z:ExistsProbeU {name: 'z'})"
            .to_string(),
        params: HashMap::new(),
    };
    executor
        .execute(&create_open_chain)
        .expect("create should succeed");

    let does_not_close = Query {
        cypher: "MATCH (x:ExistsProbeU {name: 'x'}) \
                 WHERE EXISTS { (x)-[:TRI]->()-[:TRI]->(x) } \
                 RETURN x.name AS name"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor
        .execute(&does_not_close)
        .expect("match should succeed");
    assert_eq!(
        result.rows.len(),
        0,
        "the open chain never closes back to x"
    );
}

#[test]
fn exists_inner_where_references_outer_variable() {
    let (mut executor, _ctx) = create_test_executor();

    let create = Query {
        cypher: "CREATE (a:ExistsProbeV {name: 'a', age: 20})-[:KNOWS]->\
                 (b:ExistsProbeV {name: 'b', age: 25}), \
                 (a)-[:KNOWS]->(c:ExistsProbeV {name: 'c', age: 15})"
            .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&create).expect("create should succeed");

    let query = Query {
        cypher: "MATCH (a:ExistsProbeV {name: 'a'}) \
                 WHERE EXISTS { (a)-[:KNOWS]->(x) WHERE x.age > a.age } \
                 RETURN a.name AS name"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).expect("match should succeed");
    assert_eq!(result.rows.len(), 1, "b.age (25) exceeds a.age (20)");
    assert_eq!(result.rows[0].values[0], json!("a"));
}

#[test]
fn exists_null_inner_where_filters_like_false() {
    let (mut executor, _ctx) = create_test_executor();

    let create = Query {
        cypher: "CREATE (a:ExistsProbeW {name: 'a'})-[:OWNS_W]->(c:ExistsProbeW {name: 'c'})"
            .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&create).expect("create should succeed");

    // `x.missing > 1` is NULL for every candidate (the property
    // doesn't exist). A NULL inner WHERE excludes the candidate
    // exactly like false, so EXISTS is false — and NOT EXISTS
    // must therefore return the row, not swallow it as NULL.
    let query = Query {
        cypher: "MATCH (a:ExistsProbeW {name: 'a'}) \
                 WHERE NOT EXISTS { (a)-[:OWNS_W]->(x) WHERE x.missing > 1 } \
                 RETURN a.name AS name"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).expect("match should succeed");
    assert_eq!(
        result.rows.len(),
        1,
        "NULL inner WHERE must make EXISTS false"
    );
    assert_eq!(result.rows[0].values[0], json!("a"));
}

#[test]
fn exists_bare_star_var_length_finds_a_two_hop_chain() {
    let (mut executor, _ctx) = create_test_executor();

    let create = Query {
        cypher: "CREATE (a:ExistsProbeAD {name: 'a'})-[:AD1]->(b:ExistsProbeAD {name: 'b'})-\
                 [:AD1]->(c:ExistsProbeAD {name: 'c'})"
            .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&create).expect("create should succeed");

    let matches = Query {
        cypher: "MATCH (a:ExistsProbeAD {name: 'a'}) \
                 WHERE EXISTS { (a)-[:AD1*]->(:ExistsProbeAD {name: 'c'}) } \
                 RETURN a.name AS name"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&matches).expect("match should succeed");
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!("a"));

    let no_match = Query {
        cypher: "MATCH (a:ExistsProbeAD {name: 'a'}) \
                 WHERE EXISTS { (a)-[:AD1*]->(:ExistsProbeAD {name: 'never'}) } \
                 RETURN a.name AS name"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&no_match).expect("match should succeed");
    assert_eq!(result.rows.len(), 0);
}

#[test]
fn exists_bare_star_var_length_has_a_lower_bound_of_one() {
    let (mut executor, _ctx) = create_test_executor();

    // openCypher's bare `*` means `*1..` (one or more), not
    // `*0..`. An isolated node with no `:X` edges at all must NOT
    // satisfy `EXISTS { (a)-[:X*]->() }` — the zero-length case
    // (accepting the anchor itself against an unconstrained
    // target) must not be reachable for a bare `*`.
    let create_isolated = Query {
        cypher: "CREATE (a:ExistsProbeAE {name: 'a'})".to_string(),
        params: HashMap::new(),
    };
    executor
        .execute(&create_isolated)
        .expect("create should succeed");

    let isolated_query = Query {
        cypher: "MATCH (a:ExistsProbeAE {name: 'a'}) \
                 WHERE EXISTS { (a)-[:X*]->() } \
                 RETURN a.name AS name"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor
        .execute(&isolated_query)
        .expect("match should succeed");
    assert_eq!(
        result.rows.len(),
        0,
        "an isolated node has no witness for a bare `*` (min 1 hop)"
    );

    let create_edge = Query {
        cypher: "CREATE (b:ExistsProbeAE {name: 'b'})-[:X]->(c:ExistsProbeAE {name: 'c'})"
            .to_string(),
        params: HashMap::new(),
    };
    executor
        .execute(&create_edge)
        .expect("create should succeed");

    let edge_query = Query {
        cypher: "MATCH (b:ExistsProbeAE {name: 'b'}) \
                 WHERE EXISTS { (b)-[:X*]->() } \
                 RETURN b.name AS name"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&edge_query).expect("match should succeed");
    assert_eq!(result.rows.len(), 1, "one real edge is a valid witness");
    assert_eq!(result.rows[0].values[0], json!("b"));
}

#[test]
fn exists_bounded_var_length_respects_the_max_hop() {
    let (mut executor, _ctx) = create_test_executor();

    let create = Query {
        cypher: "CREATE (a:ExistsProbeX {name: 'a'})-[:X1]->(b:ExistsProbeX {name: 'b'})-\
                 [:X1]->(c:ExistsProbeX {name: 'c'})-[:X1]->(d:ExistsProbeX {name: 'd'})"
            .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&create).expect("create should succeed");

    // `c` sits within the 1..2 bound (2 hops).
    let within_bound = Query {
        cypher: "MATCH (a:ExistsProbeX {name: 'a'}) \
                 WHERE EXISTS { (a)-[:X1*1..2]->(:ExistsProbeX {name: 'c'}) } \
                 RETURN a.name AS name"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor
        .execute(&within_bound)
        .expect("match should succeed");
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!("a"));

    // `d` is only reachable via a 3-hop path, past the `*1..2` max.
    let past_bound = Query {
        cypher: "MATCH (a:ExistsProbeX {name: 'a'}) \
                 WHERE EXISTS { (a)-[:X1*1..2]->(:ExistsProbeX {name: 'd'}) } \
                 RETURN a.name AS name"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&past_bound).expect("match should succeed");
    assert_eq!(
        result.rows.len(),
        0,
        "the only path to `d` is 3 hops, past the *1..2 max"
    );
}

#[test]
fn exists_exact_var_length_requires_the_exact_hop_count() {
    let (mut executor, _ctx) = create_test_executor();

    let create = Query {
        cypher: "CREATE (a:ExistsProbeY {name: 'a'})-[:Y1]->(b:ExistsProbeY {name: 'b'})-\
                 [:Y1]->(c:ExistsProbeY {name: 'c'})"
            .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&create).expect("create should succeed");

    let one_hop_neighbor = Query {
        cypher: "MATCH (a:ExistsProbeY {name: 'a'}) \
                 WHERE EXISTS { (a)-[:Y1*2]->(:ExistsProbeY {name: 'b'}) } \
                 RETURN a.name AS name"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor
        .execute(&one_hop_neighbor)
        .expect("match should succeed");
    assert_eq!(result.rows.len(), 0, "`b` is 1 hop away, not exactly 2");

    let two_hop_neighbor = Query {
        cypher: "MATCH (a:ExistsProbeY {name: 'a'}) \
                 WHERE EXISTS { (a)-[:Y1*2]->(:ExistsProbeY {name: 'c'}) } \
                 RETURN a.name AS name"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor
        .execute(&two_hop_neighbor)
        .expect("match should succeed");
    assert_eq!(result.rows.len(), 1, "`c` is exactly 2 hops away");
    assert_eq!(result.rows[0].values[0], json!("a"));
}

#[test]
fn exists_zero_length_var_length_matches_the_anchor_itself() {
    // ISOLATED catalog, deliberately: the shared per-process test catalog
    // hands out one label-id sequence to all ~2770 lib tests, and a node
    // stores its labels in a 64-bit `label_bits` bitmap. Past id 64 the
    // label is silently dropped and every label-scoped match over it
    // returns zero rows — measured: this test's label was getting id 77 in
    // a full run. That is not a parallelism flake (a `--test-threads=1`
    // run fails it identically); it is deterministic given how many labels
    // are registered ahead of it.
    let (mut executor, _ctx) = create_isolated_test_executor();

    let create = Query {
        cypher: "CREATE (a:ExistsProbeZ {name: 'a'})".to_string(),
        params: HashMap::new(),
    };
    executor.execute(&create).expect("create should succeed");

    // No `:Z1` relationship was ever created, so the type never
    // resolves in the catalog — the zero-length case must still
    // be evaluated on its own merits and match `a` against
    // itself, consuming no edge.
    let query = Query {
        cypher: "MATCH (a:ExistsProbeZ {name: 'a'}) \
                 WHERE EXISTS { (a)-[:Z1*0..1]->(:ExistsProbeZ {name: 'a'}) } \
                 RETURN a.name AS name"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).expect("match should succeed");
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!("a"));
}

#[test]
fn exists_undirected_var_length_traverses_both_ways() {
    // ISOLATED catalog, deliberately: the shared per-process test catalog
    // hands out one label-id sequence to all ~2770 lib tests, and a node
    // stores its labels in a 64-bit `label_bits` bitmap. Past id 64 the
    // label is silently dropped and every label-scoped match over it
    // returns zero rows — measured: this test's label was getting id 77 in
    // a full run. That is not a parallelism flake (a `--test-threads=1`
    // run fails it identically); it is deterministic given how many labels
    // are registered ahead of it.
    let (mut executor, _ctx) = create_isolated_test_executor();

    // Both edges are stored Outgoing (a -> b -> c); probing
    // undirected from `c` must still walk them backwards to `a`.
    let create = Query {
        cypher: "CREATE (a:ExistsProbeAA {name: 'a'})-[:AA1]->\
                 (b:ExistsProbeAA {name: 'b'})-[:AA1]->(c:ExistsProbeAA {name: 'c'})"
            .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&create).expect("create should succeed");

    let query = Query {
        cypher: "MATCH (c:ExistsProbeAA {name: 'c'}) \
                 WHERE EXISTS { (c)-[:AA1*]-(:ExistsProbeAA {name: 'a'}) } \
                 RETURN c.name AS name"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).expect("match should succeed");
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!("c"));
}

#[test]
fn exists_var_length_isomorphism_blocks_reusing_the_only_edge() {
    let (mut executor, _ctx) = create_test_executor();

    // Exactly one edge in the graph, stored directed a -> b.
    // BOTH segments are undirected (`-`, not `->`): the `+`
    // var-length segment (min 1 hop) must consume the only edge
    // to reach `b`, and the fixed hop that follows is undirected
    // too, so — without relationship-isomorphism enforcement — it
    // could walk the SAME edge backwards from `b` to `a` and be
    // satisfied. A directed fixed hop would fail here for an
    // unrelated reason (no OUTGOING edge from `b`) without ever
    // exercising the isomorphism check, which is why this test
    // deliberately keeps both segments undirected.
    let create = Query {
        cypher: "CREATE (a:ExistsProbeAB {name: 'a'})-[:AB1]->(b:ExistsProbeAB {name: 'b'})"
            .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&create).expect("create should succeed");

    let query = Query {
        cypher: "MATCH (a:ExistsProbeAB {name: 'a'}) \
                 WHERE EXISTS { (a)-[:AB1+]-()-[:AB1]-() } \
                 RETURN a.name AS name"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).expect("match should succeed");
    assert_eq!(
        result.rows.len(),
        0,
        "the only edge can't satisfy both the undirected var-length segment \
         and the undirected fixed hop that follows it"
    );
}

#[test]
fn not_exists_composes_with_var_length_relationships() {
    let (mut executor, _ctx) = create_test_executor();

    let create = Query {
        cypher: "CREATE (a:ExistsProbeAC {name: 'a'})-[:AC1]->(b:ExistsProbeAC {name: 'b'})"
            .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&create).expect("create should succeed");

    // Only a 1-hop path exists; an exact-2-hop probe is false, so
    // NOT EXISTS must be true.
    let query = Query {
        cypher: "MATCH (a:ExistsProbeAC {name: 'a'}) \
                 WHERE NOT EXISTS { (a)-[:AC1*2]->() } \
                 RETURN a.name AS name"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).expect("match should succeed");
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!("a"));
}

#[test]
fn exists_named_rel_variable_on_var_length_relationship_is_rejected() {
    let (mut executor, _ctx) = create_test_executor();

    let create = Query {
        cypher: "CREATE (a:ExistsProbeAF {name: 'a'})-[:AF1]->(b:ExistsProbeAF {name: 'b'})"
            .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&create).expect("create should succeed");

    // A named relationship variable on a variable-length hop would
    // bind a LIST<RELATIONSHIP> in full Cypher, which this probe
    // does not support — it must fail loudly, not silently bind a
    // single relationship value or ignore the variable.
    let query = Query {
        cypher: "MATCH (a:ExistsProbeAF {name: 'a'}) \
                 WHERE EXISTS { (a)-[r:AF1*]->() } \
                 RETURN a.name AS name"
            .to_string(),
        params: HashMap::new(),
    };
    let err = executor
        .execute(&query)
        .expect_err("a named rel-var on a var-length relationship must be rejected");
    assert!(
        err.to_string()
            .contains("ERR_VAR_LENGTH_REL_VARIABLE_NOT_IMPLEMENTED"),
        "unexpected error message: {err}"
    );
}

#[test]
fn apply_cartesian_product_rejects_when_budget_is_absurdly_low() {
    let (mut executor, _ctx) = create_test_executor();
    // A trivial 2x2 product estimates to 2 * 2 * columns * 32 bytes
    // (>= 128 bytes even at columns=1). A 1-byte budget must reject
    // it regardless of how small the product actually is.
    executor.config.cartesian_product_max_bytes = 1;

    let mut context = ExecutionContext::new(HashMap::new(), None);
    context.set_variable("a", Value::Array(vec![json!(1), json!(2)]));

    let result = executor.apply_cartesian_product(&mut context, "b", vec![json!(3), json!(4)]);

    match result {
        Err(Error::OutOfMemory(msg)) => {
            assert!(
                msg.contains("Cartesian product"),
                "OutOfMemory message should name the offending operation: {msg}"
            );
        }
        other => {
            panic!("expected Err(Error::OutOfMemory(_)) under a 1-byte budget, got {other:?}")
        }
    }
}

#[test]
fn apply_cartesian_product_succeeds_under_default_budget() {
    // Same shape as the low-budget test above, but with the
    // default (1 GiB) budget left untouched — proves the rejection
    // above comes specifically from the configured ceiling, not
    // from `apply_cartesian_product` being broken for any input.
    let (mut executor, _ctx) = create_test_executor();

    let mut context = ExecutionContext::new(HashMap::new(), None);
    context.set_variable("a", Value::Array(vec![json!(1), json!(2)]));

    executor
        .apply_cartesian_product(&mut context, "b", vec![json!(3), json!(4)])
        .expect("a 2x2 product must stay well under the default 1 GiB budget");

    assert_eq!(
        context.get_variable("a"),
        Some(&Value::Array(vec![json!(1), json!(1), json!(2), json!(2)]))
    );
    assert_eq!(
        context.get_variable("b"),
        Some(&Value::Array(vec![json!(3), json!(4), json!(3), json!(4)]))
    );
}

/// phase0_fix-materialize-recrosses-aligned-columns — DISCRIMINATING.
/// After `apply_cartesian_product` aligns two columns to length 4
/// (`a=[1,1,2,2]`, `b=[3,4,3,4]`, each index = one output row), the
/// aligned materialiser must ZIP them into exactly 4 rows, while the
/// general materialiser RE-crosses them into 4*4 = 16. The `k`-column
/// gap is `N^(k-1)`; at query scale (`N=384`, `k=3`) that same
/// re-cross is `384^3 ≈ 56.6M` rows (~13 GB), which froze the host.
#[test]
fn materialize_aligned_rows_zips_instead_of_recrossing() {
    let (mut executor, _ctx) = create_test_executor();

    let mut context = ExecutionContext::new(HashMap::new(), None);
    context.set_variable("a", Value::Array(vec![json!(1), json!(2)]));
    executor
        .apply_cartesian_product(&mut context, "b", vec![json!(3), json!(4)])
        .expect("2x2 product stays under the default budget");

    // Preconditions: both columns are aligned to length 4.
    assert_eq!(
        context.get_variable("a"),
        Some(&Value::Array(vec![json!(1), json!(1), json!(2), json!(2)]))
    );
    assert_eq!(
        context.get_variable("b"),
        Some(&Value::Array(vec![json!(3), json!(4), json!(3), json!(4)]))
    );

    // The general materialiser RE-crosses the aligned columns: 4 x 4 = 16.
    // This is the over-production the fix avoids (documented, not desired).
    let recrossed = executor
        .materialize_rows_from_variables(&context)
        .expect("materialize should succeed for this small aligned context");
    assert_eq!(
        recrossed.len(),
        16,
        "materialize_rows_from_variables re-crosses aligned columns (N^k); \
         this pins the bug the aligned path must avoid"
    );

    // The aligned materialiser ZIPS: exactly the 4 rows the columns
    // already represent, in index order.
    let zipped = executor.materialize_aligned_rows(&context);
    assert_eq!(
        zipped.len(),
        4,
        "materialize_aligned_rows must zip aligned columns to N rows, not N^k"
    );

    let mut pairs: Vec<(i64, i64)> = zipped
        .iter()
        .map(|row| {
            (
                row["a"].as_i64().expect("a is an integer"),
                row["b"].as_i64().expect("b is an integer"),
            )
        })
        .collect();
    pairs.sort_unstable();
    assert_eq!(
        pairs,
        vec![(1, 3), (1, 4), (2, 3), (2, 4)],
        "zipped rows must be the exact index-aligned (a, b) pairs"
    );
}

/// The cartesian-materialisation path inside
/// `materialize_rows_from_variables` (two-plus same-length,
/// multi-element arrays) must reject with `Error::OutOfMemory` when
/// the estimated product exceeds the configured byte budget, instead
/// of building the full row set — the same contract
/// `apply_cartesian_product` enforces.
#[test]
fn materialize_rows_rejects_cartesian_product_over_budget() {
    let (mut executor, _ctx) = create_test_executor();
    // A 1-byte budget rejects any real product (each row is >= a few
    // dozen bytes even for one column).
    executor.set_cartesian_product_max_bytes(1);

    let mut context = ExecutionContext::new(HashMap::new(), None);
    // Two same-length, multi-element arrays => the cartesian branch.
    context.set_variable("a", Value::Array(vec![json!(1), json!(2), json!(3)]));
    context.set_variable("b", Value::Array(vec![json!(4), json!(5), json!(6)]));

    let err = executor
        .materialize_rows_from_variables(&context)
        .expect_err("a cartesian materialisation over the byte budget must error");
    assert!(
        matches!(err, crate::Error::OutOfMemory(_)),
        "expected Error::OutOfMemory, got {err:?}"
    );
}
