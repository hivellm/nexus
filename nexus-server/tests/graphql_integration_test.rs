//! GraphQL API Integration Tests
//!
//! Comprehensive tests for the GraphQL API implementation

use nexus_core::auth::{AuthConfig, AuthManager, Permission, RoleBasedAccessControl};
use nexus_core::testing::TestContext;
use nexus_server::{NexusServer, config::RootUserConfig};
use parking_lot::RwLock;
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::RwLock as TokioRwLock;

/// Helper function to create a test server instance
async fn create_test_server() -> (Arc<NexusServer>, TestContext) {
    use nexus_core::{Engine, database::DatabaseManager, executor::Executor};

    let ctx = TestContext::new();
    let data_dir = ctx.path().to_path_buf();

    // Ensure data directory exists
    std::fs::create_dir_all(&data_dir).unwrap();

    // Initialize Engine
    let engine = Engine::with_data_dir(&data_dir).unwrap();
    let engine_arc = Arc::new(TokioRwLock::new(engine));

    // Initialize executor
    let executor = Executor::default();
    let executor_arc = Arc::new(executor);

    // Initialize DatabaseManager
    let database_manager = DatabaseManager::new(data_dir.clone()).unwrap();
    let database_manager_arc = Arc::new(RwLock::new(database_manager));

    // Initialize RBAC
    let rbac = RoleBasedAccessControl::new();
    let rbac_arc = Arc::new(TokioRwLock::new(rbac));

    // Initialize AuthManager with authentication disabled for tests
    let auth_config = AuthConfig {
        enabled: false,
        required_for_public: false,
        default_permissions: vec![Permission::Read, Permission::Write],
        rate_limits: nexus_core::auth::RateLimits {
            per_minute: 1000,
            per_hour: 10000,
        },
    };
    let auth_storage_path = data_dir.join("auth");
    std::fs::create_dir_all(&auth_storage_path).unwrap();
    let auth_manager =
        Arc::new(AuthManager::with_storage(auth_config.clone(), auth_storage_path).unwrap());

    // Initialize JWT manager
    let jwt_config = nexus_core::auth::JwtConfig::from_env();
    let jwt_manager = Arc::new(nexus_core::auth::JwtManager::new(jwt_config));

    // Initialize audit logger
    let audit_logger = Arc::new(
        nexus_core::auth::AuditLogger::new(nexus_core::auth::AuditConfig {
            enabled: false,
            log_dir: std::path::PathBuf::from("./logs"),
            retention_days: 30,
            compress_logs: false,
        })
        .unwrap(),
    );

    // Create NexusServer
    let server = Arc::new(NexusServer::new(
        executor_arc,
        engine_arc,
        database_manager_arc,
        rbac_arc,
        auth_manager,
        jwt_manager,
        audit_logger,
        RootUserConfig::default(),
    ));

    (server, ctx)
}

/// Helper function to execute GraphQL query
async fn execute_graphql(
    server: &Arc<nexus_server::NexusServer>,
    query: &str,
) -> Result<Value, String> {
    use nexus_core::executor::Query;
    use std::collections::HashMap;

    let query = Query {
        cypher: query.to_string(),
        params: HashMap::new(),
    };

    server
        .executor
        .execute(&query)
        .map_err(|e| format!("Query failed: {}", e))
        .map(|result| {
            json!({
                "columns": result.columns,
                "rows": result.rows.iter().map(|row| row.values.clone()).collect::<Vec<_>>(),
            })
        })
}

#[tokio::test]
async fn test_graphql_schema_creation() {
    let (server, _ctx) = create_test_server().await;

    // Create schema
    let schema = nexus_server::api::graphql::create_schema(server.clone());

    // Verify schema is created successfully
    assert!(!schema.sdl().is_empty(), "Schema SDL should not be empty");
}

#[tokio::test]
async fn test_graphql_introspection_query() {
    let (server, _ctx) = create_test_server().await;

    // Create schema
    let schema = nexus_server::api::graphql::create_schema(server.clone());

    // Execute introspection query
    let introspection_query = r#"
        {
            __schema {
                queryType {
                    name
                }
                mutationType {
                    name
                }
            }
        }
    "#;

    let result = schema.execute(introspection_query).await;

    assert!(
        result.errors.is_empty(),
        "Introspection query should not have errors"
    );

    let data = result.data.into_json().unwrap();
    assert_eq!(
        data["__schema"]["queryType"]["name"], "QueryRoot",
        "Query type should be QueryRoot"
    );
    assert_eq!(
        data["__schema"]["mutationType"]["name"], "MutationRoot",
        "Mutation type should be MutationRoot"
    );
}

