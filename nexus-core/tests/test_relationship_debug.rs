// BUG DISCOVERED: CREATE Duplication Bug #2
//
// SYMPTOMS:
// - CREATE statements say they create nodes with IDs: 1, 2, 3
// - MATCH returns 4 nodes with IDs: 0, 1, 2, 3
// - Node data is duplicated: "Acme" appears in both ID 2 and 3
//
// ROOT CAUSE:
// - Storage assigns IDs starting from 1 (next_node_id starts at 1 after first node)
// - But MATCH returns IDs offset by -1 (showing ID 0 instead of 1)
// - This creates a mismatch where:
//   - Storage ID 1 (Alice) → MATCH returns as ID 0
//   - Storage ID 2 (Bob) → MATCH returns as ID 1
//   - Storage ID 3 (Company) → MATCH returns as ID 2
//   - Storage ID ??? → MATCH returns as ID 3 (DUPLICATE!)
//
// INVESTIGATION NEEDED:
// - Check read_node_as_value() function
// - Check id() function implementation
// - Check execute_all_nodes_scan() ID mapping
//
// FILES:
// - nexus-core/tests/test_relationship_debug.rs (this file)
// - nexus-core/src/executor/mod.rs (execute_create_pattern_internal, execute_all_nodes_scan)

use nexus_core::{Engine, Error};
use tempfile::TempDir;
use tracing;

fn setup_test_engine() -> Result<(Engine, TempDir), Error> {
    let temp_dir = tempfile::tempdir().map_err(Error::Io)?;
    let engine = Engine::with_data_dir(temp_dir.path())?;
    Ok((engine, temp_dir))
}

#[test]
fn test_simple_relationship_creation() -> Result<(), Error> {
    let (mut engine, _temp_dir) = setup_test_engine()?;

    // Check initial state
    tracing::info!("\n=== Initial State ===");
    let initial_nodes =
        engine.execute_cypher("MATCH (n) RETURN id(n) AS id, labels(n) AS labels")?;
    tracing::info!("Initial nodes: {}", initial_nodes.rows.len());
    for row in &initial_nodes.rows {
        tracing::info!("  - ID: {:?}, Labels: {:?}", row.values[0], row.values[1]);
    }

    // Create nodes one by one (without RETURN to avoid duplication)
    tracing::info!("\n=== Creating Nodes ===");
    tracing::info!("Creating Alice...");
    engine.execute_cypher("CREATE (:Person {name: 'Alice'})")?;

    tracing::info!("\nCreating Bob...");
    engine.execute_cypher("CREATE (:Person {name: 'Bob'})")?;

    tracing::info!("\nCreating Company...");
    engine.execute_cypher("CREATE (:Company {name: 'Acme'})")?;

    engine.refresh_executor()?;

    // Try different queries to find the Company node
    tracing::info!("\n=== Final State ===");
    tracing::info!("\n--- Query 1: MATCH (n) ---");
    let nodes = engine
        .execute_cypher("MATCH (n) RETURN id(n) AS id, n.name AS name, labels(n) AS labels")?;
    for row in &nodes.rows {
        tracing::info!(
            "  - ID: {:?}, Name: {:?}, Labels: {:?}",
            row.values[0],
            row.values[1],
            row.values[2]
        );
    }
    tracing::info!("Total from MATCH (n): {}", nodes.rows.len());

    tracing::info!("\n--- Query 2: MATCH (c:Company) ---");
    let companies = engine.execute_cypher("MATCH (c:Company) RETURN c.name AS name")?;
    for row in &companies.rows {
        tracing::info!("  - Company: {:?}", row.values[0]);
    }
    tracing::info!("Total companies: {}", companies.rows.len());

    tracing::info!("\n--- Query 3: MATCH (p:Person) ---");
    let persons = engine.execute_cypher("MATCH (p:Person) RETURN p.name AS name")?;
    for row in &persons.rows {
        tracing::info!("  - Person: {:?}", row.values[0]);
    }
    tracing::info!("Total persons: {}", persons.rows.len());

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
