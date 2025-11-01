use nexus_core::{Engine, executor::ResultSet};
use tempfile::TempDir;

fn execute_query(engine: &mut Engine, query: &str) -> ResultSet {
    engine.execute_cypher(query).expect("Query should succeed")
}

fn get_single_value(result: &ResultSet) -> &serde_json::Value {
    assert!(!result.rows.is_empty(), "Result has no rows!");
    assert!(
        !result.rows[0].values.is_empty(),
        "First row has no values!"
    );
    &result.rows[0].values[0]
}

// ============================================================================
// STRING FUNCTIONS
// ============================================================================

#[test]
fn test_tolower_function() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    let result = execute_query(&mut engine, "RETURN toLower('HELLO WORLD') AS lower");
    assert_eq!(get_single_value(&result), "hello world");

    let result = execute_query(&mut engine, "RETURN toLower('MiXeD CaSe') AS lower");
    assert_eq!(get_single_value(&result), "mixed case");
}

#[test]
fn test_toupper_function() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    let result = execute_query(&mut engine, "RETURN toUpper('hello world') AS upper");
    assert_eq!(get_single_value(&result), "HELLO WORLD");

    let result = execute_query(&mut engine, "RETURN toUpper('MiXeD CaSe') AS upper");
    assert_eq!(get_single_value(&result), "MIXED CASE");
}

#[test]
fn test_substring_function() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    // substring(string, start)
    let result = execute_query(&mut engine, "RETURN substring('hello world', 6) AS sub");
    assert_eq!(get_single_value(&result), "world");

    // substring(string, start, length)
    let result = execute_query(&mut engine, "RETURN substring('hello world', 0, 5) AS sub");
    assert_eq!(get_single_value(&result), "hello");

    let result = execute_query(&mut engine, "RETURN substring('hello world', 6, 3) AS sub");
    assert_eq!(get_single_value(&result), "wor");
}

#[test]
fn test_trim_functions() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    let result = execute_query(&mut engine, "RETURN trim('  hello  ') AS trimmed");
    assert_eq!(get_single_value(&result), "hello");

    let result = execute_query(&mut engine, "RETURN ltrim('  hello  ') AS trimmed");
    assert_eq!(get_single_value(&result), "hello  ");

    let result = execute_query(&mut engine, "RETURN rtrim('  hello  ') AS trimmed");
    assert_eq!(get_single_value(&result), "  hello");
}

#[test]
fn test_replace_function() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    let result = execute_query(
        &mut engine,
        "RETURN replace('hello world', 'world', 'Rust') AS replaced",
    );
    assert_eq!(get_single_value(&result), "hello Rust");

    let result = execute_query(
        &mut engine,
        "RETURN replace('foo bar foo', 'foo', 'baz') AS replaced",
    );
    assert_eq!(get_single_value(&result), "baz bar baz");
}

#[test]
fn test_split_function() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    let result = execute_query(&mut engine, "RETURN split('a,b,c', ',') AS parts");
    let value = get_single_value(&result);
    assert!(value.is_array());
    let arr = value.as_array().unwrap();
    assert_eq!(arr.len(), 3);
    assert_eq!(arr[0], "a");
    assert_eq!(arr[1], "b");
    assert_eq!(arr[2], "c");
}

// ============================================================================
// MATH FUNCTIONS
// ============================================================================

#[test]
fn test_abs_function() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    let result = execute_query(&mut engine, "RETURN abs(-42) AS absolute");
    assert_eq!(get_single_value(&result).as_f64().unwrap(), 42.0);

    let result = execute_query(&mut engine, "RETURN abs(2.5) AS absolute");
    assert_eq!(get_single_value(&result).as_f64().unwrap(), 2.5);

    let result = execute_query(&mut engine, "RETURN abs(-2.5) AS absolute");
    assert_eq!(get_single_value(&result).as_f64().unwrap(), 2.5);
}

#[test]
fn test_ceil_function() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    let result = execute_query(&mut engine, "RETURN ceil(3.2) AS ceiling");
    assert_eq!(get_single_value(&result), 4.0);

    let result = execute_query(&mut engine, "RETURN ceil(-1.5) AS ceiling");
    assert_eq!(get_single_value(&result), -1.0);
}

