//! Integration tests for Cypher string operations
//!
//! Tests for STARTS WITH, ENDS WITH, CONTAINS, and regex (=~) operators

use nexus_core::{Engine, Error};
use serde_json::Value;

fn create_engine() -> Result<Engine, Error> {
    let mut engine = Engine::new()?;
    // Ensure clean database for each test
    let _ = engine.execute_cypher("MATCH (n) DETACH DELETE n", None);
    Ok(engine)
}

fn extract_first_row_value(result: &nexus_core::ResultSet, column: &str) -> Option<Value> {
    result
        .rows
        .first()
        .and_then(|row| {
            result
                .columns
                .iter()
                .position(|c| c == column)
                .and_then(|idx| row.values.get(idx).cloned())
        })
}

#[test]
fn test_starts_with_basic() -> Result<(), Error> {
    let mut engine = create_engine()?;

    engine.execute_cypher(
        "CREATE (n:Person {name: 'Alice Smith', email: 'alice@example.com'}) RETURN n",
        None,
    )?;

    let result = engine.execute_cypher(
        "MATCH (n:Person) WHERE n.name STARTS WITH 'Alice' RETURN n.name AS name",
        None,
    )?;

    assert_eq!(result.rows.len(), 1);
    let name_value = extract_first_row_value(&result, "name").unwrap();
    if let Value::Object(obj) = name_value {
        let props = obj.get("properties").unwrap().as_object().unwrap();
        assert_eq!(props.get("name").unwrap().as_str(), Some("Alice Smith"));
    }
    Ok(())
}

#[test]
fn test_starts_with_no_match() -> Result<(), Error> {
    let mut engine = create_engine()?;

    engine.execute_cypher(
        "CREATE (n:Person {name: 'Bob Johnson'}) RETURN n",
        None,
    )?;

    let result = engine.execute_cypher(
        "MATCH (n:Person) WHERE n.name STARTS WITH 'Alice' RETURN n.name AS name",
        None,
    )?;

    assert_eq!(result.rows.len(), 0);
    Ok(())
}

#[test]
fn test_ends_with_basic() -> Result<(), Error> {
    let mut engine = create_engine()?;

    engine.execute_cypher(
        "CREATE (n:Person {email: 'alice@example.com'}) RETURN n",
        None,
    )?;

    let result = engine.execute_cypher(
        "MATCH (n:Person) WHERE n.email ENDS WITH '@example.com' RETURN n.email AS email",
        None,
    )?;

    assert_eq!(result.rows.len(), 1);
    let email_value = extract_first_row_value(&result, "email").unwrap();
    if let Value::Object(obj) = email_value {
        let props = obj.get("properties").unwrap().as_object().unwrap();
        assert_eq!(props.get("email").unwrap().as_str(), Some("alice@example.com"));
    }
    Ok(())
}

#[test]
fn test_ends_with_no_match() -> Result<(), Error> {
    let mut engine = create_engine()?;

    engine.execute_cypher(
        "CREATE (n:Person {email: 'bob@other.com'}) RETURN n",
        None,
    )?;

    let result = engine.execute_cypher(
        "MATCH (n:Person) WHERE n.email ENDS WITH '@example.com' RETURN n.email AS email",
        None,
    )?;

    assert_eq!(result.rows.len(), 0);
    Ok(())
}

#[test]
fn test_contains_basic() -> Result<(), Error> {
    let mut engine = create_engine()?;

    engine.execute_cypher(
        "CREATE (n:Person {bio: 'Software engineer with 10 years experience'}) RETURN n",
        None,
    )?;

    let result = engine.execute_cypher(
        "MATCH (n:Person) WHERE n.bio CONTAINS 'engineer' RETURN n.bio AS bio",
        None,
    )?;

    assert_eq!(result.rows.len(), 1);
    let bio_value = extract_first_row_value(&result, "bio").unwrap();
    if let Value::Object(obj) = bio_value {
        let props = obj.get("properties").unwrap().as_object().unwrap();
        assert_eq!(
            props.get("bio").unwrap().as_str(),
            Some("Software engineer with 10 years experience")
        );
    }
    Ok(())
}

#[test]
fn test_contains_no_match() -> Result<(), Error> {
    let mut engine = create_engine()?;

    engine.execute_cypher(
        "CREATE (n:Person {bio: 'Marketing specialist'}) RETURN n",
        None,
    )?;

    let result = engine.execute_cypher(
        "MATCH (n:Person) WHERE n.bio CONTAINS 'engineer' RETURN n.bio AS bio",
        None,
    )?;

    assert_eq!(result.rows.len(), 0);
    Ok(())
}

#[test]
fn test_regex_match_basic() -> Result<(), Error> {
    let mut engine = create_engine()?;

    engine.execute_cypher(
        "CREATE (n:Person {email: 'alice@example.com'}) RETURN n",
        None,
    )?;

    let result = engine.execute_cypher(
        "MATCH (n:Person) WHERE n.email =~ '.*@example\\.com' RETURN n.email AS email",
        None,
    )?;

    assert_eq!(result.rows.len(), 1);
    let email_value = extract_first_row_value(&result, "email").unwrap();
    if let Value::Object(obj) = email_value {
        let props = obj.get("properties").unwrap().as_object().unwrap();
        assert_eq!(props.get("email").unwrap().as_str(), Some("alice@example.com"));
    }
    Ok(())
}

