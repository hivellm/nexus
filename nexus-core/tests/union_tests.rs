use nexus_core::{Engine, Error};
use tempfile::TempDir;

fn setup_test_engine() -> Result<(Engine, TempDir), Error> {
    let temp_dir = tempfile::tempdir().map_err(|e| Error::Io(e.to_string()))?;
    let engine = Engine::with_data_dir(temp_dir.path())?;
    Ok((engine, temp_dir))
}

fn setup_test_data(engine: &Engine) -> Result<(), Error> {
    // People
    engine.execute_cypher("CREATE (p:Person {name: 'Alice', age: 30})")?;
    engine.execute_cypher("CREATE (p:Person {name: 'Bob', age: 25})")?;
    engine.execute_cypher("CREATE (p:Person {name: 'Charlie', age: 35})")?;
    
    // Companies
    engine.execute_cypher("CREATE (c:Company {name: 'Acme Inc'})")?;
    engine.execute_cypher("CREATE (c:Company {name: 'TechCorp'})")?;
    
    // Products
    engine.execute_cypher("CREATE (p:Product {name: 'Laptop', price: 1000})")?;
    engine.execute_cypher("CREATE (p:Product {name: 'Mouse', price: 25})")?;
    engine.execute_cypher("CREATE (p:Product {name: 'Keyboard', price: 75})")?;
    
    engine.refresh_executor()?;
    Ok(())
}

#[test]
fn test_union_basic() -> Result<(), Error> {
    let (engine, _temp_dir) = setup_test_engine()?;
    setup_test_data(&engine)?;
    
    let result = engine.execute_cypher(
        "MATCH (p:Person) RETURN p.name AS name UNION MATCH (c:Company) RETURN c.name AS name"
    )?;
    
    assert_eq!(result.rows.len(), 5, "Should have 5 unique names: 3 people + 2 companies");
    
    Ok(())
}

#[test]
fn test_union_all() -> Result<(), Error> {
    let (engine, _temp_dir) = setup_test_engine()?;
    
    engine.execute_cypher("CREATE (n:A {value: 1})")?;
    engine.execute_cypher("CREATE (n:A {value: 2})")?;
    engine.execute_cypher("CREATE (n:B {value: 1})")?;
    engine.execute_cypher("CREATE (n:B {value: 3})")?;
    engine.refresh_executor()?;
    
    let result = engine.execute_cypher(
        "MATCH (a:A) RETURN a.value AS val UNION ALL MATCH (b:B) RETURN b.value AS val"
    )?;
    
    assert_eq!(result.rows.len(), 4, "UNION ALL should keep all rows including duplicates");
    
    Ok(())
}

#[test]
fn test_union_vs_union_all_duplicates() -> Result<(), Error> {
    let (engine, _temp_dir) = setup_test_engine()?;
    
    engine.execute_cypher("CREATE (n:X {val: 10})")?;
    engine.execute_cypher("CREATE (n:X {val: 20})")?;
    engine.execute_cypher("CREATE (n:Y {val: 10})")?;
    engine.execute_cypher("CREATE (n:Y {val: 30})")?;
    engine.refresh_executor()?;
    
    let union_result = engine.execute_cypher(
        "MATCH (x:X) RETURN x.val AS v UNION MATCH (y:Y) RETURN y.val AS v"
    )?;
    
    let union_all_result = engine.execute_cypher(
        "MATCH (x:X) RETURN x.val AS v UNION ALL MATCH (y:Y) RETURN y.val AS v"
    )?;
    
    assert_eq!(union_result.rows.len(), 3, "UNION should deduplicate: 10, 20, 30");
    assert_eq!(union_all_result.rows.len(), 4, "UNION ALL should keep all: 10, 20, 10, 30");
    
    Ok(())
}

