use nexus_core::{Engine, Error};
use tempfile::TempDir;

fn setup_test_engine() -> Result<(Engine, TempDir), Error> {
    let temp_dir = tempfile::tempdir().map_err(Error::Io)?;
    let engine = Engine::with_data_dir(temp_dir.path())?;
    Ok((engine, temp_dir))
}

#[test]
fn test_simple_relationship_creation() -> Result<(), Error> {
    let (mut engine, _temp_dir) = setup_test_engine()?;

    // Create nodes one by one (without RETURN to avoid duplication)
    eprintln!("Creating Alice...");
    engine.execute_cypher("CREATE (:Person {name: 'Alice'})")?;

    eprintln!("\nCreating Bob...");
    engine.execute_cypher("CREATE (:Person {name: 'Bob'})")?;

    eprintln!("\nCreating Company...");
    engine.execute_cypher("CREATE (:Company {name: 'Acme'})")?;

    engine.refresh_executor()?;

    // Try different queries to find the Company node
    eprintln!("\n--- Query 1: MATCH (n) ---");
    let nodes = engine
        .execute_cypher("MATCH (n) RETURN id(n) AS id, n.name AS name, labels(n) AS labels")?;
    for row in &nodes.rows {
        eprintln!(
            "  - ID: {:?}, Name: {:?}, Labels: {:?}",
            row.values[0], row.values[1], row.values[2]
        );
    }
    eprintln!("Total from MATCH (n): {}", nodes.rows.len());

    eprintln!("\n--- Query 2: MATCH (c:Company) ---");
    let companies = engine.execute_cypher("MATCH (c:Company) RETURN c.name AS name")?;
    for row in &companies.rows {
        eprintln!("  - Company: {:?}", row.values[0]);
    }
    eprintln!("Total companies: {}", companies.rows.len());

    eprintln!("\n--- Query 3: MATCH (p:Person) ---");
    let persons = engine.execute_cypher("MATCH (p:Person) RETURN p.name AS name")?;
    for row in &persons.rows {
        eprintln!("  - Person: {:?}", row.values[0]);
    }
    eprintln!("Total persons: {}", persons.rows.len());

    assert_eq!(
        nodes.rows.len(),
        3,
        "Should have 3 nodes total, got {}",
        nodes.rows.len()
    );
    assert_eq!(
        companies.rows.len(),
        1,
        "Should have 1 company, got {}",
        companies.rows.len()
    );
    assert_eq!(
        persons.rows.len(),
        2,
        "Should have 2 persons, got {}",
        persons.rows.len()
    );

    Ok(())
}
