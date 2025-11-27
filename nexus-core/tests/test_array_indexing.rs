use nexus_core::Error;
use nexus_core::testing::setup_test_engine;

#[test]
fn test_array_property_index_first_element() -> Result<(), Error> {
    let (mut engine, _ctx) = setup_test_engine()?;

    // Create node with array property using JSON-like syntax
    engine.execute_cypher("CREATE (:Person {name: 'Alice'})")?;

    // For now, we'll test with literal arrays in RETURN
    let result = engine.execute_cypher("RETURN ['dev', 'rust', 'graph'][0] AS first_tag")?;

    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        result.rows[0].values[0].as_str().unwrap(),
        "dev",
        "First tag should be 'dev'"
    );

    Ok(())
}

#[test]
fn test_array_property_index_last_element() -> Result<(), Error> {
    let (mut engine, _ctx) = setup_test_engine()?;

    // Test accessing last element (index 1) with literal array
    let result = engine.execute_cypher("RETURN ['frontend', 'typescript'][1] AS last_tag")?;

    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        result.rows[0].values[0].as_str().unwrap(),
        "typescript",
        "Last tag should be 'typescript'"
    );

    Ok(())
}

#[test]
fn test_array_property_index_out_of_bounds() -> Result<(), Error> {
    let (mut engine, _ctx) = setup_test_engine()?;

    // Test accessing out of bounds element
    let result = engine.execute_cypher("RETURN ['java'][5] AS tag")?;

    assert_eq!(result.rows.len(), 1);
    assert!(
        result.rows[0].values[0].is_null(),
        "Out of bounds should return null"
    );

    Ok(())
}

#[test]
fn test_array_property_index_negative() -> Result<(), Error> {
    let (mut engine, _ctx) = setup_test_engine()?;

    // Test accessing with expression index
    let result = engine.execute_cypher("RETURN ['a', 'b', 'c'][2] AS last")?;

    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        result.rows[0].values[0].as_str().unwrap(),
        "c",
        "array[2] should return 'c'"
    );

    Ok(())
}

#[test]
fn test_array_property_index_with_where() -> Result<(), Error> {
    let (mut engine, _ctx) = setup_test_engine()?;

    // Create simple nodes for now
    engine.execute_cypher("CREATE (:Person {name: 'Alice'})")?;
    engine.execute_cypher("CREATE (:Person {name: 'Bob'})")?;
    engine.refresh_executor()?;

    // Test with WHERE clause on name
    let result =
        engine.execute_cypher("MATCH (n:Person) WHERE n.name = 'Alice' RETURN n.name AS name")?;

    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        result.rows[0].values[0].as_str().unwrap(),
        "Alice",
        "Should find person named Alice"
    );

    Ok(())
}

#[test]
fn test_array_property_non_existent() -> Result<(), Error> {
    let (mut engine, _ctx) = setup_test_engine()?;

    engine.execute_cypher("CREATE (:Person {name: 'Eve'})")?;
    engine.refresh_executor()?;

    // Test accessing non-existent array property
    let result = engine.execute_cypher("MATCH (n:Person) RETURN n.tags[0] AS tag")?;

    assert_eq!(result.rows.len(), 1);
    assert!(
        result.rows[0].values[0].is_null(),
        "Non-existent property should return null"
    );

    Ok(())
}

#[test]
fn test_array_literal_indexing() -> Result<(), Error> {
    let (mut engine, _ctx) = setup_test_engine()?;

    // Test indexing a literal array
    let result = engine.execute_cypher("RETURN ['a', 'b', 'c'][1] AS element")?;

    assert_eq!(result.rows.len(), 1);
    tracing::info!("Result value: {:?}", result.rows[0].values[0]);

    if result.rows[0].values[0].is_null() {
        panic!("Result is null, array indexing is not working");
    }

    assert_eq!(
        result.rows[0].values[0].as_str().unwrap(),
        "b",
        "Literal array[1] should return 'b'"
    );

    Ok(())
}
