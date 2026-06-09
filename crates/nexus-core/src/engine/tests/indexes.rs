//! Tests for typed property indexes: correctness, seek vs scan planning,
//! comma-join planning, API CREATE INDEX (register + backfill + IF NOT EXISTS
//! + OR REPLACE).

use super::*;

// ─── phase6_fix-read-match-index-seek (issue #8, sub-task 3) ───────────────

/// CORRECTNESS-1: A typed property index seek returns the correct rows and
/// handles a missing key without panicking.
///
/// After `CREATE INDEX FOR (n:Person) ON (n.id)`, querying
/// `MATCH (n:Person {id:'b'}) RETURN n` must return exactly the node with
/// `id='b'`; querying for `id='zzz'` must return zero rows.
#[test]
#[serial_test::serial]
fn read_match_indexed_property_returns_correct_rows() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();

    // Seed three :Person nodes with distinct id values plus a name.
    engine
        .execute_cypher(
            "CREATE (:Person {id: 'a', name: 'Alice'}), \
             (:Person {id: 'b', name: 'Bob'}), \
             (:Person {id: 'c', name: 'Carol'})",
        )
        .expect("seed CREATE must succeed");

    // Create a property index on :Person(id) — backfills existing nodes.
    engine
        .execute_cypher("CREATE INDEX FOR (n:Person) ON (n.id)")
        .expect("CREATE INDEX must succeed");

    // Query for the node with id='b'.
    let result = engine
        .execute_cypher("MATCH (n:Person {id: 'b'}) RETURN n.name AS name")
        .expect("indexed MATCH must succeed");

    assert_eq!(
        result.rows.len(),
        1,
        "expected exactly 1 row for id='b', got {}",
        result.rows.len()
    );
    assert_eq!(
        result.rows[0].values[0].as_str(),
        Some("Bob"),
        "expected name='Bob' for id='b', got {:?}",
        result.rows[0].values[0]
    );

    // Query for a key that does not exist — must return 0 rows, not panic.
    let missing = engine
        .execute_cypher("MATCH (n:Person {id: 'zzz'}) RETURN n.name AS name")
        .expect("indexed MATCH for missing key must not error");

    assert_eq!(
        missing.rows.len(),
        0,
        "expected 0 rows for id='zzz' (missing key), got {}",
        missing.rows.len()
    );
}

/// CORRECTNESS-2: The indexed seek path returns the same rows as an
/// unindexed full-scan path for the same node.
///
/// Seeds :Person nodes. Queries by `id` (indexed) and by `name` (not
/// indexed) for a value that identifies the same node. Both results
/// must agree — the seek cannot silently exclude rows that the scan
/// would include.
#[test]
#[serial_test::serial]
fn read_match_indexed_equals_unindexed_results() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();

    engine
        .execute_cypher(
            "CREATE (:Person {id: 'x1', name: 'Xavier'}), \
             (:Person {id: 'x2', name: 'Yvette'}), \
             (:Person {id: 'x3', name: 'Zara'})",
        )
        .expect("seed CREATE must succeed");

    // Create a property index on :Person(id) only — `name` stays unindexed.
    engine
        .execute_cypher("CREATE INDEX FOR (n:Person) ON (n.id)")
        .expect("CREATE INDEX must succeed");

    // Indexed path: query by id (has index).
    let indexed = engine
        .execute_cypher("MATCH (n:Person {id: 'x2'}) RETURN n.name AS name")
        .expect("indexed MATCH must succeed");

    // Unindexed path: query by name (no index) for the same node.
    let scanned = engine
        .execute_cypher("MATCH (n:Person {name: 'Yvette'}) RETURN n.name AS name")
        .expect("unindexed MATCH must succeed");

    assert_eq!(
        indexed.rows.len(),
        1,
        "indexed path must return exactly 1 row for id='x2'"
    );
    assert_eq!(
        scanned.rows.len(),
        1,
        "unindexed scan must return exactly 1 row for name='Yvette'"
    );
    assert_eq!(
        indexed.rows[0].values[0], scanned.rows[0].values[0],
        "indexed seek and full scan must return the same name value; \
         indexed={:?}, scanned={:?}",
        indexed.rows[0].values[0], scanned.rows[0].values[0]
    );
}

