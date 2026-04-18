/// Tests for null comparison operators (null = null, null <> null)
use nexus_core::Error;
use nexus_core::testing::setup_test_engine;

#[test]
fn test_null_equals_null_in_return() -> Result<(), Error> {
    let (mut engine, _ctx) = setup_test_engine()?;

    // In Neo4j, null = null returns null in RETURN clause
    let result = engine.execute_cypher("RETURN null = null AS null_eq_null")?;
    assert_eq!(result.rows.len(), 1);

    // Should return null (not true or false)
    assert!(
        result.rows[0].values[0].is_null(),
        "null = null should return null in RETURN clause"
    );

    Ok(())
}

#[test]
fn test_null_not_equals_null_in_return() -> Result<(), Error> {
    let (mut engine, _ctx) = setup_test_engine()?;

    // In Neo4j, null <> null returns null in RETURN clause
    let result = engine.execute_cypher("RETURN null <> null AS null_neq_null")?;
    assert_eq!(result.rows.len(), 1);

    // Should return null (not true or false)
    assert!(
        result.rows[0].values[0].is_null(),
        "null <> null should return null in RETURN clause"
    );

    Ok(())
}

#[test]
fn test_null_equals_null_in_where() -> Result<(), Error> {
    let (mut engine, _ctx) = setup_test_engine()?;

    // Create test data
    engine.execute_cypher("CREATE (n:Node {value: null})")?;
    engine.refresh_executor()?;

    // In Neo4j, null = null evaluates to false in WHERE clause
    let result = engine.execute_cypher("MATCH (n:Node) WHERE n.value = null RETURN n")?;

    // Should return no rows (null = null is false in WHERE)
    assert_eq!(
        result.rows.len(),
        0,
        "null = null should evaluate to false in WHERE clause"
    );

    Ok(())
}

#[test]
fn test_null_not_equals_null_in_where() -> Result<(), Error> {
    let (mut engine, _ctx) = setup_test_engine()?;

    // Create test data
    engine.execute_cypher("CREATE (n:Node {value: null})")?;
    engine.refresh_executor()?;

    // In Neo4j, null <> null evaluates to false in WHERE clause
    let result = engine.execute_cypher("MATCH (n:Node) WHERE n.value <> null RETURN n")?;

    // Should return no rows (null <> null is false in WHERE)
    assert_eq!(
        result.rows.len(),
        0,
        "null <> null should evaluate to false in WHERE clause"
    );

    Ok(())
}

#[test]
fn test_null_equals_value_in_return() -> Result<(), Error> {
    let (mut engine, _ctx) = setup_test_engine()?;

    // In Neo4j, null = value returns null
    let result = engine.execute_cypher("RETURN null = 5 AS null_eq_value")?;
    assert_eq!(result.rows.len(), 1);

    // Should return null
    assert!(
        result.rows[0].values[0].is_null(),
        "null = value should return null"
    );

    Ok(())
}

#[test]
fn test_value_equals_null_in_return() -> Result<(), Error> {
    let (mut engine, _ctx) = setup_test_engine()?;

    // In Neo4j, value = null returns null
    let result = engine.execute_cypher("RETURN 5 = null AS value_eq_null")?;
    assert_eq!(result.rows.len(), 1);

    // Should return null
    assert!(
        result.rows[0].values[0].is_null(),
        "value = null should return null"
    );

    Ok(())
}

#[test]
fn test_null_equals_value_in_where() -> Result<(), Error> {
    let (mut engine, _ctx) = setup_test_engine()?;

    // Create test data
    engine.execute_cypher("CREATE (n:Node {value: 5})")?;
    engine.refresh_executor()?;

    // In Neo4j, null = value evaluates to false in WHERE clause
    let result = engine.execute_cypher("MATCH (n:Node) WHERE null = n.value RETURN n")?;

    // Should return no rows
    assert_eq!(
        result.rows.len(),
        0,
        "null = value should evaluate to false in WHERE clause"
    );

    Ok(())
}

#[test]
fn test_is_null_operator() -> Result<(), Error> {
    let (mut engine, _ctx) = setup_test_engine()?;

    // Create test data
    engine.execute_cypher("CREATE (n:Node {value: null})")?;
    engine.execute_cypher("CREATE (n:Node {value: 5})")?;
    engine.refresh_executor()?;

    // Test IS NULL in WHERE
    let result =
        engine.execute_cypher("MATCH (n:Node) WHERE n.value IS NULL RETURN count(n) AS count")?;
    assert!(!result.rows.is_empty());
    let count = result.rows[0].values[0].as_i64().unwrap();
    assert!(count >= 1, "Should find at least one node with null value");

    // Test IS NOT NULL in WHERE
    let result = engine
        .execute_cypher("MATCH (n:Node) WHERE n.value IS NOT NULL RETURN count(n) AS count")?;
    assert!(!result.rows.is_empty());
    let count = result.rows[0].values[0].as_i64().unwrap();
    assert!(
        count >= 1,
        "Should find at least one node with non-null value"
    );

    Ok(())
}
