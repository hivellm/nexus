use nexus_core::Engine;
use nexus_core::error::Error;
use tempfile::TempDir;

fn setup_test_engine() -> Result<(Engine, TempDir), Error> {
    let temp_dir =
        TempDir::new().map_err(|e| Error::Internal(format!("Failed to create temp dir: {}", e)))?;
    let engine = Engine::with_data_dir(temp_dir.path())?;
    Ok((engine, temp_dir))
}

#[test]
fn test_count_star_without_match() -> Result<(), Error> {
    let (mut engine, _temp_dir) = setup_test_engine()?;

    // RETURN count(*) without MATCH should return 1 (not 0 or null)
    let result = engine.execute_cypher("RETURN count(*) AS count")?;

    assert_eq!(result.rows.len(), 1, "Should return exactly one row");
    println!("Result: {:?}", result.rows[0].values);
    if let Some(count_val) = result.rows[0].values.first() {
        if count_val.is_null() {
            panic!("count(*) returned null, expected 1");
        }
        let count = count_val.as_i64().unwrap();
        assert_eq!(
            count, 1,
            "count(*) without MATCH should return 1, got {}",
            count
        );
    } else {
        panic!("No value returned");
    }

    Ok(())
}

#[test]
fn test_sum_literal_without_match() -> Result<(), Error> {
    let (mut engine, _temp_dir) = setup_test_engine()?;

    // RETURN sum(1) without MATCH should return 1 (not null)
    let result = engine.execute_cypher("RETURN sum(1) AS sum_val")?;

    assert_eq!(result.rows.len(), 1, "Should return exactly one row");
    println!("Result: {:?}", result.rows[0].values);
    if let Some(sum_val) = result.rows[0].values.first() {
        if sum_val.is_null() {
            panic!("sum(1) returned null, expected 1");
        }
        let sum = sum_val.as_i64().unwrap();
        assert_eq!(sum, 1, "sum(1) without MATCH should return 1, got {}", sum);
    } else {
        panic!("No value returned");
    }

    Ok(())
}

#[test]
fn test_avg_literal_without_match() -> Result<(), Error> {
    let (mut engine, _temp_dir) = setup_test_engine()?;

    // RETURN avg(10) without MATCH should return 10 (not null)
    let result = engine.execute_cypher("RETURN avg(10) AS avg_val")?;

    assert_eq!(result.rows.len(), 1, "Should return exactly one row");
    println!("Result: {:?}", result.rows[0].values);
    if let Some(avg_val) = result.rows[0].values.first() {
        if avg_val.is_null() {
            panic!("avg(10) returned null, expected 10.0");
        }
        let avg = avg_val.as_f64().unwrap();
        assert_eq!(
            avg, 10.0,
            "avg(10) without MATCH should return 10.0, got {}",
            avg
        );
    } else {
        panic!("No value returned");
    }

    Ok(())
}
