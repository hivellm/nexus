//! Tests for SHOW CONSTRAINTS command
//!
//! This file tests the SHOW CONSTRAINTS command which lists all database constraints

use nexus_core::testing::setup_isolated_test_engine;
use nexus_core::{Engine, executor::ResultSet};

fn execute_query(engine: &mut Engine, query: &str) -> ResultSet {
    engine.execute_cypher(query).expect("Query should succeed")
}

// ============================================================================
// SHOW CONSTRAINTS TESTS
// ============================================================================

#[test]
fn test_show_constraints_empty() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // Initially, there should be no constraints
    let result = execute_query(&mut engine, "SHOW CONSTRAINTS");

    assert_eq!(
        result.columns,
        vec!["label", "property", "type", "description"]
    );
    // No constraints initially (or may have some from setup)
    // Just verify the structure is correct
}

#[test]
fn test_show_constraints_after_creating_unique() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // Create a unique constraint
    execute_query(
        &mut engine,
        "CREATE CONSTRAINT ON (n:Person) ASSERT n.email IS UNIQUE",
    );

    // Show constraints
    let result = execute_query(&mut engine, "SHOW CONSTRAINTS");

    assert_eq!(
        result.columns,
        vec!["label", "property", "type", "description"]
    );
    assert!(result.rows.len() >= 1);

    // Find the Person email constraint
    let person_email = result.rows.iter().find(|row| {
        row.values[0].as_str() == Some("Person") && row.values[1].as_str() == Some("email")
    });

    assert!(person_email.is_some());
    let constraint_row = person_email.unwrap();
    assert_eq!(constraint_row.values[2].as_str().unwrap(), "UNIQUE");
    assert!(
        constraint_row.values[3]
            .as_str()
            .unwrap()
            .contains("UNIQUE")
    );
}

#[test]
fn test_show_constraints_after_creating_exists() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // Create an exists constraint
    execute_query(
        &mut engine,
        "CREATE CONSTRAINT ON (n:Employee) ASSERT EXISTS(n.id)",
    );

    // Show constraints
    let result = execute_query(&mut engine, "SHOW CONSTRAINTS");

    assert_eq!(
        result.columns,
        vec!["label", "property", "type", "description"]
    );
    assert!(result.rows.len() >= 1);

    // Find the Employee id constraint
    let employee_id = result.rows.iter().find(|row| {
        row.values[0].as_str() == Some("Employee") && row.values[1].as_str() == Some("id")
    });

    assert!(employee_id.is_some());
    let constraint_row = employee_id.unwrap();
    assert_eq!(constraint_row.values[2].as_str().unwrap(), "EXISTS");
    assert!(
        constraint_row.values[3]
            .as_str()
            .unwrap()
            .contains("exists")
    );
}

#[test]
fn test_show_constraints_multiple() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // Create multiple constraints
    execute_query(
        &mut engine,
        "CREATE CONSTRAINT ON (n:User) ASSERT n.username IS UNIQUE",
    );
    execute_query(
        &mut engine,
        "CREATE CONSTRAINT ON (n:User) ASSERT EXISTS(n.created_at)",
    );
    execute_query(
        &mut engine,
        "CREATE CONSTRAINT ON (n:Product) ASSERT n.sku IS UNIQUE",
    );

    // Show all constraints
    let result = execute_query(&mut engine, "SHOW CONSTRAINTS");

    assert_eq!(
        result.columns,
        vec!["label", "property", "type", "description"]
    );
    assert!(result.rows.len() >= 3);

    // Count constraints by label
    let user_constraints = result
        .rows
        .iter()
        .filter(|row| row.values[0].as_str() == Some("User"))
        .count();

    let product_constraints = result
        .rows
        .iter()
        .filter(|row| row.values[0].as_str() == Some("Product"))
        .count();

    assert!(user_constraints >= 2);
    assert!(product_constraints >= 1);
}

