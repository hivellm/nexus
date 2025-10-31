use nexus_core::{Engine, Error};
use tempfile::TempDir;

fn setup_test_engine() -> Result<(Engine, TempDir), Error> {
    let temp_dir = tempfile::tempdir().map_err(|e| Error::Io(e))?;
    let engine = Engine::with_data_dir(temp_dir.path())?;
    Ok((engine, temp_dir))
}

fn setup_test_data(engine: &mut Engine) -> Result<(), Error> {
    // Create diverse data for COUNT DISTINCT testing
    engine.execute_cypher("CREATE (p:Person {name: 'Alice', age: 30, city: 'NYC'})")?;
    engine.execute_cypher("CREATE (p:Person {name: 'Bob', age: 25, city: 'LA'})")?;
    engine.execute_cypher("CREATE (p:Person {name: 'Charlie', age: 30, city: 'NYC'})")?;
    engine.execute_cypher("CREATE (p:Person {name: 'David', age: 25, city: 'SF'})")?;
    engine.execute_cypher("CREATE (p:Person {name: 'Eve', age: 35, city: 'NYC'})")?;
    engine.execute_cypher("CREATE (p:Person {name: 'Frank', age: 30, city: 'LA'})")?;
    engine.execute_cypher("CREATE (p:Person {name: 'Grace', age: 25, city: 'NYC'})")?;
    engine.execute_cypher("CREATE (p:Person {name: 'Henry', age: 40, city: 'SF'})")?;
    
    // Products with duplicate prices
    engine.execute_cypher("CREATE (p:Product {name: 'Laptop', price: 1000, category: 'Electronics'})")?;
    engine.execute_cypher("CREATE (p:Product {name: 'Mouse', price: 25, category: 'Electronics'})")?;
    engine.execute_cypher("CREATE (p:Product {name: 'Keyboard', price: 75, category: 'Electronics'})")?;
    engine.execute_cypher("CREATE (p:Product {name: 'Monitor', price: 300, category: 'Electronics'})")?;
    engine.execute_cypher("CREATE (p:Product {name: 'Desk', price: 300, category: 'Furniture'})")?;
    engine.execute_cypher("CREATE (p:Product {name: 'Chair', price: 150, category: 'Furniture'})")?;
    
    engine.refresh_executor()?;
    Ok(())
}

#[test]
fn test_count_distinct_basic() -> Result<(), Error> {
    let (mut engine, _temp_dir) = setup_test_engine()?;
    setup_test_data(&mut engine)?;
    
    let result = engine.execute_cypher("MATCH (p:Person) RETURN count(DISTINCT p.age) AS unique_ages")?;
    assert_eq!(result.rows.len(), 1);
    
    let count = result.rows[0].values[0].as_i64().unwrap();
    assert_eq!(count, 4, "Should have 4 distinct ages: 25, 30, 35, 40");
    
    Ok(())
}

#[test]
fn test_count_distinct_vs_regular_count() -> Result<(), Error> {
    let (mut engine, _temp_dir) = setup_test_engine()?;
    setup_test_data(&mut engine)?;
    
    let distinct_result = engine.execute_cypher("MATCH (p:Person) RETURN count(DISTINCT p.age) AS count")?;
    let regular_result = engine.execute_cypher("MATCH (p:Person) RETURN count(p.age) AS count")?;
    
    let distinct_count = distinct_result.rows[0].values[0].as_i64().unwrap();
    let regular_count = regular_result.rows[0].values[0].as_i64().unwrap();
    
    assert_eq!(distinct_count, 4, "Distinct ages");
    assert_eq!(regular_count, 8, "Total people");
    assert!(distinct_count < regular_count, "DISTINCT should be less than regular count");
    
    Ok(())
}

#[test]
fn test_count_distinct_city() -> Result<(), Error> {
    let (mut engine, _temp_dir) = setup_test_engine()?;
    setup_test_data(&mut engine)?;
    
    let result = engine.execute_cypher("MATCH (p:Person) RETURN count(DISTINCT p.city) AS unique_cities")?;
    let count = result.rows[0].values[0].as_i64().unwrap();
    
    assert_eq!(count, 3, "Should have 3 distinct cities: NYC, LA, SF");
    
    Ok(())
}

