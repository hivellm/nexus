//! Integration tests for CASE expressions
//!
//! Tests for simple and generic CASE expressions in Cypher queries

use nexus_core::{Engine, Error};
use nexus_core::executor::ResultSet;
use serde_json::Value;

fn create_engine() -> Result<Engine, Error> {
    let mut engine = Engine::new()?;
    // Ensure clean database for each test
    let _ = engine.execute_cypher("MATCH (n) DETACH DELETE n");
    Ok(engine)
}

fn extract_first_row_value(result: &ResultSet, column_idx: usize) -> Option<&Value> {
    result.rows.get(0).and_then(|row| row.values.get(column_idx))
}

#[test]
fn test_case_simple_expression() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    // Create test data
    engine.execute_cypher(
        "CREATE (p1:Person {name: 'Alice', age: 17}), (p2:Person {name: 'Bob', age: 30}), (p3:Person {name: 'Charlie', age: 70})",
    )?;

    // Test simple CASE expression
    let result = engine.execute_cypher(
        "MATCH (n:Person) RETURN n.name AS name, CASE WHEN n.age < 18 THEN 'minor' WHEN n.age < 65 THEN 'adult' ELSE 'senior' END AS category ORDER BY n.name",
    )?;

    assert_eq!(result.rows.len(), 3);
    
    // Check first row (Alice - minor)
    let row0 = &result.rows[0];
    let name0 = row0.values.get(0).and_then(Value::as_str);
    let category0 = row0.values.get(1).and_then(Value::as_str);
    assert_eq!(name0, Some("Alice"));
    assert_eq!(category0, Some("minor"));
    
    // Check second row (Bob - adult)
    let row1 = &result.rows[1];
    let name1 = row1.values.get(0).and_then(Value::as_str);
    let category1 = row1.values.get(1).and_then(Value::as_str);
    assert_eq!(name1, Some("Bob"));
    assert_eq!(category1, Some("adult"));
    
    // Check third row (Charlie - senior)
    let row2 = &result.rows[2];
    let name2 = row2.values.get(0).and_then(Value::as_str);
    let category2 = row2.values.get(1).and_then(Value::as_str);
    assert_eq!(name2, Some("Charlie"));
    assert_eq!(category2, Some("senior"));
    
    Ok(())
}

#[test]
fn test_case_simple_with_else() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    // Create test data
    engine.execute_cypher(
        "CREATE (p1:Person {name: 'Alice', age: 17}), (p2:Person {name: 'Bob', age: 30})",
    )?;

    // Test CASE with ELSE clause
    let result = engine.execute_cypher(
        "MATCH (n:Person) RETURN n.name AS name, CASE WHEN n.age < 18 THEN 'minor' ELSE 'adult' END AS category ORDER BY n.name",
    )?;

    assert_eq!(result.rows.len(), 2);
    
    let row0 = &result.rows[0];
    assert_eq!(row0.values.get(1).and_then(Value::as_str), Some("minor"));
    
    let row1 = &result.rows[1];
    assert_eq!(row1.values.get(1).and_then(Value::as_str), Some("adult"));
    
    Ok(())
}

#[test]
fn test_case_simple_without_else() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    // Create test data
    engine.execute_cypher(
        "CREATE (p1:Person {name: 'Alice', age: 17}), (p2:Person {name: 'Bob', age: 30})",
    )?;

    // Test CASE without ELSE clause (should return NULL)
    let result = engine.execute_cypher(
        "MATCH (n:Person) WHERE n.age >= 18 RETURN n.name AS name, CASE WHEN n.age < 18 THEN 'minor' END AS category",
    )?;

    assert_eq!(result.rows.len(), 1);
    
    let row0 = &result.rows[0];
    assert_eq!(row0.values.get(1), Some(&Value::Null));
    
    Ok(())
}

