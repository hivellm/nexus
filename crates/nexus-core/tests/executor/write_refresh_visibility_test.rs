//! Regression coverage for `Engine::refresh_executor_if_mutated` — the
//! executor-rebuild skip applied when a write query's computed `mutated`
//! signal is `false` (defense-in-depth backstopped by
//! `self.storage.nodes_created() != 0`).
//!
//! The risk this guard introduces is staleness: if a `mutated` signal ever
//! under-reports (reports `false` for a write that actually changed
//! storage), the skipped rebuild would leave the executor's read-path view
//! stuck on old data, and a follow-up query in the same `Engine` would
//! silently see incorrect results. Every test here writes through
//! `execute_cypher`, then immediately reads back through a *separate*
//! `execute_cypher` call on the same engine — exactly the sequence the
//! guard must keep correct — and asserts on the observed data, never on
//! internal engine state.
//!
//! Follows the isolated-per-test-engine pattern from `side_effects.rs`.

use nexus_core::Engine;
use nexus_core::testing::TestContext;

// ---------------------------------------------------------------------
// 1. No-op MERGE correctness
// ---------------------------------------------------------------------

#[test]
fn noop_merge_onto_existing_node_leaves_data_correct_for_next_match() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("MERGE (n:NoopA {k: 1, v: 'orig'})")
        .expect("first MERGE creates");

    // Second MERGE matches the existing node; nothing changes.
    engine
        .execute_cypher("MERGE (n:NoopA {k: 1})")
        .expect("second MERGE is a no-op match");

    let read = engine
        .execute_cypher("MATCH (n:NoopA {k: 1}) RETURN n.v AS v")
        .expect("read back after no-op MERGE");
    assert_eq!(read.rows.len(), 1, "exactly one node must exist");
    assert_eq!(
        read.rows[0].values[0].as_str(),
        Some("orig"),
        "no-op MERGE must not disturb existing properties"
    );
}

#[test]
fn repeated_noop_merges_create_no_duplicates() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("MERGE (n:NoopB {k: 1})")
        .expect("seed MERGE creates");

    for i in 0..10 {
        engine
            .execute_cypher("MERGE (n:NoopB {k: 1})")
            .unwrap_or_else(|e| panic!("no-op MERGE #{i} must succeed: {e}"));
    }

    let count = engine
        .execute_cypher("MATCH (n:NoopB) RETURN count(n) AS c")
        .expect("count must succeed");
    assert_eq!(
        count.rows[0].values[0].as_i64(),
        Some(1),
        "ten repeated no-op MERGEs must not create duplicates"
    );
}

// ---------------------------------------------------------------------
// 2. Mutating MERGE (create branch)
// ---------------------------------------------------------------------

#[test]
fn merge_create_branch_node_is_immediately_visible_by_label_and_property() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("MERGE (n:FreshMerge {k: 42})")
        .expect("MERGE must take the create branch");

    let by_label = engine
        .execute_cypher("MATCH (n:FreshMerge) RETURN count(n) AS c")
        .expect("label scan after MERGE-create");
    assert_eq!(by_label.rows[0].values[0].as_i64(), Some(1));

    let by_property = engine
        .execute_cypher("MATCH (n:FreshMerge {k: 42}) RETURN n.k AS k")
        .expect("property lookup after MERGE-create");
    assert_eq!(by_property.rows.len(), 1);
    assert_eq!(by_property.rows[0].values[0].as_i64(), Some(42));
}

// ---------------------------------------------------------------------
// 3. CREATE visibility
// ---------------------------------------------------------------------

#[test]
fn standalone_create_is_immediately_visible_by_label_and_property() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (n:FreshCreate {k: 7})")
        .expect("standalone CREATE");

    let by_label = engine
        .execute_cypher("MATCH (n:FreshCreate) RETURN count(n) AS c")
        .expect("label scan after CREATE");
    assert_eq!(by_label.rows[0].values[0].as_i64(), Some(1));

    let by_property = engine
        .execute_cypher("MATCH (n:FreshCreate {k: 7}) RETURN n.k AS k")
        .expect("property lookup after CREATE");
    assert_eq!(by_property.rows.len(), 1);
    assert_eq!(by_property.rows[0].values[0].as_i64(), Some(7));
}

