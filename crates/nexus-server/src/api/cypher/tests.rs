//! Cypher endpoint integration tests. Attached via `#[cfg(test)] mod
//! tests;` in the parent module.

#![allow(unused_imports)]
use super::*;

// They are temporarily disabled until we can properly set up the test server
/*
#[tokio::test]
async fn test_execute_simple_query() {
    use crate::NexusServer;
    use nexus_core::database::DatabaseManager;
    use nexus_core::auth::RoleBasedAccessControl;
    use nexus_core::testing::TestContext;

    let ctx = TestContext::new();
    let engine = nexus_core::Engine::with_data_dir(ctx.path()).unwrap();
    let engine_arc = Arc::new(RwLock::new(engine));
    let executor = nexus_core::executor::Executor::default();
    let executor_arc = Arc::new(executor);
    let database_manager = DatabaseManager::new(ctx.path().join("databases")).unwrap();
    let database_manager_arc = Arc::new(PlRwLock::new(database_manager));
    let rbac = RoleBasedAccessControl::new();
    let rbac_arc = Arc::new(RwLock::new(rbac));
    let auth_config = nexus_core::auth::AuthConfig::default();
    let auth_manager = Arc::new(nexus_core::auth::AuthManager::new(auth_config));
    let jwt_config = nexus_core::auth::JwtConfig::default();
    let jwt_manager = Arc::new(nexus_core::auth::JwtManager::new(jwt_config));
    let audit_logger = Arc::new(
        nexus_core::auth::AuditLogger::new(nexus_core::auth::AuditConfig {
            enabled: false,
            log_dir: std::path::PathBuf::from("./logs"),
            retention_days: 30,
            compress_logs: false,
        })
        .unwrap(),
    );
    let server = Arc::new(NexusServer::new(executor_arc, engine_arc, database_manager_arc, rbac_arc, auth_manager, jwt_manager, audit_logger, nexus_server::config::RootUserConfig::default()));

    let request = CypherRequest {
        query: "MATCH (n) RETURN n LIMIT 1".to_string(),
        params: HashMap::new(),
        database: None,
    };

    let _response = execute_cypher(axum::extract::State(server), Json(request)).await;
    // Test passes if no panic occurs
}

#[tokio::test]
async fn test_execute_query_with_params() {
    let mut params = HashMap::new();
    params.insert("limit".to_string(), json!(5));

    let request = CypherRequest {
        query: "MATCH (n) RETURN n LIMIT $limit".to_string(),
        params,
        database: None,
    };

    let _response = execute_cypher(Json(request)).await;
    // Test passes if no panic occurs
}

#[tokio::test]
async fn test_execute_invalid_query() {
    let request = CypherRequest {
        query: "INVALID SYNTAX".to_string(),
        params: HashMap::new(),
        database: None,
    };

    let _response = execute_cypher(Json(request)).await;
    // Should handle invalid syntax gracefully
}

#[tokio::test]
async fn test_execute_without_executor() {
    // Don't initialize executor
    let request = CypherRequest {
        query: "MATCH (n) RETURN n".to_string(),
        params: HashMap::new(),
        database: None,
    };

    let response = execute_cypher(Json(request)).await;
    assert!(response.error.is_some());
    assert_eq!(response.error.as_ref().unwrap(), "Executor not initialized");
}

#[tokio::test]
async fn test_response_format() {
    let request = CypherRequest {
        query: "RETURN 1 as num, 'test' as str".to_string(),
        params: HashMap::new(),
        database: None,
    };

    let _response = execute_cypher(Json(request)).await;
    // Test passes if no panic occurs
}

#[tokio::test]
async fn test_execute_with_initialized_executor() {
    let request = CypherRequest {
        query: "RETURN 'hello' as greeting".to_string(),
        params: HashMap::new(),
        database: None,
    };

    let _response = execute_cypher(Json(request)).await;
    // Test passes if no panic occurs - executor may or may not be initialized
}

#[tokio::test]
async fn test_execute_with_complex_params() {
    let mut params = HashMap::new();
    params.insert("name".to_string(), json!("Alice"));
    params.insert("age".to_string(), json!(30));
    params.insert("active".to_string(), json!(true));

    let request = CypherRequest {
        query: "RETURN $name as name, $age as age, $active as active".to_string(),
        params,
        database: None,
    };

    let _response = execute_cypher(Json(request)).await;
    // Test passes if no panic occurs
}

#[tokio::test]
async fn test_execute_with_empty_result() {
    let request = CypherRequest {
        query: "MATCH (n) WHERE n.nonexistent = 'value' RETURN n".to_string(),
        params: HashMap::new(),
        database: None,
    };

    let _response = execute_cypher(Json(request)).await;
    // Test passes if no panic occurs
}

#[tokio::test]
async fn test_execute_with_multiple_rows() {
    let request = CypherRequest {
        query: "UNWIND [1, 2, 3] AS num RETURN num".to_string(),
        params: HashMap::new(),
        database: None,
    };

    let _response = execute_cypher(Json(request)).await;
    // Test passes if no panic occurs
}

#[tokio::test]
async fn test_execute_with_nested_params() {
    let mut params = HashMap::new();
    params.insert("list".to_string(), json!([1, 2, 3]));
    params.insert("obj".to_string(), json!({"key": "value"}));

    let request = CypherRequest {
        query: "RETURN $list as numbers, $obj as data".to_string(),
        params,
        database: None,
    };

    let _response = execute_cypher(Json(request)).await;
    // Test passes if no panic occurs
}

#[tokio::test]
async fn test_execute_with_null_params() {
    let mut params = HashMap::new();
    params.insert("null_value".to_string(), json!(null));

    let request = CypherRequest {
        query: "RETURN $null_value as null_val".to_string(),
        params,
        database: None,
    };

    let _response = execute_cypher(Json(request)).await;
    // Test passes if no panic occurs
}

#[tokio::test]
async fn test_execute_with_empty_query() {
    let request = CypherRequest {
        query: "".to_string(),
        params: HashMap::new(),
        database: None,
    };

    let _response = execute_cypher(Json(request)).await;
    // Should handle empty query gracefully
}

#[tokio::test]
async fn test_execute_with_very_long_query() {
    let long_query = "RETURN ".to_string() + &"x".repeat(1000);
    let request = CypherRequest {
        query: long_query,
        params: HashMap::new(),
        database: None,
    };

    let _response = execute_cypher(Json(request)).await;
    // Should handle long query gracefully
}

#[tokio::test]
async fn test_merge_node() {
    let request = CypherRequest {
        query: "MERGE (n:Person {name: \"Alice\", age: 30})".to_string(),
        params: HashMap::new(),
        database: None,
    };

    let _response = execute_cypher(Json(request)).await;
    // Test passes if no panic occurs
}

#[tokio::test]
async fn test_merge_node_without_properties() {
    let request = CypherRequest {
        query: "MERGE (n:Person)".to_string(),
        params: HashMap::new(),
        database: None,
    };

    let _response = execute_cypher(Json(request)).await;
    // Test passes if no panic occurs
}

#[tokio::test]
async fn test_set_property() {
    let request = CypherRequest {
        query: "CREATE (n:Person {name: \"Alice\"}) SET n.age = 30".to_string(),
        params: HashMap::new(),
        database: None,
    };

    let _response = execute_cypher(Json(request)).await;
    // Test passes if no panic occurs
}

#[tokio::test]
async fn test_set_label() {
    let request = CypherRequest {
        query: "CREATE (n:Person) SET n:Employee".to_string(),
        params: HashMap::new(),
        database: None,
    };

    let _response = execute_cypher(Json(request)).await;
    // Test passes if no panic occurs
}

#[tokio::test]
async fn test_delete_node() {
    let request = CypherRequest {
        query: "CREATE (n:Person {name: \"Bob\"}) DELETE n".to_string(),
        params: HashMap::new(),
        database: None,
    };

    let _response = execute_cypher(Json(request)).await;
    // Test passes if no panic occurs
}

#[tokio::test]
async fn test_detach_delete() {
    let request = CypherRequest {
        query: "CREATE (n:Person {name: \"Charlie\"}) DETACH DELETE n".to_string(),
        params: HashMap::new(),
        database: None,
    };

    let _response = execute_cypher(Json(request)).await;
    // Test passes if no panic occurs (DETACH DELETE partially supported)
}

#[tokio::test]
async fn test_remove_property() {
    let request = CypherRequest {
        query: "CREATE (n:Person {name: \"David\", age: 25}) REMOVE n.age".to_string(),
        params: HashMap::new(),
        database: None,
    };

    let _response = execute_cypher(Json(request)).await;
    // Test passes if no panic occurs
}

#[tokio::test]
async fn test_remove_label() {
    let request = CypherRequest {
        query: "CREATE (n:Person:Employee) REMOVE n:Employee".to_string(),
        params: HashMap::new(),
        database: None,
    };

    let _response = execute_cypher(Json(request)).await;
    // Test passes if no panic occurs
}
*/