#[test]
fn test_case_generic_expression() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    // Create test data
    engine.execute_cypher(
        "CREATE (p1:Person {name: 'Alice', status: 'active'}), (p2:Person {name: 'Bob', status: 'inactive'}), (p3:Person {name: 'Charlie', status: 'pending'})",
    )?;

    // Test generic CASE expression (with input)
    let result = engine.execute_cypher(
        "MATCH (n:Person) RETURN n.name AS name, CASE n.status WHEN 'active' THEN 'working' WHEN 'inactive' THEN 'idle' ELSE 'unknown' END AS state ORDER BY n.name",
    )?;

    assert_eq!(result.rows.len(), 3);
    
    // Check Alice (active -> working)
    let row0 = &result.rows[0];
    assert_eq!(row0.values.get(1).and_then(Value::as_str), Some("working"));
    
    // Check Bob (inactive -> idle)
    let row1 = &result.rows[1];
    assert_eq!(row1.values.get(1).and_then(Value::as_str), Some("idle"));
    
    // Check Charlie (pending -> unknown)
    let row2 = &result.rows[2];
    assert_eq!(row2.values.get(1).and_then(Value::as_str), Some("unknown"));
    
    Ok(())
}

#[test]
fn test_case_in_return_only() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    // Test CASE expression without MATCH (literal evaluation)
    let result = engine.execute_cypher(
        "RETURN CASE WHEN 1 < 2 THEN 'true' ELSE 'false' END AS result",
    )?;

    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        result.rows[0].values.get(0).and_then(Value::as_str),
        Some("true")
    );
    
    Ok(())
}

#[test]
fn test_case_nested_expressions() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    // Create test data
    engine.execute_cypher(
        "CREATE (p1:Person {name: 'Alice', age: 17, score: 85}), (p2:Person {name: 'Bob', age: 30, score: 60})",
    )?;

    // Test CASE with nested property access
    let result = engine.execute_cypher(
        "MATCH (n:Person) RETURN n.name AS name, CASE WHEN n.age < 18 AND n.score >= 80 THEN 'excellent' WHEN n.age < 18 THEN 'good' ELSE 'adult' END AS rating ORDER BY n.name",
    )?;

    assert_eq!(result.rows.len(), 2);
    
    let row0 = &result.rows[0];
    assert_eq!(row0.values.get(1).and_then(Value::as_str), Some("excellent"));
    
    let row1 = &result.rows[1];
    assert_eq!(row1.values.get(1).and_then(Value::as_str), Some("adult"));
    
    Ok(())
}

#[test]
fn test_case_with_numeric_values() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    engine.execute_cypher(
        "CREATE (p1:Product {name: 'A', price: 10}), (p2:Product {name: 'B', price: 50}), (p3:Product {name: 'C', price: 100})",
    )?;

    // Test CASE returning numeric values
    let result = engine.execute_cypher(
        "MATCH (n:Product) RETURN n.name AS name, CASE WHEN n.price < 20 THEN 1 WHEN n.price < 80 THEN 2 ELSE 3 END AS tier ORDER BY n.name",
    )?;

    assert_eq!(result.rows.len(), 3);
    assert_eq!(result.rows[0].values.get(1).and_then(Value::as_u64), Some(1));
    assert_eq!(result.rows[1].values.get(1).and_then(Value::as_u64), Some(2));
    assert_eq!(result.rows[2].values.get(1).and_then(Value::as_u64), Some(3));
    
    Ok(())
}

#[test]
fn test_case_with_null_properties() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    engine.execute_cypher(
        "CREATE (p1:Person {name: 'Alice', age: 30}), (p2:Person {name: 'Bob'})",
    )?;

    // Test CASE handling NULL properties
    let result = engine.execute_cypher(
        "MATCH (n:Person) RETURN n.name AS name, CASE WHEN n.age IS NULL THEN 'unknown' WHEN n.age < 18 THEN 'minor' ELSE 'adult' END AS category ORDER BY n.name",
    )?;

    assert_eq!(result.rows.len(), 2);
    assert_eq!(result.rows[0].values.get(1).and_then(Value::as_str), Some("adult"));
    assert_eq!(result.rows[1].values.get(1).and_then(Value::as_str), Some("unknown"));
    
    Ok(())
}