#[test]
fn test_count_distinct_multiple_labels() -> Result<(), Error> {
    let (mut engine, _temp_dir) = setup_test_engine()?;
    
    engine.execute_cypher("CREATE (p:Person:Employee {name: 'Alice', dept: 'Engineering'})")?;
    engine.execute_cypher("CREATE (p:Person:Employee {name: 'Bob', dept: 'Sales'})")?;
    engine.execute_cypher("CREATE (p:Person:Employee {name: 'Charlie', dept: 'Engineering'})")?;
    engine.execute_cypher("CREATE (p:Person:Manager {name: 'David', dept: 'Engineering'})")?;
    engine.refresh_executor()?;
    
    let result = engine.execute_cypher("MATCH (p:Person:Employee) RETURN count(DISTINCT p.dept) AS unique_depts")?;
    let count = result.rows[0].values[0].as_i64().unwrap();
    
    assert_eq!(count, 2, "Should have 2 distinct departments: Engineering, Sales");
    
    Ok(())
}

#[test]
fn test_count_distinct_with_where() -> Result<(), Error> {
    let (mut engine, _temp_dir) = setup_test_engine()?;
    setup_test_data(&mut engine)?;
    
    let result = engine.execute_cypher("MATCH (p:Person) WHERE p.age >= 30 RETURN count(DISTINCT p.city) AS cities")?;
    let count = result.rows[0].values[0].as_i64().unwrap();
    
    assert_eq!(count, 3, "People age >= 30 are in 3 cities");
    
    Ok(())
}

#[test]
fn test_count_distinct_products_price() -> Result<(), Error> {
    let (mut engine, _temp_dir) = setup_test_engine()?;
    setup_test_data(&mut engine)?;
    
    let result = engine.execute_cypher("MATCH (p:Product) RETURN count(DISTINCT p.price) AS unique_prices")?;
    let count = result.rows[0].values[0].as_i64().unwrap();
    
    assert_eq!(count, 5, "Should have 5 distinct prices: 25, 75, 150, 300, 1000");
    
    Ok(())
}

#[test]
fn test_count_distinct_by_category() -> Result<(), Error> {
    let (mut engine, _temp_dir) = setup_test_engine()?;
    setup_test_data(&mut engine)?;
    
    let result = engine.execute_cypher("MATCH (p:Product) WHERE p.category = 'Electronics' RETURN count(DISTINCT p.price) AS prices")?;
    let count = result.rows[0].values[0].as_i64().unwrap();
    
    assert_eq!(count, 4, "Electronics have 4 distinct prices");
    
    Ok(())
}

#[test]
fn test_count_distinct_empty_result() -> Result<(), Error> {
    let (mut engine, _temp_dir) = setup_test_engine()?;
    setup_test_data(&mut engine)?;
    
    let result = engine.execute_cypher("MATCH (p:NonExistent) RETURN count(DISTINCT p.name) AS count")?;
    let count = result.rows[0].values[0].as_i64().unwrap();
    
    assert_eq!(count, 0, "Non-existent label should return 0");
    
    Ok(())
}

#[test]
fn test_count_distinct_all_same_value() -> Result<(), Error> {
    let (mut engine, _temp_dir) = setup_test_engine()?;
    
    engine.execute_cypher("CREATE (p:Item {type: 'A'})")?;
    engine.execute_cypher("CREATE (p:Item {type: 'A'})")?;
    engine.execute_cypher("CREATE (p:Item {type: 'A'})")?;
    engine.refresh_executor()?;
    
    let result = engine.execute_cypher("MATCH (i:Item) RETURN count(DISTINCT i.type) AS unique_types")?;
    let count = result.rows[0].values[0].as_i64().unwrap();
    
    assert_eq!(count, 1, "All same values should return 1");
    
    Ok(())
}