#[test]
fn test_floor_function() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    let result = execute_query(&mut engine, "RETURN floor(3.9) AS floored");
    assert_eq!(get_single_value(&result), 3.0);

    let result = execute_query(&mut engine, "RETURN floor(-1.5) AS floored");
    assert_eq!(get_single_value(&result), -2.0);
}

#[test]
fn test_round_function() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    let result = execute_query(&mut engine, "RETURN round(3.5) AS rounded");
    assert_eq!(get_single_value(&result), 4.0);

    let result = execute_query(&mut engine, "RETURN round(3.4) AS rounded");
    assert_eq!(get_single_value(&result), 3.0);
}

#[test]
fn test_sqrt_function() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    let result = execute_query(&mut engine, "RETURN sqrt(16) AS square_root");
    assert_eq!(get_single_value(&result), 4.0);

    let result = execute_query(&mut engine, "RETURN sqrt(9) AS square_root");
    assert_eq!(get_single_value(&result), 3.0);
}

#[test]
fn test_pow_function() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    let result = execute_query(&mut engine, "RETURN pow(2, 3) AS power");
    assert_eq!(get_single_value(&result), 8.0);

    let result = execute_query(&mut engine, "RETURN pow(10, 2) AS power");
    assert_eq!(get_single_value(&result), 100.0);
}

// ============================================================================
// TYPE CONVERSION FUNCTIONS
// ============================================================================

#[test]
fn test_tointeger_function() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    let result = execute_query(&mut engine, "RETURN toInteger('42') AS int");
    assert_eq!(get_single_value(&result), 42);

    let result = execute_query(&mut engine, "RETURN toInteger(2.7) AS int");
    assert_eq!(get_single_value(&result), 2);

    let result = execute_query(&mut engine, "RETURN toInteger('invalid') AS int");
    assert!(get_single_value(&result).is_null());
}

#[test]
fn test_tofloat_function() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    let result = execute_query(&mut engine, "RETURN toFloat('2.5') AS float");
    assert_eq!(get_single_value(&result), 2.5);

    let result = execute_query(&mut engine, "RETURN toFloat(42) AS float");
    assert_eq!(get_single_value(&result), 42.0);
}

#[test]
fn test_tostring_function() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    let result = execute_query(&mut engine, "RETURN toString(42) AS str");
    assert_eq!(get_single_value(&result), "42");

    let result = execute_query(&mut engine, "RETURN toString(2.5) AS str");
    assert_eq!(get_single_value(&result), "2.5");

    let result = execute_query(&mut engine, "RETURN toString(true) AS str");
    assert_eq!(get_single_value(&result), "true");
}

#[test]
fn test_toboolean_function() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    let result = execute_query(&mut engine, "RETURN toBoolean('true') AS bool");
    assert_eq!(get_single_value(&result), true);

    let result = execute_query(&mut engine, "RETURN toBoolean('false') AS bool");
    assert_eq!(get_single_value(&result), false);

    let result = execute_query(&mut engine, "RETURN toBoolean(1) AS bool");
    assert_eq!(get_single_value(&result), true);

    let result = execute_query(&mut engine, "RETURN toBoolean(0) AS bool");
    assert_eq!(get_single_value(&result), false);
}

// ============================================================================
// LIST FUNCTIONS
// ============================================================================

#[test]
fn test_size_function() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    let result = execute_query(&mut engine, "RETURN size([1, 2, 3, 4, 5]) AS length");
    assert_eq!(get_single_value(&result), 5);

    let result = execute_query(&mut engine, "RETURN size('hello') AS length");
    assert_eq!(get_single_value(&result), 5);

    let result = execute_query(&mut engine, "RETURN size([]) AS length");
    assert_eq!(get_single_value(&result), 0);
}