#[test]
fn test_case_in_where_clause() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    engine.execute_cypher(
        "CREATE (p1:Person {name: 'Alice', age: 17}), (p2:Person {name: 'Bob', age: 30}), (p3:Person {name: 'Charlie', age: 70})",
    )?;

    // Test CASE in WHERE clause
    let result = engine.execute_cypher(
        "MATCH (n:Person) WHERE CASE WHEN n.age < 18 THEN 'minor' ELSE 'adult' END = 'minor' RETURN n.name AS name ORDER BY n.name",
    )?;

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values.get(0).and_then(Value::as_str), Some("Alice"));
    
    Ok(())
}

#[test]
fn test_case_multiple_when_clauses() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    engine.execute_cypher(
        "CREATE (p1:Person {score: 45}), (p2:Person {score: 75}), (p3:Person {score: 95}), (p4:Person {score: 30})",
    )?;

    // Test CASE with many WHEN clauses
    let result = engine.execute_cypher(
        "MATCH (n:Person) RETURN CASE WHEN n.score >= 90 THEN 'A' WHEN n.score >= 80 THEN 'B' WHEN n.score >= 70 THEN 'C' WHEN n.score >= 60 THEN 'D' ELSE 'F' END AS grade ORDER BY n.score",
    )?;

    assert_eq!(result.rows.len(), 4);
    assert_eq!(result.rows[0].values.get(0).and_then(Value::as_str), Some("F")); // 30
    assert_eq!(result.rows[1].values.get(0).and_then(Value::as_str), Some("F")); // 45
    assert_eq!(result.rows[2].values.get(0).and_then(Value::as_str), Some("C")); // 75
    assert_eq!(result.rows[3].values.get(0).and_then(Value::as_str), Some("A")); // 95
    
    Ok(())
}

#[test]
fn test_case_generic_without_else() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    engine.execute_cypher(
        "CREATE (p1:Person {status: 'active'}), (p2:Person {status: 'inactive'}), (p3:Person {status: 'pending'})",
    )?;

    // Test generic CASE without ELSE (should return NULL for unmatched)
    let result = engine.execute_cypher(
        "MATCH (n:Person) RETURN n.status AS status, CASE n.status WHEN 'active' THEN 'working' WHEN 'inactive' THEN 'idle' END AS state ORDER BY n.status",
    )?;

    assert_eq!(result.rows.len(), 3);
    assert_eq!(result.rows[0].values.get(1).and_then(Value::as_str), Some("working")); // active
    assert_eq!(result.rows[1].values.get(1).and_then(Value::as_str), Some("idle")); // inactive
    assert_eq!(result.rows[2].values.get(1), Some(&Value::Null)); // pending (no match)
    
    Ok(())
}

#[test]
fn test_case_with_string_comparisons() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    engine.execute_cypher(
        "CREATE (p1:Person {name: 'Alice', city: 'NYC'}), (p2:Person {name: 'Bob', city: 'LA'}), (p3:Person {name: 'Charlie', city: 'Chicago'})",
    )?;

    // Test CASE with string comparisons
    let result = engine.execute_cypher(
        "MATCH (n:Person) RETURN n.name AS name, CASE WHEN n.city = 'NYC' THEN 'East' WHEN n.city = 'LA' THEN 'West' ELSE 'Other' END AS region ORDER BY n.name",
    )?;

    assert_eq!(result.rows.len(), 3);
    assert_eq!(result.rows[0].values.get(1).and_then(Value::as_str), Some("East"));
    assert_eq!(result.rows[1].values.get(1).and_then(Value::as_str), Some("West"));
    assert_eq!(result.rows[2].values.get(1).and_then(Value::as_str), Some("Other"));
    
    Ok(())
}

#[test]
fn test_case_with_boolean_results() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    engine.execute_cypher(
        "CREATE (p1:Person {age: 17}), (p2:Person {age: 30})",
    )?;

    // Test CASE returning boolean values
    let result = engine.execute_cypher(
        "MATCH (n:Person) RETURN n.age AS age, CASE WHEN n.age < 18 THEN true ELSE false END AS is_minor ORDER BY n.age",
    )?;

    assert_eq!(result.rows.len(), 2);
    assert_eq!(result.rows[0].values.get(1).and_then(Value::as_bool), Some(true));
    assert_eq!(result.rows[1].values.get(1).and_then(Value::as_bool), Some(false));
    
    Ok(())
}

