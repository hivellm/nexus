//! Administrative Cypher commands routed away from `execute_cypher`:
//! database management (`CREATE/DROP/ALTER/USE DATABASE`, `SHOW DATABASES`),
//! user/role/grant commands, query-management introspection, and API-key
//! lifecycle (`CREATE/SHOW/REVOKE/DELETE API KEY`).

use super::*;

pub(crate) async fn execute_database_commands(
    server: Arc<NexusServer>,
    ast: &nexus_core::executor::parser::CypherQuery,
    start_time: std::time::Instant,
) -> Json<CypherResponse> {
    let mut columns = Vec::new();
    let mut rows = Vec::new();

    for clause in &ast.clauses {
        match clause {
            nexus_core::executor::parser::Clause::UseDatabase(use_db) => {
                // Set columns if not already set
                if columns.is_empty() {
                    columns = vec!["database".to_string(), "message".to_string()];
                }

                let dbm = server.database_manager.clone();
                let needle = use_db.name.clone();
                let db_exists = tokio::task::spawn_blocking(move || {
                    let manager = dbm.read();
                    manager.list_databases().iter().any(|db| db.name == needle)
                })
                .await
                .expect("spawn_blocking panicked");

                if db_exists {
                    rows.push(serde_json::json!([
                        use_db.name.clone(),
                        format!("Switched to database '{}'", use_db.name)
                    ]));
                } else {
                    let execution_time = start_time.elapsed().as_millis() as u64;
                    return Json(CypherResponse {
                        columns: vec![],
                        rows: vec![],
                        execution_time_ms: execution_time,
                        error: Some(format!("Database '{}' does not exist", use_db.name)),
                    });
                }
            }
            nexus_core::executor::parser::Clause::ShowDatabases => {
                columns = vec!["name".to_string(), "default".to_string()];

                let dbm = server.database_manager.clone();
                let (databases, default_db) = tokio::task::spawn_blocking(move || {
                    let manager = dbm.read();
                    (
                        manager.list_databases(),
                        manager.default_database_name().to_string(),
                    )
                })
                .await
                .expect("spawn_blocking panicked");

                for db in databases {
                    rows.push(serde_json::json!([db.name.clone(), db.name == default_db]));
                }
            }
            nexus_core::executor::parser::Clause::CreateDatabase(create_db) => {
                columns = vec!["name".to_string(), "message".to_string()];

                let dbm = server.database_manager.clone();
                let name_for_task = create_db.name.clone();
                let result = tokio::task::spawn_blocking(move || {
                    let manager = dbm.write();
                    manager.create_database(&name_for_task).map(|_| ())
                })
                .await
                .expect("spawn_blocking panicked");

                match result {
                    Ok(()) => {
                        rows.push(serde_json::json!([
                            create_db.name.clone(),
                            format!("Database '{}' created successfully", create_db.name)
                        ]));
                    }
                    Err(e) => {
                        let execution_time = start_time.elapsed().as_millis() as u64;
                        return Json(CypherResponse {
                            columns: vec![],
                            rows: vec![],
                            execution_time_ms: execution_time,
                            error: Some(format!("Failed to create database: {}", e)),
                        });
                    }
                }
            }
            nexus_core::executor::parser::Clause::DropDatabase(drop_db) => {
                columns = vec!["message".to_string()];

                let dbm = server.database_manager.clone();
                let name_for_task = drop_db.name.clone();
                let if_exists = drop_db.if_exists;
                let result = tokio::task::spawn_blocking(move || {
                    let manager = dbm.write();
                    manager.drop_database(&name_for_task, if_exists).map(|_| ())
                })
                .await
                .expect("spawn_blocking panicked");

                match result {
                    Ok(()) => {
                        rows.push(serde_json::json!([format!(
                            "Database '{}' dropped successfully",
                            drop_db.name
                        )]));
                    }
                    Err(e) => {
                        let execution_time = start_time.elapsed().as_millis() as u64;
                        return Json(CypherResponse {
                            columns: vec![],
                            rows: vec![],
                            execution_time_ms: execution_time,
                            error: Some(format!("Failed to drop database: {}", e)),
                        });
                    }
                }
            }
            _ => {}
        }
    }

    let execution_time = start_time.elapsed().as_millis() as u64;
    Json(CypherResponse {
        columns,
        rows,
        execution_time_ms: execution_time,
        error: None,
    })
}

