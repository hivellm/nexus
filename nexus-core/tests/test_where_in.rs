/// Test WHERE IN operator after fixing label_id=0 bug
use nexus_core::{Engine, Error};

#[test]
fn test_where_in_operator() -> Result<(), Error> {
    let temp_dir = tempfile::tempdir().map_err(Error::Io)?;
    let mut engine = Engine::with_data_dir(temp_dir.path())?;

    // Create exactly 3 nodes
    engine.execute_cypher("CREATE (:Person {name: 'Alice'})")?;
    engine.execute_cypher("CREATE (:Person {name: 'Bob'})")?;
    engine.execute_cypher("CREATE (:Person {name: 'Charlie'})")?;

    // Query with IN operator - should return exactly 2 nodes (Alice and Bob)
    let result = engine.execute_cypher(
        "MATCH (n:Person) WHERE n.name IN ['Alice', 'Bob'] RETURN count(n) AS count",
    )?;

    let count = result.rows[0].values[0].as_i64().unwrap();
    assert_eq!(count, 2, "WHERE IN should return 2 nodes, got {}", count);

    Ok(())
}

#[test]
fn test_where_in_empty_list() -> Result<(), Error> {
    let temp_dir = tempfile::tempdir().map_err(Error::Io)?;
    let mut engine = Engine::with_data_dir(temp_dir.path())?;

    engine.execute_cypher("CREATE (:Person {name: 'Alice'})")?;
    engine.execute_cypher("CREATE (:Person {name: 'Bob'})")?;

    // Query with empty IN list - should return 0 nodes
    let result =
        engine.execute_cypher("MATCH (n:Person) WHERE n.name IN [] RETURN count(n) AS count")?;

    let count = result.rows[0].values[0].as_i64().unwrap();
    assert_eq!(
        count, 0,
        "Empty IN list should return 0 nodes, got {}",
        count
    );

    Ok(())
}