#[test]
fn test_case_nested_case() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    engine.execute_cypher(
        "CREATE (p1:Person {age: 17, score: 95}), (p2:Person {age: 17, score: 60}), (p3:Person {age: 30, score: 85})",
    )?;

    // Test nested CASE expressions
    let result = engine.execute_cypher(
        "MATCH (n:Person) RETURN n.age AS age, CASE WHEN n.age < 18 THEN CASE WHEN n.score >= 80 THEN 'excellent' ELSE 'good' END ELSE 'adult' END AS rating ORDER BY n.age, n.score",
    )?;

    assert_eq!(result.rows.len(), 3);
    assert_eq!(result.rows[0].values.get(1).and_then(Value::as_str), Some("good")); // 17, 60
    assert_eq!(result.rows[1].values.get(1).and_then(Value::as_str), Some("excellent")); // 17, 95
    assert_eq!(result.rows[2].values.get(1).and_then(Value::as_str), Some("adult")); // 30
    
    Ok(())
}

#[test]
fn test_case_with_complex_conditions() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    engine.execute_cypher(
        "CREATE (p1:Person {age: 25, salary: 50000, department: 'IT'}), (p2:Person {age: 35, salary: 80000, department: 'Sales'}), (p3:Person {age: 28, salary: 60000, department: 'IT'})",
    )?;

    // Test CASE with complex boolean conditions
    let result = engine.execute_cypher(
        "MATCH (n:Person) RETURN n.age AS age, CASE WHEN n.age >= 30 AND n.salary > 70000 THEN 'senior' WHEN n.department = 'IT' AND n.salary > 55000 THEN 'mid-level' ELSE 'junior' END AS level ORDER BY n.age",
    )?;

    assert_eq!(result.rows.len(), 3);
    assert_eq!(result.rows[0].values.get(1).and_then(Value::as_str), Some("junior")); // 25, 50000, IT
    assert_eq!(result.rows[1].values.get(1).and_then(Value::as_str), Some("mid-level")); // 28, 60000, IT
    assert_eq!(result.rows[2].values.get(1).and_then(Value::as_str), Some("senior")); // 35, 80000, Sales
    
    Ok(())
}

#[test]
fn test_case_in_order_by() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    engine.execute_cypher(
        "CREATE (p1:Person {name: 'Alice', priority: 'low'}), (p2:Person {name: 'Bob', priority: 'high'}), (p3:Person {name: 'Charlie', priority: 'medium'})",
    )?;

    // Test CASE in ORDER BY clause
    let result = engine.execute_cypher(
        "MATCH (n:Person) RETURN n.name AS name ORDER BY CASE n.priority WHEN 'high' THEN 1 WHEN 'medium' THEN 2 WHEN 'low' THEN 3 ELSE 4 END",
    )?;

    assert_eq!(result.rows.len(), 3);
    assert_eq!(result.rows[0].values.get(0).and_then(Value::as_str), Some("Bob")); // high (1)
    assert_eq!(result.rows[1].values.get(0).and_then(Value::as_str), Some("Charlie")); // medium (2)
    assert_eq!(result.rows[2].values.get(0).and_then(Value::as_str), Some("Alice")); // low (3)
    
    Ok(())
}

#[test]
fn test_case_first_match_wins() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    engine.execute_cypher(
        "CREATE (p1:Person {age: 25})",
    )?;

    // Test that first matching WHEN clause wins (even if later ones also match)
    let result = engine.execute_cypher(
        "MATCH (n:Person) RETURN CASE WHEN n.age >= 20 THEN 'adult' WHEN n.age >= 18 THEN 'young-adult' ELSE 'minor' END AS category",
    )?;

    assert_eq!(result.rows.len(), 1);
    // Age 25 matches both >= 20 and >= 18, but first match (>= 20) should win
    assert_eq!(result.rows[0].values.get(0).and_then(Value::as_str), Some("adult"));
    
    Ok(())
}