/// Execute user management commands (SHOW USERS, CREATE USER, GRANT, REVOKE)
#[cfg_attr(test, allow(dead_code))]
pub(crate) async fn execute_user_commands(
    server: Arc<NexusServer>,
    ast: &nexus_core::executor::parser::CypherQuery,
    start_time: std::time::Instant,
) -> Json<CypherResponse> {
    let mut columns = Vec::new();
    let mut rows = Vec::new();
    let mut rbac = server.rbac.write().await;

    for clause in &ast.clauses {
        match clause {
            nexus_core::executor::parser::Clause::ShowUsers => {
                columns = vec![
                    "username".to_string(),
                    "roles".to_string(),
                    "is_active".to_string(),
                ];
                let users = rbac.list_users();

                for user in users {
                    rows.push(serde_json::json!([
                        user.username.clone(),
                        user.roles.clone(),
                        user.is_active
                    ]));
                }
            }
            nexus_core::executor::parser::Clause::ShowUser(show_user) => {
                columns = vec![
                    "username".to_string(),
                    "id".to_string(),
                    "email".to_string(),
                    "roles".to_string(),
                    "permissions".to_string(),
                    "is_active".to_string(),
                    "is_root".to_string(),
                ];

                let users_list = rbac.list_users();
                let user = users_list.iter().find(|u| u.username == show_user.username);

                if let Some(user) = user {
                    let permissions: Vec<String> = user
                        .additional_permissions
                        .permissions()
                        .iter()
                        .map(|p| p.to_string())
                        .collect();

                    rows.push(serde_json::json!([
                        user.username.clone(),
                        user.id.clone(),
                        user.email.clone().unwrap_or_default(),
                        user.roles.clone(),
                        permissions,
                        user.is_active,
                        user.is_root
                    ]));
                } else {
                    let execution_time = start_time.elapsed().as_millis() as u64;
                    return Json(CypherResponse {
                        columns: vec![],
                        rows: vec![],
                        execution_time_ms: execution_time,
                        error: Some(format!("User '{}' not found", show_user.username)),
                    });
                }
            }
            nexus_core::executor::parser::Clause::DropUser(drop_user) => {
                columns = vec!["username".to_string(), "message".to_string()];

                let users_list = rbac.list_users();
                let user_info = users_list
                    .iter()
                    .find(|u| u.username == drop_user.username)
                    .map(|u| (u.id.clone(), u.is_root));

                if let Some((user_id, is_root)) = user_info {
                    // Prevent deletion of root user
                    if is_root {
                        let execution_time = start_time.elapsed().as_millis() as u64;
                        return Json(CypherResponse {
                            columns: vec![],
                            rows: vec![],
                            execution_time_ms: execution_time,
                            error: Some(
                                "Cannot delete root user. Use DISABLE instead.".to_string(),
                            ),
                        });
                    }

                    if let Some(_removed_user) = rbac.remove_user(&user_id) {
                        rows.push(serde_json::json!([
                            drop_user.username.clone(),
                            format!("User '{}' deleted successfully", drop_user.username)
                        ]));
                    } else {
                        let execution_time = start_time.elapsed().as_millis() as u64;
                        return Json(CypherResponse {
                            columns: vec![],
                            rows: vec![],
                            execution_time_ms: execution_time,
                            error: Some(format!("Failed to delete user '{}'", drop_user.username)),
                        });
                    }
                } else if drop_user.if_exists {
                    rows.push(serde_json::json!([
                        drop_user.username.clone(),
                        format!("User '{}' does not exist (IF EXISTS)", drop_user.username)
                    ]));
                } else {
                    let execution_time = start_time.elapsed().as_millis() as u64;
                    return Json(CypherResponse {
                        columns: vec![],
                        rows: vec![],
                        execution_time_ms: execution_time,
                        error: Some(format!("User '{}' not found", drop_user.username)),
                    });
                }
            }
            nexus_core::executor::parser::Clause::CreateUser(create_user) => {
                columns = vec!["username".to_string(), "message".to_string()];

                // Check if user already exists (by username)
                let users_list = rbac.list_users();
                let existing_user = users_list
                    .iter()
                    .find(|u| u.username == create_user.username);

                if existing_user.is_some() && !create_user.if_not_exists {
                    let execution_time = start_time.elapsed().as_millis() as u64;
                    return Json(CypherResponse {
                        columns: vec![],
                        rows: vec![],
                        execution_time_ms: execution_time,
                        error: Some(format!("User '{}' already exists", create_user.username)),
                    });
                }

                if existing_user.is_none() {
                    let user_id = uuid::Uuid::new_v4().to_string();
                    let user = if let Some(password) = &create_user.password {
                        // Hash password with SHA512
                        let password_hash = nexus_core::auth::hash_password(password);
                        nexus_core::auth::User::with_password_hash(
                            user_id.clone(),
                            create_user.username.clone(),
                            password_hash,
                        )
                    } else {
                        nexus_core::auth::User::new(user_id.clone(), create_user.username.clone())
                    };
                    rbac.add_user(user);

                    // Check if root should be disabled after first admin creation
                    drop(rbac); // Release lock before async call
                    server.check_and_disable_root_if_needed().await;
                    rbac = server.rbac.write().await; // Reacquire lock
                }

                rows.push(serde_json::json!([
                    create_user.username.clone(),
                    format!("User '{}' created successfully", create_user.username)
                ]));
            }
            nexus_core::executor::parser::Clause::Grant(grant) => {
                columns = vec![
                    "target".to_string(),
                    "permissions".to_string(),
                    "message".to_string(),
                ];

                // Parse permissions
                let permissions: Result<Vec<Permission>, _> = grant
                    .permissions
                    .iter()
                    .map(|p| match p.to_uppercase().as_str() {
                        "READ" => Ok(Permission::Read),
                        "WRITE" => Ok(Permission::Write),
                        "ADMIN" => Ok(Permission::Admin),
                        "SUPER" => Ok(Permission::Super),
                        "QUEUE" => Ok(Permission::Queue),
                        "CHATROOM" => Ok(Permission::Chatroom),
                        "REST" => Ok(Permission::Rest),
                        _ => Err(format!("Unknown permission: {}", p)),
                    })
                    .collect();

                let permissions = match permissions {
                    Ok(p) => p,
                    Err(e) => {
                        let execution_time = start_time.elapsed().as_millis() as u64;
                        return Json(CypherResponse {
                            columns: vec![],
                            rows: vec![],
                            execution_time_ms: execution_time,
                            error: Some(e),
                        });
                    }
                };

                // Check if target is a user (by username or id) or role
                let users_list = rbac.list_users();
                let target_user = users_list
                    .iter()
                    .find(|u| u.username == grant.target || u.id == grant.target);

                // Check if trying to modify root user
                if let Some(user) = target_user {
                    if user.is_root {
                        let execution_time = start_time.elapsed().as_millis() as u64;
                        return Json(CypherResponse {
                            columns: vec![],
                            rows: vec![],
                            execution_time_ms: execution_time,
                            error: Some("Cannot modify root user permissions. Only root users can modify root users.".to_string()),
                        });
                    }
                }

                let user_id = target_user.map(|u| u.id.clone());

                if let Some(user_id) = user_id {
                    // Grant to user
                    let is_admin_grant = permissions.iter().any(|p| matches!(p, Permission::Admin));
                    if let Some(user_mut) = rbac.get_user_mut(&user_id) {
                        for perm in &permissions {
                            user_mut.add_permission(perm.clone());
                        }
                    }

                    // Check if root should be disabled after granting Admin permission
                    if is_admin_grant {
                        drop(rbac); // Release lock before async call
                        server.check_and_disable_root_if_needed().await;
                        rbac = server.rbac.write().await; // Reacquire lock
                    }

                    rows.push(serde_json::json!([
                        grant.target.clone(),
                        grant.permissions.clone(),
                        format!("Granted permissions to user '{}'", grant.target)
                    ]));
                } else if let Some(role) = rbac.get_role_mut(&grant.target) {
                    // Grant to role
                    for perm in &permissions {
                        role.add_permission(perm.clone());
                    }
                    rows.push(serde_json::json!([
                        grant.target.clone(),
                        grant.permissions.clone(),
                        format!("Granted permissions to role '{}'", grant.target)
                    ]));
                } else {
                    let execution_time = start_time.elapsed().as_millis() as u64;
                    return Json(CypherResponse {
                        columns: vec![],
                        rows: vec![],
                        execution_time_ms: execution_time,
                        error: Some(format!("User or role '{}' not found", grant.target)),
                    });
                }
            }
            nexus_core::executor::parser::Clause::Revoke(revoke) => {
                columns = vec![
                    "target".to_string(),
                    "permissions".to_string(),
                    "message".to_string(),
                ];

                // Parse permissions
                let permissions: Result<Vec<Permission>, _> = revoke
                    .permissions
                    .iter()
                    .map(|p| match p.to_uppercase().as_str() {
                        "READ" => Ok(Permission::Read),
                        "WRITE" => Ok(Permission::Write),
                        "ADMIN" => Ok(Permission::Admin),
                        "SUPER" => Ok(Permission::Super),
                        "QUEUE" => Ok(Permission::Queue),
                        "CHATROOM" => Ok(Permission::Chatroom),
                        "REST" => Ok(Permission::Rest),
                        _ => Err(format!("Unknown permission: {}", p)),
                    })
                    .collect();

                let permissions = match permissions {
                    Ok(p) => p,
                    Err(e) => {
                        let execution_time = start_time.elapsed().as_millis() as u64;
                        return Json(CypherResponse {
                            columns: vec![],
                            rows: vec![],
                            execution_time_ms: execution_time,
                            error: Some(e),
                        });
                    }
                };

                // Check if target is a user (by username or id) or role
                let users_list = rbac.list_users();
                let target_user = users_list
                    .iter()
                    .find(|u| u.username == revoke.target || u.id == revoke.target);

                // Check if trying to modify root user
                if let Some(user) = target_user {
                    if user.is_root {
                        let execution_time = start_time.elapsed().as_millis() as u64;
                        return Json(CypherResponse {
                            columns: vec![],
                            rows: vec![],
                            execution_time_ms: execution_time,
                            error: Some("Cannot modify root user permissions. Only root users can modify root users.".to_string()),
                        });
                    }
                }

                let user_id = target_user.map(|u| u.id.clone());

                if let Some(user_id) = user_id {
                    // Revoke from user
                    if let Some(user_mut) = rbac.get_user_mut(&user_id) {
                        for perm in &permissions {
                            user_mut.remove_permission(perm);
                        }
                    }
                    rows.push(serde_json::json!([
                        revoke.target.clone(),
                        revoke.permissions.clone(),
                        format!("Revoked permissions from user '{}'", revoke.target)
                    ]));
                } else if let Some(role) = rbac.get_role_mut(&revoke.target) {
                    // Revoke from role
                    for perm in &permissions {
                        role.remove_permission(perm);
                    }
                    rows.push(serde_json::json!([
                        revoke.target.clone(),
                        revoke.permissions.clone(),
                        format!("Revoked permissions from role '{}'", revoke.target)
                    ]));
                } else {
                    let execution_time = start_time.elapsed().as_millis() as u64;
                    return Json(CypherResponse {
                        columns: vec![],
                        rows: vec![],
                        execution_time_ms: execution_time,
                        error: Some(format!("User or role '{}' not found", revoke.target)),
                    });
                }
            }
            _ => {}
        }
    }

    let execution_time = start_time.elapsed().as_millis() as u64;
    Json(CypherResponse {
        columns,
        rows,
        execution_time_ms: execution_time,
        error: None,
    })
}

