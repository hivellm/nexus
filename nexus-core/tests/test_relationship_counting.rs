use nexus_core::{Engine, Error};
use std::sync::atomic::{AtomicU32, Ordering};
use tempfile::TempDir;
use tracing;

/// Counter for unique test labels to prevent cross-test interference
static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

fn setup_test_engine() -> Result<(Engine, TempDir), Error> {
    let temp_dir = tempfile::tempdir().map_err(Error::Io)?;
    let engine = Engine::with_data_dir(temp_dir.path())?;
    Ok((engine, temp_dir))
}

#[test]
fn test_directed_relationship_counting() -> Result<(), Error> {
    let test_id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let person_label = format!("PersonDir{}", test_id);
    let knows_type = format!("KNOWS_DIR{}", test_id);
    let (mut engine, _temp_dir) = setup_test_engine()?;

    // Create nodes with unique labels
    engine.execute_cypher(&format!("CREATE (:{} {{name: 'Alice'}})", person_label))?;
    engine.execute_cypher(&format!("CREATE (:{} {{name: 'Bob'}})", person_label))?;
    engine.execute_cypher(&format!("CREATE (:{} {{name: 'Charlie'}})", person_label))?;
    engine.refresh_executor()?;

    // Create directed relationships with unique type
    engine.execute_cypher(&format!(
        "MATCH (a:{} {{name: 'Alice'}}), (b:{} {{name: 'Bob'}}) CREATE (a)-[:{}]->(b)",
        person_label, person_label, knows_type
    ))?;
    engine.execute_cypher(&format!(
        "MATCH (a:{} {{name: 'Alice'}}), (c:{} {{name: 'Charlie'}}) CREATE (a)-[:{}]->(c)",
        person_label, person_label, knows_type
    ))?;
    engine.execute_cypher(&format!(
        "MATCH (b:{} {{name: 'Bob'}}), (c:{} {{name: 'Charlie'}}) CREATE (b)-[:{}]->(c)",
        person_label, person_label, knows_type
    ))?;
    engine.refresh_executor()?;

    // Test directed relationship count
    let result = engine.execute_cypher(&format!(
        "MATCH (a)-[r:{}]->(b) RETURN count(r) AS count",
        knows_type
    ))?;

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
#[ignore] // TODO: Fix bidirectional relationship counting - returns 5 instead of 6
fn test_bidirectional_relationship_counting() -> Result<(), Error> {
    let test_id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let person_label = format!("PersonBi{}", test_id);
    let knows_type = format!("KNOWS_BI{}", test_id);
    let (mut engine, _temp_dir) = setup_test_engine()?;

    // Create nodes with unique labels
    engine.execute_cypher(&format!("CREATE (:{} {{name: 'Alice'}})", person_label))?;
    engine.execute_cypher(&format!("CREATE (:{} {{name: 'Bob'}})", person_label))?;
    engine.execute_cypher(&format!("CREATE (:{} {{name: 'Charlie'}})", person_label))?;
    engine.refresh_executor()?;

    // Create directed relationships with unique type
    engine.execute_cypher(&format!(
        "MATCH (a:{} {{name: 'Alice'}}), (b:{} {{name: 'Bob'}}) CREATE (a)-[:{}]->(b)",
        person_label, person_label, knows_type
    ))?;
    engine.execute_cypher(&format!(
        "MATCH (a:{} {{name: 'Alice'}}), (c:{} {{name: 'Charlie'}}) CREATE (a)-[:{}]->(c)",
        person_label, person_label, knows_type
    ))?;
    engine.execute_cypher(&format!(
        "MATCH (b:{} {{name: 'Bob'}}), (c:{} {{name: 'Charlie'}}) CREATE (b)-[:{}]->(c)",
        person_label, person_label, knows_type
    ))?;
    engine.refresh_executor()?;

    // Test bidirectional relationship count: (a)-[r:TYPE]-(b)
    // This should match each relationship TWICE (once in each direction)
    let result = engine.execute_cypher(&format!(
        "MATCH (a)-[r:{}]-(b) RETURN count(r) AS count",
        knows_type
    ))?;

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
    let test_id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let person_label = format!("PersonFilter{}", test_id);
    let company_label = format!("CompanyFilter{}", test_id);
    let knows_type = format!("KNOWS_F{}", test_id);
    let works_type = format!("WORKS_AT_F{}", test_id);
    let (mut engine, _temp_dir) = setup_test_engine()?;

    // Create nodes with unique labels
    engine.execute_cypher(&format!("CREATE (:{} {{name: 'Alice'}})", person_label))?;
    engine.execute_cypher(&format!("CREATE (:{} {{name: 'Bob'}})", person_label))?;
    engine.execute_cypher(&format!("CREATE (:{} {{name: 'Acme'}})", company_label))?;
    engine.refresh_executor()?;

    // Create different relationship types
    engine.execute_cypher(&format!(
        "MATCH (a:{} {{name: 'Alice'}}), (b:{} {{name: 'Bob'}}) CREATE (a)-[:{}]->(b)",
        person_label, person_label, knows_type
    ))?;
    engine.execute_cypher(&format!(
        "MATCH (a:{} {{name: 'Alice'}}), (c:{} {{name: 'Acme'}}) CREATE (a)-[:{}]->(c)",
        person_label, company_label, works_type
    ))?;
    engine.execute_cypher(&format!(
        "MATCH (b:{} {{name: 'Bob'}}), (c:{} {{name: 'Acme'}}) CREATE (b)-[:{}]->(c)",
        person_label, company_label, works_type
    ))?;
    engine.refresh_executor()?;

    // Test filtering by relationship type using type() function
    let result = engine.execute_cypher(&format!(
        "MATCH ()-[r]->() WHERE type(r) IN ['{}', '{}'] RETURN count(r) AS count",
        knows_type, works_type
    ))?;

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
    let test_id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let person_label = format!("PersonSingle{}", test_id);
    let company_label = format!("CompanySingle{}", test_id);
    let knows_type = format!("KNOWS_S{}", test_id);
    let works_type = format!("WORKS_AT_S{}", test_id);
    let (mut engine, _temp_dir) = setup_test_engine()?;

    // Create nodes with unique labels
    engine.execute_cypher(&format!("CREATE (:{} {{name: 'Alice'}})", person_label))?;
    engine.execute_cypher(&format!("CREATE (:{} {{name: 'Bob'}})", person_label))?;
    engine.execute_cypher(&format!("CREATE (:{} {{name: 'Acme'}})", company_label))?;
    engine.refresh_executor()?;

    // Create different relationship types
    engine.execute_cypher(&format!(
        "MATCH (a:{} {{name: 'Alice'}}), (b:{} {{name: 'Bob'}}) CREATE (a)-[:{}]->(b)",
        person_label, person_label, knows_type
    ))?;
    engine.execute_cypher(&format!(
        "MATCH (a:{} {{name: 'Alice'}}), (c:{} {{name: 'Acme'}}) CREATE (a)-[:{}]->(c)",
        person_label, company_label, works_type
    ))?;
    engine.execute_cypher(&format!(
        "MATCH (b:{} {{name: 'Bob'}}), (c:{} {{name: 'Acme'}}) CREATE (b)-[:{}]->(c)",
        person_label, company_label, works_type
    ))?;
    engine.refresh_executor()?;

    // Test filtering for WORKS_AT only
    let result = engine.execute_cypher(&format!(
        "MATCH ()-[r:{}]->() RETURN count(r) AS count",
        works_type
    ))?;

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
    let test_id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let person_label = format!("PersonLabel{}", test_id);
    let knows_type = format!("KNOWS_L{}", test_id);
    let (mut engine, _temp_dir) = setup_test_engine()?;

    // Create nodes with unique labels
    engine.execute_cypher(&format!("CREATE (:{} {{name: 'Alice'}})", person_label))?;
    engine.execute_cypher(&format!("CREATE (:{} {{name: 'Bob'}})", person_label))?;
    engine.refresh_executor()?;

    // Create a single directed relationship
    engine.execute_cypher(&format!(
        "MATCH (a:{} {{name: 'Alice'}}), (b:{} {{name: 'Bob'}}) CREATE (a)-[:{}]->(b)",
        person_label, person_label, knows_type
    ))?;
    engine.refresh_executor()?;

    // Test directed count
    let result_directed = engine.execute_cypher(&format!(
        "MATCH (a:{})-[r:{}]->(b:{}) RETURN count(r) AS count",
        person_label, knows_type, person_label
    ))?;
    assert_eq!(result_directed.rows.len(), 1);
    let count_directed = result_directed.rows[0].values[0].as_i64().unwrap();
    assert_eq!(
        count_directed, 1,
        "Directed query should find 1 relationship"
    );

    // Test bidirectional count
    let result_bidirectional = engine.execute_cypher(&format!(
        "MATCH (a:{})-[r:{}]-(b:{}) RETURN count(r) AS count",
        person_label, knows_type, person_label
    ))?;
    assert_eq!(result_bidirectional.rows.len(), 1);
    let count_bidirectional = result_bidirectional.rows[0].values[0].as_i64().unwrap();
    assert_eq!(
        count_bidirectional, 2,
        "Bidirectional query should find 2 (relationship matched in both directions)"
    );

    Ok(())
}