#[test]
fn test_union_empty_left() -> Result<(), Error> {
    let (engine, _temp_dir) = setup_test_engine()?;
    setup_test_data(&engine)?;
    
    let result = engine.execute_cypher(
        "MATCH (n:NonExistent) RETURN n.name AS name UNION MATCH (p:Person) RETURN p.name AS name"
    )?;
    
    assert_eq!(result.rows.len(), 3, "Should return only right side results");
    
    Ok(())
}

#[test]
fn test_union_empty_right() -> Result<(), Error> {
    let (engine, _temp_dir) = setup_test_engine()?;
    setup_test_data(&engine)?;
    
    let result = engine.execute_cypher(
        "MATCH (p:Person) RETURN p.name AS name UNION MATCH (n:NonExistent) RETURN n.name AS name"
    )?;
    
    assert_eq!(result.rows.len(), 3, "Should return only left side results");
    
    Ok(())
}

#[test]
fn test_union_both_empty() -> Result<(), Error> {
    let (engine, _temp_dir) = setup_test_engine()?;
    setup_test_data(&engine)?;
    
    let result = engine.execute_cypher(
        "MATCH (n:NonExistent1) RETURN n.name AS name UNION MATCH (m:NonExistent2) RETURN m.name AS name"
    )?;
    
    assert_eq!(result.rows.len(), 0, "Both empty should return 0 rows");
    
    Ok(())
}

#[test]
fn test_union_three_way() -> Result<(), Error> {
    let (engine, _temp_dir) = setup_test_engine()?;
    setup_test_data(&engine)?;
    
    let result = engine.execute_cypher(
        "MATCH (p:Person) RETURN p.name AS name 
         UNION MATCH (c:Company) RETURN c.name AS name 
         UNION MATCH (pr:Product) RETURN pr.name AS name"
    )?;
    
    assert_eq!(result.rows.len(), 8, "Should have 3 people + 2 companies + 3 products");
    
    Ok(())
}

#[test]
fn test_union_with_where() -> Result<(), Error> {
    let (engine, _temp_dir) = setup_test_engine()?;
    setup_test_data(&engine)?;
    
    let result = engine.execute_cypher(
        "MATCH (p:Person) WHERE p.age >= 30 RETURN p.name AS name 
         UNION 
         MATCH (c:Company) RETURN c.name AS name"
    )?;
    
    assert_eq!(result.rows.len(), 4, "2 people (age >= 30) + 2 companies");
    
    Ok(())
}

#[test]
fn test_union_numeric_values() -> Result<(), Error> {
    let (engine, _temp_dir) = setup_test_engine()?;
    
    engine.execute_cypher("CREATE (n:Num1 {val: 10})")?;
    engine.execute_cypher("CREATE (n:Num1 {val: 20})")?;
    engine.execute_cypher("CREATE (n:Num2 {val: 20})")?;
    engine.execute_cypher("CREATE (n:Num2 {val: 30})")?;
    engine.refresh_executor()?;
    
    let result = engine.execute_cypher(
        "MATCH (n1:Num1) RETURN n1.val AS value UNION MATCH (n2:Num2) RETURN n2.val AS value"
    )?;
    
    assert_eq!(result.rows.len(), 3, "Should deduplicate: 10, 20, 30");
    
    Ok(())
}

#[test]
fn test_union_multiple_columns() -> Result<(), Error> {
    let (engine, _temp_dir) = setup_test_engine()?;
    
    engine.execute_cypher("CREATE (p:Person {name: 'Alice', age: 30})")?;
    engine.execute_cypher("CREATE (p:Person {name: 'Bob', age: 25})")?;
    engine.execute_cypher("CREATE (e:Employee {name: 'Alice', age: 30})")?; // Duplicate
    engine.execute_cypher("CREATE (e:Employee {name: 'Charlie', age: 35})")?;
    engine.refresh_executor()?;
    
    let result = engine.execute_cypher(
        "MATCH (p:Person) RETURN p.name AS name, p.age AS age 
         UNION 
         MATCH (e:Employee) RETURN e.name AS name, e.age AS age"
    )?;
    
    assert_eq!(result.rows.len(), 3, "Should deduplicate (Alice, 30): Bob, Alice, Charlie");
    
    Ok(())
}

