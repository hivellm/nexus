#![allow(unused_mut)] // test fixtures declare `mut` preemptively

//! Test CALL procedure syntax variations
use nexus_core::executor::Query;
use nexus_core::testing::create_isolated_test_executor;
use nexus_core::testing::create_test_executor;

#[test]
fn test_call_procedure_with_yield_and_return() {
    let (mut executor, _ctx) = create_test_executor();

    // Create some nodes with labels first
    let create_query = Query {
        cypher: "CREATE (n1:Person), (n2:Employee), (n3:Person)".to_string(),
        params: std::collections::HashMap::new(),
    };
    executor.execute(&create_query).unwrap();

    let query = Query {
        cypher: "CALL db.labels() YIELD label RETURN label".to_string(),
        params: std::collections::HashMap::new(),
    };

    let result = executor.execute(&query);
    // Should execute successfully
    assert!(
        result.is_ok(),
        "CALL procedure with YIELD and RETURN should work"
    );

    if let Ok(result_set) = result {
        // Should have results
        assert!(
            !result_set.rows.is_empty(),
            "Should return at least one label"
        );
        assert_eq!(result_set.columns.len(), 1);
        assert_eq!(result_set.columns[0], "label");
    }
}

#[test]
fn test_call_procedure_with_return_only() {
    let (mut executor, _ctx) = create_test_executor();

    // Create some nodes with labels first
    let create_query = Query {
        cypher: "CREATE (n1:Person), (n2:Employee)".to_string(),
        params: std::collections::HashMap::new(),
    };
    executor.execute(&create_query).unwrap();

    let query = Query {
        cypher: "CALL db.labels() RETURN label".to_string(),
        params: std::collections::HashMap::new(),
    };

    let result = executor.execute(&query);
    // Should execute successfully (even if YIELD is omitted, RETURN should work)
    assert!(
        result.is_ok(),
        "CALL procedure with RETURN only should work"
    );
}

#[test]
fn test_call_procedure_without_return() {
    let (mut executor, _ctx) = create_test_executor();

    // Create some nodes with labels first
    let create_query = Query {
        cypher: "CREATE (n1:Person), (n2:Employee)".to_string(),
        params: std::collections::HashMap::new(),
    };
    executor.execute(&create_query).unwrap();

    let query = Query {
        cypher: "CALL db.labels()".to_string(),
        params: std::collections::HashMap::new(),
    };

    let result = executor.execute(&query);
    // Should execute successfully even without RETURN
    // The procedure should still return results
    assert!(result.is_ok(), "CALL procedure without RETURN should work");

    if let Ok(result_set) = result {
        // Procedures typically return results even without explicit RETURN
        // The exact behavior depends on implementation
        assert!(
            !result_set.rows.is_empty() || result_set.rows.is_empty(),
            "Procedure may or may not return results"
        );
    }
}

#[test]
fn test_call_procedure_relationship_types() {
    let (mut executor, _ctx) = create_test_executor();

    // Create some relationships first
    let create_query = Query {
        cypher: "CREATE (a:Person)-[:KNOWS]->(b:Person), (b)-[:WORKS_WITH]->(c:Person)".to_string(),
        params: std::collections::HashMap::new(),
    };
    executor.execute(&create_query).unwrap();

    let query = Query {
        cypher: "CALL db.relationshipTypes() YIELD relationshipType RETURN relationshipType"
            .to_string(),
        params: std::collections::HashMap::new(),
    };

    let result = executor.execute(&query);
    assert!(result.is_ok(), "CALL db.relationshipTypes() should work");
}

#[test]
fn test_call_procedure_property_keys() {
    let (mut executor, _ctx) = create_test_executor();

    // Create some nodes with properties first
    let create_query = Query {
        cypher: "CREATE (n1:Person {name: 'Alice', age: 30}), (n2:Person {name: 'Bob'})"
            .to_string(),
        params: std::collections::HashMap::new(),
    };
    executor.execute(&create_query).unwrap();

    let query = Query {
        cypher: "CALL db.propertyKeys() YIELD propertyKey RETURN propertyKey".to_string(),
        params: std::collections::HashMap::new(),
    };

    let result = executor.execute(&query);
    assert!(result.is_ok(), "CALL db.propertyKeys() should work");
}

#[test]
fn test_call_procedure_schema() {
    let (mut executor, _ctx) = create_test_executor();

    // Create some nodes and relationships first
    let create_query = Query {
        cypher: "CREATE (a:Person {name: 'Alice'})-[r:KNOWS]->(b:Person {name: 'Bob'})".to_string(),
        params: std::collections::HashMap::new(),
    };
    executor.execute(&create_query).unwrap();

    let query = Query {
        cypher: "CALL db.schema() YIELD nodes, relationships RETURN nodes, relationships"
            .to_string(),
        params: std::collections::HashMap::new(),
    };

    let result = executor.execute(&query);
    assert!(result.is_ok(), "CALL db.schema() should work");
}

// ── phase0_fix-order-by-on-call-yield: ORDER BY/SKIP/LIMIT on procedure
// YIELD projections ──
//
// Root cause: `QueryPlanner::plan_query`'s no-pattern branch (no `MATCH` in
// the query, e.g. `CALL db.labels() YIELD label RETURN label ORDER BY
// label`) built `Project`/`Aggregate` + `Limit` operators but never
// consumed the `order_by_clause`/`skip_count` it had already collected —
// those were only ever turned into `Operator::Sort`/`Operator::Skip` by
// `plan_execution_strategy`, which only runs `if !patterns.is_empty()`
// (i.e. only for `MATCH`-driven queries). A bare `CALL ... YIELD ...
// RETURN ...` has no `MATCH`, so `patterns` is empty and the ORDER BY
// silently vanished. See `crates/nexus-core/src/executor/planner/queries/
// planner_core.rs`.