#[test]
fn create_on_conflict_match_resolves_to_existing_without_duplicate() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (n:ConflictNode {_id: 'str:cn-1', v: 'first'})")
        .expect("first create");

    // ON CONFLICT MATCH resolves to the existing node: no new record is
    // written, so the write must report unmutated -- but the existing
    // node's data must remain correct and visible.
    engine
        .execute_cypher("CREATE (n:ConflictNode {_id: 'str:cn-1', v: 'second'}) ON CONFLICT MATCH")
        .expect("ON CONFLICT MATCH must succeed, not error");

    let count = engine
        .execute_cypher("MATCH (n:ConflictNode) RETURN count(n) AS c")
        .expect("count after ON CONFLICT MATCH");
    assert_eq!(
        count.rows[0].values[0].as_i64(),
        Some(1),
        "ON CONFLICT MATCH must not create a duplicate"
    );

    let read = engine
        .execute_cypher("MATCH (n:ConflictNode {_id: 'str:cn-1'}) RETURN n.v AS v")
        .expect("read back the resolved node");
    assert_eq!(
        read.rows[0].values[0].as_str(),
        Some("first"),
        "ON CONFLICT MATCH resolves to the existing node and must not overwrite it"
    );
}

// ---------------------------------------------------------------------
// 4. SET-only write (no node/rel creation) -- pins the `other_mutation`
//    flag; a count-delta-only signal would wrongly skip the refresh.
// ---------------------------------------------------------------------

#[test]
fn set_only_write_is_immediately_visible_to_where_on_new_value() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (n:SetOnly {status: 'pending'})")
        .expect("seed CREATE");

    engine
        .execute_cypher("MATCH (n:SetOnly) SET n.status = 'done'")
        .expect("SET-only write");

    let read = engine
        .execute_cypher("MATCH (n:SetOnly) WHERE n.status = 'done' RETURN n.status AS status")
        .expect("read back filtering on the NEW value");
    assert_eq!(
        read.rows.len(),
        1,
        "the new value must be immediately visible to a WHERE filter"
    );
    assert_eq!(read.rows[0].values[0].as_str(), Some("done"));

    let stale = engine
        .execute_cypher("MATCH (n:SetOnly) WHERE n.status = 'pending' RETURN n")
        .expect("read back filtering on the OLD value");
    assert_eq!(
        stale.rows.len(),
        0,
        "the old value must no longer match after the SET-only write"
    );
}

// ---------------------------------------------------------------------
// 5. REMOVE-only write -- same staleness risk as SET.
// ---------------------------------------------------------------------

#[test]
fn remove_only_write_is_immediately_visible() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (n:RemoveOnly {tag: 'x'})")
        .expect("seed CREATE");

    engine
        .execute_cypher("MATCH (n:RemoveOnly) REMOVE n.tag")
        .expect("REMOVE-only write");

    let read = engine
        .execute_cypher("MATCH (n:RemoveOnly) RETURN n.tag AS tag")
        .expect("read back after REMOVE");
    assert_eq!(read.rows.len(), 1);
    assert!(
        read.rows[0].values[0].is_null(),
        "the removed property must read back as null immediately, got {:?}",
        read.rows[0].values[0]
    );
}

// ---------------------------------------------------------------------
// 6. DELETE
// ---------------------------------------------------------------------

#[test]
fn delete_removes_node_from_subsequent_match() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (n:Doomed {k: 1})")
        .expect("seed CREATE");

    engine
        .execute_cypher("MATCH (n:Doomed) DELETE n")
        .expect("DELETE");

    let read = engine
        .execute_cypher("MATCH (n:Doomed) RETURN count(n) AS c")
        .expect("count after DELETE");
    assert_eq!(
        read.rows[0].values[0].as_i64(),
        Some(0),
        "the deleted node must be invisible to a follow-up MATCH"
    );
}

