use nexus_core::{Engine, Error, executor::ResultSet};
use serde_json::json;
use tempfile::TempDir;

/// Helper to execute query and return result
fn execute_query(engine: &mut Engine, query: &str) -> Result<ResultSet, Error> {
    engine.execute_cypher(query)
}

/// Helper to convert ResultSet to JSON for easier assertions
fn result_to_json(result: &ResultSet) -> serde_json::Value {
    json!({
        "columns": result.columns,
        "rows": result.rows.iter().map(|row| &row.values).collect::<Vec<_>>()
    })
}

#[test]
fn test_unwind_basic_list_literal() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    let result = execute_query(&mut engine, "UNWIND [1, 2, 3] AS x RETURN x").unwrap();
    let json_result = result_to_json(&result);

    let columns = json_result["columns"].as_array().unwrap();
    assert_eq!(columns.len(), 1);
    assert_eq!(columns[0], "x");

    let rows = json_result["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0][0], 1);
    assert_eq!(rows[1][0], 2);
    assert_eq!(rows[2][0], 3);
}

#[test]
fn test_unwind_with_strings() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    let result = execute_query(
        &mut engine,
        "UNWIND ['apple', 'banana', 'cherry'] AS fruit RETURN fruit",
    )
    .unwrap();
    let json_result = result_to_json(&result);

    let rows = json_result["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0][0], "apple");
    assert_eq!(rows[1][0], "banana");
    assert_eq!(rows[2][0], "cherry");
}

#[test]
fn test_unwind_empty_list() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    let result = execute_query(&mut engine, "UNWIND [] AS x RETURN x").unwrap();
    let json_result = result_to_json(&result);

    let rows = json_result["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 0); // Empty list produces no rows
}

#[test]
fn test_unwind_null_list() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    let result = execute_query(&mut engine, "UNWIND null AS x RETURN x").unwrap();
    let json_result = result_to_json(&result);

    let rows = json_result["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 0); // NULL list produces no rows
}

#[test]
#[ignore = "CREATE with array properties not yet supported"]
fn test_unwind_with_variable_reference() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    // Create nodes with list properties
    execute_query(
        &mut engine,
        "CREATE (p:Person {name: 'Alice', hobbies: ['reading', 'coding', 'gaming']})",
    )
    .unwrap();

    execute_query(
        &mut engine,
        "CREATE (p:Person {name: 'Bob', hobbies: ['cooking', 'sports']})",
    )
    .unwrap();

    // UNWIND with variable reference - expand hobbies for each person
    let result = execute_query(
        &mut engine,
        "MATCH (p:Person) UNWIND p.hobbies AS hobby RETURN p.name, hobby ORDER BY p.name, hobby",
    )
    .unwrap();
    let json_result = result_to_json(&result);

    let rows = json_result["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 5); // Alice:3 + Bob:2

    // Alice's hobbies (sorted)
    assert_eq!(rows[0][0], "Alice");
    assert_eq!(rows[0][1], "coding");
    assert_eq!(rows[1][0], "Alice");
    assert_eq!(rows[1][1], "gaming");
    assert_eq!(rows[2][0], "Alice");
    assert_eq!(rows[2][1], "reading");

    // Bob's hobbies (sorted)
    assert_eq!(rows[3][0], "Bob");
    assert_eq!(rows[3][1], "cooking");
    assert_eq!(rows[4][0], "Bob");
    assert_eq!(rows[4][1], "sports");
}

#[test]
#[ignore = "WHERE after UNWIND needs operator reordering - known limitation"]
fn test_unwind_with_where_filtering() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    let result = execute_query(
        &mut engine,
        "UNWIND [1, 2, 3, 4, 5] AS num WHERE num > 2 RETURN num",
    )
    .unwrap();
    let json_result = result_to_json(&result);

    let rows = json_result["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0][0], 3);
    assert_eq!(rows[1][0], 4);
    assert_eq!(rows[2][0], 5);
}

#[test]
#[ignore = "CREATE with array properties not yet supported"]
fn test_unwind_with_match_and_where() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    // Create test data
    execute_query(
        &mut engine,
        "CREATE (p:Person {name: 'Alice', tags: ['developer', 'reader', 'gamer']})",
    )
    .unwrap();

    execute_query(
        &mut engine,
        "CREATE (p:Person {name: 'Bob', tags: ['designer', 'artist']})",
    )
    .unwrap();

    // UNWIND with WHERE filtering on unwound values
    let result = execute_query(
        &mut engine,
        "MATCH (p:Person) UNWIND p.tags AS tag WHERE tag = 'developer' OR tag = 'designer' RETURN p.name, tag ORDER BY p.name",
    )
    .unwrap();
    let json_result = result_to_json(&result);

    let rows = json_result["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0][0], "Alice");
    assert_eq!(rows[0][1], "developer");
    assert_eq!(rows[1][0], "Bob");
    assert_eq!(rows[1][1], "designer");
}