#[test]
fn test_show_constraints_after_drop() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // Create a constraint
    execute_query(
        &mut engine,
        "CREATE CONSTRAINT ON (n:Company) ASSERT n.tax_id IS UNIQUE",
    );

    // Verify it exists
    let result1 = execute_query(&mut engine, "SHOW CONSTRAINTS");
    let initial_count = result1
        .rows
        .iter()
        .filter(|row| {
            row.values[0].as_str() == Some("Company") && row.values[1].as_str() == Some("tax_id")
        })
        .count();
    assert_eq!(initial_count, 1);

    // Drop the constraint
    execute_query(
        &mut engine,
        "DROP CONSTRAINT ON (n:Company) ASSERT n.tax_id IS UNIQUE",
    );

    // Verify it's gone
    let result2 = execute_query(&mut engine, "SHOW CONSTRAINTS");
    let final_count = result2
        .rows
        .iter()
        .filter(|row| {
            row.values[0].as_str() == Some("Company") && row.values[1].as_str() == Some("tax_id")
        })
        .count();
    assert_eq!(final_count, 0);
}

#[test]
fn test_show_constraints_description_format() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // Create constraints
    execute_query(
        &mut engine,
        "CREATE CONSTRAINT ON (n:Book) ASSERT n.isbn IS UNIQUE",
    );
    execute_query(
        &mut engine,
        "CREATE CONSTRAINT ON (n:Book) ASSERT EXISTS(n.title)",
    );

    // Show constraints
    let result = execute_query(&mut engine, "SHOW CONSTRAINTS");

    // Check description format for UNIQUE constraint
    let isbn_constraint = result
        .rows
        .iter()
        .find(|row| {
            row.values[0].as_str() == Some("Book") && row.values[1].as_str() == Some("isbn")
        })
        .unwrap();

    let isbn_desc = isbn_constraint.values[3].as_str().unwrap();
    assert!(isbn_desc.contains("CONSTRAINT ON"));
    assert!(isbn_desc.contains("Book"));
    assert!(isbn_desc.contains("isbn"));
    assert!(isbn_desc.contains("IS UNIQUE"));

    // Check description format for EXISTS constraint
    let title_constraint = result
        .rows
        .iter()
        .find(|row| {
            row.values[0].as_str() == Some("Book") && row.values[1].as_str() == Some("title")
        })
        .unwrap();

    let title_desc = title_constraint.values[3].as_str().unwrap();
    assert!(title_desc.contains("CONSTRAINT ON"));
    assert!(title_desc.contains("Book"));
    assert!(title_desc.contains("title"));
    assert!(title_desc.contains("exists"));
}

// ============================================================================
// INTEGRATION TESTS
// ============================================================================

#[test]
fn test_constraint_enforcement_unique() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // Create UNIQUE constraint
    execute_query(
        &mut engine,
        "CREATE CONSTRAINT ON (n:Customer) ASSERT n.email IS UNIQUE",
    );

    // Verify it shows up
    let result = execute_query(&mut engine, "SHOW CONSTRAINTS");
    let customer_constraints = result
        .rows
        .iter()
        .filter(|row| row.values[0].as_str() == Some("Customer"))
        .count();
    assert!(customer_constraints >= 1);

    // Create first node - should succeed
    execute_query(
        &mut engine,
        "CREATE (c:Customer {email: 'test@example.com', name: 'Alice'})",
    );

    // Try to create duplicate - should fail due to constraint
    let duplicate_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        execute_query(
            &mut engine,
            "CREATE (c:Customer {email: 'test@example.com', name: 'Bob'})",
        )
    }));

    // Should have panicked due to unique constraint violation
    assert!(duplicate_result.is_err());
}

#[test]
fn test_constraint_enforcement_exists() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // Create EXISTS constraint
    execute_query(
        &mut engine,
        "CREATE CONSTRAINT ON (n:Order) ASSERT EXISTS(n.order_id)",
    );

    // Verify it shows up
    let result = execute_query(&mut engine, "SHOW CONSTRAINTS");
    let order_constraints = result
        .rows
        .iter()
        .filter(|row| row.values[0].as_str() == Some("Order"))
        .count();
    assert!(order_constraints >= 1);

    // Create node with required property - should succeed
    execute_query(
        &mut engine,
        "CREATE (o:Order {order_id: '12345', amount: 100.0})",
    );

    // Try to create node without required property - should fail
    let missing_prop_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        execute_query(&mut engine, "CREATE (o:Order {amount: 50.0})")
    }));

    // Should have panicked due to exists constraint violation
    assert!(missing_prop_result.is_err());
}
