use nexus_core::Engine;
use nexus_core::executor::parser::CypherParser;
use serde_json::Value;
use std::fs;
use std::path::Path;

fn create_engine() -> Engine {
    Engine::new().expect("Failed to create engine")
}

fn extract_first_row_value(result: nexus_core::executor::ResultSet) -> Option<Value> {
    result
        .rows
        .first()
        .and_then(|row| row.values.first().cloned())
}

#[test]
fn test_load_csv_parsing() {
    // Test LOAD CSV parsing
    let query = "LOAD CSV FROM 'file:///test.csv' AS row";
    let mut parser = CypherParser::new(query.to_string());
    let result = parser.parse();
    
    assert!(result.is_ok(), "LOAD CSV should parse successfully");
    let ast = result.unwrap();
    
    if let Some(nexus_core::executor::parser::Clause::LoadCsv(load_csv)) = ast.clauses.first() {
        assert_eq!(load_csv.url, "file:///test.csv");
        assert_eq!(load_csv.variable, "row");
        assert!(!load_csv.with_headers);
        assert_eq!(load_csv.field_terminator, None);
    } else {
        panic!("Should contain LoadCsv clause");
    }
}

#[test]
fn test_load_csv_with_headers_parsing() {
    // Test LOAD CSV WITH HEADERS parsing
    let query = "LOAD CSV FROM 'file:///test.csv' WITH HEADERS AS row";
    let mut parser = CypherParser::new(query.to_string());
    let result = parser.parse();
    
    assert!(result.is_ok(), "LOAD CSV WITH HEADERS should parse successfully");
    let ast = result.unwrap();
    
    if let Some(nexus_core::executor::parser::Clause::LoadCsv(load_csv)) = ast.clauses.first() {
        assert_eq!(load_csv.url, "file:///test.csv");
        assert_eq!(load_csv.variable, "row");
        assert!(load_csv.with_headers);
    } else {
        panic!("Should contain LoadCsv clause");
    }
}

#[test]
fn test_load_csv_with_fieldterminator_parsing() {
    // Test LOAD CSV WITH FIELDTERMINATOR parsing
    let query = "LOAD CSV FROM 'file:///test.csv' FIELDTERMINATOR ';' AS row";
    let mut parser = CypherParser::new(query.to_string());
    let result = parser.parse();
    
    assert!(result.is_ok(), "LOAD CSV WITH FIELDTERMINATOR should parse successfully");
    let ast = result.unwrap();
    
    if let Some(nexus_core::executor::parser::Clause::LoadCsv(load_csv)) = ast.clauses.first() {
        assert_eq!(load_csv.url, "file:///test.csv");
        assert_eq!(load_csv.variable, "row");
        assert_eq!(load_csv.field_terminator, Some(";".to_string()));
    } else {
        panic!("Should contain LoadCsv clause");
    }
}

#[test]
fn test_load_csv_execution() {
    let mut engine = create_engine();
    
    // Create a temporary CSV file
    let csv_content = "Alice,30\nBob,25\nCharlie,35";
    let temp_dir = std::env::temp_dir();
    let csv_path = temp_dir.join("test_load_csv.csv");
    fs::write(&csv_path, csv_content).expect("Failed to write test CSV");
    
    // Execute LOAD CSV
    let query = format!("LOAD CSV FROM 'file://{}' AS row RETURN row", csv_path.display());
    let result = engine.execute_cypher(&query).unwrap();
    
    assert_eq!(result.columns, vec!["row"]);
    assert_eq!(result.rows.len(), 3);
    
    // Verify first row
    if let Some(Value::Array(fields)) = extract_first_row_value(result.clone()) {
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0], Value::String("Alice".to_string()));
        assert_eq!(fields[1], Value::String("30".to_string()));
    } else {
        panic!("Expected array for CSV row");
    }
    
    // Cleanup
    let _ = fs::remove_file(&csv_path);
}

#[test]
fn test_load_csv_with_headers_execution() {
    let mut engine = create_engine();
    
    // Create a temporary CSV file with headers
    let csv_content = "name,age\nAlice,30\nBob,25";
    let temp_dir = std::env::temp_dir();
    let csv_path = temp_dir.join("test_load_csv_headers.csv");
    fs::write(&csv_path, csv_content).expect("Failed to write test CSV");
    
    // Execute LOAD CSV WITH HEADERS
    let query = format!("LOAD CSV FROM 'file://{}' WITH HEADERS AS row RETURN row", csv_path.display());
    let result = engine.execute_cypher(&query).unwrap();
    
    assert_eq!(result.columns, vec!["row"]);
    // Should have 2 rows (header skipped)
    assert_eq!(result.rows.len(), 2);
    
    // Cleanup
    let _ = fs::remove_file(&csv_path);
}

#[test]
fn test_load_csv_nonexistent_file() {
    let mut engine = create_engine();
    
    // Try to load non-existent CSV file
    let query = "LOAD CSV FROM 'file:///nonexistent.csv' AS row RETURN row";
    let result = engine.execute_cypher(query);
    
    assert!(result.is_err(), "Should fail for non-existent file");
    assert!(result.unwrap_err().to_string().contains("not found"));
}