#[test]
fn test_head_function() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    let result = execute_query(&mut engine, "RETURN head([1, 2, 3]) AS first");
    assert_eq!(get_single_value(&result), 1);

    let result = execute_query(&mut engine, "RETURN head(['a', 'b', 'c']) AS first");
    assert_eq!(get_single_value(&result), "a");

    let result = execute_query(&mut engine, "RETURN head([]) AS first");
    assert!(get_single_value(&result).is_null());
}

#[test]
fn test_tail_function() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    let result = execute_query(&mut engine, "RETURN tail([1, 2, 3, 4]) AS rest");
    let arr = get_single_value(&result).as_array().unwrap();
    assert_eq!(arr.len(), 3);
    assert_eq!(arr[0], 2);
    assert_eq!(arr[1], 3);
    assert_eq!(arr[2], 4);

    let result = execute_query(&mut engine, "RETURN tail([1]) AS rest");
    let arr = get_single_value(&result).as_array().unwrap();
    assert_eq!(arr.len(), 0);
}

#[test]
fn test_last_function() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    let result = execute_query(&mut engine, "RETURN last([1, 2, 3]) AS final");
    assert_eq!(get_single_value(&result), 3);

    let result = execute_query(&mut engine, "RETURN last(['a', 'b', 'c']) AS final");
    assert_eq!(get_single_value(&result), "c");

    let result = execute_query(&mut engine, "RETURN last([]) AS final");
    assert!(get_single_value(&result).is_null());
}

#[test]
fn test_range_function() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    // range(start, end)
    let result = execute_query(&mut engine, "RETURN range(1, 5) AS numbers");
    let arr = get_single_value(&result).as_array().unwrap();
    assert_eq!(arr.len(), 5);
    assert_eq!(arr[0], 1);
    assert_eq!(arr[4], 5);

    // range(start, end, step)
    let result = execute_query(&mut engine, "RETURN range(0, 10, 2) AS numbers");
    let arr = get_single_value(&result).as_array().unwrap();
    assert_eq!(arr.len(), 6);
    assert_eq!(arr, &[0, 2, 4, 6, 8, 10]);

    // Negative step
    let result = execute_query(&mut engine, "RETURN range(10, 0, -2) AS numbers");
    let arr = get_single_value(&result).as_array().unwrap();
    assert_eq!(arr, &[10, 8, 6, 4, 2, 0]);
}

#[test]
fn test_reverse_function() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    let result = execute_query(&mut engine, "RETURN reverse([1, 2, 3]) AS reversed");
    let arr = get_single_value(&result).as_array().unwrap();
    assert_eq!(arr, &[3, 2, 1]);

    let result = execute_query(&mut engine, "RETURN reverse(['a', 'b', 'c']) AS reversed");
    let arr = get_single_value(&result).as_array().unwrap();
    assert_eq!(
        arr,
        &["a", "b", "c"]
            .iter()
            .rev()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
    );
}

// ============================================================================
// COMBINED/COMPLEX TESTS
// ============================================================================

#[test]
fn test_nested_string_functions() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    let result = execute_query(&mut engine, "RETURN toUpper(trim('  hello  ')) AS result");
    assert_eq!(get_single_value(&result), "HELLO");

    let result = execute_query(
        &mut engine,
        "RETURN substring(toLower('HELLO WORLD'), 6, 5) AS result",
    );
    assert_eq!(get_single_value(&result), "world");
}

#[test]
fn test_functions_with_nodes() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    // Create test node
    execute_query(&mut engine, "CREATE (p:Person {name: 'ALICE', age: 30})");

    // toLower on node property
    let result = execute_query(
        &mut engine,
        "MATCH (p:Person) RETURN toLower(p.name) AS lower_name",
    );
    assert_eq!(get_single_value(&result), "alice");

    // Math on node property
    let result = execute_query(
        &mut engine,
        "MATCH (p:Person) RETURN abs(p.age - 50) AS age_diff",
    );
    assert_eq!(get_single_value(&result).as_f64().unwrap(), 20.0);
}