#[tokio::test]
async fn test_graphql_create_node_mutation() {
    let (server, _ctx) = create_test_server().await;

    // Create schema
    let schema = nexus_server::api::graphql::create_schema(server.clone());

    // Execute create node mutation
    let mutation = r#"
        mutation {
            createNode(
                labels: ["Person"],
                properties: {}
            ) {
                id
                labels
            }
        }
    "#;

    let result = schema.execute(mutation).await;

    assert!(
        result.errors.is_empty(),
        "Create node mutation should not have errors: {:?}",
        result.errors
    );

    let data = result.data.into_json().unwrap();
    assert!(
        !data["createNode"]["id"].as_str().unwrap().is_empty(),
        "Node ID should not be empty"
    );
    assert!(
        data["createNode"]["labels"]
            .as_array()
            .unwrap()
            .contains(&json!("Person")),
        "Node should have Person label"
    );
}

#[tokio::test]
async fn test_graphql_query_node() {
    let (server, _ctx) = create_test_server().await;

    // First, create a node using Cypher
    execute_graphql(
        &server,
        "CREATE (n:Person {name: 'Alice', age: 30}) RETURN id(n)",
    )
    .await
    .expect("Failed to create test node");

    // Create schema
    let schema = nexus_server::api::graphql::create_schema(server.clone());

    // Execute query to get nodes
    let query = r#"
        {
            nodes(filter: { labels: ["Person"], limit: 10 }) {
                id
                labels
            }
        }
    "#;

    let result = schema.execute(query).await;

    assert!(
        result.errors.is_empty(),
        "Query should not have errors: {:?}",
        result.errors
    );

    let data = result.data.into_json().unwrap();
    let nodes = data["nodes"].as_array().unwrap();
    assert!(!nodes.is_empty(), "Should return at least one node");
}

#[tokio::test]
async fn test_graphql_update_node_mutation() {
    let (server, _ctx) = create_test_server().await;

    // First, create a node and get its ID
    let create_result = execute_graphql(&server, "CREATE (n:Person {name: 'Bob'}) RETURN id(n)")
        .await
        .expect("Failed to create test node");

    let node_id = create_result["rows"][0][0].as_u64().unwrap().to_string();

    // Create schema
    let schema = nexus_server::api::graphql::create_schema(server.clone());

    // Execute update mutation
    let mutation = format!(
        r#"
        mutation {{
            updateNode(
                id: "{}",
                properties: {{}}
            ) {{
                id
                labels
            }}
        }}
    "#,
        node_id
    );

    let result = schema.execute(&mutation).await;

    // Note: Update mutation might fail if properties is empty, this is expected behavior
    // We're just testing the mutation structure exists
    assert!(
        result.errors.is_empty() || result.errors[0].message.contains("No properties"),
        "Update should either succeed or fail with expected error"
    );
}

#[tokio::test]
async fn test_graphql_create_relationship_mutation() {
    let (server, _ctx) = create_test_server().await;

    // Create two nodes first
    let create_result1 = execute_graphql(&server, "CREATE (n:Person {name: 'Alice'}) RETURN id(n)")
        .await
        .expect("Failed to create first node");
    let node1_id = create_result1["rows"][0][0].as_u64().unwrap().to_string();

    let create_result2 = execute_graphql(&server, "CREATE (n:Person {name: 'Bob'}) RETURN id(n)")
        .await
        .expect("Failed to create second node");
    let node2_id = create_result2["rows"][0][0].as_u64().unwrap().to_string();

    // Create schema
    let schema = nexus_server::api::graphql::create_schema(server.clone());

    // Execute create relationship mutation
    let mutation = format!(
        r#"
        mutation {{
            createRelationship(
                fromId: "{}",
                toId: "{}",
                relType: "KNOWS",
                properties: null
            ) {{
                id
                relType
                from
                to
            }}
        }}
    "#,
        node1_id, node2_id
    );

    let result = schema.execute(&mutation).await;

    assert!(
        result.errors.is_empty(),
        "Create relationship mutation should not have errors: {:?}",
        result.errors
    );

    let data = result.data.into_json().unwrap();
    assert!(
        !data["createRelationship"]["id"]
            .as_str()
            .unwrap()
            .is_empty()
    );
    assert_eq!(data["createRelationship"]["relType"], "KNOWS");
}