#[test]
fn delete_matching_nothing_leaves_data_correct() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (n:Survivor {k: 1})")
        .expect("seed CREATE");

    // Matches nothing -- deleted_count == 0, refresh should be safely
    // skippable without losing visibility of existing data.
    engine
        .execute_cypher("MATCH (n:Ghost) DELETE n")
        .expect("DELETE matching nothing must not error");

    let read = engine
        .execute_cypher("MATCH (n:Survivor) RETURN n.k AS k")
        .expect("read back the untouched node");
    assert_eq!(read.rows.len(), 1);
    assert_eq!(read.rows[0].values[0].as_i64(), Some(1));
}

// ---------------------------------------------------------------------
// 7. MATCH...CREATE
// ---------------------------------------------------------------------

#[test]
fn match_create_when_match_matches_nothing_creates_nothing() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    // No :GhostAnchor node exists, so the CREATE arm never runs.
    engine
        .execute_cypher("MATCH (a:GhostAnchor) CREATE (a)-[:R]->(x:Spurious)")
        .expect("MATCH...CREATE with an empty MATCH must not error");

    let read = engine
        .execute_cypher("MATCH (n:Spurious) RETURN count(n) AS c")
        .expect("count after empty-MATCH CREATE");
    assert_eq!(
        read.rows[0].values[0].as_i64(),
        Some(0),
        "nothing should have been created when the MATCH matched nothing"
    );
}

#[test]
fn match_create_when_match_matches_creates_node_and_is_visible() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (a:RealAnchor {k: 1})")
        .expect("seed anchor");

    engine
        .execute_cypher("MATCH (a:RealAnchor {k: 1}) CREATE (x:Spawned {k: 2})")
        .expect("MATCH...CREATE with a matching MATCH, creating an unrelated node");

    let node = engine
        .execute_cypher("MATCH (n:Spawned) RETURN n.k AS k")
        .expect("read back the created node");
    assert_eq!(node.rows.len(), 1);
    assert_eq!(node.rows[0].values[0].as_i64(), Some(2));
}

#[test]
fn match_create_when_match_matches_creates_relationship_and_is_visible() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (:RelAnchorA {k: 1})")
        .expect("seed a");
    engine
        .execute_cypher("CREATE (:RelAnchorB {k: 2})")
        .expect("seed b");

    engine
        .execute_cypher("MATCH (a:RelAnchorA {k: 1}), (b:RelAnchorB {k: 2}) CREATE (a)-[:R]->(b)")
        .expect(
            "MATCH...CREATE with a matching MATCH, creating a relationship between existing nodes",
        );

    let edge = engine
        .execute_cypher("MATCH (a:RelAnchorA)-[r:R]->(b:RelAnchorB) RETURN count(r) AS c")
        .expect("read back the created relationship");
    assert_eq!(edge.rows[0].values[0].as_i64(), Some(1));
}

/// Same scenarios as above, but reached through `PROFILE`, which routes
/// `MATCH...CREATE` through the engine's *internal* AST dispatch path
/// (`DispatchSource::Internal`) instead of the top-level query-text path.
/// That internal branch computes its own `mutated` signal from a
/// node/relationship count delta and explicitly calls
/// `refresh_executor_if_mutated` -- the exact code this suite defends.
#[test]
fn profiled_match_create_when_match_matches_nothing_creates_nothing() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("PROFILE MATCH (a:GhostAnchor2) CREATE (a)-[:R]->(x:Spurious2)")
        .expect("PROFILE MATCH...CREATE with an empty MATCH must not error");

    let read = engine
        .execute_cypher("MATCH (n:Spurious2) RETURN count(n) AS c")
        .expect("count after empty-MATCH PROFILE CREATE");
    assert_eq!(
        read.rows[0].values[0].as_i64(),
        Some(0),
        "nothing should have been created when the profiled MATCH matched nothing"
    );
}

