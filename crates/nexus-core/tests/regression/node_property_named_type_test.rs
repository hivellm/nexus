//! A node carrying a property literally named `type` must not be mistaken for
//! a relationship.
//!
//! The executor used to answer "is this row value a relationship?" with
//! `obj.contains_key("type")`, because `read_relationship_as_value_with_store`
//! stores the relationship type under that key. `type` is also an ordinary
//! property name — LDBC SNB's `Organisation.type` (company/university) and
//! `Place.type` (city/country/continent) are exactly that — so every such NODE
//! read as a relationship.
//!
//! The damage landed in `update_result_set_from_rows`' row-deduplication key:
//! the misidentified node was picked by `HashMap` iteration order, the real
//! node variables were then excluded from the key, and unrelated rows
//! collapsed into one. Silently, and with a DIFFERENT result on every run of
//! the same read-only database — on LDBC SF0.1,
//! `MATCH (o:Organisation)-[r:IS_LOCATED_IN]->(p:Place) RETURN count(r)`
//! returned 5305 / 5267 / 5233 where the answer is 7955.
//!
//! Fixed by marking relationships structurally (`_nexus_rel_type`, written
//! only by the relationship constructor).

use nexus_core::Engine;
use nexus_core::testing::TestContext;

/// Enough rows that a key collapsing onto the shared target is overwhelmingly
/// likely to drop several of them, but small enough to stay fast.
const ORGS: usize = 20;

fn seeded() -> (Engine, TestContext) {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine");
    // Both endpoint labels carry a property named `type`, like the LDBC data
    // that surfaced this.
    engine
        .execute_cypher("CREATE (:City {id: 100, type: 'city'})")
        .expect("seed city");
    for i in 0..ORGS {
        engine
            .execute_cypher(&format!("CREATE (:Org {{id: {i}, type: 'company'}})"))
            .expect("seed org");
        // One edge per statement with BOTH endpoints pinned by id. The
        // multi-row form (`MATCH (o:Org), (c:City) CREATE …`) writes one edge
        // per row-pair TWICE — a separate write-path defect that would
        // silently seed the wrong graph here.
        engine
            .execute_cypher(&format!(
                "MATCH (o:Org {{id: {i}}}), (c:City {{id: 100}}) CREATE (o)-[:IS_LOCATED_IN]->(c)"
            ))
            .expect("connect org to city");
    }
    // Guard the fixture itself: if seeding ever stops producing exactly one
    // edge per org, the assertions below would be testing a different graph.
    let seeded = engine
        .execute_cypher("MATCH ()-[r:IS_LOCATED_IN]->() RETURN count(r)")
        .expect("count seeded edges")
        .rows[0]
        .values[0]
        .as_i64();
    assert_eq!(
        seeded,
        Some(ORGS as i64),
        "fixture must hold exactly one edge per org"
    );
    (engine, ctx)
}

fn count(engine: &mut Engine, cypher: &str) -> i64 {
    engine
        .execute_cypher(cypher)
        .unwrap_or_else(|e| panic!("`{cypher}` failed: {e}"))
        .rows[0]
        .values[0]
        .as_i64()
        .expect("count is an integer")
}

#[test]
fn rows_of_nodes_with_a_type_property_are_not_deduplicated_away() {
    let (mut engine, _ctx) = seeded();

    // Every org has its own edge to the shared city. Before the fix the
    // dedup key degenerated to `rel_<whichever object HashMap yielded first>`,
    // so the rows that keyed on the shared city collapsed into one.
    assert_eq!(
        count(
            &mut engine,
            "MATCH (o:Org)-[r:IS_LOCATED_IN]->(c:City) RETURN count(r)"
        ),
        ORGS as i64,
        "every org's edge must survive row deduplication"
    );
    assert_eq!(
        count(
            &mut engine,
            "MATCH (o:Org)-[r:IS_LOCATED_IN]->(c:City) RETURN count(DISTINCT o)"
        ),
        ORGS as i64,
        "every org must still be reachable through the expand"
    );
}

#[test]
fn the_result_is_the_same_on_every_run() {
    // The failure mode was non-deterministic: `HashMap` iteration decided
    // which object was mistaken for the relationship, so consecutive runs of
    // an unchanged database returned different counts. Ten runs of a
    // read-only database must agree.
    let (mut engine, _ctx) = seeded();
    let counts: Vec<i64> = (0..10)
        .map(|_| {
            count(
                &mut engine,
                "MATCH (o:Org)-[r:IS_LOCATED_IN]->(c:City) RETURN count(r)",
            )
        })
        .collect();
    assert!(
        counts.iter().all(|c| *c == ORGS as i64),
        "an unchanged database must answer identically every time; got {counts:?}"
    );
}

#[test]
fn a_node_with_a_type_property_is_not_a_relationship() {
    // The predicate itself, through the Cypher surface: `type(x)` applies to
    // relationships. The node's own `type` property must still be readable as
    // a plain property — the fix must not hide user data.
    let (mut engine, _ctx) = seeded();
    let result = engine
        .execute_cypher("MATCH (o:Org {id: 0}) RETURN o.type AS t")
        .expect("read the property");
    assert_eq!(
        result.rows[0].values[0].as_str(),
        Some("company"),
        "a property named `type` must still round-trip as a property"
    );

    // And the relationship's type is still reported as before.
    let result = engine
        .execute_cypher("MATCH (:Org)-[r:IS_LOCATED_IN]->(:City) RETURN DISTINCT type(r) AS t")
        .expect("read the relationship type");
    assert_eq!(result.rows[0].values[0].as_str(), Some("IS_LOCATED_IN"));
}

#[test]
fn relationships_with_a_type_property_keep_their_own_type() {
    // The mirror case: a relationship whose PROPERTY map contains `type`.
    // The stored property wins the `type` key in the flat Neo4j-shaped
    // object, but the structural marker still identifies the value as a
    // relationship, so rows must not collapse.
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine");
    engine
        .execute_cypher("CREATE (:A {id: 1}), (:A {id: 2}), (:B {id: 9})")
        .expect("seed");
    for id in [1, 2] {
        engine
            .execute_cypher(&format!(
                "MATCH (a:A {{id: {id}}}), (b:B {{id: 9}}) CREATE (a)-[:R {{type: 'custom'}}]->(b)"
            ))
            .expect("connect");
    }

    assert_eq!(
        count(&mut engine, "MATCH (:A)-[r:R]->(:B) RETURN count(r)"),
        2,
        "both relationships must survive"
    );
}