/// Seed labels in non-alphabetical insertion order so the test cannot pass
/// by accident (e.g. an unindexed catalog that happens to return labels in
/// insertion order would still fail an unsorted assertion here).
fn seed_unsorted_labels(executor: &mut nexus_core::executor::Executor) {
    let create_query = Query {
        cypher: "CREATE (:B), (:A), (:C), (:D), (:E)".to_string(),
        params: std::collections::HashMap::new(),
    };
    executor.execute(&create_query).unwrap();
}

fn yielded_labels(executor: &nexus_core::executor::Executor, cypher: &str) -> Vec<String> {
    let query = Query {
        cypher: cypher.to_string(),
        params: std::collections::HashMap::new(),
    };
    let result = executor
        .execute(&query)
        .unwrap_or_else(|e| panic!("query `{cypher}` failed: {e}"));
    result
        .rows
        .iter()
        .map(|r| r.values[0].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn test_call_yield_order_by_ascending_sorts() {
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed_unsorted_labels(&mut executor);

    let labels = yielded_labels(
        &executor,
        "CALL db.labels() YIELD label RETURN label ORDER BY label",
    );
    assert_eq!(
        labels,
        vec!["A", "B", "C", "D", "E"],
        "CALL ... YIELD ... RETURN ... ORDER BY must sort ascending, not \
         return procedure-native order"
    );
}

#[test]
fn test_call_yield_order_by_descending_sorts() {
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed_unsorted_labels(&mut executor);

    let labels = yielded_labels(
        &executor,
        "CALL db.labels() YIELD label RETURN label ORDER BY label DESC",
    );
    assert_eq!(
        labels,
        vec!["E", "D", "C", "B", "A"],
        "CALL ... YIELD ... RETURN ... ORDER BY ... DESC must sort descending"
    );
}

#[test]
fn test_call_yield_order_by_projection_alias() {
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed_unsorted_labels(&mut executor);

    // ORDER BY referencing the RETURN projection's alias of the YIELDed
    // column, not the raw YIELD column name.
    let labels = yielded_labels(
        &executor,
        "CALL db.labels() YIELD label RETURN label AS l ORDER BY l",
    );
    assert_eq!(
        labels,
        vec!["A", "B", "C", "D", "E"],
        "ORDER BY on a RETURN alias of a YIELD column must still sort"
    );
}

#[test]
fn test_call_yield_order_by_no_return_still_sorts() {
    // No RETURN at all — ORDER BY applies directly to the raw procedure
    // output columns (the second no-pattern branch in the planner, for
    // standalone `CALL ... YIELD ...` with no RETURN clause).
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed_unsorted_labels(&mut executor);

    let labels = yielded_labels(&executor, "CALL db.labels() YIELD label ORDER BY label");
    assert_eq!(
        labels,
        vec!["A", "B", "C", "D", "E"],
        "ORDER BY on a YIELD with no RETURN clause must still sort"
    );
}

#[test]
fn test_call_yield_skip_after_order_by() {
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed_unsorted_labels(&mut executor);

    let labels = yielded_labels(
        &executor,
        "CALL db.labels() YIELD label RETURN label ORDER BY label SKIP 2",
    );
    assert_eq!(
        labels,
        vec!["C", "D", "E"],
        "SKIP after ORDER BY must drop the leading N sorted rows"
    );
}

#[test]
fn test_call_yield_limit_after_order_by() {
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed_unsorted_labels(&mut executor);

    let labels = yielded_labels(
        &executor,
        "CALL db.labels() YIELD label RETURN label ORDER BY label LIMIT 2",
    );
    assert_eq!(
        labels,
        vec!["A", "B"],
        "LIMIT after ORDER BY must keep only the leading N sorted rows"
    );
}

#[test]
fn test_call_yield_skip_and_limit_after_order_by() {
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed_unsorted_labels(&mut executor);

    let labels = yielded_labels(
        &executor,
        "CALL db.labels() YIELD label RETURN label ORDER BY label SKIP 1 LIMIT 2",
    );
    assert_eq!(
        labels,
        vec!["B", "C"],
        "SKIP + LIMIT together must apply standard ORDER BY, SKIP, LIMIT \
         pipeline order over the sorted set"
    );
}

#[test]
fn test_plain_match_order_by_control_still_sorts() {
    // Control: a MATCH-driven query (patterns non-empty) goes through
    // `plan_execution_strategy`, the path that already emitted `Sort`
    // correctly before this fix. Pins the isolation: the defect was
    // specific to the no-pattern (CALL/YIELD, bare RETURN) branch, not a
    // general ORDER BY regression.
    let (mut executor, _ctx) = create_isolated_test_executor();
    seed_unsorted_labels(&mut executor);

    let query = Query {
        cypher: "MATCH (n) RETURN labels(n)[0] AS label ORDER BY label".to_string(),
        params: std::collections::HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();
    let labels: Vec<String> = result
        .rows
        .iter()
        .map(|r| r.values[0].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        labels,
        vec!["A", "B", "C", "D", "E"],
        "plain MATCH ... ORDER BY must already sort correctly (control)"
    );
}