#[test]
fn profiled_match_create_when_match_matches_creates_node_and_is_visible() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (a:RealAnchor2 {k: 1})")
        .expect("seed anchor");

    engine
        .execute_cypher("PROFILE MATCH (a:RealAnchor2 {k: 1}) CREATE (x:Spawned2 {k: 2})")
        .expect("PROFILE MATCH...CREATE with a matching MATCH, creating an unrelated node");

    let node = engine
        .execute_cypher("MATCH (n:Spawned2) RETURN n.k AS k")
        .expect("read back the profiled-created node");
    assert_eq!(node.rows.len(), 1);
    assert_eq!(node.rows[0].values[0].as_i64(), Some(2));
}

#[test]
fn profiled_match_create_when_match_matches_creates_relationship_and_is_visible() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (:RelAnchorA2 {k: 1})")
        .expect("seed a");
    engine
        .execute_cypher("CREATE (:RelAnchorB2 {k: 2})")
        .expect("seed b");

    engine
        .execute_cypher(
            "PROFILE MATCH (a:RelAnchorA2 {k: 1}), (b:RelAnchorB2 {k: 2}) CREATE (a)-[:R]->(b)",
        )
        .expect("PROFILE MATCH...CREATE, creating a relationship between existing nodes");

    let edge = engine
        .execute_cypher("MATCH (a:RelAnchorA2)-[r:R]->(b:RelAnchorB2) RETURN count(r) AS c")
        .expect("read back the profiled-created relationship");
    assert_eq!(edge.rows[0].values[0].as_i64(), Some(1));
}

// ---------------------------------------------------------------------
// 8. UNWIND ... CREATE batch
// ---------------------------------------------------------------------

#[test]
fn unwind_create_batch_all_nodes_visible_afterward() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("UNWIND range(1, 20) AS i CREATE (:Batched {seq: i})")
        .expect("UNWIND-driven CREATE batch");

    let count = engine
        .execute_cypher("MATCH (n:Batched) RETURN count(n) AS c")
        .expect("count after UNWIND CREATE batch");
    assert_eq!(count.rows[0].values[0].as_i64(), Some(20));

    let last = engine
        .execute_cypher("MATCH (n:Batched {seq: 20}) RETURN n.seq AS seq")
        .expect("read back the last item of the batch");
    assert_eq!(last.rows.len(), 1);
    assert_eq!(last.rows[0].values[0].as_i64(), Some(20));
}

// ---------------------------------------------------------------------
// 9. Relationship creation visibility -- pins the rel-count half of the
//    mutated signal (node count unchanged, only relationship_count
//    moves).
// ---------------------------------------------------------------------

#[test]
fn create_relationship_between_existing_nodes_is_immediately_visible() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (:RelA {name: 'a'})")
        .expect("seed a");
    engine
        .execute_cypher("CREATE (:RelB {name: 'b'})")
        .expect("seed b");

    engine
        .execute_cypher(
            "MATCH (a:RelA {name: 'a'}), (b:RelB {name: 'b'}) CREATE (a)-[:LINKED]->(b)",
        )
        .expect("CREATE relationship between existing nodes");

    let traversal = engine
        .execute_cypher("MATCH (a:RelA)-[r:LINKED]->(b:RelB) RETURN count(r) AS c")
        .expect("traversal after relationship CREATE");
    assert_eq!(
        traversal.rows[0].values[0].as_i64(),
        Some(1),
        "the newly created relationship must be immediately visible to traversal"
    );
}