#[test]
fn test_union_identical_queries() -> Result<(), Error> {
    let (engine, _temp_dir) = setup_test_engine()?;
    setup_test_data(&engine)?;
    
    let result = engine.execute_cypher(
        "MATCH (p:Person) RETURN p.name AS name UNION MATCH (p:Person) RETURN p.name AS name"
    )?;
    
    assert_eq!(result.rows.len(), 3, "Identical queries should deduplicate to original count");
    
    Ok(())
}

#[test]
fn test_union_mixed_types() -> Result<(), Error> {
    let (engine, _temp_dir) = setup_test_engine()?;
    
    engine.execute_cypher("CREATE (n:TypeA {value: 'text'})")?;
    engine.execute_cypher("CREATE (n:TypeB {value: 'text'})")?; // Same value
    engine.execute_cypher("CREATE (n:TypeC {value: 'different'})")?;
    engine.refresh_executor()?;
    
    let result = engine.execute_cypher(
        "MATCH (a:TypeA) RETURN a.value AS val 
         UNION MATCH (b:TypeB) RETURN b.value AS val 
         UNION MATCH (c:TypeC) RETURN c.value AS val"
    )?;
    
    assert_eq!(result.rows.len(), 2, "Should deduplicate: 'text', 'different'");
    
    Ok(())
}

#[test]
fn test_union_order_independence() -> Result<(), Error> {
    let (engine, _temp_dir) = setup_test_engine()?;
    setup_test_data(&engine)?;
    
    let result1 = engine.execute_cypher(
        "MATCH (p:Person) RETURN p.name AS name UNION MATCH (c:Company) RETURN c.name AS name"
    )?;
    
    let result2 = engine.execute_cypher(
        "MATCH (c:Company) RETURN c.name AS name UNION MATCH (p:Person) RETURN p.name AS name"
    )?;
    
    assert_eq!(result1.rows.len(), result2.rows.len(), "Order should not matter for UNION");
    
    Ok(())
}

#[test]
fn test_union_with_nulls() -> Result<(), Error> {
    let (engine, _temp_dir) = setup_test_engine()?;
    
    engine.execute_cypher("CREATE (n:NodeA {value: 10})")?;
    engine.execute_cypher("CREATE (n:NodeB {})")?; // No value property
    engine.execute_cypher("CREATE (n:NodeC {value: 10})")?;
    engine.refresh_executor()?;
    
    let result = engine.execute_cypher(
        "MATCH (a:NodeA) RETURN a.value AS val 
         UNION MATCH (b:NodeB) RETURN b.value AS val 
         UNION MATCH (c:NodeC) RETURN c.value AS val"
    )?;
    
    // Should have at least 1 row (the value 10, may also have null)
    assert!(result.rows.len() >= 1, "Should handle null values in UNION");
    
    Ok(())
}

#[test]
fn test_union_large_result_sets() -> Result<(), Error> {
    let (engine, _temp_dir) = setup_test_engine()?;
    
    // Create 50 nodes in Set1 with values 0-49
    for i in 0..50 {
        engine.execute_cypher(&format!("CREATE (n:Set1 {{val: {}}})", i))?;
    }
    
    // Create 50 nodes in Set2 with values 25-74 (overlaps 25-49)
    for i in 25..75 {
        engine.execute_cypher(&format!("CREATE (n:Set2 {{val: {}}})", i))?;
    }
    
    engine.refresh_executor()?;
    
    let result = engine.execute_cypher(
        "MATCH (s1:Set1) RETURN s1.val AS value UNION MATCH (s2:Set2) RETURN s2.val AS value"
    )?;
    
    assert_eq!(result.rows.len(), 75, "Should have 75 unique values (0-74)");
    
    Ok(())
}

