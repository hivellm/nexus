use nexus_core::Error;
use nexus_core::testing::setup_isolated_test_engine;

#[test]
fn test_directed_relationship_counting() -> Result<(), Error> {
    let (mut engine, _ctx) = setup_isolated_test_engine()?;

    // Create nodes
    engine.execute_cypher("CREATE (:Person {name: 'Alice'})")?;
    engine.execute_cypher("CREATE (:Person {name: 'Bob'})")?;
    engine.execute_cypher("CREATE (:Person {name: 'Charlie'})")?;
    engine.refresh_executor()?;

    // Create directed relationships
    engine.execute_cypher(
        "MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}) CREATE (a)-[:KNOWS]->(b)",
    )?;
    engine.execute_cypher(
        "MATCH (a:Person {name: 'Alice'}), (c:Person {name: 'Charlie'}) CREATE (a)-[:KNOWS]->(c)",
    )?;
    engine.execute_cypher(
        "MATCH (b:Person {name: 'Bob'}), (c:Person {name: 'Charlie'}) CREATE (b)-[:KNOWS]->(c)",
    )?;
    engine.refresh_executor()?;

    // Test directed relationship count
    let result = engine.execute_cypher("MATCH (a)-[r:KNOWS]->(b) RETURN count(r) AS count")?;

    assert_eq!(result.rows.len(), 1, "Should return 1 row");
    let count = result.rows[0].values[0].as_i64().unwrap();
    assert_eq!(
        count, 3,
        "Directed KNOWS relationships should be 3, got {}",
        count
    );

    Ok(())
}

#[test]
fn test_bidirectional_relationship_counting() -> Result<(), Error> {
    let (mut engine, _ctx) = setup_isolated_test_engine()?;

    // Create nodes
    engine.execute_cypher("CREATE (:Person {name: 'Alice'})")?;
    engine.execute_cypher("CREATE (:Person {name: 'Bob'})")?;
    engine.execute_cypher("CREATE (:Person {name: 'Charlie'})")?;
    engine.refresh_executor()?;

    // Create directed relationships
    engine.execute_cypher(
        "MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}) CREATE (a)-[:KNOWS]->(b)",
    )?;
    engine.execute_cypher(
        "MATCH (a:Person {name: 'Alice'}), (c:Person {name: 'Charlie'}) CREATE (a)-[:KNOWS]->(c)",
    )?;
    engine.execute_cypher(
        "MATCH (b:Person {name: 'Bob'}), (c:Person {name: 'Charlie'}) CREATE (b)-[:KNOWS]->(c)",
    )?;
    engine.refresh_executor()?;

    // Test bidirectional relationship count: (a)-[r:TYPE]-(b)
    // This should match each relationship TWICE (once in each direction)
    let result = engine.execute_cypher("MATCH (a)-[r:KNOWS]-(b) RETURN count(r) AS count")?;

    assert_eq!(result.rows.len(), 1, "Should return 1 row");
    let count = result.rows[0].values[0].as_i64().unwrap();
    assert_eq!(
        count, 6,
        "Bidirectional KNOWS relationships should be 6 (3 rels * 2 directions), got {}",
        count
    );

    Ok(())
}

#[test]
fn test_relationship_type_filtering() -> Result<(), Error> {
    let (mut engine, _ctx) = setup_isolated_test_engine()?;

    // Create nodes
    engine.execute_cypher("CREATE (:Person {name: 'Alice'})")?;
    engine.execute_cypher("CREATE (:Person {name: 'Bob'})")?;
    engine.execute_cypher("CREATE (:Company {name: 'Acme'})")?;
    engine.refresh_executor()?;

    // Create different relationship types
    engine.execute_cypher(
        "MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}) CREATE (a)-[:KNOWS]->(b)",
    )?;
    engine.execute_cypher(
        "MATCH (a:Person {name: 'Alice'}), (c:Company {name: 'Acme'}) CREATE (a)-[:WORKS_AT]->(c)",
    )?;
    engine.execute_cypher(
        "MATCH (b:Person {name: 'Bob'}), (c:Company {name: 'Acme'}) CREATE (b)-[:WORKS_AT]->(c)",
    )?;
    engine.refresh_executor()?;

    // Test filtering by relationship type using type() function
    let result = engine.execute_cypher(
        "MATCH ()-[r]->() WHERE type(r) IN ['KNOWS', 'WORKS_AT'] RETURN count(r) AS count",
    )?;

    assert_eq!(result.rows.len(), 1, "Should return 1 row");
    let count = result.rows[0].values[0].as_i64().unwrap();
    assert_eq!(
        count, 3,
        "Should count all relationships (1 KNOWS + 2 WORKS_AT), got {}",
        count
    );

    Ok(())
}

#[test]
fn test_relationship_type_filtering_single_type() -> Result<(), Error> {
    let (mut engine, _ctx) = setup_isolated_test_engine()?;

    // Create nodes
    engine.execute_cypher("CREATE (:Person {name: 'Alice'})")?;
    engine.execute_cypher("CREATE (:Person {name: 'Bob'})")?;
    engine.execute_cypher("CREATE (:Company {name: 'Acme'})")?;
    engine.refresh_executor()?;

    // Create different relationship types
    engine.execute_cypher(
        "MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}) CREATE (a)-[:KNOWS]->(b)",
    )?;
    engine.execute_cypher(
        "MATCH (a:Person {name: 'Alice'}), (c:Company {name: 'Acme'}) CREATE (a)-[:WORKS_AT]->(c)",
    )?;
    engine.execute_cypher(
        "MATCH (b:Person {name: 'Bob'}), (c:Company {name: 'Acme'}) CREATE (b)-[:WORKS_AT]->(c)",
    )?;
    engine.refresh_executor()?;

    // Test filtering for WORKS_AT only
    let result = engine.execute_cypher("MATCH ()-[r:WORKS_AT]->() RETURN count(r) AS count")?;

    assert_eq!(result.rows.len(), 1, "Should return 1 row");
    let count = result.rows[0].values[0].as_i64().unwrap();
    assert_eq!(
        count, 2,
        "Should count only WORKS_AT relationships, got {}",
        count
    );

    Ok(())
}

#[test]
fn test_relationship_direction_with_labels() -> Result<(), Error> {
    let (mut engine, _ctx) = setup_isolated_test_engine()?;

    // Create nodes
    engine.execute_cypher("CREATE (:Person {name: 'Alice'})")?;
    engine.execute_cypher("CREATE (:Person {name: 'Bob'})")?;
    engine.refresh_executor()?;

    // Create a single directed relationship
    engine.execute_cypher(
        "MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}) CREATE (a)-[:KNOWS]->(b)",
    )?;
    engine.refresh_executor()?;

    // Test directed count
    let result_directed =
        engine.execute_cypher("MATCH (a:Person)-[r:KNOWS]->(b:Person) RETURN count(r) AS count")?;
    assert_eq!(result_directed.rows.len(), 1);
    let count_directed = result_directed.rows[0].values[0].as_i64().unwrap();
    assert_eq!(
        count_directed, 1,
        "Directed query should find 1 relationship"
    );

    // Test bidirectional count
    let result_bidirectional =
        engine.execute_cypher("MATCH (a:Person)-[r:KNOWS]-(b:Person) RETURN count(r) AS count")?;
    assert_eq!(result_bidirectional.rows.len(), 1);
    let count_bidirectional = result_bidirectional.rows[0].values[0].as_i64().unwrap();
    assert_eq!(
        count_bidirectional, 2,
        "Bidirectional query should find 2 (relationship matched in both directions)"
    );

    Ok(())
}
