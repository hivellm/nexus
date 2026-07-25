//! Coverage for `ResultSet::side_effects` — the mutation counters the
//! openCypher TCK asserts on via `And the side effects should be:` /
//! `And no side effects` (see `crates/nexus-core/tests/tck/opencypher/`).
//!
//! Every test uses an isolated per-test catalog via
//! `Engine::with_isolated_catalog` + `testing::TestContext`, matching the
//! pattern in `cypher_external_id_write_paths.rs`.

use nexus_core::Engine;
use nexus_core::testing::TestContext;

#[test]
fn create_node_reports_one_node_created_and_nothing_else() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    let result = engine
        .execute_cypher("CREATE (n:X)")
        .expect("CREATE must succeed");

    let effects = result.side_effects;
    assert_eq!(effects.nodes_created, 1, "exactly one node was created");
    assert_eq!(
        effects.labels_added, 1,
        "the :X label on the created node counts toward +labels"
    );
    assert_eq!(effects.nodes_deleted, 0);
    assert_eq!(effects.relationships_created, 0);
    assert_eq!(effects.relationships_deleted, 0);
    assert_eq!(effects.properties_set, 0);
    assert_eq!(effects.properties_removed, 0);
    assert_eq!(effects.labels_removed, 0);
}

#[test]
fn read_only_match_reports_no_side_effects() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (n:X)")
        .expect("seed CREATE must succeed");

    let result = engine
        .execute_cypher("MATCH (n) RETURN n")
        .expect("MATCH must succeed");

    assert_eq!(
        result.side_effects,
        nexus_core::executor::types::SideEffects::default(),
        "a read-only query must report an all-zero SideEffects, got {:?}",
        result.side_effects
    );
}

#[test]
fn merge_onto_existing_node_reports_no_creation() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    let first = engine
        .execute_cypher("MERGE (n:W {k: 1})")
        .expect("first MERGE must succeed");
    assert_eq!(first.side_effects.nodes_created, 1, "first MERGE creates");

    // Second MERGE matches the existing node -- nothing is created.
    let second = engine
        .execute_cypher("MERGE (n:W {k: 1})")
        .expect("second MERGE must succeed");
    assert_eq!(
        second.side_effects.nodes_created, 0,
        "MERGE onto an existing node must not report a creation"
    );

    let count = engine
        .execute_cypher("MATCH (n:W) RETURN count(n)")
        .expect("count must succeed");
    assert_eq!(
        count.rows[0].values[0].as_i64(),
        Some(1),
        "only one node should exist"
    );
}

#[test]
fn external_id_conflict_policy_match_reports_no_creation() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    let first = engine
        .execute_cypher("CREATE (n:E {_id: 'str:dup'})")
        .expect("first create must succeed");
    assert_eq!(first.side_effects.nodes_created, 1);

    // ON CONFLICT MATCH resolves to the existing node: no record is written,
    // so this must not be counted as a creation.
    let second = engine
        .execute_cypher("CREATE (n:E {_id: 'str:dup'}) ON CONFLICT MATCH")
        .expect("ON CONFLICT MATCH must succeed, not error");
    assert_eq!(
        second.side_effects.nodes_created, 0,
        "ON CONFLICT MATCH resolved to an existing node; nothing was created"
    );
}

#[test]
fn create_relationship_reports_one_relationship_and_two_nodes() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    let result = engine
        .execute_cypher("CREATE (a:A)-[:R]->(b:B)")
        .expect("CREATE relationship must succeed");

    let effects = result.side_effects;
    assert_eq!(
        effects.relationships_created, 1,
        "exactly one relationship was created"
    );
    assert_eq!(effects.nodes_created, 2, "both endpoint nodes were created");
    assert_eq!(
        effects.labels_added, 2,
        "labels :A and :B on the two created nodes both count"
    );
    assert_eq!(effects.nodes_deleted, 0);
    assert_eq!(effects.relationships_deleted, 0);
    assert_eq!(effects.properties_set, 0);
    assert_eq!(effects.properties_removed, 0);
    assert_eq!(effects.labels_removed, 0);
}

#[test]
fn delete_node_reports_one_node_deleted() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (n:DelNodeProbe)")
        .expect("seed CREATE must succeed");

    let result = engine
        .execute_cypher("MATCH (n:DelNodeProbe) DELETE n")
        .expect("DELETE must succeed");

    let effects = result.side_effects;
    assert_eq!(effects.nodes_deleted, 1, "exactly one node was deleted");
    assert_eq!(effects.nodes_created, 0, "a DELETE creates nothing");
    assert_eq!(effects.relationships_deleted, 0);
    assert_eq!(effects.relationships_created, 0);
}

#[test]
fn delete_relationship_reports_one_relationship_deleted() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (a:DelRelA)-[:DelRelR]->(b:DelRelB)")
        .expect("seed CREATE must succeed");

    let result = engine
        .execute_cypher("MATCH ()-[r:DelRelR]->() DELETE r")
        .expect("DELETE relationship must succeed");

    let effects = result.side_effects;
    assert_eq!(
        effects.relationships_deleted, 1,
        "exactly one relationship was deleted"
    );
    assert_eq!(
        effects.nodes_deleted, 0,
        "the endpoint nodes survive a relationship-only DELETE"
    );
    assert_eq!(effects.nodes_created, 0);
    assert_eq!(effects.relationships_created, 0);
}

