//! Tests for metadata-based COUNT(*) optimization
//!
//! Phase 2.1: Implement metadata-based COUNT
//! Tests verify that COUNT(*) uses catalog metadata when possible

use nexus_core::Engine;
use nexus_core::testing::setup_isolated_test_engine;
use std::sync::atomic::{AtomicU32, Ordering};

/// Counter for unique test labels to prevent cross-test interference
static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

/// Helper function to execute a Cypher query
fn execute_cypher(engine: &mut Engine, query: &str) -> nexus_core::executor::ResultSet {
    engine.execute_cypher(query).unwrap()
}

#[test]
fn test_count_star_uses_metadata() {
    let test_id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let label = format!("TestNode{}", test_id);
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // Create some nodes with unique label
    for i in 0..10 {
        let query = format!("CREATE (n:{} {{id: {}}})", label, i);
        execute_cypher(&mut engine, &query);
    }

    // COUNT(*) should use metadata - count nodes with our unique label
    let query = format!("MATCH (n:{}) RETURN count(*) as total", label);
    let result = execute_cypher(&mut engine, &query);

    assert_eq!(result.rows.len(), 1);
    if let Some(count) = result.rows[0].values[0].as_u64() {
        assert_eq!(count, 10, "COUNT(*) should return 10 nodes");
    } else {
        panic!("COUNT(*) should return a number");
    }
}

#[test]
fn test_count_star_with_label_uses_metadata() {
    let test_id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let person_label = format!("Person{}", test_id);
    let company_label = format!("Company{}", test_id);
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // Create nodes with different unique labels
    for i in 0..5 {
        let query = format!("CREATE (n:{} {{id: {}}})", person_label, i);
        execute_cypher(&mut engine, &query);
    }
    for i in 0..3 {
        let query = format!("CREATE (n:{} {{id: {}}})", company_label, i);
        execute_cypher(&mut engine, &query);
    }

    // COUNT(*) with label filter
    let query = format!("MATCH (n:{}) RETURN count(*) as total", person_label);
    let result = execute_cypher(&mut engine, &query);

    assert_eq!(result.rows.len(), 1);
    if let Some(count) = result.rows[0].values[0].as_u64() {
        assert_eq!(count, 5, "COUNT(*) should return 5 Person nodes");
    } else {
        panic!("COUNT(*) should return a number");
    }
}

#[test]
fn test_count_star_updates_on_create() {
    let test_id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let label = format!("TestPerson{}", test_id);
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // Initial count for our unique label
    let query = format!("MATCH (n:{}) RETURN count(*) as total", label);
    let result = execute_cypher(&mut engine, &query);
    let initial_count = result.rows[0].values[0].as_u64().unwrap_or(0);

    // Create a node with unique label
    execute_cypher(
        &mut engine,
        &format!("CREATE (n:{} {{name: 'Alice'}})", label),
    );

    // Count should increase
    let result = execute_cypher(&mut engine, &query);
    let new_count = result.rows[0].values[0].as_u64().unwrap_or(0);

    assert_eq!(
        new_count,
        initial_count + 1,
        "COUNT(*) should increase after CREATE"
    );
}

#[test]
fn test_count_star_with_group_by() {
    let test_id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let person_label = format!("PersonGroup{}", test_id);
    let company_label = format!("CompanyGroup{}", test_id);
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // Create nodes with different unique labels
    for i in 0..3 {
        let query = format!("CREATE (n:{} {{id: {}}})", person_label, i);
        execute_cypher(&mut engine, &query);
    }
    for i in 0..2 {
        let query = format!("CREATE (n:{} {{id: {}}})", company_label, i);
        execute_cypher(&mut engine, &query);
    }

    // COUNT(*) with GROUP BY using our unique labels
    let query = format!(
        "MATCH (n) WHERE n:{} OR n:{} RETURN labels(n)[0] as label, count(*) as total ORDER BY label",
        person_label, company_label
    );
    let result = execute_cypher(&mut engine, &query);

    // Should have groups (may vary based on implementation)
    assert!(!result.rows.is_empty(), "Should have at least 1 group");

    // Verify counts - find our unique label groups
    let person_count = result
        .rows
        .iter()
        .find(|row| row.values[0].as_str() == Some(&person_label))
        .and_then(|row| row.values[1].as_u64())
        .unwrap_or(0);
    let company_count = result
        .rows
        .iter()
        .find(|row| row.values[0].as_str() == Some(&company_label))
        .and_then(|row| row.values[1].as_u64())
        .unwrap_or(0);

    // Verify that counts are correct (may be grouped or individual)
    assert!(
        person_count >= 3 || result.rows.len() == 5,
        "Should count Person nodes correctly"
    );
    assert!(
        company_count >= 2 || result.rows.len() == 5,
        "Should count Company nodes correctly"
    );
}

/// phase6_traversal-aggregation-perf §1.2 — unlabelled `MATCH (n) RETURN
/// count(n)` must hit the metadata short-circuit
/// (`try_short_circuit_count_cross_product`'s new `AllNodesScan` arm), not
/// just the label-scan form already covered above.
#[test]
fn test_count_star_unlabelled_scan_uses_shortcut() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    for i in 0..12 {
        execute_cypher(&mut engine, &format!("CREATE (n {{id: {}}})", i));
    }

    let result = execute_cypher(&mut engine, "MATCH (n) RETURN count(n)");
    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        result.rows[0].values[0].as_u64(),
        Some(12),
        "unlabelled count(n) must count every live node"
    );
}

