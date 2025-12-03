//! Tests for regex functions
//!
//! Tests for the regex string manipulation functions:
//! - regexMatch(string, pattern) - Test if pattern matches
//! - regexReplace(string, pattern, replacement) - Replace first match
//! - regexReplaceAll(string, pattern, replacement) - Replace all matches
//! - regexExtract(string, pattern) - Extract first match
//! - regexExtractAll(string, pattern) - Extract all matches
//! - regexExtractGroups(string, pattern) - Extract capture groups
//! - regexSplit(string, pattern) - Split by regex pattern

use nexus_core::testing::setup_isolated_test_engine;
use serde_json::json;

// ============================================================================
// regexMatch Tests
// ============================================================================

#[test]
fn test_regex_match_simple() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    let result = engine
        .execute_cypher(r#"RETURN regexMatch("hello world", "world") AS result"#)
        .unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!(true));
}

#[test]
fn test_regex_match_pattern() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    let result = engine
        .execute_cypher(r#"RETURN regexMatch("hello123world", "[0-9]+") AS result"#)
        .unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!(true));
}

#[test]
fn test_regex_match_no_match() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    let result = engine
        .execute_cypher(r#"RETURN regexMatch("hello", "[0-9]+") AS result"#)
        .unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!(false));
}

#[test]
fn test_regex_match_email_pattern() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    let result = engine
        .execute_cypher(r#"RETURN regexMatch("test@example.com", "[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}") AS result"#)
        .unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!(true));
}

// ============================================================================
// regexReplace Tests
// ============================================================================

#[test]
fn test_regex_replace_first() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    let result = engine
        .execute_cypher(
            r#"RETURN regexReplace("hello world world", "world", "universe") AS result"#,
        )
        .unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!("hello universe world"));
}

#[test]
fn test_regex_replace_pattern() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    let result = engine
        .execute_cypher(r#"RETURN regexReplace("hello123world", "[0-9]+", "-") AS result"#)
        .unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!("hello-world"));
}

// ============================================================================
// regexReplaceAll Tests
// ============================================================================

#[test]
fn test_regex_replace_all() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    let result = engine
        .execute_cypher(
            r#"RETURN regexReplaceAll("hello world world", "world", "universe") AS result"#,
        )
        .unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!("hello universe universe"));
}

#[test]
fn test_regex_replace_all_pattern() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    let result = engine
        .execute_cypher(r#"RETURN regexReplaceAll("a1b2c3", "[0-9]", "X") AS result"#)
        .unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!("aXbXcX"));
}

#[test]
fn test_regex_replace_all_whitespace() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    let result = engine
        .execute_cypher(r#"RETURN regexReplaceAll("hello   world  foo", "\\s+", " ") AS result"#)
        .unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!("hello world foo"));
}

// ============================================================================
// regexExtract Tests
// ============================================================================

#[test]
fn test_regex_extract_simple() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    let result = engine
        .execute_cypher(r#"RETURN regexExtract("hello123world456", "[0-9]+") AS result"#)
        .unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!("123"));
}

#[test]
fn test_regex_extract_no_match() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    let result = engine
        .execute_cypher(r#"RETURN regexExtract("hello", "[0-9]+") AS result"#)
        .unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!(null));
}

#[test]
fn test_regex_extract_word() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    let result = engine
        .execute_cypher(r#"RETURN regexExtract("The quick brown fox", "\\b\\w{5}\\b") AS result"#)
        .unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!("quick"));
}

// ============================================================================
// regexExtractAll Tests
// ============================================================================

#[test]
fn test_regex_extract_all() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    let result = engine
        .execute_cypher(r#"RETURN regexExtractAll("a1b2c3d4", "[0-9]") AS result"#)
        .unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!(["1", "2", "3", "4"]));
}

#[test]
fn test_regex_extract_all_words() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    let result = engine
        .execute_cypher(r#"RETURN regexExtractAll("hello world foo bar", "\\w+") AS result"#)
        .unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        result.rows[0].values[0],
        json!(["hello", "world", "foo", "bar"])
    );
}

