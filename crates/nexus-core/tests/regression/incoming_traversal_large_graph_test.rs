//! Incoming (and undirected) relationship traversal must find a node's edges
//! regardless of how many relationships the graph holds.
//!
//! `find_relationships` used to serve an INCOMING expansion by verifying the
//! node's `first_rel_ptr` (which heads only its OUTGOING chain), finding it did
//! not point at an incoming edge, and falling back to a scan that only ever
//! probed relationship ids `0..=10_000`. On any graph with more relationships
//! than that, an incoming edge sitting at a higher id was silently invisible —
//! `MATCH (n)<-[:T]-(m)` returned nothing even though the edge existed. On a
//! loaded LDBC graph this made every `(:Person)<-[:HAS_CREATOR]-(:Message)`
//! return zero. Now it reads the store's authoritative both-direction adjacency
//! index instead, so it is O(degree) and complete at any scale.

use nexus_core::Engine;
use nexus_core::testing::TestContext;

/// Enough filler edges that a target edge placed afterwards lands beyond the
/// old scan window (relationship ids `0..=10_000`).
const FILLER_EDGES: usize = 10_050;

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
fn incoming_edge_beyond_the_old_scan_window_is_found() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine");

    // A hub with many OUTGOING edges — enough that its own `first_rel_ptr`
    // chain is long and every relationship id up to the old 10_000 cap is
    // consumed by filler.
    engine.execute_cypher("CREATE (:Hub {k: 1})").expect("hub");
    engine
        .execute_cypher(&format!(
            "UNWIND range(1, {FILLER_EDGES}) AS i \
             CREATE (:Leaf {{i: i}})"
        ))
        .expect("leaves");
    engine
        .execute_cypher("MATCH (h:Hub {k: 1}), (l:Leaf) CREATE (h)-[:OUT]->(l)")
        .expect("outgoing filler edges");

    // One INCOMING edge, created last, so its relationship id is past the old
    // scan window.
    engine
        .execute_cypher("CREATE (:Other {k: 2})")
        .expect("other");
    engine
        .execute_cypher("MATCH (o:Other {k: 2}), (h:Hub {k: 1}) CREATE (o)-[:IN]->(h)")
        .expect("incoming edge");

    assert_eq!(
        count(&mut engine, "MATCH ()-[r:IN]->() RETURN count(r)"),
        1,
        "sanity: the incoming edge exists"
    );
    assert_eq!(
        count(
            &mut engine,
            "MATCH (h:Hub {k: 1})<-[:IN]-(o) RETURN count(o)"
        ),
        1,
        "incoming traversal must find the edge even past the old 10k scan window"
    );
    // The undirected form must see it too.
    assert_eq!(
        count(
            &mut engine,
            "MATCH (h:Hub {k: 1})-[r:IN]-(o) RETURN count(r)"
        ),
        1,
        "undirected traversal must find the incoming edge"
    );
    // And the outgoing side is unchanged.
    assert_eq!(
        count(
            &mut engine,
            "MATCH (h:Hub {k: 1})-[:OUT]->(l) RETURN count(l)"
        ),
        FILLER_EDGES as i64,
        "every outgoing edge is still found"
    );
}

#[test]
fn incoming_traversal_survives_a_reopen() {
    // The adjacency index is rebuilt from the records on open, so a reopened
    // store must serve incoming traversal identically.
    let ctx = TestContext::new();
    {
        let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine");
        engine
            .execute_cypher("CREATE (:A {k: 1}), (:B {k: 2})")
            .expect("seed");
        engine
            .execute_cypher("MATCH (a:A {k: 1}), (b:B {k: 2}) CREATE (a)-[:R]->(b)")
            .expect("edge");
        engine.flush().expect("flush");
    }
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("reopen");
    assert_eq!(
        count(&mut engine, "MATCH (b:B {k: 2})<-[:R]-(a) RETURN count(a)"),
        1,
        "incoming traversal must work after a reopen (index rebuilt from records)"
    );
}