/// Execute query management commands (SHOW QUERIES, TERMINATE QUERY)
#[cfg_attr(test, allow(dead_code))]
pub(crate) async fn execute_query_management_commands(
    server: Arc<NexusServer>,
    ast: &nexus_core::executor::parser::CypherQuery,
    start_time: std::time::Instant,
) -> Json<CypherResponse> {
    let mut columns = Vec::new();
    let mut rows = Vec::new();

    for clause in &ast.clauses {
        match clause {
            nexus_core::executor::parser::Clause::ShowQueries => {
                columns = vec![
                    "queryId".to_string(),
                    "query".to_string(),
                    "connectionId".to_string(),
                    "startedAt".to_string(),
                    "elapsedTimeMs".to_string(),
                    "status".to_string(),
                ];

                let tracker = server.dbms_procedures.get_connection_tracker();
                let queries = tracker.get_running_queries();

                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();

                for query_info in queries {
                    let elapsed_ms = (now - query_info.started_at) * 1000;
                    let status = if query_info.is_running {
                        "running"
                    } else if query_info.cancelled {
                        "cancelled"
                    } else {
                        "completed"
                    };

                    rows.push(serde_json::json!([
                        query_info.query_id,
                        query_info.query,
                        query_info.connection_id,
                        query_info.started_at,
                        elapsed_ms,
                        status
                    ]));
                }
            }
            nexus_core::executor::parser::Clause::TerminateQuery(terminate_clause) => {
                columns = vec!["queryId".to_string(), "message".to_string()];

                let tracker = server.dbms_procedures.get_connection_tracker();
                let cancelled = tracker.cancel_query(&terminate_clause.query_id);

                if cancelled {
                    rows.push(serde_json::json!([
                        terminate_clause.query_id.clone(),
                        format!(
                            "Query '{}' terminated successfully",
                            terminate_clause.query_id
                        )
                    ]));
                } else {
                    let execution_time = start_time.elapsed().as_millis() as u64;
                    return Json(CypherResponse {
                        columns: vec![],
                        rows: vec![],
                        execution_time_ms: execution_time,
                        error: Some(format!(
                            "Query '{}' not found or already completed",
                            terminate_clause.query_id
                        )),
                    });
                }
            }
            _ => {}
        }
    }

    let execution_time = start_time.elapsed().as_millis() as u64;
    Json(CypherResponse {
        columns,
        rows,
        execution_time_ms: execution_time,
        error: None,
    })
}