#[test]
fn test_regex_extract_all_no_matches() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    let result = engine
        .execute_cypher(r#"RETURN regexExtractAll("hello", "[0-9]+") AS result"#)
        .unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!([]));
}

// ============================================================================
// regexExtractGroups Tests
// ============================================================================

#[test]
fn test_regex_extract_groups() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    let result = engine
        .execute_cypher(
            r#"RETURN regexExtractGroups("John Smith", "([A-Z][a-z]+) ([A-Z][a-z]+)") AS result"#,
        )
        .unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!(["John", "Smith"]));
}

#[test]
fn test_regex_extract_groups_date() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    let result = engine
        .execute_cypher(r#"RETURN regexExtractGroups("2024-01-15", "([0-9]{4})-([0-9]{2})-([0-9]{2})") AS result"#)
        .unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!(["2024", "01", "15"]));
}

#[test]
fn test_regex_extract_groups_no_match() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    let result = engine
        .execute_cypher(r#"RETURN regexExtractGroups("hello", "([0-9]+)") AS result"#)
        .unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!(null));
}

// ============================================================================
// regexSplit Tests
// ============================================================================

#[test]
fn test_regex_split_simple() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    let result = engine
        .execute_cypher(r#"RETURN regexSplit("a1b2c3d", "[0-9]") AS result"#)
        .unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!(["a", "b", "c", "d"]));
}

#[test]
fn test_regex_split_whitespace() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    let result = engine
        .execute_cypher(r#"RETURN regexSplit("hello   world  foo", "\\s+") AS result"#)
        .unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!(["hello", "world", "foo"]));
}

#[test]
fn test_regex_split_comma_or_semicolon() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    let result = engine
        .execute_cypher(r#"RETURN regexSplit("a,b;c,d;e", "[,;]") AS result"#)
        .unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!(["a", "b", "c", "d", "e"]));
}

// ============================================================================
// Edge Cases and Error Handling
// ============================================================================

#[test]
fn test_regex_invalid_pattern() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // Invalid regex pattern should return false for match
    let result = engine
        .execute_cypher(r#"RETURN regexMatch("hello", "[invalid(") AS result"#)
        .unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!(false));
}

#[test]
fn test_regex_null_handling() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    let result = engine
        .execute_cypher(r#"RETURN regexMatch(null, "test") AS result"#)
        .unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!(null));
}

#[test]
fn test_regex_empty_string() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    let result = engine
        .execute_cypher(r#"RETURN regexExtractAll("", "[a-z]") AS result"#)
        .unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!([]));
}

#[test]
fn test_regex_special_characters() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // Test with escaped special regex characters
    let result = engine
        .execute_cypher(r#"RETURN regexMatch("hello.world", "\\.") AS result"#)
        .unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!(true));
}

// ============================================================================
// Integration with Node Properties
// ============================================================================

#[test]
fn test_regex_with_node_property() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // Create a node with email
    engine
        .execute_cypher(r#"CREATE (u:User {email: "john.doe@example.com"})"#)
        .unwrap();

    // Use regex to extract username from email
    let result = engine
        .execute_cypher(r#"MATCH (u:User) RETURN regexExtract(u.email, "^[^@]+") AS username"#)
        .unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!("john.doe"));
}

#[test]
fn test_regex_filter_with_where() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // Create nodes
    engine
        .execute_cypher(r#"CREATE (u1:User {phone: "123-456-7890"})"#)
        .unwrap();
    engine
        .execute_cypher(r#"CREATE (u2:User {phone: "invalid"})"#)
        .unwrap();
    engine
        .execute_cypher(r#"CREATE (u3:User {phone: "987-654-3210"})"#)
        .unwrap();

    // Filter by phone format using regex
    let result = engine
        .execute_cypher(
            r#"MATCH (u:User) WHERE regexMatch(u.phone, "^[0-9]{3}-[0-9]{3}-[0-9]{4}$") RETURN u.phone ORDER BY u.phone"#,
        )
        .unwrap();

    assert_eq!(result.rows.len(), 2);
    assert_eq!(result.rows[0].values[0], json!("123-456-7890"));
    assert_eq!(result.rows[1].values[0], json!("987-654-3210"));
}