#[test]
fn merge_relationship_between_existing_nodes_is_immediately_visible() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (:RelC {name: 'c'})")
        .expect("seed c");
    engine
        .execute_cypher("CREATE (:RelD {name: 'd'})")
        .expect("seed d");

    // Only a relationship is created here -- node_count is unchanged,
    // only relationship_count moves. This is the case a node-count-only
    // signal would miss. Uses the variable-less relationship form
    // (`-[:MERGED]->`, no `r:`) — the ordinary way to write a relationship
    // MERGE.
    engine
        .execute_cypher("MATCH (a:RelC {name: 'c'}), (b:RelD {name: 'd'}) MERGE (a)-[:MERGED]->(b)")
        .expect("MERGE relationship between existing nodes");

    let traversal = engine
        .execute_cypher("MATCH (a:RelC)-[r:MERGED]->(b:RelD) RETURN count(r) AS c")
        .expect("traversal after relationship MERGE");
    assert_eq!(
        traversal.rows[0].values[0].as_i64(),
        Some(1),
        "the newly merged relationship must be immediately visible to traversal"
    );
}

// ---------------------------------------------------------------------
// 10. MATCH...CREATE with an inline-created relationship endpoint --
//     regression coverage for phase0_fix-match-create-inline-node-rel-dropped
//     (`.rulebook/tasks/phase0_fix-match-create-inline-node-rel-dropped`).
//     The reported bug: `MATCH (a) CREATE (a)-[:T]->(x:Y {...})` persists
//     the inline-created node `x` but silently drops the relationship --
//     no error, just missing data. These tests exercise the natural
//     "attach a new node to an existing one" shape that the split tests
//     above (`match_create_when_match_matches_creates_node_and_is_visible`
//     / `match_create_when_match_matches_creates_relationship_and_is_visible`)
//     were deliberately narrowed to avoid.
// ---------------------------------------------------------------------

#[test]
fn match_create_bound_source_inline_target_node_and_relationship_both_visible() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (a:InlineX {k: 1})")
        .expect("seed anchor");

    engine
        .execute_cypher("MATCH (a:InlineX {k: 1}) CREATE (a)-[:CR]->(x:InlineY {k: 2})")
        .expect("MATCH...CREATE with an inline-created relationship target");

    let node = engine
        .execute_cypher("MATCH (n:InlineY) RETURN n.k AS k")
        .expect("read back the inline-created target node");
    assert_eq!(
        node.rows.len(),
        1,
        "the inline target node must be persisted"
    );
    assert_eq!(node.rows[0].values[0].as_i64(), Some(2));

    let edge = engine
        .execute_cypher("MATCH (a:InlineX)-[r:CR]->(x:InlineY) RETURN count(r) AS c")
        .expect("read back the relationship to the inline-created target");
    assert_eq!(
        edge.rows[0].values[0].as_i64(),
        Some(1),
        "the relationship to the inline-created target node must not be silently dropped"
    );
}

#[test]
fn match_create_inline_target_mirrored_direction_relationship_visible() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (a:InlineMirrorA {k: 1})")
        .expect("seed anchor");

    // Mirrored surface syntax: the inline-created node is written FIRST,
    // the bound node LAST, joined with a reversed (`<-`) arrow --
    // `MATCH (a) CREATE (x:Y)<-[:T]-(a)`.
    engine
        .execute_cypher("MATCH (a:InlineMirrorA {k: 1}) CREATE (x:InlineMirrorB {k: 2})<-[:CR]-(a)")
        .expect("MATCH...CREATE with a mirrored-direction inline-created target");

    let node = engine
        .execute_cypher("MATCH (n:InlineMirrorB) RETURN n.k AS k")
        .expect("read back the inline-created target node");
    assert_eq!(
        node.rows.len(),
        1,
        "the inline target node must be persisted"
    );
    assert_eq!(node.rows[0].values[0].as_i64(), Some(2));

    // Direction handling in CREATE is out of scope for this fix (tracked
    // separately); assert only that the relationship itself was not
    // silently dropped, via a direction-agnostic match.
    let edge = engine
        .execute_cypher("MATCH (a:InlineMirrorA)-[r:CR]-(x:InlineMirrorB) RETURN count(r) AS c")
        .expect("read back the relationship to the mirrored-direction inline target");
    assert_eq!(
        edge.rows[0].values[0].as_i64(),
        Some(1),
        "the mirrored-direction relationship must not be silently dropped"
    );
}