#[test]
fn test_count_distinct_with_null_values() -> Result<(), Error> {
    let (mut engine, _temp_dir) = setup_test_engine()?;
    
    engine.execute_cypher("CREATE (p:Node {value: 10})")?;
    engine.execute_cypher("CREATE (p:Node {value: 20})")?;
    engine.execute_cypher("CREATE (p:Node {value: 10})")?;
    engine.execute_cypher("CREATE (p:Node {})")?; // No value property
    engine.refresh_executor()?;
    
    let result = engine.execute_cypher("MATCH (n:Node) RETURN count(DISTINCT n.value) AS unique_values")?;
    let count = result.rows[0].values[0].as_i64().unwrap();
    
    assert_eq!(count, 2, "Should count 2 distinct non-null values: 10, 20");
    
    Ok(())
}

#[test]
fn test_count_distinct_numeric_values() -> Result<(), Error> {
    let (mut engine, _temp_dir) = setup_test_engine()?;
    
    for i in 0..10 {
        let age = (i % 3) * 10 + 20; // Creates ages: 20, 30, 40, 20, 30, 40, ...
        engine.execute_cypher(&format!("CREATE (p:Test {{age: {}}})", age))?;
    }
    engine.refresh_executor()?;
    
    let result = engine.execute_cypher("MATCH (t:Test) RETURN count(DISTINCT t.age) AS ages")?;
    let count = result.rows[0].values[0].as_i64().unwrap();
    
    assert_eq!(count, 3, "Should have 3 distinct ages: 20, 30, 40");
    
    Ok(())
}

#[test]
fn test_count_distinct_string_values() -> Result<(), Error> {
    let (mut engine, _temp_dir) = setup_test_engine()?;
    
    let names = vec!["Alice", "Bob", "Alice", "Charlie", "Bob", "Alice", "David"];
    for name in names {
        engine.execute_cypher(&format!("CREATE (p:User {{name: '{}'}})", name))?;
    }
    engine.refresh_executor()?;
    
    let result = engine.execute_cypher("MATCH (u:User) RETURN count(DISTINCT u.name) AS unique_names")?;
    let count = result.rows[0].values[0].as_i64().unwrap();
    
    assert_eq!(count, 4, "Should have 4 distinct names: Alice, Bob, Charlie, David");
    
    Ok(())
}

#[test]
fn test_count_distinct_large_dataset() -> Result<(), Error> {
    let (mut engine, _temp_dir) = setup_test_engine()?;
    
    // Create 100 nodes with 10 distinct values
    for i in 0..100 {
        let value = i % 10;
        engine.execute_cypher(&format!("CREATE (n:BigData {{value: {}}})", value))?;
    }
    engine.refresh_executor()?;
    
    let result = engine.execute_cypher("MATCH (n:BigData) RETURN count(DISTINCT n.value) AS distinct_values")?;
    let count = result.rows[0].values[0].as_i64().unwrap();
    
    assert_eq!(count, 10, "Should have 10 distinct values from 0-9");
    
    Ok(())
}

#[test]
fn test_count_distinct_case_sensitive() -> Result<(), Error> {
    let (mut engine, _temp_dir) = setup_test_engine()?;
    
    engine.execute_cypher("CREATE (p:Text {word: 'hello'})")?;
    engine.execute_cypher("CREATE (p:Text {word: 'Hello'})")?;
    engine.execute_cypher("CREATE (p:Text {word: 'HELLO'})")?;
    engine.execute_cypher("CREATE (p:Text {word: 'hello'})")?;
    engine.refresh_executor()?;
    
    let result = engine.execute_cypher("MATCH (t:Text) RETURN count(DISTINCT t.word) AS words")?;
    let count = result.rows[0].values[0].as_i64().unwrap();
    
    assert_eq!(count, 3, "Case-sensitive distinct: hello, Hello, HELLO");
    
    Ok(())
}

#[test]
fn test_count_distinct_with_limit() -> Result<(), Error> {
    let (mut engine, _temp_dir) = setup_test_engine()?;
    setup_test_data(&mut engine)?;
    
    // LIMIT should not affect COUNT DISTINCT result
    let result = engine.execute_cypher("MATCH (p:Person) RETURN count(DISTINCT p.age) AS ages LIMIT 1")?;
    let count = result.rows[0].values[0].as_i64().unwrap();
    
    assert_eq!(count, 4, "LIMIT should not affect aggregation");
    
    Ok(())
}