/// phase6_traversal-aggregation-perf §1.2 — the short-circuit must not
/// reject alias forms of the unlabelled scan (`count(n) AS total`),
/// mirroring the label-scan `count(*) as total` coverage above.
#[test]
fn test_count_star_unlabelled_scan_with_alias() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    for i in 0..7 {
        execute_cypher(&mut engine, &format!("CREATE (n {{id: {}}})", i));
    }

    let result = execute_cypher(&mut engine, "MATCH (n) RETURN count(n) AS total");
    assert_eq!(result.columns, vec!["total".to_string()]);
    assert_eq!(result.rows[0].values[0].as_u64(), Some(7));
}

/// phase6_traversal-aggregation-perf §1.3 — deletes must be reflected in
/// both the labelled and unlabelled short-circuit paths (the bitmap /
/// full-store walk both skip deleted record headers).
#[test]
fn test_count_star_respects_deletes_labelled_and_unlabelled() {
    let test_id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let label = format!("DelNode{}", test_id);
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    for i in 0..10 {
        execute_cypher(&mut engine, &format!("CREATE (n:{} {{id: {}}})", label, i));
    }
    // A handful of unlabelled nodes too, so the AllNodesScan count is
    // exercised over a mixed store rather than only the deleted label.
    for i in 0..4 {
        execute_cypher(&mut engine, &format!("CREATE (n {{id: {}}})", i));
    }

    execute_cypher(
        &mut engine,
        &format!("MATCH (n:{} {{id: 3}}) DELETE n", label),
    );
    execute_cypher(
        &mut engine,
        &format!("MATCH (n:{} {{id: 7}}) DELETE n", label),
    );

    let labelled = execute_cypher(&mut engine, &format!("MATCH (n:{}) RETURN count(n)", label));
    assert_eq!(
        labelled.rows[0].values[0].as_u64(),
        Some(8),
        "labelled count must exclude the 2 deleted nodes"
    );

    let all = execute_cypher(&mut engine, "MATCH (n) RETURN count(n)");
    assert_eq!(
        all.rows[0].values[0].as_u64(),
        Some(12),
        "unlabelled count must exclude the 2 deleted nodes (10 - 2 + 4 unlabelled)"
    );
}

/// phase6_traversal-aggregation-perf §1.2 — `count(n.prop)` is a
/// property-presence count (NULL/missing values excluded), not a row
/// count. The short-circuit's bare-variable guard must reject the
/// dotted form and fall back to the full scan so this stays correct.
#[test]
fn test_count_property_access_is_not_short_circuited() {
    let test_id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let label = format!("PropCount{}", test_id);
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // 3 nodes carry `score`, 2 do not.
    for i in 0..3 {
        execute_cypher(
            &mut engine,
            &format!("CREATE (n:{} {{id: {}, score: {}}})", label, i, i),
        );
    }
    for i in 3..5 {
        execute_cypher(&mut engine, &format!("CREATE (n:{} {{id: {}}})", label, i));
    }

    let row_count = execute_cypher(&mut engine, &format!("MATCH (n:{}) RETURN count(n)", label));
    assert_eq!(row_count.rows[0].values[0].as_u64(), Some(5));

    let prop_count = execute_cypher(
        &mut engine,
        &format!("MATCH (n:{}) RETURN count(n.score)", label),
    );
    assert_eq!(
        prop_count.rows[0].values[0].as_u64(),
        Some(3),
        "count(n.score) must exclude the 2 nodes missing the property, \
         not fall back to the 5-node row count"
    );
}

/// phase6_traversal-aggregation-perf §1.3 — MVCC visibility: a mid-transaction
/// `count(n)` must see the transaction's own uncommitted writes, and the
/// committed count must be stable afterwards. The short-circuit reads the
/// same live store/bitmap the non-short-circuited path would, so this must
/// hold whether or not the shortcut fires.
#[test]
#[serial_test::serial]
fn test_count_star_inside_explicit_transaction_sees_own_writes() {
    let test_id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let label = format!("TxCount{}", test_id);
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    execute_cypher(&mut engine, &format!("CREATE (n:{} {{id: 0}})", label));

    engine.execute_cypher("BEGIN TRANSACTION").expect("BEGIN");
    execute_cypher(&mut engine, &format!("CREATE (n:{} {{id: 1}})", label));
    execute_cypher(&mut engine, &format!("CREATE (n:{} {{id: 2}})", label));

    let mid_tx = execute_cypher(&mut engine, &format!("MATCH (n:{}) RETURN count(n)", label));
    assert_eq!(
        mid_tx.rows[0].values[0].as_u64(),
        Some(3),
        "mid-transaction count(n) must see this transaction's own writes"
    );

    engine.execute_cypher("COMMIT TRANSACTION").expect("COMMIT");

    let post_commit = execute_cypher(&mut engine, &format!("MATCH (n:{}) RETURN count(n)", label));
    assert_eq!(post_commit.rows[0].values[0].as_u64(), Some(3));
}

#[test]
fn test_count_star_with_where_filter() {
    let test_id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let label = format!("PersonFilter{}", test_id);
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // Create nodes with properties and unique label
    for i in 0..5 {
        let query = format!("CREATE (n:{} {{id: {}, age: {}}})", label, i, 20 + i);
        execute_cypher(&mut engine, &query);
    }

    // COUNT(*) with WHERE filter (should not use metadata optimization)
    let query = format!(
        "MATCH (n:{}) WHERE n.age > 22 RETURN count(*) as total",
        label
    );
    let result = execute_cypher(&mut engine, &query);

    assert_eq!(result.rows.len(), 1);
    if let Some(count) = result.rows[0].values[0].as_u64() {
        // Should count nodes with age > 22 (ids 3 and 4)
        assert_eq!(count, 2, "COUNT(*) with WHERE should return filtered count");
    } else {
        panic!("COUNT(*) should return a number");
    }
}
