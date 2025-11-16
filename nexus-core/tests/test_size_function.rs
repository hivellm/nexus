use nexus_core::{Engine, Error};
use tempfile::TempDir;

fn setup_test_engine() -> Result<(Engine, TempDir), Error> {
    let temp_dir = tempfile::tempdir().map_err(Error::Io)?;
    let engine = Engine::with_data_dir(temp_dir.path())?;
    Ok((engine, temp_dir))
}

#[test]
fn test_size_function_with_literal_array() -> Result<(), Error> {
    let (mut engine, _temp_dir) = setup_test_engine()?;

    // Test size() with literal array
    let result = engine.execute_cypher("RETURN size(['a', 'b', 'c']) AS size")?;

    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        result.rows[0].values[0].as_i64().unwrap(),
        3,
        "size(['a', 'b', 'c']) should return 3"
    );

    Ok(())
}

#[test]
fn test_size_function_with_empty_array() -> Result<(), Error> {
    let (mut engine, _temp_dir) = setup_test_engine()?;

    // Test size() with empty array
    let result = engine.execute_cypher("RETURN size([]) AS size")?;

    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        result.rows[0].values[0].as_i64().unwrap(),
        0,
        "size([]) should return 0"
    );

    Ok(())
}

#[test]
fn test_size_function_with_string() -> Result<(), Error> {
    let (mut engine, _temp_dir) = setup_test_engine()?;

    // Test size() with string
    let result = engine.execute_cypher("RETURN size('hello') AS size")?;

    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        result.rows[0].values[0].as_i64().unwrap(),
        5,
        "size('hello') should return 5"
    );

    Ok(())
}

#[test]
fn test_size_function_with_null() -> Result<(), Error> {
    let (mut engine, _temp_dir) = setup_test_engine()?;

    // Test size() with null
    let result = engine.execute_cypher("RETURN size(null) AS size")?;

    assert_eq!(result.rows.len(), 1);
    assert!(
        result.rows[0].values[0].is_null(),
        "size(null) should return null"
    );

    Ok(())
}

#[test]
fn test_size_function_with_nested_array() -> Result<(), Error> {
    let (mut engine, _temp_dir) = setup_test_engine()?;

    // Test size() with nested array
    let result = engine.execute_cypher("RETURN size([[1, 2], [3, 4], [5]]) AS size")?;

    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        result.rows[0].values[0].as_i64().unwrap(),
        3,
        "size([[1, 2], [3, 4], [5]]) should return 3 (number of subarrays)"
    );

    Ok(())
}

#[test]
fn test_size_with_array_indexing() -> Result<(), Error> {
    let (mut engine, _temp_dir) = setup_test_engine()?;

    // Test combining size() with array indexing
    let result = engine.execute_cypher("RETURN size(['hello', 'world'][0]) AS size")?;

    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        result.rows[0].values[0].as_i64().unwrap(),
        5,
        "size(array[0]) should return length of first element"
    );

    Ok(())
}
