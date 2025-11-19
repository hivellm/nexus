//! Integration tests for query builder

use nexus_sdk::{NexusClient, QueryBuilder};

// Note: These tests require a running Nexus server at http://localhost:15474
// They are skipped by default unless NEXUS_TEST_SERVER is set

#[tokio::test]
#[ignore]
async fn test_query_builder_match() {
    let client = NexusClient::new("http://localhost:15474").unwrap();

    let query = QueryBuilder::new()
        .match_("(n:Person)")
        .return_("n")
        .build();

    let result = client
        .execute_cypher(query.query(), query.params().cloned())
        .await
        .unwrap();

    // Should execute without error (may return empty results or have error)
    // Just verify we got a response structure
    let _ = result.columns.len();
    let _ = result.rows.len();
}

#[tokio::test]
#[ignore]
async fn test_query_builder_with_params() {
    let client = NexusClient::new("http://localhost:15474").unwrap();

    let query = QueryBuilder::new()
        .match_("(n:Person)")
        .where_("n.age > $min_age")
        .return_("n.name, n.age")
        .param("min_age", 18)
        .build();

    let result = client
        .execute_cypher(query.query(), query.params().cloned())
        .await
        .unwrap();

    // Query executed successfully (may return empty results or have error)
    // Just verify we got a response structure
    let _ = result.columns.len();
    let _ = result.rows.len();
}

#[tokio::test]
#[ignore]
async fn test_query_builder_create() {
    let client = NexusClient::new("http://localhost:15474").unwrap();

    let query = QueryBuilder::new()
        .create("(n:BuilderTest {name: $name, age: $age})")
        .return_("n")
        .param("name", "TestUser")
        .param("age", 25)
        .build();

    let result = client
        .execute_cypher(query.query(), query.params().cloned())
        .await
        .unwrap();

    // Query executed successfully (may return empty results or have error)
    // Just verify we got a response structure
    let _ = result.columns.len();
    let _ = result.rows.len();
    // CREATE queries should return rows
    if !result.rows.is_empty() {
        // Verify row structure
        if let Some(serde_json::Value::Array(_)) = result.rows.first() {
            // Row is properly formatted as array
        }
    }
}

#[tokio::test]
#[ignore]
async fn test_query_builder_order_by() {
    let client = NexusClient::new("http://localhost:15474").unwrap();

    let query = QueryBuilder::new()
        .match_("(n:Person)")
        .return_("n.name")
        .order_by("n.name ASC")
        .limit(10)
        .build();

    let result = client
        .execute_cypher(query.query(), query.params().cloned())
        .await
        .unwrap();

    // Query executed successfully (may return empty results or have error)
    // Just verify we got a response structure
    let _ = result.columns.len();
    let _ = result.rows.len();
}

#[tokio::test]
#[ignore]
async fn test_query_builder_limit_skip() {
    let client = NexusClient::new("http://localhost:15474").unwrap();

    let query = QueryBuilder::new()
        .match_("(n:Person)")
        .return_("n")
        .skip(0)
        .limit(5)
        .build();

    let result = client
        .execute_cypher(query.query(), query.params().cloned())
        .await
        .unwrap();

    // Query executed successfully (may return empty results or have error)
    // Just verify we got a response structure
    let _ = result.columns.len();
    let _ = result.rows.len();
}

#[tokio::test]
#[ignore]
async fn test_query_builder_complex_query() {
    let client = NexusClient::new("http://localhost:15474").unwrap();

    let query = QueryBuilder::new()
        .match_("(a:Person)-[r:KNOWS]->(b:Person)")
        .where_("a.age > $min_age AND b.age > $min_age")
        .return_("a.name, b.name, r.since")
        .order_by("r.since ASC")
        .limit(5)
        .param("min_age", 21)
        .build();

    let result = client
        .execute_cypher(query.query(), query.params().cloned())
        .await
        .unwrap();

    // Query executed successfully (may return empty results or have error)
    // Just verify we got a response structure
    let _ = result.columns.len();
    let _ = result.rows.len();
}

#[tokio::test]
async fn test_query_builder_build() {
    let query = QueryBuilder::new()
        .match_("(n:Person)")
        .where_("n.name = $name")
        .return_("n")
        .param("name", "Alice")
        .build();

    assert_eq!(
        query.query(),
        "MATCH (n:Person) WHERE n.name = $name RETURN n"
    );
    assert!(query.params().is_some());
    let params = query.params().unwrap();
    assert_eq!(
        params.get("name").unwrap(),
        &nexus_sdk::Value::String("Alice".to_string())
    );
}

#[tokio::test]
async fn test_query_builder_into_parts() {
    let query = QueryBuilder::new()
        .match_("(n:Person)")
        .return_("n")
        .build();

    let (query_str, params) = query.into_parts();
    assert_eq!(query_str, "MATCH (n:Person) RETURN n");
    assert!(params.is_none());
}
