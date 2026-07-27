//! openCypher side-effect counting semantics. The counters existed but
//! diverged from the spec on three axes; these regressions pin the fixed
//! behaviour against the exact TCK scenarios (Create1, Set1, Set3).

use nexus_core::executor::types::SideEffects;
use nexus_core::testing::setup_isolated_test_engine;

/// Run `query` (optionally after a `setup` statement) and return its side
/// effects. When `setup` is provided the executor is refreshed between the
/// two so the second statement sees the committed graph.
fn effects(setup: Option<&str>, query: &str) -> SideEffects {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    if let Some(s) = setup {
        engine.execute_cypher(s).unwrap();
        engine.refresh_executor().unwrap();
    }
    engine.execute_cypher(query).unwrap().side_effects
}

// ── 1.1 +labels counted DISTINCT per statement (not per node,label) ──────────

#[test]
fn two_nodes_same_label_count_one_distinct_label() {
    // Create1[4]: CREATE (:Label), (:Label) → +labels 1
    let e = effects(None, "CREATE (:Label), (:Label)");
    assert_eq!(e.nodes_created, 2);
    assert_eq!(e.labels_added, 1);
}

#[test]
fn single_node_multiple_labels_counts_each_once() {
    // Create1[5]: CREATE (:A:B:C:D) → +labels 4
    let e = effects(None, "CREATE (:A:B:C:D)");
    assert_eq!(e.nodes_created, 1);
    assert_eq!(e.labels_added, 4);
}

#[test]
fn distinct_labels_across_several_nodes() {
    // Create1[6]: CREATE (:B:A:D), (:B:C), (:D:E:B) → distinct {A,B,C,D,E} = 5
    let e = effects(None, "CREATE (:B:A:D), (:B:C), (:D:E:B)");
    assert_eq!(e.nodes_created, 3);
    assert_eq!(e.labels_added, 5);
}

#[test]
fn set_label_on_node_with_existing_label_counts_only_the_new_one() {
    // Set3[3]: existing :A, SET n:Foo → +labels 1 (A already present)
    let e = effects(Some("CREATE (:A)"), "MATCH (n:A) SET n:Foo RETURN n");
    assert_eq!(e.labels_added, 1);
}

// ── 1.2 overwriting an existing property counts +1 AND -1 ────────────────────

#[test]
fn overwriting_a_property_counts_set_and_removed() {
    // Set1[1]: (:A {name:'Andres'}) then SET n.name = 'Michael' → +1 / -1
    let e = effects(
        Some("CREATE (:A {name: 'Andres'})"),
        "MATCH (n:A) SET n.name = 'Michael' RETURN n",
    );
    assert_eq!(e.properties_set, 1);
    assert_eq!(e.properties_removed, 1);
}

#[test]
fn setting_a_brand_new_property_counts_set_only() {
    // Set1[3]: (:A) then SET (n).name = 'neo4j' → +1 only
    let e = effects(
        Some("CREATE (:A)"),
        "MATCH (n:A) SET n.name = 'neo4j' RETURN n",
    );
    assert_eq!(e.properties_set, 1);
    assert_eq!(e.properties_removed, 0);
}

// ── 1.3 null-valued map keys are not counted as properties ───────────────────

#[test]
fn null_valued_key_is_not_counted_on_create() {
    // Create1[11]: CREATE (n {id: 12, name: null}) → +properties 1
    let e = effects(None, "CREATE (n {id: 12, name: null})");
    assert_eq!(e.nodes_created, 1);
    assert_eq!(e.properties_set, 1);
}
