//! openCypher treats a map key written with a NULL value as **absent**, not as
//! present-and-null. Every assertion here checks the observable graph state —
//! `keys()`, property access — alongside the `+properties` / `-properties`
//! counters, because the counters alone were already right: they carried a
//! private null filter while the UNFILTERED map went on to storage, so the graph
//! contradicted the count that reported it and `keys(n)` returned a phantom key.
//! A test that asserted only the counter is exactly what let that through.

use nexus_core::executor::types::SideEffects;
use nexus_core::testing::setup_isolated_test_engine;

struct Outcome {
    effects: SideEffects,
    keys: Vec<String>,
}

/// Run `setup` then `write`, and read back `keys(...)` for the entity `observe`
/// projects. Returns the write's side effects and the sorted key list.
fn write_then_keys(setup: &[&str], write: &str, observe: &str) -> Outcome {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    for s in setup {
        engine.execute_cypher(s).expect("setup should succeed");
        engine.refresh_executor().unwrap();
    }
    let effects = engine
        .execute_cypher(write)
        .expect("write should succeed")
        .side_effects;
    engine.refresh_executor().unwrap();
    let rs = engine.execute_cypher(observe).expect("read back");
    let mut keys: Vec<String> = rs
        .rows
        .first()
        .and_then(|r| r.values.first())
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|k| k.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    keys.sort();
    Outcome { effects, keys }
}

#[test]
fn create_node_omits_a_null_valued_key() {
    // openCypher TCK `clauses/create` Create2: `+properties 1`, and the node has
    // no `name` key at all.
    let out = write_then_keys(
        &[],
        "CREATE (n:P {id: 12, name: null})",
        "MATCH (n:P) RETURN keys(n)",
    );
    assert_eq!(
        out.keys,
        vec!["id".to_string()],
        "a null-valued key must be absent, not stored"
    );
    assert_eq!(
        out.effects.properties_set, 1,
        "+properties counts only `id`"
    );
}

#[test]
fn a_null_valued_property_reads_back_as_null_and_is_not_a_key() {
    // The distinction that matters: property access still yields NULL (absent
    // and null are indistinguishable when READ), while `keys()` proves the key
    // was never stored.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (n:P {id: 12, name: null})")
        .expect("CREATE");
    engine.refresh_executor().unwrap();

    let rs = engine
        .execute_cypher("MATCH (n:P) RETURN n.name IS NULL, size(keys(n))")
        .expect("read back");
    assert_eq!(rs.rows[0].values[0].as_bool(), Some(true));
    assert_eq!(rs.rows[0].values[1].as_i64(), Some(1));
}

#[test]
fn create_relationship_omits_a_null_valued_key() {
    let out = write_then_keys(
        &[],
        "CREATE (:P)-[:R {w: 1, tag: null}]->(:P)",
        "MATCH ()-[r:R]->() RETURN keys(r)",
    );
    assert_eq!(
        out.keys,
        vec!["w".to_string()],
        "the relationship path needs the same rule as the node path"
    );
    assert_eq!(out.effects.properties_set, 1);
}

#[test]
fn create_with_every_value_null_stores_no_properties() {
    let out = write_then_keys(
        &[],
        "CREATE (n:P {a: null, b: null})",
        "MATCH (n:P) RETURN keys(n)",
    );
    assert!(out.keys.is_empty(), "expected no keys, got {:?}", out.keys);
    assert_eq!(out.effects.properties_set, 0);
}

// ── The update paths: already conformant, pinned so they stay that way ───────

#[test]
fn set_property_to_null_removes_an_existing_key() {
    let out = write_then_keys(
        &["CREATE (n:P {id: 1, name: 'x'})"],
        "MATCH (n:P) SET n.name = null",
        "MATCH (n:P) RETURN keys(n)",
    );
    assert_eq!(out.keys, vec!["id".to_string()]);
    assert_eq!(out.effects.properties_removed, 1, "-properties 1");
}

#[test]
fn set_property_to_null_on_an_absent_key_is_a_no_op() {
    let out = write_then_keys(
        &["CREATE (n:P {id: 1})"],
        "MATCH (n:P) SET n.name = null",
        "MATCH (n:P) RETURN keys(n)",
    );
    assert_eq!(out.keys, vec!["id".to_string()]);
    assert_eq!(
        out.effects.properties_removed, 0,
        "nothing was there to remove"
    );
    assert_eq!(out.effects.properties_set, 0);
}

#[test]
fn map_merge_with_a_null_value_removes_that_key() {
    let out = write_then_keys(
        &["CREATE (n:P {id: 1, name: 'x'})"],
        "MATCH (n:P) SET n += {name: null}",
        "MATCH (n:P) RETURN keys(n)",
    );
    assert_eq!(out.keys, vec!["id".to_string()]);
    assert_eq!(out.effects.properties_removed, 1);
}

/// openCypher TCK `clauses/set` `Set4` [3] "Null values in a property map are
/// removed with SET". Also pins the whole-replace COUNTING rule, which is not a
/// diff: every pre-existing key counts as removed and every new non-null key as
/// set, so `{name: 'A', name2: 'B'}` → `{name: 'B', name2: null, baz: 'C'}` is
/// `+properties 2` / `-properties 2`.
#[test]
fn whole_entity_replace_drops_null_valued_keys() {
    let out = write_then_keys(
        &["CREATE (n:X {name: 'A', name2: 'B'})"],
        "MATCH (n:X) SET n = {name: 'B', name2: null, baz: 'C'}",
        "MATCH (n:X) RETURN keys(n)",
    );
    assert_eq!(out.keys, vec!["baz".to_string(), "name".to_string()]);
    assert_eq!(out.effects.properties_set, 2);
    assert_eq!(out.effects.properties_removed, 2);
}

#[test]
fn merge_on_create_setting_null_stores_no_key() {
    let out = write_then_keys(
        &[],
        "MERGE (n:P {id: 1}) ON CREATE SET n.name = null",
        "MATCH (n:P) RETURN keys(n)",
    );
    assert_eq!(out.keys, vec!["id".to_string()]);
    // `+properties 1` is the merge pattern's own `id`, not the null `name`.
    assert_eq!(out.effects.properties_set, 1);
}

/// A null in a MERGE pattern is not "absent" — it makes the pattern
/// unsatisfiable, and Cypher rejects it rather than merging on a missing key.
/// Nexus already matches Neo4j's wording here; pinned so the null-is-absent rule
/// above is not over-applied to MERGE's matching half.
#[test]
fn merge_on_a_null_property_value_is_rejected() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let err = engine
        .execute_cypher("MERGE (n:P {id: 1, name: null})")
        .expect_err("MERGE on a null property value must be rejected");
    assert!(
        err.to_string().contains("null property value"),
        "got: {err}"
    );
}
