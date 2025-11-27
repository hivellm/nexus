use nexus_core::{Engine, Error};

fn main() -> Result<(), Error> {
    let temp_dir = tempfile::tempdir().map_err(Error::Io)?;
    let mut engine = Engine::with_data_dir(temp_dir.path())?;
    
    engine.execute_cypher("CREATE (n:Node {value: 10})")?;
    engine.execute_cypher("CREATE (n:Node {value: 20})")?;
    engine.execute_cypher("CREATE (n:Node {})")?;
    engine.refresh_executor()?;
    
    // First check what nodes exist
    let nodes = engine.execute_cypher("MATCH (n:Node) RETURN n.value")?;
    println!("=== Nodes found ===");
    println!("Columns: {:?}", nodes.columns);
    for (i, row) in nodes.rows.iter().enumerate() {
        println!("Row {}: {:?}", i, row);
    }
    
    // Now try avg
    let result = engine.execute_cypher("MATCH (n:Node) RETURN avg(n.value) AS avg")?;
    println!("\n=== AVG result ===");
    println!("Columns: {:?}", result.columns);
    for (i, row) in result.rows.iter().enumerate() {
        println!("Row {}: {:?}", i, row);
    }
    
    // Check if value is f64 or something else
    if !result.rows.is_empty() && !result.rows[0].values.is_empty() {
        let val = &result.rows[0].values[0];
        println!("\nValue type debug:");
        println!("  is_null: {}", val.is_null());
        println!("  is_number: {}", val.is_number());
        println!("  is_i64: {}", val.is_i64());
        println!("  is_u64: {}", val.is_u64());
        println!("  is_f64: {}", val.is_f64());
        println!("  as_f64: {:?}", val.as_f64());
        println!("  as_i64: {:?}", val.as_i64());
    }
    
    // Test SUM
    let sum_result = engine.execute_cypher("MATCH (n:Node) RETURN sum(n.value) AS sum")?;
    println!("\n=== SUM result ===");
    println!("Columns: {:?}", sum_result.columns);
    for (i, row) in sum_result.rows.iter().enumerate() {
        println!("Row {}: {:?}", i, row);
    }
    
    // Test COUNT DISTINCT
    engine.execute_cypher("CREATE (n:Node {value: 1})")?;
    engine.execute_cypher("CREATE (n:Node {value: 2})")?;
    engine.execute_cypher("CREATE (n:Node {value: 1})")?;
    engine.refresh_executor()?;
    
    let count_result = engine.execute_cypher("MATCH (n:Node) RETURN count(DISTINCT n.value) AS count")?;
    println!("\n=== COUNT(DISTINCT) result ===");
    println!("Columns: {:?}", count_result.columns);
    for (i, row) in count_result.rows.iter().enumerate() {
        println!("Row {}: {:?}", i, row);
    }
    
    Ok(())
}