#[test]
fn test_functions_in_where_clause() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    execute_query(&mut engine, "CREATE (p:Person {name: 'Alice', score: 85})");
    execute_query(&mut engine, "CREATE (p:Person {name: 'bob', score: 92})");
    execute_query(
        &mut engine,
        "CREATE (p:Person {name: 'CHARLIE', score: 78})",
    );

    // Filter using string function
    let result = execute_query(
        &mut engine,
        "MATCH (p:Person) WHERE toLower(p.name) = 'alice' RETURN p.name",
    );
    assert_eq!(result.rows.len(), 1);
    assert_eq!(&result.rows[0].values[0], "Alice");

    // Filter using math function
    // Alice: abs(85-90) = 5, NOT < 5
    // Bob: abs(92-90) = 2, YES < 5
    // Charlie: abs(78-90) = 12, NOT < 5
    let result = execute_query(
        &mut engine,
        "MATCH (p:Person) WHERE abs(p.score - 90) < 5 RETURN p.name ORDER BY p.name",
    );
    assert_eq!(result.rows.len(), 1);
    assert_eq!(&result.rows[0].values[0], "bob");

    // Test with <= instead
    let result = execute_query(
        &mut engine,
        "MATCH (p:Person) WHERE abs(p.score - 90) <= 5 RETURN p.name ORDER BY p.name",
    );
    assert_eq!(result.rows.len(), 2);
    assert_eq!(&result.rows[0].values[0], "Alice");
    assert_eq!(&result.rows[1].values[0], "bob");
}

#[test]
fn test_list_functions_with_unwind() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    // Use range with UNWIND
    let result = execute_query(&mut engine, "UNWIND range(1, 3) AS num RETURN num");
    assert_eq!(result.rows.len(), 3);
    assert_eq!(&result.rows[0].values[0], 1);
    assert_eq!(&result.rows[1].values[0], 2);
    assert_eq!(&result.rows[2].values[0], 3);
}

#[test]
fn test_type_conversion_chain() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    // String -> Integer -> String
    let result = execute_query(&mut engine, "RETURN toString(toInteger('42')) AS result");
    assert_eq!(get_single_value(&result), "42");

    // Integer -> Float -> back to Integer
    let result = execute_query(&mut engine, "RETURN toInteger(toFloat(10)) AS result");
    assert_eq!(get_single_value(&result), 10);

    // String -> Float -> String (note: maintains decimal)
    let result = execute_query(&mut engine, "RETURN toString(toFloat('2.5')) AS result");
    assert_eq!(get_single_value(&result), "2.5");
}

#[test]
fn test_null_handling_in_functions() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    // String functions with null
    let result = execute_query(&mut engine, "RETURN toLower(null) AS result");
    assert!(get_single_value(&result).is_null());

    // Math functions with null
    let result = execute_query(&mut engine, "RETURN abs(null) AS result");
    assert!(get_single_value(&result).is_null());

    // List functions with null
    let result = execute_query(&mut engine, "RETURN size(null) AS result");
    assert!(get_single_value(&result).is_null());
}

// ============================================================================
// AGGREGATION FUNCTIONS
// ============================================================================

#[test]
fn test_collect_basic() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    // Create test nodes
    execute_query(&mut engine, "CREATE (p:Person {name: 'Alice', age: 30})");
    execute_query(&mut engine, "CREATE (p:Person {name: 'Bob', age: 25})");
    execute_query(&mut engine, "CREATE (p:Person {name: 'Charlie', age: 35})");

    // COLLECT all names
    let result = execute_query(
        &mut engine,
        "MATCH (p:Person) RETURN collect(p.name) AS names",
    );
    let arr = get_single_value(&result).as_array().unwrap();
    assert_eq!(arr.len(), 3);
    assert!(arr.contains(&serde_json::json!("Alice")));
    assert!(arr.contains(&serde_json::json!("Bob")));
    assert!(arr.contains(&serde_json::json!("Charlie")));
}

