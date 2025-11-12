//! Unit tests for Cypher API handlers
//!
//! Tests for database management and user management command execution

#[cfg(test)]
mod tests {
    use crate::NexusServer;
    use crate::api::cypher::{execute_database_commands, execute_user_commands};
    use nexus_core::auth::RoleBasedAccessControl;
    use nexus_core::database::DatabaseManager;
    use nexus_core::executor::parser::CypherParser;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::sync::RwLock;

    async fn create_test_server() -> Arc<NexusServer> {
        let temp_dir = TempDir::new().unwrap();
        let engine = nexus_core::Engine::with_data_dir(temp_dir.path()).unwrap();
        let engine_arc = Arc::new(RwLock::new(engine));

        let executor = nexus_core::executor::Executor::default();
        let executor_arc = Arc::new(RwLock::new(executor));

        let db_path = temp_dir.path().join("databases");
        std::fs::create_dir_all(&db_path).unwrap();
        let database_manager = DatabaseManager::new(db_path).unwrap();
        let database_manager_arc = Arc::new(RwLock::new(database_manager));

        let rbac = RoleBasedAccessControl::new();
        let rbac_arc = Arc::new(RwLock::new(rbac));

        Arc::new(NexusServer::new(
            executor_arc,
            engine_arc,
            database_manager_arc,
            rbac_arc,
        ))
    }

    #[tokio::test]
    async fn test_show_databases_returns_default() {
        let server = create_test_server().await;
        let start_time = std::time::Instant::now();

        let mut parser = CypherParser::new("SHOW DATABASES".to_string());
        let ast = parser.parse().unwrap();

        let response = execute_database_commands(server, &ast, start_time).await;
        let response = response.0;

        assert!(response.error.is_none(), "Error: {:?}", response.error);
        assert_eq!(response.columns, vec!["name", "default"]);
        assert!(
            !response.rows.is_empty(),
            "Should have at least default database"
        );

        // Should have at least the default database
        let has_default = response.rows.iter().any(|row| {
            if let Some(arr) = row.as_array() {
                arr.len() >= 2 && arr[1].as_bool().unwrap_or(false)
            } else {
                false
            }
        });
        assert!(has_default, "Should have default database");
    }

    #[tokio::test]
    async fn test_create_database_success() {
        let server = create_test_server().await;
        let start_time = std::time::Instant::now();

        let mut parser = CypherParser::new("CREATE DATABASE testdb_unit".to_string());
        let ast = parser.parse().unwrap();

        let response = execute_database_commands(server.clone(), &ast, start_time).await;
        let response = response.0;

        assert!(response.error.is_none());
        assert_eq!(response.columns, vec!["name", "message"]);
        assert_eq!(response.rows.len(), 1);

        // Verify database was created
        let mut parser2 = CypherParser::new("SHOW DATABASES".to_string());
        let ast2 = parser2.parse().unwrap();
        let start_time2 = std::time::Instant::now();
        let response2 = execute_database_commands(server, &ast2, start_time2).await;
        let response2 = response2.0;

        let has_testdb = response2.rows.iter().any(|row| {
            if let Some(arr) = row.as_array() {
                !arr.is_empty() && arr[0].as_str() == Some("testdb_unit")
            } else {
                false
            }
        });
        assert!(has_testdb, "Should have testdb_unit database");
    }

    #[tokio::test]
    async fn test_create_database_duplicate_error() {
        let server = create_test_server().await;
        let start_time = std::time::Instant::now();

        // Create database first
        let mut parser1 = CypherParser::new("CREATE DATABASE testdb_dup".to_string());
        let ast1 = parser1.parse().unwrap();
        let _ = execute_database_commands(server.clone(), &ast1, start_time).await;

        // Try to create again
        let start_time2 = std::time::Instant::now();
        let mut parser2 = CypherParser::new("CREATE DATABASE testdb_dup".to_string());
        let ast2 = parser2.parse().unwrap();
        let response = execute_database_commands(server, &ast2, start_time2).await;
        let response = response.0;

        assert!(response.error.is_some());
        assert!(response.error.unwrap().contains("already exists"));
    }

    #[tokio::test]
    async fn test_drop_database_success() {
        let server = create_test_server().await;
        let start_time = std::time::Instant::now();

        // Create database first
        let mut parser1 = CypherParser::new("CREATE DATABASE testdb_drop".to_string());
        let ast1 = parser1.parse().unwrap();
        let _ = execute_database_commands(server.clone(), &ast1, start_time).await;

        // Drop it
        let start_time2 = std::time::Instant::now();
        let mut parser2 = CypherParser::new("DROP DATABASE testdb_drop".to_string());
        let ast2 = parser2.parse().unwrap();
        let response = execute_database_commands(server, &ast2, start_time2).await;
        let response = response.0;

        assert!(response.error.is_none());
        assert_eq!(response.columns, vec!["message"]);
        assert_eq!(response.rows.len(), 1);
    }