#[test]
fn match_create_bound_source_with_two_chained_inline_targets_creates_all_relationships() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (a:ChainAnchor {k: 1})")
        .expect("seed anchor");

    engine
        .execute_cypher(
            "MATCH (a:ChainAnchor {k: 1}) CREATE (a)-[:R1]->(b:ChainMid {k: 2})-[:R2]->(c:ChainEnd {k: 3})",
        )
        .expect("MATCH...CREATE with a multi-hop pattern mixing a bound source and two inline targets");

    let nodes = engine
        .execute_cypher("MATCH (n) WHERE n:ChainMid OR n:ChainEnd RETURN count(n) AS c")
        .expect("count both inline-created nodes");
    assert_eq!(
        nodes.rows[0].values[0].as_i64(),
        Some(2),
        "both inline nodes in the chain must be persisted"
    );

    // The direct 2-hop pattern count is now trustworthy
    // (phase0_fix-multi-hop-count-star-incorrect fixed the phantom-count gap
    // for multi-hop patterns; see multi_hop_count_test.rs): the whole chain
    // `a-[:R1]->b-[:R2]->c` must count as exactly one match.
    let two_hop = engine
        .execute_cypher(
            "MATCH (a:ChainAnchor)-[:R1]->(b:ChainMid)-[:R2]->(c:ChainEnd) RETURN count(*) AS c",
        )
        .expect("read back the full 2-hop chain");
    assert_eq!(
        two_hop.rows[0].values[0].as_i64(),
        Some(1),
        "the full bound-source -> inline -> inline chain must count as one 2-hop match"
    );

    // Per-hop counts kept as extra coverage (each edge persisted individually).
    let r1_only = engine
        .execute_cypher("MATCH (a:ChainAnchor)-[r1:R1]->(b:ChainMid) RETURN count(r1) AS c")
        .expect("read back R1 alone");
    assert_eq!(
        r1_only.rows[0].values[0].as_i64(),
        Some(1),
        "the first hop (bound source -> inline target) must be persisted"
    );

    let r2_only = engine
        .execute_cypher("MATCH (b:ChainMid)-[r2:R2]->(c:ChainEnd) RETURN count(r2) AS c")
        .expect("read back R2 alone");
    assert_eq!(
        r2_only.rows[0].values[0].as_i64(),
        Some(1),
        "the second hop (inline target -> inline target) must be persisted, not just the first hop"
    );
}

/// PROFILE/internal-dispatch variant of the reported bug shape -- pins
/// the fix across both the top-level query-text dispatch and the
/// internal AST-override dispatch used by PROFILE / CALL-subquery
/// recursion (`DispatchSource::Internal` in `query_pipeline.rs`).
#[test]
fn profiled_match_create_bound_source_inline_target_node_and_relationship_both_visible() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (a:PInlineX {k: 1})")
        .expect("seed anchor");

    engine
        .execute_cypher("PROFILE MATCH (a:PInlineX {k: 1}) CREATE (a)-[:CR]->(x:PInlineY {k: 2})")
        .expect("PROFILE MATCH...CREATE with an inline-created relationship target");

    let node = engine
        .execute_cypher("MATCH (n:PInlineY) RETURN n.k AS k")
        .expect("read back the profiled inline-created target node");
    assert_eq!(
        node.rows.len(),
        1,
        "the inline target node must be persisted"
    );
    assert_eq!(node.rows[0].values[0].as_i64(), Some(2));

    let edge = engine
        .execute_cypher("MATCH (a:PInlineX)-[r:CR]->(x:PInlineY) RETURN count(r) AS c")
        .expect("read back the profiled relationship to the inline-created target");
    assert_eq!(
        edge.rows[0].values[0].as_i64(),
        Some(1),
        "the profiled relationship to the inline-created target node must not be silently dropped"
    );
}
