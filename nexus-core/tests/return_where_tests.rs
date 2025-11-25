//! Tests for RETURN ... WHERE clause syntax (without MATCH)
//!
//! In Cypher, you can use WHERE clause with RETURN even without MATCH:
//! - RETURN expression WHERE condition
//!
//! This is useful for filtering literal values or expressions

use nexus_core::{Engine, Error};
use tempfile::TempDir;

fn setup_test_engine() -> Result<(Engine, TempDir), Error> {
    let temp_dir = tempfile::tempdir().map_err(Error::Io)?;
    let engine = Engine::with_data_dir(temp_dir.path())?;
    Ok((engine, temp_dir))
}

#[test]
fn test_return_where_simple_comparison() -> Result<(), Error> {
    // RETURN value WHERE condition should filter based on condition
    let (mut engine, _temp_dir) = setup_test_engine()?;

    // This should return 1 row if condition is true
    let result = engine.execute_cypher("RETURN 5 AS val WHERE 5 > 3")?;
    assert_eq!(
        result.rows.len(),
        1,
        "Should return 1 row when condition is true"
    );
    assert_eq!(result.rows[0].values[0].as_i64().unwrap(), 5);

    // This should return 0 rows if condition is false
    let result = engine.execute_cypher("RETURN 5 AS val WHERE 5 > 10")?;
    assert_eq!(
        result.rows.len(),
        0,
        "Should return 0 rows when condition is false"
    );

    Ok(())
}

#[test]
fn test_return_where_boolean_literal() -> Result<(), Error> {
    let (mut engine, _temp_dir) = setup_test_engine()?;

    // WHERE true should return the row
    let result = engine.execute_cypher("RETURN 42 AS val WHERE true")?;
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0].as_i64().unwrap(), 42);

    // WHERE false should return no rows
    let result = engine.execute_cypher("RETURN 42 AS val WHERE false")?;
    assert_eq!(result.rows.len(), 0);

    Ok(())
}

#[test]
fn test_return_where_is_null() -> Result<(), Error> {
    let (mut engine, _temp_dir) = setup_test_engine()?;

    // WHERE null IS NULL should return the row
    let result = engine.execute_cypher("RETURN 42 AS val WHERE null IS NULL")?;
    assert_eq!(result.rows.len(), 1);

    // WHERE 5 IS NOT NULL should return the row
    let result = engine.execute_cypher("RETURN 42 AS val WHERE 5 IS NOT NULL")?;
    assert_eq!(result.rows.len(), 1);

    Ok(())
}

#[test]
fn test_return_where_in_operator() -> Result<(), Error> {
    let (mut engine, _temp_dir) = setup_test_engine()?;

    // WHERE value IN list should filter correctly
    let result = engine.execute_cypher("RETURN 5 AS val WHERE 5 IN [1, 2, 5]")?;
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0].as_i64().unwrap(), 5);

    // WHERE value NOT IN list should return 0 rows
    let result = engine.execute_cypher("RETURN 5 AS val WHERE 5 IN [1, 2, 3]")?;
    assert_eq!(result.rows.len(), 0);

    Ok(())
}

#[test]
fn test_return_where_string_operators() -> Result<(), Error> {
    let (mut engine, _temp_dir) = setup_test_engine()?;

    // WHERE string STARTS WITH prefix
    let result = engine.execute_cypher("RETURN 'hello' AS val WHERE 'hello' STARTS WITH 'he'")?;
    assert_eq!(result.rows.len(), 1);

    // WHERE string ENDS WITH suffix
    let result = engine.execute_cypher("RETURN 'hello' AS val WHERE 'hello' ENDS WITH 'lo'")?;
    assert_eq!(result.rows.len(), 1);

    // WHERE string CONTAINS substring
    let result = engine.execute_cypher("RETURN 'hello' AS val WHERE 'hello' CONTAINS 'll'")?;
    assert_eq!(result.rows.len(), 1);

    Ok(())
}

#[test]
fn test_return_where_logical_operators() -> Result<(), Error> {
    let (mut engine, _temp_dir) = setup_test_engine()?;

    // WHERE AND condition
    let result = engine.execute_cypher("RETURN 5 AS val WHERE 5 > 3 AND 2 < 4")?;
    assert_eq!(result.rows.len(), 1);

    // WHERE OR condition
    let result = engine.execute_cypher("RETURN 5 AS val WHERE 5 > 10 OR 2 < 4")?;
    assert_eq!(result.rows.len(), 1);

    // WHERE NOT condition
    let result = engine.execute_cypher("RETURN 5 AS val WHERE NOT (5 > 10)")?;
    assert_eq!(result.rows.len(), 1);

    Ok(())
}

#[test]
fn test_return_where_complex_expression() -> Result<(), Error> {
    let (mut engine, _temp_dir) = setup_test_engine()?;

    // Complex WHERE condition with nested expressions
    let result = engine.execute_cypher("RETURN 42 AS val WHERE (5 > 3 AND 2 < 4) OR (10 > 20)")?;
    assert_eq!(result.rows.len(), 1);

    Ok(())
}