/// CORRECTNESS-3: A comma-joined MATCH seeded by index seeks on both legs
/// returns exactly one combined row identifying the correct pair of nodes.
///
/// Creates decoy nodes for each label to rule out accidental full-scan
/// cartesian products that would produce spurious extra rows.
#[test]
#[serial_test::serial]
fn comma_join_indexed_endpoints_returns_correct_row() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();

    // Seed :Turn nodes — one target (id='t1') plus two decoys.
    engine
        .execute_cypher(
            "CREATE (:Turn {id: 't1', label: 'turn-one'}), \
             (:Turn {id: 't2', label: 'turn-two'}), \
             (:Turn {id: 't3', label: 'turn-three'})",
        )
        .expect("Turn seed must succeed");

    // Seed :ToolCall nodes — one target (id='c1') plus two decoys.
    engine
        .execute_cypher(
            "CREATE (:ToolCall {id: 'c1', label: 'call-one'}), \
             (:ToolCall {id: 'c2', label: 'call-two'}), \
             (:ToolCall {id: 'c3', label: 'call-three'})",
        )
        .expect("ToolCall seed must succeed");

    // Create indexes on both labels.
    engine
        .execute_cypher("CREATE INDEX FOR (n:Turn) ON (n.id)")
        .expect("CREATE INDEX on Turn must succeed");
    engine
        .execute_cypher("CREATE INDEX FOR (n:ToolCall) ON (n.id)")
        .expect("CREATE INDEX on ToolCall must succeed");

    // Comma-join: each leg is constrained by its own index seek.
    let result = engine
        .execute_cypher(
            "MATCH (a:Turn {id: 't1'}), (b:ToolCall {id: 'c1'}) \
             RETURN a.label AS turn_label, b.label AS call_label",
        )
        .expect("comma-join MATCH must succeed");

    assert_eq!(
        result.rows.len(),
        1,
        "expected exactly 1 combined row for t1+c1; \
         got {} rows — a full-scan cartesian product would return 9",
        result.rows.len()
    );
    assert_eq!(
        result.rows[0].values[0].as_str(),
        Some("turn-one"),
        "first column must be turn-one's label, got {:?}",
        result.rows[0].values[0]
    );
    assert_eq!(
        result.rows[0].values[1].as_str(),
        Some("call-one"),
        "second column must be call-one's label, got {:?}",
        result.rows[0].values[1]
    );
}

/// SCALING GUARD-1: an indexed single-property selector plans a
/// `NodeIndexSeek` (point lookup), never a `NodeByLabel` full scan.
#[test]
#[serial_test::serial]
fn indexed_selector_plans_node_index_seek() {
    use crate::executor::types::Operator;
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();

    engine
        .execute_cypher("CREATE (:Person {id: 'a'}), (:Person {id: 'b'})")
        .expect("seed CREATE must succeed");
    engine
        .execute_cypher("CREATE INDEX FOR (n:Person) ON (n.id)")
        .expect("CREATE INDEX must succeed");

    let plan = engine
        .executor
        .parse_and_plan("MATCH (n:Person {id: 'b'}) RETURN n")
        .expect("plan must succeed");

    assert!(
        plan.iter()
            .any(|op| matches!(op, Operator::NodeIndexSeek { .. })),
        "indexed selector must plan a NodeIndexSeek; plan = {plan:?}"
    );
    assert!(
        !plan
            .iter()
            .any(|op| matches!(op, Operator::NodeByLabel { .. })),
        "indexed selector must NOT fall back to a NodeByLabel scan; plan = {plan:?}"
    );
}

/// SCALING GUARD-2: a selector on a property with no covering index falls
/// back to a `NodeByLabel` scan and emits no `NodeIndexSeek`.
#[test]
#[serial_test::serial]
fn unindexed_selector_plans_node_by_label() {
    use crate::executor::types::Operator;
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();

    engine
        .execute_cypher("CREATE (:Person {id: 'a', name: 'Alice'})")
        .expect("seed CREATE must succeed");
    // Index :Person(id) only — `name` has no index.
    engine
        .execute_cypher("CREATE INDEX FOR (n:Person) ON (n.id)")
        .expect("CREATE INDEX must succeed");

    let plan = engine
        .executor
        .parse_and_plan("MATCH (n:Person {name: 'Alice'}) RETURN n")
        .expect("plan must succeed");

    assert!(
        plan.iter()
            .any(|op| matches!(op, Operator::NodeByLabel { .. })),
        "unindexed selector must plan a NodeByLabel scan; plan = {plan:?}"
    );
    assert!(
        !plan
            .iter()
            .any(|op| matches!(op, Operator::NodeIndexSeek { .. })),
        "unindexed selector must NOT plan a NodeIndexSeek; plan = {plan:?}"
    );
}

/// SCALING GUARD-3: a comma-joined MATCH with both legs indexed plans TWO
/// `NodeIndexSeek` operators — proving each endpoint is a point lookup and
/// the join is not a cartesian product of two full label scans.
#[test]
#[serial_test::serial]
fn comma_join_both_legs_plan_index_seek() {
    use crate::executor::types::Operator;
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();

    engine
        .execute_cypher("CREATE (:Turn {id: 't1'}), (:ToolCall {id: 'c1'})")
        .expect("seed CREATE must succeed");
    engine
        .execute_cypher("CREATE INDEX FOR (n:Turn) ON (n.id)")
        .expect("CREATE INDEX on Turn must succeed");
    engine
        .execute_cypher("CREATE INDEX FOR (n:ToolCall) ON (n.id)")
        .expect("CREATE INDEX on ToolCall must succeed");

    let plan = engine
        .executor
        .parse_and_plan("MATCH (a:Turn {id: 't1'}), (b:ToolCall {id: 'c1'}) RETURN a, b")
        .expect("plan must succeed");

    let seeks = plan
        .iter()
        .filter(|op| matches!(op, Operator::NodeIndexSeek { .. }))
        .count();
    assert_eq!(
        seeks, 2,
        "comma-join with both legs indexed must plan 2 NodeIndexSeek ops \
         (one per leg), got {seeks}; plan = {plan:?}"
    );
}