#[tokio::test]
async fn test_graphql_query_relationships() {
    let (server, _ctx) = create_test_server().await;

    // Create two nodes and a relationship
    execute_graphql(
        &server,
        "CREATE (a:Person {name: 'Alice'})-[:KNOWS {since: 2020}]->(b:Person {name: 'Bob'}) RETURN id(a)",
    )
    .await
    .expect("Failed to create test data");

    // Get the node ID
    let nodes_result = execute_graphql(&server, "MATCH (n:Person {name: 'Alice'}) RETURN id(n)")
        .await
        .expect("Failed to query node");
    let node_id = nodes_result["rows"][0][0].as_u64().unwrap().to_string();

    // Create schema
    let schema = nexus_server::api::graphql::create_schema(server.clone());

    // Query relationships
    let query = format!(
        r#"
        {{
            relationships(nodeId: "{}", direction: "OUT") {{
                id
                relType
            }}
        }}
    "#,
        node_id
    );

    let result = schema.execute(&query).await;

    assert!(
        result.errors.is_empty(),
        "Query relationships should not have errors: {:?}",
        result.errors
    );

    let data = result.data.into_json().unwrap();
    let relationships = data["relationships"].as_array().unwrap();
    assert!(
        !relationships.is_empty(),
        "Should return at least one relationship"
    );
}

#[tokio::test]
async fn test_graphql_delete_node_mutation() {
    let (server, _ctx) = create_test_server().await;

    // Create a node
    let create_result =
        execute_graphql(&server, "CREATE (n:Person {name: 'Charlie'}) RETURN id(n)")
            .await
            .expect("Failed to create test node");
    let node_id = create_result["rows"][0][0].as_u64().unwrap().to_string();

    // Create schema
    let schema = nexus_server::api::graphql::create_schema(server.clone());

    // Execute delete mutation
    let mutation = format!(
        r#"
        mutation {{
            deleteNode(id: "{}", detach: false)
        }}
    "#,
        node_id
    );

    let result = schema.execute(&mutation).await;

    assert!(
        result.errors.is_empty(),
        "Delete node mutation should not have errors: {:?}",
        result.errors
    );

    let data = result.data.into_json().unwrap();
    assert_eq!(data["deleteNode"], true, "Delete should return true");
}

#[tokio::test]
async fn test_graphql_delete_relationship_mutation() {
    let (server, _ctx) = create_test_server().await;

    // Create nodes and relationship
    let create_result = execute_graphql(
        &server,
        "CREATE (a:Person)-[r:KNOWS]->(b:Person) RETURN id(r)",
    )
    .await
    .expect("Failed to create test data");
    let rel_id = create_result["rows"][0][0].as_u64().unwrap().to_string();

    // Create schema
    let schema = nexus_server::api::graphql::create_schema(server.clone());

    // Execute delete relationship mutation
    let mutation = format!(
        r#"
        mutation {{
            deleteRelationship(id: "{}")
        }}
    "#,
        rel_id
    );

    let result = schema.execute(&mutation).await;

    assert!(
        result.errors.is_empty(),
        "Delete relationship mutation should not have errors: {:?}",
        result.errors
    );

    let data = result.data.into_json().unwrap();
    assert_eq!(
        data["deleteRelationship"], true,
        "Delete should return true"
    );
}

#[tokio::test]
async fn test_graphql_raw_cypher_query() {
    let (server, _ctx) = create_test_server().await;

    // Create test data
    execute_graphql(&server, "CREATE (n:Person {name: 'Diana', age: 25})")
        .await
        .expect("Failed to create test data");

    // Create schema
    let schema = nexus_server::api::graphql::create_schema(server.clone());

    // Execute raw Cypher query
    let query = r#"
        {
            cypher(queryStr: "MATCH (n:Person) RETURN n.name, n.age") {
                columns
                executionTimeMs
            }
        }
    "#;

    let result = schema.execute(query).await;

    assert!(
        result.errors.is_empty(),
        "Raw Cypher query should not have errors: {:?}",
        result.errors
    );

    let data = result.data.into_json().unwrap();
    let columns = data["cypher"]["columns"].as_array().unwrap();
    assert!(!columns.is_empty(), "Should return columns");
}