    #[tokio::test]
    async fn test_drop_database_nonexistent_error() {
        let server = create_test_server().await;
        let start_time = std::time::Instant::now();

        let mut parser = CypherParser::new("DROP DATABASE nonexistent_db_12345".to_string());
        let ast = parser.parse().unwrap();

        let response = execute_database_commands(server, &ast, start_time).await;
        let response = response.0;

        assert!(response.error.is_some());
        assert!(response.error.unwrap().contains("does not exist"));
    }

    #[tokio::test]
    async fn test_show_users_empty() {
        let server = create_test_server().await;
        let start_time = std::time::Instant::now();

        let mut parser = CypherParser::new("SHOW USERS".to_string());
        let ast = parser.parse().unwrap();

        let response = execute_user_commands(server, &ast, start_time).await;
        let response = response.0;

        assert!(response.error.is_none());
        assert_eq!(response.columns, vec!["username", "roles", "is_active"]);
        // Should be empty initially
        assert_eq!(response.rows.len(), 0);
    }

    #[tokio::test]
    async fn test_create_user_success() {
        let server = create_test_server().await;
        let start_time = std::time::Instant::now();

        let mut parser = CypherParser::new("CREATE USER testuser_unit".to_string());
        let ast = parser.parse().unwrap();

        let response = execute_user_commands(server.clone(), &ast, start_time).await;
        let response = response.0;

        assert!(response.error.is_none());
        assert_eq!(response.columns, vec!["username", "message"]);
        assert_eq!(response.rows.len(), 1);

        // Verify user was created
        let mut parser2 = CypherParser::new("SHOW USERS".to_string());
        let ast2 = parser2.parse().unwrap();
        let start_time2 = std::time::Instant::now();
        let response2 = execute_user_commands(server, &ast2, start_time2).await;
        let response2 = response2.0;

        let has_user = response2.rows.iter().any(|row| {
            if let Some(arr) = row.as_array() {
                !arr.is_empty() && arr[0].as_str() == Some("testuser_unit")
            } else {
                false
            }
        });
        assert!(has_user, "Should have testuser_unit user");
    }

    #[tokio::test]
    async fn test_create_user_duplicate_error() {
        let server = create_test_server().await;
        let start_time = std::time::Instant::now();

        // Create user first
        let mut parser1 = CypherParser::new("CREATE USER testuser_dup".to_string());
        let ast1 = parser1.parse().unwrap();
        let _ = execute_user_commands(server.clone(), &ast1, start_time).await;

        // Try to create again
        let start_time2 = std::time::Instant::now();
        let mut parser2 = CypherParser::new("CREATE USER testuser_dup".to_string());
        let ast2 = parser2.parse().unwrap();
        let response = execute_user_commands(server, &ast2, start_time2).await;
        let response = response.0;

        assert!(response.error.is_some());
        assert!(response.error.unwrap().contains("already exists"));
    }

    #[tokio::test]
    async fn test_create_user_if_not_exists() {
        let server = create_test_server().await;
        let start_time = std::time::Instant::now();

        // Create user first
        let mut parser1 = CypherParser::new("CREATE USER testuser_ifne IF NOT EXISTS".to_string());
        let ast1 = parser1.parse().unwrap();
        let response1 = execute_user_commands(server.clone(), &ast1, start_time).await;
        let response1 = response1.0;
        assert!(response1.error.is_none());

        // Try to create again with IF NOT EXISTS (should not error)
        let start_time2 = std::time::Instant::now();
        let mut parser2 = CypherParser::new("CREATE USER testuser_ifne IF NOT EXISTS".to_string());
        let ast2 = parser2.parse().unwrap();
        let response2 = execute_user_commands(server, &ast2, start_time2).await;
        let response2 = response2.0;

        assert!(
            response2.error.is_none(),
            "IF NOT EXISTS should not error on duplicate"
        );
    }

    #[tokio::test]
    async fn test_grant_permission_success() {
        let server = create_test_server().await;
        let start_time = std::time::Instant::now();

        // Create user first
        let mut parser1 = CypherParser::new("CREATE USER testuser_grant".to_string());
        let ast1 = parser1.parse().unwrap();
        let _ = execute_user_commands(server.clone(), &ast1, start_time).await;

        // Grant permission
        let start_time2 = std::time::Instant::now();
        let mut parser2 = CypherParser::new("GRANT READ TO testuser_grant".to_string());
        let ast2 = parser2.parse().unwrap();
        let response = execute_user_commands(server, &ast2, start_time2).await;
        let response = response.0;

        assert!(response.error.is_none());
        assert_eq!(response.columns, vec!["target", "permissions", "message"]);
        assert_eq!(response.rows.len(), 1);
    }