#[test]
fn test_collect_with_group_by() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    execute_query(
        &mut engine,
        "CREATE (p:Person {city: 'NYC', name: 'Alice'})",
    );
    execute_query(&mut engine, "CREATE (p:Person {city: 'NYC', name: 'Bob'})");
    execute_query(
        &mut engine,
        "CREATE (p:Person {city: 'LA', name: 'Charlie'})",
    );

    // GROUP BY city and collect names
    let result = execute_query(
        &mut engine,
        "MATCH (p:Person) RETURN p.city AS city, collect(p.name) AS names ORDER BY city",
    );

    assert_eq!(result.rows.len(), 2);

    // Find LA and NYC rows (order from ORDER BY should be deterministic)
    let la_row = result
        .rows
        .iter()
        .find(|r| r.values[0] == "LA")
        .expect("LA row not found");
    let nyc_row = result
        .rows
        .iter()
        .find(|r| r.values[0] == "NYC")
        .expect("NYC row not found");

    // LA group should have 1 name
    let la_names = la_row.values[1].as_array().unwrap();
    assert_eq!(la_names.len(), 1);
    assert_eq!(la_names[0], "Charlie");

    // NYC group should have 2 names
    let nyc_names = nyc_row.values[1].as_array().unwrap();
    assert_eq!(nyc_names.len(), 2);
}

#[test]
fn test_collect_distinct() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    execute_query(&mut engine, "CREATE (p:Person {skill: 'Rust'})");
    execute_query(&mut engine, "CREATE (p:Person {skill: 'Python'})");
    execute_query(&mut engine, "CREATE (p:Person {skill: 'Rust'})");
    execute_query(&mut engine, "CREATE (p:Person {skill: 'Python'})");
    execute_query(&mut engine, "CREATE (p:Person {skill: 'Go'})");

    // COLLECT DISTINCT skills
    let result = execute_query(
        &mut engine,
        "MATCH (p:Person) RETURN collect(DISTINCT p.skill) AS skills",
    );
    let arr = get_single_value(&result).as_array().unwrap();
    assert_eq!(arr.len(), 3); // Only unique values
}

#[test]
fn test_collect_empty_result() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    // COLLECT on empty result set
    let result = execute_query(
        &mut engine,
        "MATCH (p:NonExistent) RETURN collect(p.name) AS names",
    );
    let arr = get_single_value(&result).as_array().unwrap();
    assert_eq!(arr.len(), 0);
}

#[test]
fn test_collect_with_nulls() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    execute_query(&mut engine, "CREATE (p:Person {name: 'Alice', age: 30})");
    execute_query(&mut engine, "CREATE (p:Person {name: 'Bob'})"); // No age
    execute_query(&mut engine, "CREATE (p:Person {name: 'Charlie', age: 35})");

    // COLLECT should skip NULL values
    let result = execute_query(
        &mut engine,
        "MATCH (p:Person) RETURN collect(p.age) AS ages",
    );
    let arr = get_single_value(&result).as_array().unwrap();
    assert_eq!(arr.len(), 2); // Only non-null ages
}

#[test]
fn test_collect_with_count() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    execute_query(
        &mut engine,
        "CREATE (p:Person {city: 'NYC', name: 'Alice'})",
    );
    execute_query(&mut engine, "CREATE (p:Person {city: 'NYC', name: 'Bob'})");
    execute_query(
        &mut engine,
        "CREATE (p:Person {city: 'LA', name: 'Charlie'})",
    );

    // Multiple aggregations
    let result = execute_query(
        &mut engine,
        "MATCH (p:Person) RETURN p.city AS city, collect(p.name) AS names, count(p.name) AS count ORDER BY city",
    );

    assert_eq!(result.rows.len(), 2);

    // Find LA and NYC rows (order may vary)
    let la_row = result
        .rows
        .iter()
        .find(|r| r.values[0] == "LA")
        .expect("LA row not found");
    let nyc_row = result
        .rows
        .iter()
        .find(|r| r.values[0] == "NYC")
        .expect("NYC row not found");

    // LA should have 1 person
    assert_eq!(la_row.values[2], 1);
    let la_names = la_row.values[1].as_array().unwrap();
    assert_eq!(la_names.len(), 1);

    // NYC should have 2 people
    assert_eq!(nyc_row.values[2], 2);
    let nyc_names = nyc_row.values[1].as_array().unwrap();
    assert_eq!(nyc_names.len(), 2);
}
