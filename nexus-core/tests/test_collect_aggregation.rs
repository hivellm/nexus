use nexus_core::Error;
use nexus_core::testing::setup_test_engine;

#[test]
fn test_collect_with_head() -> Result<(), Error> {
    let (mut engine, _ctx) = setup_test_engine()?;

    // Create multiple nodes
    engine.execute_cypher("CREATE (:Person {name: 'Alice'})")?;
    engine.execute_cypher("CREATE (:Person {name: 'Bob'})")?;
    engine.execute_cypher("CREATE (:Person {name: 'Charlie'})")?;
    engine.execute_cypher("CREATE (:Person {name: 'Dave'})")?;
    engine.execute_cypher("CREATE (:Person {name: 'Eve'})")?;
    engine.refresh_executor()?;

    // Test head(collect(n.name)) - should return 1 row with first name
    let result =
        engine.execute_cypher("MATCH (n:Person) RETURN head(collect(n.name)) AS first_name")?;

    // Neo4j returns 1 row, not 5
    assert_eq!(
        result.rows.len(),
        1,
        "head(collect()) should return 1 row, got {}",
        result.rows.len()
    );

    // The result should be a single string (first name)
    let first_name = result.rows[0].values[0].as_str();
    assert!(
        first_name.is_some(),
        "First name should be a string, got: {:?}",
        result.rows[0].values[0]
    );

    Ok(())
}

#[test]
fn test_collect_with_tail() -> Result<(), Error> {
    let (mut engine, _ctx) = setup_test_engine()?;

    engine.execute_cypher("CREATE (:Person {name: 'Alice'})")?;
    engine.execute_cypher("CREATE (:Person {name: 'Bob'})")?;
    engine.execute_cypher("CREATE (:Person {name: 'Charlie'})")?;
    engine.refresh_executor()?;

    // Test tail(collect(n.name)) - should return 1 row with array of remaining names
    let result =
        engine.execute_cypher("MATCH (n:Person) RETURN tail(collect(n.name)) AS remaining")?;

    assert_eq!(
        result.rows.len(),
        1,
        "tail(collect()) should return 1 row, got {}",
        result.rows.len()
    );

    // The result should be an array
    let remaining = result.rows[0].values[0].as_array();
    assert!(
        remaining.is_some(),
        "tail(collect()) should return an array, got: {:?}",
        result.rows[0].values[0]
    );

    Ok(())
}

#[test]
fn test_collect_with_reverse() -> Result<(), Error> {
    let (mut engine, _ctx) = setup_test_engine()?;

    engine.execute_cypher("CREATE (:Person {name: 'Alice'})")?;
    engine.execute_cypher("CREATE (:Person {name: 'Bob'})")?;
    engine.execute_cypher("CREATE (:Person {name: 'Charlie'})")?;
    engine.refresh_executor()?;

    // Test reverse(collect(n.name)) - should return 1 row with reversed array
    let result =
        engine.execute_cypher("MATCH (n:Person) RETURN reverse(collect(n.name)) AS reversed")?;

    assert_eq!(
        result.rows.len(),
        1,
        "reverse(collect()) should return 1 row, got {}",
        result.rows.len()
    );

    // The result should be an array
    let reversed = result.rows[0].values[0].as_array();
    assert!(
        reversed.is_some(),
        "reverse(collect()) should return an array, got: {:?}",
        result.rows[0].values[0]
    );

    Ok(())
}

#[test]
fn test_collect_without_nesting() -> Result<(), Error> {
    let (mut engine, _ctx) = setup_test_engine()?;

    engine.execute_cypher("CREATE (:Person {name: 'Alice'})")?;
    engine.execute_cypher("CREATE (:Person {name: 'Bob'})")?;
    engine.execute_cypher("CREATE (:Person {name: 'Charlie'})")?;
    engine.refresh_executor()?;

    // Test plain collect(n.name) - should return 1 row with array of all names
    let result = engine.execute_cypher("MATCH (n:Person) RETURN collect(n.name) AS names")?;

    assert_eq!(
        result.rows.len(),
        1,
        "collect() should return 1 row, got {}",
        result.rows.len()
    );

    // The result should be an array with 3 elements
    let names = result.rows[0].values[0].as_array().unwrap();
    assert_eq!(
        names.len(),
        3,
        "collect() should return array with 3 names, got {}",
        names.len()
    );

    Ok(())
}

#[test]
fn test_count_all() -> Result<(), Error> {
    let (mut engine, _ctx) = setup_test_engine()?;

    engine.execute_cypher("CREATE (:Person {name: 'Alice'})")?;
    engine.execute_cypher("CREATE (:Person {name: 'Bob'})")?;
    engine.execute_cypher("CREATE (:Person {name: 'Charlie'})")?;
    engine.refresh_executor()?;

    // Test count(*) - should return 1 row with count
    let result = engine.execute_cypher("MATCH (n:Person) RETURN count(*) AS count")?;

    assert_eq!(
        result.rows.len(),
        1,
        "count(*) should return 1 row, got {}",
        result.rows.len()
    );

    let count = result.rows[0].values[0].as_i64().unwrap();
    assert_eq!(count, 3, "count(*) should return 3, got {}", count);

    Ok(())
}