#[test]
fn test_regex_match_no_match() -> Result<(), Error> {
    let mut engine = create_engine()?;

    engine.execute_cypher(
        "CREATE (n:Person {email: 'bob@other.com'}) RETURN n",
        None,
    )?;

    let result = engine.execute_cypher(
        "MATCH (n:Person) WHERE n.email =~ '.*@example\\.com' RETURN n.email AS email",
        None,
    )?;

    assert_eq!(result.rows.len(), 0);
    Ok(())
}

#[test]
fn test_regex_match_invalid_pattern() -> Result<(), Error> {
    let mut engine = create_engine()?;

    engine.execute_cypher(
        "CREATE (n:Person {email: 'alice@example.com'}) RETURN n",
        None,
    )?;

    // Invalid regex pattern should return false (no matches)
    let result = engine.execute_cypher(
        "MATCH (n:Person) WHERE n.email =~ '[invalid' RETURN n.email AS email",
        None,
    )?;

    assert_eq!(result.rows.len(), 0);
    Ok(())
}

#[test]
fn test_string_operators_combined() -> Result<(), Error> {
    let mut engine = create_engine()?;

    engine.execute_cypher(
        "CREATE (n:Person {email: 'alice@example.com', name: 'Alice Smith'}) RETURN n",
        None,
    )?;

    // Test multiple string operators combined with AND
    let result = engine.execute_cypher(
        "MATCH (n:Person) WHERE n.email STARTS WITH 'alice' AND n.email ENDS WITH '.com' AND n.email CONTAINS '@' RETURN n.email AS email",
        None,
    )?;

    assert_eq!(result.rows.len(), 1);
    let email_value = extract_first_row_value(&result, "email").unwrap();
    if let Value::Object(obj) = email_value {
        let props = obj.get("properties").unwrap().as_object().unwrap();
        assert_eq!(props.get("email").unwrap().as_str(), Some("alice@example.com"));
    }
    Ok(())
}

#[test]
fn test_string_operators_case_sensitive() -> Result<(), Error> {
    let mut engine = create_engine()?;

    engine.execute_cypher(
        "CREATE (n:Person {name: 'Alice'}) RETURN n",
        None,
    )?;

    // STARTS WITH is case-sensitive
    let result = engine.execute_cypher(
        "MATCH (n:Person) WHERE n.name STARTS WITH 'alice' RETURN n.name AS name",
        None,
    )?;

    assert_eq!(result.rows.len(), 0); // Should not match due to case sensitivity
    Ok(())
}

#[test]
fn test_string_operators_empty_string() -> Result<(), Error> {
    let mut engine = create_engine()?;

    engine.execute_cypher(
        "CREATE (n:Person {name: 'Alice'}) RETURN n",
        None,
    )?;

    // Empty string should match everything
    let result = engine.execute_cypher(
        "MATCH (n:Person) WHERE n.name STARTS WITH '' RETURN n.name AS name",
        None,
    )?;

    assert_eq!(result.rows.len(), 1);
    Ok(())
}

#[test]
fn test_regex_match_complex_pattern() -> Result<(), Error> {
    let mut engine = create_engine()?;

    engine.execute_cypher(
        "CREATE (n:Person {phone: '123-456-7890'}) RETURN n",
        None,
    )?;

    // Test regex for phone number pattern
    let result = engine.execute_cypher(
        "MATCH (n:Person) WHERE n.phone =~ '\\d{3}-\\d{3}-\\d{4}' RETURN n.phone AS phone",
        None,
    )?;

    assert_eq!(result.rows.len(), 1);
    let phone_value = extract_first_row_value(&result, "phone").unwrap();
    if let Value::Object(obj) = phone_value {
        let props = obj.get("properties").unwrap().as_object().unwrap();
        assert_eq!(props.get("phone").unwrap().as_str(), Some("123-456-7890"));
    }
    Ok(())
}

#[test]
fn test_string_operators_in_return() -> Result<(), Error> {
    let mut engine = create_engine()?;

    engine.execute_cypher(
        "CREATE (n:Person {name: 'Alice Smith'}) RETURN n",
        None,
    )?;

    // Test string operators in RETURN clause
    let result = engine.execute_cypher(
        "MATCH (n:Person) RETURN n.name STARTS WITH 'Alice' AS starts_with_alice, n.name CONTAINS 'Smith' AS contains_smith",
        None,
    )?;

    assert_eq!(result.rows.len(), 1);
    let starts_value = extract_first_row_value(&result, "starts_with_alice").unwrap();
    let contains_value = extract_first_row_value(&result, "contains_smith").unwrap();
    assert_eq!(starts_value.as_bool(), Some(true));
    assert_eq!(contains_value.as_bool(), Some(true));
    Ok(())
}
