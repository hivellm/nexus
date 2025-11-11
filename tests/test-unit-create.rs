use nexus_core::{Engine, Error};
use tempfile::TempDir;

fn main() -> Result<(), Error> {
    let temp_dir = tempfile::tempdir().map_err(|e| Error::Io(e))?;
    let mut engine = Engine::with_data_dir(temp_dir.path())?;
    
    println!("Testing CREATE...");
    match engine.execute_cypher("CREATE (p:Person {name: 'Alice'})") {
        Ok(_) => println!("✅ CREATE worked!"),
        Err(e) => {
            println!("❌ CREATE failed: {:?}", e);
            return Err(e);
        }
    }
    
    println!("\nTesting MATCH...");
    match engine.execute_cypher("MATCH (p:Person) RETURN count(*) AS c") {
        Ok(result) => {
            println!("✅ MATCH worked! Rows: {}", result.rows.len());
            if result.rows.len() > 0 {
                println!("   Count: {:?}", result.rows[0].values[0]);
            }
        }
        Err(e) => {
            println!("❌ MATCH failed: {:?}", e);
            return Err(e);
        }
    }
    
    Ok(())
}