#[test]
fn test_case_with_empty_string() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    engine.execute_cypher(
        "CREATE (p1:Person {name: 'Alice', status: ''}), (p2:Person {name: 'Bob', status: 'active'})",
    )?;

    // Test CASE handling empty strings
    let result = engine.execute_cypher(
        "MATCH (n:Person) RETURN n.name AS name, CASE WHEN n.status = '' THEN 'empty' WHEN n.status = 'active' THEN 'active' ELSE 'other' END AS status_type ORDER BY n.name",
    )?;

    assert_eq!(result.rows.len(), 2);
    assert_eq!(result.rows[0].values.get(1).and_then(Value::as_str), Some("empty"));
    assert_eq!(result.rows[1].values.get(1).and_then(Value::as_str), Some("active"));
    
    Ok(())
}

#[test]
fn test_case_generic_with_numeric_input() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    engine.execute_cypher(
        "CREATE (p1:Person {code: 1}), (p2:Person {code: 2}), (p3:Person {code: 3})",
    )?;

    // Test generic CASE with numeric input
    let result = engine.execute_cypher(
        "MATCH (n:Person) RETURN n.code AS code, CASE n.code WHEN 1 THEN 'first' WHEN 2 THEN 'second' ELSE 'other' END AS position ORDER BY n.code",
    )?;

    assert_eq!(result.rows.len(), 3);
    assert_eq!(result.rows[0].values.get(1).and_then(Value::as_str), Some("first"));
    assert_eq!(result.rows[1].values.get(1).and_then(Value::as_str), Some("second"));
    assert_eq!(result.rows[2].values.get(1).and_then(Value::as_str), Some("other"));
    
    Ok(())
}

#[test]
fn test_case_with_inequality_operators() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    engine.execute_cypher(
        "CREATE (p1:Person {score: 50}), (p2:Person {score: 75}), (p3:Person {score: 90})",
    )?;

    // Test CASE with various comparison operators
    let result = engine.execute_cypher(
        "MATCH (n:Person) RETURN n.score AS score, CASE WHEN n.score > 80 THEN 'high' WHEN n.score >= 60 THEN 'medium' WHEN n.score < 60 THEN 'low' END AS level ORDER BY n.score",
    )?;

    assert_eq!(result.rows.len(), 3);
    assert_eq!(result.rows[0].values.get(1).and_then(Value::as_str), Some("low")); // 50 < 60
    assert_eq!(result.rows[1].values.get(1).and_then(Value::as_str), Some("medium")); // 75 >= 60
    assert_eq!(result.rows[2].values.get(1).and_then(Value::as_str), Some("high")); // 90 > 80
    
    Ok(())
}

#[test]
fn test_case_with_or_conditions() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    engine.execute_cypher(
        "CREATE (p1:Person {age: 15}), (p2:Person {age: 25}), (p3:Person {age: 70})",
    )?;

    // Test CASE with OR conditions
    let result = engine.execute_cypher(
        "MATCH (n:Person) RETURN n.age AS age, CASE WHEN n.age < 18 OR n.age >= 65 THEN 'special' ELSE 'normal' END AS category ORDER BY n.age",
    )?;

    assert_eq!(result.rows.len(), 3);
    assert_eq!(result.rows[0].values.get(1).and_then(Value::as_str), Some("special")); // 15 < 18
    assert_eq!(result.rows[1].values.get(1).and_then(Value::as_str), Some("normal")); // 25
    assert_eq!(result.rows[2].values.get(1).and_then(Value::as_str), Some("special")); // 70 >= 65
    
    Ok(())
}

#[test]
fn test_case_with_not_conditions() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    engine.execute_cypher(
        "CREATE (p1:Person {active: true}), (p2:Person {active: false})",
    )?;

    // Test CASE with NOT conditions
    let result = engine.execute_cypher(
        "MATCH (n:Person) RETURN n.active AS active, CASE WHEN NOT n.active THEN 'inactive' ELSE 'active' END AS status ORDER BY n.active",
    )?;

    assert_eq!(result.rows.len(), 2);
    assert_eq!(result.rows[0].values.get(1).and_then(Value::as_str), Some("inactive")); // false
    assert_eq!(result.rows[1].values.get(1).and_then(Value::as_str), Some("active")); // true
    
    Ok(())
}

