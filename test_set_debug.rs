// Simple debug test for SET clause persistence
use std::process::Command;

fn main() {
    // Test SET persistence
    println!("=== SET Debug Test ===\n");

    // 1. Create a node
    println!("1. Creating node...");
    let create = Command::new("curl")
        .args(["-s", "-X", "POST", "http://localhost:15474/cypher",
               "-H", "Content-Type: application/json",
               "-d", r#"{"query": "CREATE (n:DebugItem {name: 'test', value: 1}) RETURN n.value"}"#])
        .output()
        .expect("Failed to execute curl");
    println!("Create result: {}\n", String::from_utf8_lossy(&create.stdout));

    // 2. Verify initial value
    println!("2. Verifying initial value...");
    let verify1 = Command::new("curl")
        .args(["-s", "-X", "POST", "http://localhost:15474/cypher",
               "-H", "Content-Type: application/json",
               "-d", r#"{"query": "MATCH (n:DebugItem {name: 'test'}) RETURN n.value"}"#])
        .output()
        .expect("Failed to execute curl");
    println!("Initial value: {}\n", String::from_utf8_lossy(&verify1.stdout));

    // 3. SET new value
    println!("3. Setting new value...");
    let set_result = Command::new("curl")
        .args(["-s", "-X", "POST", "http://localhost:15474/cypher",
               "-H", "Content-Type: application/json",
               "-d", r#"{"query": "MATCH (n:DebugItem {name: 'test'}) SET n.value = 999 RETURN n.value"}"#])
        .output()
        .expect("Failed to execute curl");
    println!("SET result: {}\n", String::from_utf8_lossy(&set_result.stdout));

    // 4. Verify new value
    println!("4. Verifying new value...");
    let verify2 = Command::new("curl")
        .args(["-s", "-X", "POST", "http://localhost:15474/cypher",
               "-H", "Content-Type: application/json",
               "-d", r#"{"query": "MATCH (n:DebugItem {name: 'test'}) RETURN n.value"}"#])
        .output()
        .expect("Failed to execute curl");
    println!("After SET value: {}\n", String::from_utf8_lossy(&verify2.stdout));

    // 5. Cleanup
    println!("5. Cleanup...");
    let cleanup = Command::new("curl")
        .args(["-s", "-X", "POST", "http://localhost:15474/cypher",
               "-H", "Content-Type: application/json",
               "-d", r#"{"query": "MATCH (n:DebugItem) DELETE n"}"#])
        .output()
        .expect("Failed to execute curl");
    println!("Cleanup: {}", String::from_utf8_lossy(&cleanup.stdout));
}