/// ISSUE #9: `CREATE INDEX` through the executor/API path must register the
/// typed property index AND backfill existing nodes — not merely intern the
/// catalog key. Before the fix `has_index` stayed false, so reads fell back
/// to a full label scan (Nexus.Performance.UnindexedPropertyAccess) and
/// index-backed MERGE existence degraded to O(N).
#[test]
#[serial_test::serial]
fn api_create_index_registers_and_populates() {
    use crate::executor::types::Operator;
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();

    engine
        .execute_cypher("CREATE (:Turn {id: 't1'}), (:Turn {id: 't2'}), (:Turn {id: 't3'})")
        .expect("seed CREATE must succeed");

    // Create the index via the executor/API path (the code path issue #9 fixed).
    engine
        .executor
        .execute_create_index("Turn", "id", None, false, false)
        .expect("executor CREATE INDEX must succeed");

    // The typed index must now be registered AND backfilled with existing data.
    let label_id = engine.catalog.get_label_id("Turn").expect("label exists");
    let key_id = engine.catalog.get_key_id("id").expect("key exists");
    assert!(
        engine.indexes.property_index.has_index(label_id, key_id),
        "API CREATE INDEX must register the typed property index"
    );
    let hits = engine
        .indexes
        .property_index
        .find_exact(
            label_id,
            key_id,
            crate::index::PropertyValue::String("t2".into()),
        )
        .expect("find_exact must succeed");
    assert_eq!(
        hits.len(),
        1,
        "backfill must index the existing node with id='t2'"
    );

    // The read planner must now choose a NodeIndexSeek (no fallback scan).
    let plan = engine
        .executor
        .parse_and_plan("MATCH (n:Turn {id: 't2'}) RETURN n")
        .expect("plan must succeed");
    assert!(
        plan.iter()
            .any(|op| matches!(op, Operator::NodeIndexSeek { .. })),
        "indexed read must plan NodeIndexSeek after API CREATE INDEX; plan = {plan:?}"
    );

    // Regression on the issue's exact symptom: no UnindexedPropertyAccess.
    let result = engine
        .execute_cypher("MATCH (n:Turn {id: 't2'}) RETURN n.id")
        .expect("read must succeed");
    assert!(
        !result
            .notifications
            .iter()
            .any(|n| n.code == "Nexus.Performance.UnindexedPropertyAccess"),
        "must not emit UnindexedPropertyAccess once the index is populated; \
         notifications = {:?}",
        result.notifications
    );
}

/// ISSUE #9: executor `CREATE INDEX` honours duplicate / IF NOT EXISTS on
/// the typed property index (not the unrelated spatial R-tree registry).
#[test]
#[serial_test::serial]
fn api_create_index_if_not_exists_and_duplicate() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();
    engine
        .execute_cypher("CREATE (:Turn {id: 't1'})")
        .expect("seed CREATE must succeed");

    engine
        .executor
        .execute_create_index("Turn", "id", None, false, false)
        .expect("first CREATE INDEX must succeed");

    // Duplicate without IF NOT EXISTS / OR REPLACE must error.
    let dup = engine
        .executor
        .execute_create_index("Turn", "id", None, false, false);
    assert!(
        dup.is_err(),
        "duplicate CREATE INDEX must error without IF NOT EXISTS / OR REPLACE"
    );

    // IF NOT EXISTS must succeed silently on an existing index.
    engine
        .executor
        .execute_create_index("Turn", "id", None, true, false)
        .expect("CREATE INDEX IF NOT EXISTS must be a no-op success");
}

/// ISSUE #9: executor `CREATE INDEX ... OR REPLACE` rebuilds and re-backfills
/// the typed property index.
#[test]
#[serial_test::serial]
fn api_create_index_or_replace_repopulates() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();
    engine
        .execute_cypher("CREATE (:Turn {id: 't1'})")
        .expect("seed CREATE must succeed");

    engine
        .executor
        .execute_create_index("Turn", "id", None, false, false)
        .expect("first CREATE INDEX must succeed");
    engine
        .executor
        .execute_create_index("Turn", "id", None, false, true)
        .expect("CREATE INDEX OR REPLACE must succeed");

    let label_id = engine.catalog.get_label_id("Turn").expect("label exists");
    let key_id = engine.catalog.get_key_id("id").expect("key exists");
    assert!(
        engine.indexes.property_index.has_index(label_id, key_id),
        "OR REPLACE must leave the index registered"
    );
    let hits = engine
        .indexes
        .property_index
        .find_exact(
            label_id,
            key_id,
            crate::index::PropertyValue::String("t1".into()),
        )
        .expect("find_exact must succeed");
    assert_eq!(
        hits.len(),
        1,
        "OR REPLACE must re-backfill existing nodes (id='t1')"
    );
}
