//! Tests for constraint enforcement (NODE KEY, property-type, relationship
//! NOT NULL, backfill rejection, relaxed mode) and Cypher 25 DDL dispatch.

use super::*;

// ──────────── phase6_opencypher-constraint-enforcement ────────────
//
// One engine per test spawns an LMDB env, and this suite already
// sits near the Windows TLS slot cap. The tests below bundle every
// scenario for one constraint kind into a single engine instance.

#[test]
fn constraint_enforcement_all_kinds() {
    use crate::constraints::ScalarType;
    // `setup_test_engine` (non-isolated) reuses the shared LMDB env,
    // keeping the Windows TLS slot budget healthy for the rest of
    // the suite.
    let (mut engine, _ctx) = crate::testing::setup_test_engine().unwrap();

    // ─── NODE KEY (composite unique + NOT NULL) ───
    engine
        .add_node_key_constraint("Person", &["tenantId", "id"], Some("person_key"))
        .expect("register NODE KEY");
    engine
        .create_node(
            vec!["Person".to_string()],
            serde_json::json!({ "tenantId": "t1", "id": 1, "name": "Alice" }),
        )
        .expect("first tuple accepted");
    // Duplicate tuple → NODE_KEY violation.
    let err = engine
        .create_node(
            vec!["Person".to_string()],
            serde_json::json!({ "tenantId": "t1", "id": 1, "name": "Bob" }),
        )
        .expect_err("duplicate tuple must be rejected");
    assert!(err.to_string().contains("NODE_KEY"));
    // Missing component → NODE_KEY violation (implicit NOT NULL).
    let err = engine
        .create_node(
            vec!["Person".to_string()],
            serde_json::json!({ "tenantId": "t1" }),
        )
        .expect_err("missing component must be rejected");
    assert!(err.to_string().contains("NODE_KEY"));
    // Different tuple → accepted.
    engine
        .create_node(
            vec!["Person".to_string()],
            serde_json::json!({ "tenantId": "t1", "id": 2 }),
        )
        .expect("distinct tuple accepted");

    // ─── Property-type ───
    engine
        .add_property_type_constraint("Person", "age", ScalarType::Integer, Some("person_age_int"))
        .unwrap();
    let err = engine
        .create_node(
            vec!["Person".to_string()],
            serde_json::json!({ "tenantId": "t2", "id": 1, "age": "thirty" }),
        )
        .expect_err("STRING age rejected under IS :: INTEGER");
    assert!(err.to_string().contains("PROPERTY_TYPE"));

    // ─── Relationship NOT NULL ───
    engine
        .add_rel_not_null_constraint("CONNECTS", "weight", Some("rel_weight_required"))
        .unwrap();
    let a = engine
        .create_node(vec!["X".to_string()], serde_json::json!({"i": 1}))
        .unwrap();
    let b = engine
        .create_node(vec!["X".to_string()], serde_json::json!({"i": 2}))
        .unwrap();
    let err = engine
        .create_relationship(a, b, "CONNECTS".to_string(), serde_json::json!({}))
        .expect_err("rel without required property rejected");
    assert!(err.to_string().contains("RELATIONSHIP_PROPERTY_EXISTENCE"));
    engine
        .create_relationship(
            a,
            b,
            "CONNECTS".to_string(),
            serde_json::json!({"weight": 1.5}),
        )
        .expect("rel with weight accepted");

    // ─── Backfill rejection — same engine, TLS-friendly ───
    engine
        .create_node(
            vec!["Thing".to_string()],
            serde_json::json!({"name": "no-id"}),
        )
        .unwrap();
    let err = engine
        .add_node_key_constraint("Thing", &["id"], Some("thing_id"))
        .expect_err("existing row without id should abort NODE_KEY CREATE");
    assert!(err.to_string().contains("NODE_KEY"));
    assert!(err.to_string().contains("backfill"));

    // ─── Relaxed mode ───
    engine.set_relaxed_constraint_enforcement(true);
    engine
        .add_property_type_constraint("Doc", "age", ScalarType::Integer, None)
        .unwrap();
    engine
        .create_node(
            vec!["Doc".to_string()],
            serde_json::json!({ "age": "thirty" }),
        )
        .expect("relaxed mode logs instead of rejecting");
    engine.set_relaxed_constraint_enforcement(false);
}

// `scalar_type_canonical_values` was moved into
// `crate::constraints::tests` where it doesn't pay the LMDB TLS
// cost of a sibling `setup_isolated_test_engine` in this file.

// phase6_opencypher-constraint-enforcement — Cypher 25 DDL dispatch
// into the extended constraint APIs.
#[test]
fn cypher25_ddl_routes_through_extended_constraint_apis() {
    let (mut engine, _ctx) = crate::testing::setup_test_engine().unwrap();

    // NODE KEY via DDL.
    engine
        .execute_cypher(
            "CREATE CONSTRAINT person_key FOR (p:Person) \
             REQUIRE (p.tenantId, p.id) IS NODE KEY",
        )
        .expect("NODE KEY DDL must succeed");
    engine
        .create_node(
            vec!["Person".to_string()],
            serde_json::json!({"tenantId": "t1", "id": 1}),
        )
        .expect("first tuple accepted");
    let err = engine
        .create_node(
            vec!["Person".to_string()],
            serde_json::json!({"tenantId": "t1", "id": 1}),
        )
        .expect_err("duplicate tuple rejected via DDL-registered NODE KEY");
    assert!(err.to_string().contains("NODE_KEY"));

    // Property-type via DDL.
    engine
        .execute_cypher("CREATE CONSTRAINT FOR (p:Person) REQUIRE p.age IS :: INTEGER")
        .expect("property-type DDL must succeed");
    let err = engine
        .create_node(
            vec!["Person".to_string()],
            serde_json::json!({"tenantId": "t2", "id": 1, "age": "thirty"}),
        )
        .expect_err("STRING age rejected under IS :: INTEGER DDL");
    assert!(err.to_string().contains("PROPERTY_TYPE"));

    // Relationship NOT NULL via DDL.
    engine
        .execute_cypher("CREATE CONSTRAINT FOR ()-[r:CONNECTS]-() REQUIRE r.weight IS NOT NULL")
        .expect("rel NOT NULL DDL must succeed");
    let a = engine
        .create_node(vec!["X".to_string()], serde_json::json!({"i": 1}))
        .unwrap();
    let b = engine
        .create_node(vec!["X".to_string()], serde_json::json!({"i": 2}))
        .unwrap();
    let err = engine
        .create_relationship(a, b, "CONNECTS".to_string(), serde_json::json!({}))
        .expect_err("rel missing weight rejected via DDL-registered NOT NULL");
    assert!(err.to_string().contains("RELATIONSHIP_PROPERTY_EXISTENCE"));
}