#[test]
fn test_case_single_when_no_else() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    engine.execute_cypher(
        "CREATE (p1:Person {age: 17}), (p2:Person {age: 30})",
    )?;

    // Test CASE with single WHEN and no ELSE
    let result = engine.execute_cypher(
        "MATCH (n:Person) RETURN n.age AS age, CASE WHEN n.age < 18 THEN 'minor' END AS category ORDER BY n.age",
    )?;

    assert_eq!(result.rows.len(), 2);
    assert_eq!(result.rows[0].values.get(1).and_then(Value::as_str), Some("minor")); // 17
    assert_eq!(result.rows[1].values.get(1), Some(&Value::Null)); // 30 (no match)
    
    Ok(())
}

#[test]
fn test_case_generic_single_when() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    engine.execute_cypher(
        "CREATE (p1:Person {status: 'active'}), (p2:Person {status: 'inactive'})",
    )?;

    // Test generic CASE with single WHEN
    let result = engine.execute_cypher(
        "MATCH (n:Person) RETURN n.status AS status, CASE n.status WHEN 'active' THEN 'working' END AS state ORDER BY n.status",
    )?;

    assert_eq!(result.rows.len(), 2);
    assert_eq!(result.rows[0].values.get(1).and_then(Value::as_str), Some("working")); // active
    assert_eq!(result.rows[1].values.get(1), Some(&Value::Null)); // inactive (no match)
    
    Ok(())
}

#[test]
fn test_case_with_float_comparisons() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    engine.execute_cypher(
        "CREATE (p1:Product {price: 9.99}), (p2:Product {price: 19.99}), (p3:Product {price: 29.99})",
    )?;

    // Test CASE with float comparisons
    let result = engine.execute_cypher(
        "MATCH (n:Product) RETURN n.price AS price, CASE WHEN n.price < 10.0 THEN 'cheap' WHEN n.price < 25.0 THEN 'moderate' ELSE 'expensive' END AS category ORDER BY n.price",
    )?;

    assert_eq!(result.rows.len(), 3);
    assert_eq!(result.rows[0].values.get(1).and_then(Value::as_str), Some("cheap")); // 9.99
    assert_eq!(result.rows[1].values.get(1).and_then(Value::as_str), Some("moderate")); // 19.99
    assert_eq!(result.rows[2].values.get(1).and_then(Value::as_str), Some("expensive")); // 29.99
    
    Ok(())
}

#[test]
fn test_case_with_null_in_conditions() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    engine.execute_cypher(
        "CREATE (p1:Person {age: 20}), (p2:Person {age: NULL})",
    )?;

    // Test CASE handling NULL in conditions
    let result = engine.execute_cypher(
        "MATCH (n:Person) RETURN CASE WHEN n.age IS NULL THEN 'unknown' WHEN n.age < 18 THEN 'minor' ELSE 'adult' END AS category ORDER BY n.age",
    )?;

    assert_eq!(result.rows.len(), 2);
    // NULL age should match first condition
    assert_eq!(result.rows[0].values.get(0).and_then(Value::as_str), Some("unknown"));
    assert_eq!(result.rows[1].values.get(0).and_then(Value::as_str), Some("adult"));
    
    Ok(())
}

#[test]
fn test_case_returning_null_explicitly() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    engine.execute_cypher(
        "CREATE (p1:Person {age: 17}), (p2:Person {age: 30})",
    )?;

    // Test CASE explicitly returning NULL
    let result = engine.execute_cypher(
        "MATCH (n:Person) RETURN n.age AS age, CASE WHEN n.age < 18 THEN 'minor' WHEN n.age >= 18 THEN NULL ELSE 'unknown' END AS category ORDER BY n.age",
    )?;

    assert_eq!(result.rows.len(), 2);
    assert_eq!(result.rows[0].values.get(1).and_then(Value::as_str), Some("minor")); // 17
    assert_eq!(result.rows[1].values.get(1), Some(&Value::Null)); // 30 (explicit NULL)
    
    Ok(())
}