    #[tokio::test]
    async fn test_grant_permission_nonexistent_user_error() {
        let server = create_test_server().await;
        let start_time = std::time::Instant::now();

        let mut parser = CypherParser::new("GRANT READ TO nonexistent_user_12345".to_string());
        let ast = parser.parse().unwrap();

        let response = execute_user_commands(server, &ast, start_time).await;
        let response = response.0;

        assert!(response.error.is_some());
        assert!(response.error.unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn test_grant_invalid_permission_error() {
        let server = create_test_server().await;
        let start_time = std::time::Instant::now();

        // Create user first
        let mut parser1 = CypherParser::new("CREATE USER testuser_invalid".to_string());
        let ast1 = parser1.parse().unwrap();
        let _ = execute_user_commands(server.clone(), &ast1, start_time).await;

        // Grant invalid permission
        let start_time2 = std::time::Instant::now();
        let mut parser2 = CypherParser::new("GRANT INVALID_PERM TO testuser_invalid".to_string());
        let ast2 = parser2.parse().unwrap();
        let response = execute_user_commands(server, &ast2, start_time2).await;
        let response = response.0;

        assert!(response.error.is_some());
        assert!(response.error.unwrap().contains("Unknown permission"));
    }

    #[tokio::test]
    async fn test_revoke_permission_success() {
        let server = create_test_server().await;
        let start_time = std::time::Instant::now();

        // Create user and grant permission first
        let mut parser1 = CypherParser::new("CREATE USER testuser_revoke".to_string());
        let ast1 = parser1.parse().unwrap();
        let _ = execute_user_commands(server.clone(), &ast1, start_time).await;

        let start_time2 = std::time::Instant::now();
        let mut parser2 = CypherParser::new("GRANT READ TO testuser_revoke".to_string());
        let ast2 = parser2.parse().unwrap();
        let _ = execute_user_commands(server.clone(), &ast2, start_time2).await;

        // Revoke permission
        let start_time3 = std::time::Instant::now();
        let mut parser3 = CypherParser::new("REVOKE READ FROM testuser_revoke".to_string());
        let ast3 = parser3.parse().unwrap();
        let response = execute_user_commands(server, &ast3, start_time3).await;
        let response = response.0;

        assert!(response.error.is_none());
        assert_eq!(response.columns, vec!["target", "permissions", "message"]);
        assert_eq!(response.rows.len(), 1);
    }

    #[tokio::test]
    async fn test_revoke_permission_nonexistent_user_error() {
        let server = create_test_server().await;
        let start_time = std::time::Instant::now();

        let mut parser = CypherParser::new("REVOKE READ FROM nonexistent_user_12345".to_string());
        let ast = parser.parse().unwrap();

        let response = execute_user_commands(server, &ast, start_time).await;
        let response = response.0;

        assert!(response.error.is_some());
        assert!(response.error.unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn test_grant_multiple_permissions() {
        let server = create_test_server().await;
        let start_time = std::time::Instant::now();

        // Create user first
        let mut parser1 = CypherParser::new("CREATE USER testuser_multi".to_string());
        let ast1 = parser1.parse().unwrap();
        let _ = execute_user_commands(server.clone(), &ast1, start_time).await;

        // Grant multiple permissions
        let start_time2 = std::time::Instant::now();
        let mut parser2 = CypherParser::new("GRANT READ, WRITE TO testuser_multi".to_string());
        let ast2 = parser2.parse().unwrap();
        let response = execute_user_commands(server, &ast2, start_time2).await;
        let response = response.0;

        assert!(response.error.is_none());
        assert_eq!(response.rows.len(), 1);
    }

    #[tokio::test]
    async fn test_revoke_multiple_permissions() {
        let server = create_test_server().await;
        let start_time = std::time::Instant::now();

        // Create user and grant permissions first
        let mut parser1 = CypherParser::new("CREATE USER testuser_multi_revoke".to_string());
        let ast1 = parser1.parse().unwrap();
        let _ = execute_user_commands(server.clone(), &ast1, start_time).await;

        let start_time2 = std::time::Instant::now();
        let mut parser2 =
            CypherParser::new("GRANT READ, WRITE TO testuser_multi_revoke".to_string());
        let ast2 = parser2.parse().unwrap();
        let _ = execute_user_commands(server.clone(), &ast2, start_time2).await;

        // Revoke multiple permissions
        let start_time3 = std::time::Instant::now();
        let mut parser3 =
            CypherParser::new("REVOKE READ, WRITE FROM testuser_multi_revoke".to_string());
        let ast3 = parser3.parse().unwrap();
        let response = execute_user_commands(server, &ast3, start_time3).await;
        let response = response.0;

        assert!(response.error.is_none());
        assert_eq!(response.rows.len(), 1);
    }
}