#[test]
fn detach_delete_reports_node_and_its_relationship() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (a:DetachA)-[:DetachR]->(b:DetachB)")
        .expect("seed CREATE must succeed");

    // DETACH DELETE a removes node a and its one edge; b survives.
    let result = engine
        .execute_cypher("MATCH (a:DetachA) DETACH DELETE a")
        .expect("DETACH DELETE must succeed");

    let effects = result.side_effects;
    assert_eq!(effects.nodes_deleted, 1, "the matched node is deleted");
    assert_eq!(
        effects.relationships_deleted, 1,
        "its one relationship is detached and deleted"
    );
    assert_eq!(effects.nodes_created, 0);
}

#[test]
fn set_label_on_existing_node_reports_one_label_added() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (n:SetLblBase)")
        .expect("seed CREATE must succeed");

    let result = engine
        .execute_cypher("MATCH (n:SetLblBase) SET n:SetLblExtra")
        .expect("SET label must succeed");

    let effects = result.side_effects;
    assert_eq!(effects.labels_added, 1, "the newly added label counts");
    assert_eq!(
        effects.nodes_created, 0,
        "SET on a matched node creates nothing"
    );
    assert_eq!(effects.labels_removed, 0);
}

#[test]
fn set_label_already_present_is_idempotent() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (n:IdemLbl)")
        .expect("seed CREATE must succeed");

    let result = engine
        .execute_cypher("MATCH (n:IdemLbl) SET n:IdemLbl")
        .expect("idempotent SET label must succeed");

    assert_eq!(
        result.side_effects.labels_added, 0,
        "re-adding a label the node already has counts nothing (TCK idempotent)"
    );
}

#[test]
fn remove_label_reports_one_label_removed() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (n:RmLblA:RmLblB)")
        .expect("seed CREATE must succeed");

    let result = engine
        .execute_cypher("MATCH (n:RmLblA) REMOVE n:RmLblB")
        .expect("REMOVE label must succeed");

    let effects = result.side_effects;
    assert_eq!(effects.labels_removed, 1, "the removed label counts");
    assert_eq!(effects.labels_added, 0);
    assert_eq!(effects.nodes_deleted, 0, "REMOVE label deletes no node");
}

#[test]
fn remove_absent_label_is_idempotent() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (n:AbsentLblA)")
        .expect("seed CREATE must succeed");

    let result = engine
        .execute_cypher("MATCH (n:AbsentLblA) REMOVE n:AbsentLblB")
        .expect("REMOVE of an absent label must succeed");

    assert_eq!(
        result.side_effects.labels_removed, 0,
        "removing a label the node does not have counts nothing (TCK idempotent)"
    );
}

#[test]
fn create_node_with_inline_properties_counts_each_key() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    let result = engine
        .execute_cypher("CREATE (n:PropNode {a: 1, b: 2})")
        .expect("CREATE with properties must succeed");

    let effects = result.side_effects;
    assert_eq!(
        effects.properties_set, 2,
        "both inline keys count toward +properties"
    );
    assert_eq!(effects.nodes_created, 1);
    assert_eq!(effects.labels_added, 1);
    assert_eq!(effects.properties_removed, 0);
}

#[test]
fn create_relationship_with_inline_property_counts_it() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    let result = engine
        .execute_cypher("CREATE (a:RelPropA)-[:RelPropR {w: 5}]->(b:RelPropB)")
        .expect("CREATE with relationship property must succeed");

    let effects = result.side_effects;
    assert_eq!(
        effects.properties_set, 1,
        "the relationship's inline property counts"
    );
    assert_eq!(effects.relationships_created, 1);
    assert_eq!(effects.nodes_created, 2);
}

#[test]
fn set_property_reports_one_property_set() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (n:SetPropNode)")
        .expect("seed CREATE must succeed");

    let result = engine
        .execute_cypher("MATCH (n:SetPropNode) SET n.x = 5")
        .expect("SET property must succeed");

    let effects = result.side_effects;
    assert_eq!(effects.properties_set, 1, "the SET write counts");
    assert_eq!(effects.properties_removed, 0);
    assert_eq!(effects.nodes_created, 0);
}

#[test]
fn set_property_to_null_removes_it() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (n:SetNullNode {x: 1})")
        .expect("seed CREATE must succeed");

    let result = engine
        .execute_cypher("MATCH (n:SetNullNode) SET n.x = null")
        .expect("SET null must succeed");

    let effects = result.side_effects;
    assert_eq!(
        effects.properties_removed, 1,
        "SET n.x = null removes the key (TCK -properties)"
    );
    assert_eq!(effects.properties_set, 0, "no property was set to a value");
}

#[test]
fn remove_property_reports_one_property_removed() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (n:RmPropNode {x: 1})")
        .expect("seed CREATE must succeed");

    let result = engine
        .execute_cypher("MATCH (n:RmPropNode) REMOVE n.x")
        .expect("REMOVE property must succeed");

    assert_eq!(
        result.side_effects.properties_removed, 1,
        "the removed key counts"
    );
}

#[test]
fn remove_absent_property_is_idempotent() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (n:RmAbsentNode)")
        .expect("seed CREATE must succeed");

    let result = engine
        .execute_cypher("MATCH (n:RmAbsentNode) REMOVE n.missing")
        .expect("REMOVE of an absent property must succeed");

    assert_eq!(
        result.side_effects.properties_removed, 0,
        "removing a property the node does not have counts nothing (TCK idempotent)"
    );
}

#[test]
fn set_map_merge_counts_each_non_null_key() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (n:MapMergeNode)")
        .expect("seed CREATE must succeed");

    let result = engine
        .execute_cypher("MATCH (n:MapMergeNode) SET n += {a: 1, b: 2}")
        .expect("SET += map must succeed");

    let effects = result.side_effects;
    assert_eq!(effects.properties_set, 2, "both merged keys count");
    assert_eq!(effects.properties_removed, 0);
}