// GH issue #5 Bug 2 — `RETURN <nodeVar>` must serialize the node object
// (not null), and a bare `RETURN t` must name the column `t`, not `result`.
#[tokio::test]
async fn create_return_node_var_returns_node_object() {
    use crate::NexusServer;
    use nexus_core::auth::RoleBasedAccessControl;
    use nexus_core::database::DatabaseManager;
    use nexus_core::testing::TestContext;
    use std::sync::Arc;
    use tokio::sync::RwLock;
    // database_manager uses parking_lot::RwLock; engine/rbac use tokio::RwLock.
    use parking_lot::RwLock as PlRwLock;

    let ctx = TestContext::new();
    let engine = nexus_core::Engine::with_data_dir(ctx.path()).unwrap();
    let engine_arc = Arc::new(RwLock::new(engine));
    let executor = nexus_core::executor::Executor::default();
    let executor_arc = Arc::new(executor);
    let database_manager = DatabaseManager::new(ctx.path().join("databases")).unwrap();
    let database_manager_arc = Arc::new(PlRwLock::new(database_manager));
    let rbac = RoleBasedAccessControl::new();
    let rbac_arc = Arc::new(RwLock::new(rbac));
    let auth_config = nexus_core::auth::AuthConfig::default();
    let auth_manager = Arc::new(nexus_core::auth::AuthManager::new(auth_config));
    let jwt_config = nexus_core::auth::JwtConfig::default();
    let jwt_manager = Arc::new(nexus_core::auth::JwtManager::new(jwt_config));
    let audit_logger = Arc::new(
        nexus_core::auth::AuditLogger::new(nexus_core::auth::AuditConfig {
            enabled: false,
            log_dir: std::path::PathBuf::from("./logs"),
            retention_days: 30,
            compress_logs: false,
        })
        .unwrap(),
    );
    let server = Arc::new(NexusServer::new(
        executor_arc,
        engine_arc,
        database_manager_arc,
        rbac_arc,
        auth_manager,
        jwt_manager,
        audit_logger,
        crate::config::RootUserConfig::default(),
    ));

    // CREATE ... RETURN t — must return the node object, column named "t".
    let req = CypherRequest {
        query: "CREATE (t:ProbeNode {id: \"probe-1\", title: \"hello\", n: 42}) RETURN t"
            .to_string(),
        params: HashMap::new(),
        database: None,
    };
    let resp = execute_cypher(axum::extract::State(server.clone()), None, axum::Json(req))
        .await
        .0;
    assert!(
        resp.error.is_none(),
        "CREATE...RETURN t errored: {:?}",
        resp.error
    );
    assert_eq!(
        resp.columns,
        vec!["t".to_string()],
        "bare RETURN t must name the column 't', not 'result'"
    );
    assert_eq!(resp.rows.len(), 1);
    let row = resp.rows[0].as_array().expect("row must be an array");
    let node = row[0]
        .as_object()
        .expect("RETURN t must be a node object, not null");
    assert_eq!(node.get("id").and_then(|v| v.as_str()), Some("probe-1"));
    assert_eq!(node.get("title").and_then(|v| v.as_str()), Some("hello"));
    assert_eq!(node.get("n").and_then(|v| v.as_i64()), Some(42));
    assert!(
        node.contains_key("_nexus_id"),
        "node object must carry _nexus_id"
    );

    // MATCH ... RETURN t — shape parity with the CREATE path.
    let req2 = CypherRequest {
        query: "MATCH (t:ProbeNode) RETURN t".to_string(),
        params: HashMap::new(),
        database: None,
    };
    let resp2 = execute_cypher(axum::extract::State(server), None, axum::Json(req2))
        .await
        .0;
    assert!(
        resp2.error.is_none(),
        "MATCH...RETURN t errored: {:?}",
        resp2.error
    );
    assert_eq!(resp2.rows.len(), 1);
    let node2 = resp2.rows[0].as_array().expect("row must be an array")[0]
        .as_object()
        .expect("MATCH RETURN t must be a node object, not null");
    assert_eq!(node2.get("title").and_then(|v| v.as_str()), Some("hello"));
}
