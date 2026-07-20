//! Correctness coverage for index-backed node MERGE
//! (phase6_fix-planner-merge-unindexed-on2). The fix makes
//! `MERGE (n:Label {key: v})` resolve the existence check via the
//! property index instead of an all-label scan; these tests pin the
//! MERGE semantics (create-if-absent, match-if-present, no duplicates)
//! both with and without a covering index.

use nexus_core::Engine;
use nexus_core::testing::TestContext;

fn count_label(engine: &mut Engine, label: &str) -> u64 {
    let q = format!("MATCH (n:{label}) RETURN count(n) AS c");
    let r = engine.execute_cypher(&q).expect("count query");
    r.rows[0].values[0].as_u64().unwrap_or(u64::MAX)
}

#[test]
fn merge_with_index_matches_existing_and_creates_absent() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine");

    // Seed N nodes and a covering index so MERGE takes the index-seek path.
    for i in 0..50 {
        engine
            .execute_cypher(&format!("CREATE (:M {{k: {i}}})"))
            .expect("seed create");
    }
    engine
        .execute_cypher("CREATE INDEX FOR (n:M) ON (n.k)")
        .expect("create index");
    assert_eq!(count_label(&mut engine, "M"), 50);

    // MERGE an EXISTING key -> matches, no new node.
    engine
        .execute_cypher("MERGE (n:M {k: 25})")
        .expect("merge existing");
    assert_eq!(
        count_label(&mut engine, "M"),
        50,
        "MERGE on an existing key must not create a duplicate"
    );

    // MERGE an ABSENT key -> creates exactly one.
    engine
        .execute_cypher("MERGE (n:M {k: 999})")
        .expect("merge absent");
    assert_eq!(
        count_label(&mut engine, "M"),
        51,
        "MERGE on an absent key must create exactly one node"
    );

    // Idempotent: repeating the same MERGE creates nothing more.
    engine
        .execute_cypher("MERGE (n:M {k: 999})")
        .expect("merge again");
    assert_eq!(
        count_label(&mut engine, "M"),
        51,
        "MERGE must be idempotent"
    );
}

#[test]
fn merge_without_index_still_correct() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine");

    engine.execute_cypher("CREATE (:U {k: 1})").expect("seed");
    // No index on (U, k): MERGE falls back to the label scan but must
    // still be correct.
    engine
        .execute_cypher("MERGE (n:U {k: 1})")
        .expect("merge existing");
    assert_eq!(
        count_label(&mut engine, "U"),
        1,
        "no duplicate on fallback path"
    );
    engine
        .execute_cypher("MERGE (n:U {k: 2})")
        .expect("merge absent");
    assert_eq!(
        count_label(&mut engine, "U"),
        2,
        "creates absent on fallback path"
    );
}
