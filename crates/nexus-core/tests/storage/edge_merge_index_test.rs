//! Edge-MERGE existence via the exact `(src, type, dst)` index
//! (phase6_add-src-type-dst-edge-index). The index is a verified hint with
//! a chain-walk fallback, so these tests pin MERGE-edge idempotency both in
//! a live session and after a reopen (where the index is rebuilt from
//! storage).

use nexus_core::Engine;
use nexus_core::testing::TestContext;

fn rel_count(engine: &mut Engine) -> u64 {
    let r = engine
        .execute_cypher("MATCH ()-[r:R]->() RETURN count(r) AS c")
        .expect("count rels");
    r.rows[0].values[0].as_u64().unwrap_or(u64::MAX)
}

fn seed_two_nodes(engine: &mut Engine) {
    engine
        .execute_cypher("CREATE (:N {id: 1}), (:N {id: 2})")
        .expect("seed nodes");
}

const MERGE_EDGE: &str = "MATCH (a:N {id: 1}), (b:N {id: 2}) MERGE (a)-[r:R]->(b)";

#[test]
fn edge_merge_is_idempotent_in_session() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine");
    seed_two_nodes(&mut engine);

    engine.execute_cypher(MERGE_EDGE).expect("merge edge 1");
    assert_eq!(rel_count(&mut engine), 1, "first MERGE creates one edge");

    engine.execute_cypher(MERGE_EDGE).expect("merge edge 2");
    assert_eq!(
        rel_count(&mut engine),
        1,
        "second MERGE must match the existing edge, not duplicate it"
    );

    engine.execute_cypher(MERGE_EDGE).expect("merge edge 3");
    assert_eq!(rel_count(&mut engine), 1, "MERGE edge stays idempotent");
}

#[test]
fn edge_merge_is_idempotent_after_reopen() {
    let ctx = TestContext::new();
    {
        let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine");
        seed_two_nodes(&mut engine);
        engine.execute_cypher(MERGE_EDGE).expect("merge edge");
        assert_eq!(rel_count(&mut engine), 1);
        engine.flush().expect("flush");
    }
    // Reopen the same data dir: the relationship/edge index is rebuilt from
    // storage, so the MERGE existence fast path can see the persisted edge.
    {
        let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("reopen");
        assert_eq!(rel_count(&mut engine), 1, "edge persisted across reopen");
        engine
            .execute_cypher(MERGE_EDGE)
            .expect("merge edge after reopen");
        assert_eq!(
            rel_count(&mut engine),
            1,
            "MERGE after reopen must match the persisted edge (rebuilt index), \
             not create a duplicate"
        );
    }
}