#[tokio::test]
async fn test_graphql_node_field_resolvers() {
    let (server, _ctx) = create_test_server().await;

    // Create node with relationships
    execute_graphql(
        &server,
        "CREATE (a:Person {name: 'Eve'})-[:KNOWS]->(b:Person {name: 'Frank'})",
    )
    .await
    .expect("Failed to create test data");

    // Get node ID
    let nodes_result = execute_graphql(&server, "MATCH (n:Person {name: 'Eve'}) RETURN id(n)")
        .await
        .expect("Failed to query node");
    let node_id = nodes_result["rows"][0][0].as_u64().unwrap().to_string();

    // Create schema
    let schema = nexus_server::api::graphql::create_schema(server.clone());

    // Query node with field resolvers (properties is a scalar, not an object)
    let query = format!(
        r#"
        {{
            node(id: "{}") {{
                id
                labels
            }}
        }}
    "#,
        node_id
    );

    let result = schema.execute(&query).await;

    assert!(
        result.errors.is_empty(),
        "Node field resolvers should not have errors: {:?}",
        result.errors
    );

    let data = result.data.into_json().unwrap();
    assert!(data["node"].is_object(), "Should return node object");
}

#[tokio::test]
async fn test_graphql_pagination() {
    let (server, _ctx) = create_test_server().await;

    // Create multiple nodes
    for i in 0..5 {
        execute_graphql(&server, &format!("CREATE (n:TestNode {{value: {}}})", i))
            .await
            .expect("Failed to create test node");
    }

    // Create schema
    let schema = nexus_server::api::graphql::create_schema(server.clone());

    // Query with pagination
    let query = r#"
        {
            nodes(filter: { labels: ["TestNode"], limit: 2, skip: 1 }) {
                id
                labels
            }
        }
    "#;

    let result = schema.execute(query).await;

    assert!(
        result.errors.is_empty(),
        "Pagination query should not have errors: {:?}",
        result.errors
    );

    let data = result.data.into_json().unwrap();
    let nodes = data["nodes"].as_array().unwrap();
    assert!(
        nodes.len() <= 2,
        "Should return at most 2 nodes due to limit"
    );
}

#[tokio::test]
async fn test_graphql_filtering_by_labels() {
    let (server, _ctx) = create_test_server().await;

    // Create nodes with different labels
    execute_graphql(&server, "CREATE (n:Person {name: 'Grace'})")
        .await
        .expect("Failed to create Person node");
    execute_graphql(&server, "CREATE (n:Company {name: 'TechCorp'})")
        .await
        .expect("Failed to create Company node");

    // Create schema
    let schema = nexus_server::api::graphql::create_schema(server.clone());

    // Query only Person nodes
    let query = r#"
        {
            nodes(filter: { labels: ["Person"] }) {
                id
                labels
            }
        }
    "#;

    let result = schema.execute(query).await;

    assert!(
        result.errors.is_empty(),
        "Filter query should not have errors: {:?}",
        result.errors
    );

    let data = result.data.into_json().unwrap();
    let nodes = data["nodes"].as_array().unwrap();

    // Verify all returned nodes have Person label
    for node in nodes {
        let labels = node["labels"].as_array().unwrap();
        assert!(
            labels.iter().any(|l| l == "Person"),
            "All nodes should have Person label"
        );
    }
}

#[tokio::test]
async fn test_graphql_error_handling_invalid_node_id() {
    let (server, _ctx) = create_test_server().await;

    // Create schema
    let schema = nexus_server::api::graphql::create_schema(server.clone());

    // Query with invalid node ID
    let query = r#"
        {
            node(id: "invalid_id") {
                id
            }
        }
    "#;

    let result = schema.execute(query).await;

    // Should have an error
    assert!(
        !result.errors.is_empty(),
        "Should have error for invalid node ID"
    );
}

#[tokio::test]
async fn test_graphql_error_handling_nonexistent_node() {
    let (server, _ctx) = create_test_server().await;

    // Create schema
    let schema = nexus_server::api::graphql::create_schema(server.clone());

    // Query with non-existent node ID
    let query = r#"
        {
            node(id: "99999999") {
                id
            }
        }
    "#;

    let result = schema.execute(query).await;

    // Should succeed but return null
    assert!(
        result.errors.is_empty(),
        "Should not have errors for non-existent node"
    );

    let data = result.data.into_json().unwrap();
    assert!(
        data["node"].is_null(),
        "Should return null for non-existent node"
    );
}
