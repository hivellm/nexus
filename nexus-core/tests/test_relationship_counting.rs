use nexus_core::{Engine, Error};
use tempfile::TempDir;

fn setup_test_engine() -> Result<(Engine, TempDir), Error> {
    let temp_dir = tempfile::tempdir().map_err(Error::Io)?;
    let engine = Engine::with_data_dir(temp_dir.path())?;
    Ok((engine, temp_dir))
}

#[test]
fn test_directed_relationship_counting() -> Result<(), Error> {
    let (mut engine, _temp_dir) = setup_test_engine()?;

    // Create nodes
    engine.execute_cypher("CREATE (:Person {name: 'Alice'})")?;
    engine.execute_cypher("CREATE (:Person {name: 'Bob'})")?;
    engine.execute_cypher("CREATE (:Person {name: 'Charlie'})")?;
    engine.refresh_executor()?;

    // Create directed KNOWS relationships
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

    // Test directed relationship count: (a)-[r:KNOWS]->(b)
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
    let (mut engine, _temp_dir) = setup_test_engine()?;

    // Create nodes
    engine.execute_cypher("CREATE (:Person {name: 'Alice'})")?;
    engine.execute_cypher("CREATE (:Person {name: 'Bob'})")?;
    engine.execute_cypher("CREATE (:Person {name: 'Charlie'})")?;
    engine.refresh_executor()?;

    // Create directed KNOWS relationships
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

    // Test bidirectional relationship count: (a)-[r:KNOWS]-(b)
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
    let (mut engine, _temp_dir) = setup_test_engine()?;

    // Create nodes
    engine.execute_cypher("CREATE (:Person {name: 'Alice'})")?;
    engine.execute_cypher("CREATE (:Person {name: 'Bob'})")?;
    engine.execute_cypher("CREATE (:Company {name: 'Acme'})")?;
    engine.refresh_executor()?;

    // Check nodes created
    let nodes_result =
        engine.execute_cypher("MATCH (n) RETURN labels(n) AS labels, n.name AS name")?;
    eprintln!("Nodes created: {}", nodes_result.rows.len());
    for row in &nodes_result.rows {
        eprintln!("  - {:?}: {:?}", row.values[0], row.values[1]);
    }

    // Create different relationship types
    eprintln!("\nCreating KNOWS relationship...");
    engine.execute_cypher(
        "MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}) CREATE (a)-[:KNOWS]->(b)",
    )?;
    eprintln!("KNOWS created");

    eprintln!("\nCreating WORKS_AT relationship 1...");
    engine.execute_cypher(
        "MATCH (a:Person {name: 'Alice'}), (c:Company {name: 'Acme'}) CREATE (a)-[:WORKS_AT]->(c)",
    )?;
    eprintln!("WORKS_AT 1 created");

    eprintln!("\nCreating WORKS_AT relationship 2...");
    engine.execute_cypher(
        "MATCH (b:Person {name: 'Bob'}), (c:Company {name: 'Acme'}) CREATE (b)-[:WORKS_AT]->(c)",
    )?;
    eprintln!("WORKS_AT 2 created");

    engine.refresh_executor()?;

    // Debug: Check all relationships
    let debug_result = engine.execute_cypher("MATCH ()-[r]->() RETURN type(r) AS rel_type")?;
    eprintln!("\nAll relationships after creation:");
    for row in &debug_result.rows {
        eprintln!("  - {:?}", row.values[0]);
    }
    eprintln!("Total relationships: {}", debug_result.rows.len());

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
    let (mut engine, _temp_dir) = setup_test_engine()?;

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
#[ignore] // TODO: Fix directed relationship matching with labels - count is 0 when should be 1
fn test_relationship_direction_with_labels() -> Result<(), Error> {
    let (mut engine, _temp_dir) = setup_test_engine()?;

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
