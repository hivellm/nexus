//! Tests for filtering single-row expression streams with WHERE.
//!
//! Neo4j Cypher only permits WHERE immediately after MATCH /
//! OPTIONAL MATCH / WITH. To filter a literal / expression with no
//! graph pattern, the canonical pattern is:
//!
//! ```cypher
//! UNWIND [<expr>] AS val
//! WITH val WHERE <cond>
//! RETURN val
//! ```
//!
//! An earlier Nexus extension accepted `RETURN <expr> WHERE <cond>`
//! as shorthand; that parse-time shortcut was removed in commit
//! a9c86b27 to reach Neo4j 2025.09.0 300/300 compat. The tests in
//! this file used to exercise the old shape — they now exercise the
//! migration shape that commit documented, plus a regression guard
//! that the old shape still rejects with an actionable message.

use nexus_core::Error;
use nexus_core::testing::setup_test_engine;

#[test]
fn test_with_where_simple_comparison() -> Result<(), Error> {
    let (mut engine, _ctx) = setup_test_engine()?;

    // Condition true → one row
    let result = engine.execute_cypher("UNWIND [5] AS val WITH val WHERE val > 3 RETURN val")?;
    assert_eq!(
        result.rows.len(),
        1,
        "Should return 1 row when condition is true"
    );
    assert_eq!(result.rows[0].values[0].as_i64().unwrap(), 5);

    // Condition false → zero rows
    let result = engine.execute_cypher("UNWIND [5] AS val WITH val WHERE val > 10 RETURN val")?;
    assert_eq!(
        result.rows.len(),
        0,
        "Should return 0 rows when condition is false"
    );

    Ok(())
}

#[test]
fn test_with_where_boolean_literal() -> Result<(), Error> {
    let (mut engine, _ctx) = setup_test_engine()?;

    let result = engine.execute_cypher("UNWIND [42] AS val WITH val WHERE true RETURN val")?;
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0].as_i64().unwrap(), 42);

    let result = engine.execute_cypher("UNWIND [42] AS val WITH val WHERE false RETURN val")?;
    assert_eq!(result.rows.len(), 0);

    Ok(())
}

#[test]
fn test_with_where_is_null() -> Result<(), Error> {
    let (mut engine, _ctx) = setup_test_engine()?;

    let result =
        engine.execute_cypher("UNWIND [42] AS val WITH val WHERE null IS NULL RETURN val")?;
    assert_eq!(result.rows.len(), 1);

    let result =
        engine.execute_cypher("UNWIND [42] AS val WITH val WHERE val IS NOT NULL RETURN val")?;
    assert_eq!(result.rows.len(), 1);

    Ok(())
}

#[test]
fn test_with_where_in_operator() -> Result<(), Error> {
    let (mut engine, _ctx) = setup_test_engine()?;

    let result =
        engine.execute_cypher("UNWIND [5] AS val WITH val WHERE val IN [1, 2, 5] RETURN val")?;
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0].as_i64().unwrap(), 5);

    let result =
        engine.execute_cypher("UNWIND [5] AS val WITH val WHERE val IN [1, 2, 3] RETURN val")?;
    assert_eq!(result.rows.len(), 0);

    Ok(())
}

#[test]
fn test_with_where_string_operators() -> Result<(), Error> {
    let (mut engine, _ctx) = setup_test_engine()?;

    let result = engine
        .execute_cypher("UNWIND ['hello'] AS val WITH val WHERE val STARTS WITH 'he' RETURN val")?;
    assert_eq!(result.rows.len(), 1);

    let result = engine
        .execute_cypher("UNWIND ['hello'] AS val WITH val WHERE val ENDS WITH 'lo' RETURN val")?;
    assert_eq!(result.rows.len(), 1);

    let result = engine
        .execute_cypher("UNWIND ['hello'] AS val WITH val WHERE val CONTAINS 'll' RETURN val")?;
    assert_eq!(result.rows.len(), 1);

    Ok(())
}

#[test]
fn test_with_where_logical_operators() -> Result<(), Error> {
    let (mut engine, _ctx) = setup_test_engine()?;

    let result =
        engine.execute_cypher("UNWIND [5] AS val WITH val WHERE val > 3 AND 2 < 4 RETURN val")?;
    assert_eq!(result.rows.len(), 1);

    let result =
        engine.execute_cypher("UNWIND [5] AS val WITH val WHERE val > 10 OR 2 < 4 RETURN val")?;
    assert_eq!(result.rows.len(), 1);

    let result =
        engine.execute_cypher("UNWIND [5] AS val WITH val WHERE NOT (val > 10) RETURN val")?;
    assert_eq!(result.rows.len(), 1);

    Ok(())
}

#[test]
fn test_with_where_complex_expression() -> Result<(), Error> {
    let (mut engine, _ctx) = setup_test_engine()?;

    let result = engine.execute_cypher(
        "UNWIND [42] AS val WITH val WHERE (val > 3 AND 2 < 4) OR (10 > 20) RETURN val",
    )?;
    assert_eq!(result.rows.len(), 1);

    Ok(())
}

/// Regression guard: the old Nexus-specific `RETURN <expr> WHERE <cond>`
/// shorthand must still be rejected at parse time with an error message
/// that guides callers to the `WITH <vars> WHERE <cond>` migration —
/// this is exactly the shape Neo4j 2025.09.0 emits and the reason we
/// reach 300/300 compat.
#[test]
fn test_return_where_rejected_with_neo4j_error() -> Result<(), Error> {
    let (mut engine, _ctx) = setup_test_engine()?;

    let err = engine
        .execute_cypher("RETURN 5 AS val WHERE 5 > 3")
        .expect_err("standalone RETURN … WHERE must be a syntax error");

    let msg = err.to_string();
    assert!(
        msg.contains("Invalid input 'WHERE'"),
        "error should quote the offending keyword: {msg}"
    );
    assert!(
        msg.contains("'WITH'"),
        "error should advertise WITH as a valid alternative to guide migration: {msg}"
    );

    Ok(())
}