#[test]
#[ignore = "CREATE with array properties not yet supported"]
fn test_unwind_in_complex_query() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    // Create test data
    execute_query(
        &mut engine,
        "CREATE (p:Person {name: 'Alice', skills: ['Rust', 'Python', 'JavaScript']})",
    )
    .unwrap();

    execute_query(
        &mut engine,
        "CREATE (p:Person {name: 'Bob', skills: ['Python', 'Go']})",
    )
    .unwrap();

    execute_query(
        &mut engine,
        "CREATE (p:Person {name: 'Charlie', skills: ['Rust', 'C++']})",
    )
    .unwrap();

    // Complex query: MATCH -> UNWIND -> WHERE -> GROUP BY -> RETURN
    let result = execute_query(
        &mut engine,
        "MATCH (p:Person) 
         UNWIND p.skills AS skill 
         RETURN skill, count(p.name) AS developers
         ORDER BY developers DESC, skill",
    )
    .unwrap();
    let json_result = result_to_json(&result);

    let rows = json_result["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 5); // 5 unique skills

    // Python: 2 developers
    assert_eq!(rows[0][0], "Python");
    assert_eq!(rows[0][1], 2);

    // Rust: 2 developers
    assert_eq!(rows[1][0], "Rust");
    assert_eq!(rows[1][1], 2);

    // C++, Go, JavaScript: 1 each (sorted by name after count)
    assert_eq!(rows[2][0], "C++");
    assert_eq!(rows[2][1], 1);
    assert_eq!(rows[3][0], "Go");
    assert_eq!(rows[3][1], 1);
    assert_eq!(rows[4][0], "JavaScript");
    assert_eq!(rows[4][1], 1);
}

#[test]
#[ignore = "Nested UNWIND needs proper variable binding - known issue"]
fn test_unwind_nested_lists() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    let result = execute_query(
        &mut engine,
        "UNWIND [[1, 2], [3, 4]] AS inner_list 
         UNWIND inner_list AS num 
         RETURN num",
    )
    .unwrap();
    let json_result = result_to_json(&result);

    let rows = json_result["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[0][0], 1);
    assert_eq!(rows[1][0], 2);
    assert_eq!(rows[2][0], 3);
    assert_eq!(rows[3][0], 4);
}

#[test]
#[ignore = "Aggregation after UNWIND needs operator reordering - known limitation"]
fn test_unwind_with_aggregation() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    let result = execute_query(
        &mut engine,
        "UNWIND [1, 2, 3, 4, 5] AS num 
         RETURN sum(num) AS total, avg(num) AS average, count(num) AS count",
    )
    .unwrap();
    let json_result = result_to_json(&result);

    let rows = json_result["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0][0], 15); // sum
    assert_eq!(rows[0][1], 3.0); // average
    assert_eq!(rows[0][2], 5); // count
}

#[test]
fn test_unwind_creates_cartesian_product() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    // Create two nodes
    execute_query(&mut engine, "CREATE (p:Person {name: 'Alice'})").unwrap();
    execute_query(&mut engine, "CREATE (p:Person {name: 'Bob'})").unwrap();

    // MATCH returns 2 rows, UNWIND expands to 2 * 3 = 6 rows
    let result = execute_query(
        &mut engine,
        "MATCH (p:Person) UNWIND [1, 2, 3] AS num RETURN p.name, num ORDER BY p.name, num",
    )
    .unwrap();
    let json_result = result_to_json(&result);

    let rows = json_result["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 6);

    // Alice with each number
    assert_eq!(rows[0][0], "Alice");
    assert_eq!(rows[0][1], 1);
    assert_eq!(rows[1][0], "Alice");
    assert_eq!(rows[1][1], 2);
    assert_eq!(rows[2][0], "Alice");
    assert_eq!(rows[2][1], 3);

    // Bob with each number
    assert_eq!(rows[3][0], "Bob");
    assert_eq!(rows[3][1], 1);
    assert_eq!(rows[4][0], "Bob");
    assert_eq!(rows[4][1], 2);
    assert_eq!(rows[5][0], "Bob");
    assert_eq!(rows[5][1], 3);
}

#[test]
#[ignore = "WHERE after UNWIND needs operator reordering - known limitation"]
fn test_unwind_with_null_in_list() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    let result = execute_query(
        &mut engine,
        "UNWIND [1, null, 3, null, 5] AS x WHERE x IS NOT NULL RETURN x",
    )
    .unwrap();
    let json_result = result_to_json(&result);

    let rows = json_result["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0][0], 1);
    assert_eq!(rows[1][0], 3);
    assert_eq!(rows[2][0], 5);
}