/// Execute API key management commands
#[cfg_attr(test, allow(dead_code))]
pub(crate) async fn execute_api_key_commands(
    server: Arc<NexusServer>,
    ast: &nexus_core::executor::parser::CypherQuery,
    start_time: std::time::Instant,
) -> Json<CypherResponse> {
    use chrono::{DateTime, Duration, Utc};
    use nexus_core::auth::Permission;

    let mut columns = Vec::new();
    let mut rows = Vec::new();

    let auth_manager = &server.auth_manager;

    // Helper function to parse duration string (e.g., "7d", "24h", "30m")
    fn parse_duration(duration_str: &str) -> Result<DateTime<Utc>, String> {
        let duration_str = duration_str.trim();
        if duration_str.is_empty() {
            return Err("Duration cannot be empty".to_string());
        }

        let (num_str, unit) = if let Some(pos) = duration_str
            .char_indices()
            .find(|(_, c)| c.is_alphabetic())
            .map(|(i, _)| i)
        {
            let (num, unit) = duration_str.split_at(pos);
            (num, unit)
        } else {
            return Err(format!("Invalid duration format: {}", duration_str));
        };

        let num: i64 = num_str
            .parse()
            .map_err(|_| format!("Invalid number in duration: {}", num_str))?;

        let duration = match unit.to_lowercase().as_str() {
            "s" | "sec" | "second" | "seconds" => Duration::seconds(num),
            "m" | "min" | "minute" | "minutes" => Duration::minutes(num),
            "h" | "hr" | "hour" | "hours" => Duration::hours(num),
            "d" | "day" | "days" => Duration::days(num),
            "w" | "week" | "weeks" => Duration::weeks(num),
            "mo" | "month" | "months" => Duration::days(num * 30), // Approximate
            "y" | "yr" | "year" | "years" => Duration::days(num * 365), // Approximate
            _ => return Err(format!("Unknown duration unit: {}", unit)),
        };

        Ok(Utc::now() + duration)
    }

    for clause in &ast.clauses {
        match clause {
            nexus_core::executor::parser::Clause::CreateApiKey(create_key) => {
                columns = vec![
                    "key_id".to_string(),
                    "name".to_string(),
                    "key".to_string(),
                    "message".to_string(),
                ];

                // Parse permissions
                let permissions: Result<Vec<Permission>, _> = create_key
                    .permissions
                    .iter()
                    .map(|p| match p.to_uppercase().as_str() {
                        "READ" => Ok(Permission::Read),
                        "WRITE" => Ok(Permission::Write),
                        "ADMIN" => Ok(Permission::Admin),
                        "SUPER" => Ok(Permission::Super),
                        "QUEUE" => Ok(Permission::Queue),
                        "CHATROOM" => Ok(Permission::Chatroom),
                        "REST" => Ok(Permission::Rest),
                        _ => Err(format!("Unknown permission: {}", p)),
                    })
                    .collect();

                let permissions = match permissions {
                    Ok(p) => {
                        if p.is_empty() {
                            // Default permissions if none specified
                            vec![Permission::Read, Permission::Write]
                        } else {
                            p
                        }
                    }
                    Err(e) => {
                        let execution_time = start_time.elapsed().as_millis() as u64;
                        return Json(CypherResponse {
                            columns: vec![],
                            rows: vec![],
                            execution_time_ms: execution_time,
                            error: Some(e),
                        });
                    }
                };

                // Resolve user_id if username is provided
                let user_id = if let Some(ref username) = create_key.user_id {
                    let rbac = server.rbac.read().await;
                    let users_list = rbac.list_users();
                    match users_list.iter().find(|u| u.username == *username) {
                        Some(user) => Some(user.id.clone()),
                        None => {
                            let execution_time = start_time.elapsed().as_millis() as u64;
                            return Json(CypherResponse {
                                columns: vec![],
                                rows: vec![],
                                execution_time_ms: execution_time,
                                error: Some(format!("User '{}' not found", username)),
                            });
                        }
                    }
                } else {
                    None
                };

                // Parse expiration if provided
                let expires_at = if let Some(ref duration_str) = create_key.expires_in {
                    match parse_duration(duration_str) {
                        Ok(dt) => Some(dt),
                        Err(e) => {
                            let execution_time = start_time.elapsed().as_millis() as u64;
                            return Json(CypherResponse {
                                columns: vec![],
                                rows: vec![],
                                execution_time_ms: execution_time,
                                error: Some(e),
                            });
                        }
                    }
                } else {
                    None
                };

                // Generate API key
                let result = if let Some(user_id) = user_id {
                    if let Some(expires_at) = expires_at {
                        // User ID with expiration
                        auth_manager.generate_api_key_for_user_with_expiration(
                            create_key.name.clone(),
                            user_id,
                            permissions,
                            expires_at,
                        )
                    } else {
                        // User ID without expiration
                        auth_manager.generate_api_key_for_user(
                            create_key.name.clone(),
                            user_id,
                            permissions,
                        )
                    }
                } else if let Some(expires_at) = expires_at {
                    // Temporary key without user
                    auth_manager.generate_temporary_api_key(
                        create_key.name.clone(),
                        permissions,
                        expires_at,
                    )
                } else {
                    // Regular key without user
                    auth_manager.generate_api_key(create_key.name.clone(), permissions)
                };

                match result {
                    Ok((api_key, full_key)) => {
                        rows.push(serde_json::json!([
                            api_key.id.clone(),
                            api_key.name.clone(),
                            full_key,
                            format!("API key '{}' created successfully", create_key.name)
                        ]));
                    }
                    Err(e) => {
                        let execution_time = start_time.elapsed().as_millis() as u64;
                        return Json(CypherResponse {
                            columns: vec![],
                            rows: vec![],
                            execution_time_ms: execution_time,
                            error: Some(format!("Failed to create API key: {}", e)),
                        });
                    }
                }
            }
            nexus_core::executor::parser::Clause::ShowApiKeys(show_keys) => {
                columns = vec![
                    "key_id".to_string(),
                    "name".to_string(),
                    "user_id".to_string(),
                    "permissions".to_string(),
                    "created_at".to_string(),
                    "expires_at".to_string(),
                    "is_active".to_string(),
                    "is_revoked".to_string(),
                ];

                let api_keys = if let Some(ref username) = show_keys.user_id {
                    // Get keys for specific user
                    let rbac = server.rbac.read().await;
                    let users_list = rbac.list_users();
                    if let Some(user) = users_list.iter().find(|u| u.username == *username) {
                        auth_manager.get_api_keys_for_user(&user.id)
                    } else {
                        let execution_time = start_time.elapsed().as_millis() as u64;
                        return Json(CypherResponse {
                            columns: vec![],
                            rows: vec![],
                            execution_time_ms: execution_time,
                            error: Some(format!("User '{}' not found", username)),
                        });
                    }
                } else {
                    // Get all keys
                    auth_manager.list_api_keys()
                };

                for api_key in api_keys {
                    let permissions: Vec<String> =
                        api_key.permissions.iter().map(|p| p.to_string()).collect();
                    rows.push(serde_json::json!([
                        api_key.id.clone(),
                        api_key.name.clone(),
                        api_key.user_id.clone().unwrap_or_default(),
                        permissions,
                        api_key.created_at.to_rfc3339(),
                        api_key
                            .expires_at
                            .map(|dt| dt.to_rfc3339())
                            .unwrap_or_default(),
                        api_key.is_active,
                        api_key.is_revoked,
                    ]));
                }
            }
            nexus_core::executor::parser::Clause::RevokeApiKey(revoke_key) => {
                columns = vec!["key_id".to_string(), "message".to_string()];

                match auth_manager.revoke_api_key(&revoke_key.key_id, revoke_key.reason.clone()) {
                    Ok(_) => {
                        rows.push(serde_json::json!([
                            revoke_key.key_id.clone(),
                            format!("API key '{}' revoked successfully", revoke_key.key_id)
                        ]));
                    }
                    Err(e) => {
                        let execution_time = start_time.elapsed().as_millis() as u64;
                        return Json(CypherResponse {
                            columns: vec![],
                            rows: vec![],
                            execution_time_ms: execution_time,
                            error: Some(format!("Failed to revoke API key: {}", e)),
                        });
                    }
                }
            }
            nexus_core::executor::parser::Clause::DeleteApiKey(delete_key) => {
                columns = vec!["key_id".to_string(), "message".to_string()];

                if auth_manager.delete_api_key(&delete_key.key_id) {
                    rows.push(serde_json::json!([
                        delete_key.key_id.clone(),
                        format!("API key '{}' deleted successfully", delete_key.key_id)
                    ]));
                } else {
                    let execution_time = start_time.elapsed().as_millis() as u64;
                    return Json(CypherResponse {
                        columns: vec![],
                        rows: vec![],
                        execution_time_ms: execution_time,
                        error: Some(format!("API key '{}' not found", delete_key.key_id)),
                    });
                }
            }
            _ => {}
        }
    }

    let execution_time = start_time.elapsed().as_millis() as u64;
    Json(CypherResponse {
        columns,
        rows,
        execution_time_ms: execution_time,
        error: None,
    })
}
