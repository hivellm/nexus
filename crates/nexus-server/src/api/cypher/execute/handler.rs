//! `execute_cypher` — the main Cypher query HTTP handler. Drives the
//! planner, runs the executor, handles write paths (CREATE / MERGE /
//! SET / REMOVE / DELETE / FOREACH), tracks metrics, and builds the
//! JSON response.

use super::super::*;
use super::write_ops::execute_create_or_merge;

pub async fn execute_cypher(
    State(server): State<Arc<NexusServer>>,
    auth_context: Option<Extension<Option<AuthContext>>>,
    Json(request): Json<CypherRequest>,
) -> Json<CypherResponse> {
    tracing::debug!("[CYPHER-API] Received query: {}", request.query);
    let auth_context = auth_context.and_then(|e| e.0);
    let start_time = std::time::Instant::now();
    let query_for_tracking = request.query.clone();

    // Register connection and query for tracking
    // Note: ConnectInfo requires special router setup, using fallback for now
    let client_address = "unknown".to_string(); // Will be improved when ConnectInfo is enabled
    let (_connection_id, _query_guard) = register_connection_and_query_fallback(
        &server,
        &query_for_tracking,
        &client_address,
        &auth_context,
    );
    // Hold the guard for the entire handler lifetime — its `Drop`
    // impl calls `complete_query` so panics and early returns can't
    // leak a "running" entry. The previous design recycled
    // `connection_id` as `query_id` and called
    // `mark_query_completed` manually on the success/error tails;
    // that path silently no-op'd on `connection_id != query_id`
    // and leaked on every other return point. Keep `query_id` as a
    // string for the existing tracing/metrics call sites that read
    // it; resolve to the guard's id so logs match the tracker.
    let _query_id = _query_guard.query_id().to_string();

    tracing::info!("Executing Cypher query: {}", request.query);

    // Extract actor info from auth context for audit logging
    let actor_info = auth_context
        .as_ref()
        .map(|ctx| {
            let api_key_id = Some(ctx.api_key.id.clone());
            let user_id = ctx.api_key.user_id.clone();
            let username = None; // Username not available in ApiKey
            (user_id, username, api_key_id)
        })
        .unwrap_or((None, None, None));
    // Parse query first to check for admin commands
    use nexus_core::executor::parser::CypherParser;
    let mut parser = CypherParser::new(request.query.clone());
    let ast = match parser.parse() {
        Ok(ast) => ast,
        Err(e) => {
            let execution_time = start_time.elapsed().as_millis() as u64;
            tracing::error!("Parse error: {}", e);
            return Json(CypherResponse {
                columns: vec![],
                rows: vec![],
                execution_time_ms: execution_time,
                error: Some(format!("Parse error: {}", e)),
                notifications: Vec::new(),
            });
        }
    };

    // Check for database management commands
    let has_db_cmd = ast.clauses.iter().any(|c| {
        matches!(
            c,
            nexus_core::executor::parser::Clause::CreateDatabase(_)
                | nexus_core::executor::parser::Clause::DropDatabase(_)
                | nexus_core::executor::parser::Clause::ShowDatabases
                | nexus_core::executor::parser::Clause::UseDatabase(_)
        )
    });

    if has_db_cmd {
        return execute_database_commands(server, &ast, start_time).await;
    }

    // Check for user management commands
    let has_user_cmd = ast.clauses.iter().any(|c| {
        matches!(
            c,
            nexus_core::executor::parser::Clause::ShowUsers
                | nexus_core::executor::parser::Clause::ShowUser(_)
                | nexus_core::executor::parser::Clause::CreateUser(_)
                | nexus_core::executor::parser::Clause::DropUser(_)
                | nexus_core::executor::parser::Clause::Grant(_)
                | nexus_core::executor::parser::Clause::Revoke(_)
        )
    });

    // Check for API key management commands
    let has_api_key_cmd = ast.clauses.iter().any(|c| {
        matches!(
            c,
            nexus_core::executor::parser::Clause::CreateApiKey(_)
                | nexus_core::executor::parser::Clause::ShowApiKeys(_)
                | nexus_core::executor::parser::Clause::RevokeApiKey(_)
                | nexus_core::executor::parser::Clause::DeleteApiKey(_)
        )
    });

    if has_api_key_cmd {
        return execute_api_key_commands(server, &ast, start_time).await;
    }

    if has_user_cmd {
        return execute_user_commands(server, &ast, start_time).await;
    }

    // Check for query management commands (SHOW QUERIES, TERMINATE QUERY)
    let has_query_mgmt_cmd = ast.clauses.iter().any(|c| {
        matches!(
            c,
            nexus_core::executor::parser::Clause::ShowQueries
                | nexus_core::executor::parser::Clause::TerminateQuery(_)
        )
    });

    if has_query_mgmt_cmd {
        return execute_query_management_commands(server.clone(), &ast, start_time).await;
    }

    // Check for SHOW CONSTRAINTS or SHOW FUNCTIONS commands
    let has_show_constraints_or_functions = ast.clauses.iter().any(|c| {
        matches!(
            c,
            nexus_core::executor::parser::Clause::ShowConstraints
                | nexus_core::executor::parser::Clause::ShowFunctions
                | nexus_core::executor::parser::Clause::CreateConstraint(_)
                | nexus_core::executor::parser::Clause::DropConstraint(_)
                | nexus_core::executor::parser::Clause::CreateFunction(_)
                | nexus_core::executor::parser::Clause::DropFunction(_)
        )
    });

    if has_show_constraints_or_functions {
        // Use Engine for these commands
        {
            let mut engine = server.engine.write().await;
            match engine.execute_cypher(&request.query) {
                Ok(result) => {
                    let execution_time = start_time.elapsed().as_millis() as u64;
                    let rows: Vec<serde_json::Value> = result
                        .rows
                        .into_iter()
                        .map(|row| serde_json::Value::Array(row.values))
                        .collect();
                    return Json(CypherResponse {
                        columns: result.columns,
                        rows,
                        execution_time_ms: execution_time,
                        error: None,
                        notifications: Vec::new(),
                    });
                }
                Err(e) => {
                    let execution_time = start_time.elapsed().as_millis() as u64;
                    return Json(CypherResponse {
                        columns: vec![],
                        rows: vec![],
                        execution_time_ms: execution_time,
                        error: Some(format!("Execution error: {}", e)),
                        notifications: Vec::new(),
                    });
                }
            }
        }
    }

    // Route property-index DDL (CREATE INDEX / DROP INDEX) through the engine
    // so that `property_index.create_index` + `populate_index` are called and
    // subsequent reads / MERGE existence checks see the index (issue #9).
    // Spatial and fulltext index DDL stays on the executor fallthrough below.
    let is_property_index_ddl = ast.clauses.iter().any(|c| match c {
        nexus_core::executor::parser::Clause::CreateIndex(ci) => {
            !matches!(ci.index_type.as_deref(), Some("spatial") | Some("fulltext"))
        }
        nexus_core::executor::parser::Clause::DropIndex(_) => true,
        _ => false,
    });

    if is_property_index_ddl {
        let mut engine = server.engine.write().await;
        match engine.execute_cypher(&request.query) {
            Ok(result) => {
                let execution_time = start_time.elapsed().as_millis() as u64;
                // Preserve the single-column ["index"] shape that the executor
                // path returned so existing clients are unaffected.
                // The engine returns ["index", "message"] with :Label(prop)
                // style names; reformat to "Label.property.property" to match.
                let rows: Vec<serde_json::Value> = result
                    .rows
                    .into_iter()
                    .map(|row| {
                        // row.values[0] is the index name from the engine
                        // (e.g. ":Turn(id)").  We need "Turn.id.property".
                        let engine_name = match row.values.first() {
                            Some(serde_json::Value::String(s)) => s.clone(),
                            Some(v) => v.to_string(),
                            None => String::new(),
                        };
                        // Strip leading ":" and convert "(prop)" → ".prop.property"
                        let reformatted = if let Some(stripped) = engine_name.strip_prefix(':') {
                            if let Some(paren) = stripped.find('(') {
                                let label = &stripped[..paren];
                                let rest = &stripped[paren + 1..];
                                let prop = rest.trim_end_matches(')');
                                format!("{}.{}.property", label, prop)
                            } else {
                                engine_name.clone()
                            }
                        } else {
                            engine_name.clone()
                        };
                        serde_json::Value::Array(vec![serde_json::Value::String(reformatted)])
                    })
                    .collect();
                return Json(CypherResponse {
                    columns: vec!["index".to_string()],
                    rows,
                    execution_time_ms: execution_time,
                    error: None,
                    notifications: Vec::new(),
                });
            }
            Err(e) => {
                let execution_time = start_time.elapsed().as_millis() as u64;
                return Json(CypherResponse {
                    columns: vec![],
                    rows: vec![],
                    execution_time_ms: execution_time,
                    error: Some(format!("Execution error: {}", e)),
                    notifications: Vec::new(),
                });
            }
        }
    }

    // UNWIND-driven writes (issue #13): `UNWIND list AS row MERGE/SET/...`.
    // The string-prefix routing below only flags writes that *start with*
    // CREATE/MERGE, so an UNWIND-prefixed write falls through to the read
    // executor and silently persists nothing (200 / count 0). Detect it from
    // the parsed AST and route through the engine write path, which iterates
    // the rows.
    {
        use nexus_core::executor::parser::Clause;
        let has_unwind = ast.clauses.iter().any(|c| matches!(c, Clause::Unwind(_)));
        let has_write = ast.clauses.iter().any(|c| {
            matches!(
                c,
                Clause::Merge(_)
                    | Clause::Set(_)
                    | Clause::Remove(_)
                    | Clause::Foreach(_)
                    | Clause::Create(_)
            )
        });
        if has_unwind && has_write {
            let mut engine = server.engine.write().await;
            let execution_time = start_time.elapsed().as_millis() as u64;
            return match engine.execute_cypher_with_params(&request.query, request.params.clone()) {
                Ok(result) => {
                    let rows: Vec<serde_json::Value> = result
                        .rows
                        .into_iter()
                        .map(|row| serde_json::Value::Array(row.values))
                        .collect();
                    Json(CypherResponse {
                        columns: result.columns,
                        rows,
                        execution_time_ms: execution_time,
                        error: None,
                        notifications: result.notifications,
                    })
                }
                Err(e) => Json(CypherResponse {
                    columns: vec![],
                    rows: vec![],
                    execution_time_ms: execution_time,
                    error: Some(format!("Execution error: {}", e)),
                    notifications: Vec::new(),
                }),
            };
        }
    }

    // Check if this is a CREATE, MERGE, SET, DELETE, REMOVE, or MATCH query
    let query_upper = request.query.trim().to_uppercase();
    // DDL statements (`CREATE INDEX`, `CREATE SPATIAL INDEX`,
    // `CREATE FULLTEXT INDEX`, `CREATE CONSTRAINT`, `DROP INDEX`,
    // `DROP CONSTRAINT`) MUST fall through to the executor path so
    // the `execute_create_index` / `execute_drop_index` operators
    // register the index on the shared `IndexManager` + the
    // executor-shared `spatial_indexes` map. The node-CREATE branch
    // below only understands `CREATE (node { ... })` patterns — it
    // silently no-ops DDL, which manifested as
    // `ERR_SPATIAL_INDEX_NOT_FOUND` on every subsequent
    // `spatial.*` call in slice-A smoke tests.
    let is_ddl_query = query_upper.starts_with("CREATE INDEX")
        || query_upper.starts_with("CREATE OR REPLACE INDEX")
        || query_upper.starts_with("CREATE SPATIAL INDEX")
        || query_upper.starts_with("CREATE OR REPLACE SPATIAL INDEX")
        || query_upper.starts_with("CREATE FULLTEXT INDEX")
        || query_upper.starts_with("CREATE OR REPLACE FULLTEXT INDEX")
        || query_upper.starts_with("CREATE CONSTRAINT")
        || query_upper.starts_with("CREATE OR REPLACE CONSTRAINT")
        || query_upper.starts_with("DROP INDEX")
        || query_upper.starts_with("DROP CONSTRAINT");
    let is_create_query = !is_ddl_query && query_upper.starts_with("CREATE");
    let is_merge_query = query_upper.starts_with("MERGE");
    let _is_set_query = query_upper.starts_with("SET");
    let _is_delete_query = query_upper.starts_with("DELETE");
    let _is_remove_query = query_upper.starts_with("REMOVE");
    // MATCH queries can start with MATCH or have MATCH after UNWIND/WITH/OPTIONAL clauses
    // We need to detect MATCH anywhere in the query to route it through the Engine
    let is_match_query = query_upper.starts_with("MATCH")
        || query_upper.contains(" MATCH ")
        || query_upper.contains(" MATCH(")
        || query_upper.starts_with("OPTIONAL MATCH")
        || query_upper.contains(" OPTIONAL MATCH");

    if is_create_query || is_merge_query {
        return execute_create_or_merge(
            server,
            &request,
            &ast,
            start_time,
            actor_info,
            is_merge_query,
        )
        .await;
    }

    // For MATCH queries, use the engine's executor to access the shared storage
    if is_match_query {
        {
            // Use the engine's execute_cypher method which uses its internal executor
            let mut engine_guard = server.engine.write().await;
            match engine_guard.execute_cypher_with_params(&request.query, request.params.clone()) {
                Ok(result_set) => {
                    let execution_time = start_time.elapsed().as_millis() as u64;
                    tracing::info!(
                        "MATCH query executed successfully in {}ms, {} rows returned",
                        execution_time,
                        result_set.rows.len()
                    );

                    return Json(CypherResponse {
                        columns: result_set.columns,
                        rows: result_set
                            .rows
                            .into_iter()
                            .map(|row| serde_json::Value::Array(row.values))
                            .collect(),
                        execution_time_ms: execution_time,
                        error: None,
                        notifications: result_set.notifications,
                    });
                }
                Err(e) => {
                    let execution_time = start_time.elapsed().as_millis() as u64;
                    tracing::error!("MATCH query execution failed: {}", e);

                    return Json(CypherResponse {
                        columns: vec![],
                        rows: vec![],
                        execution_time_ms: execution_time,
                        error: Some(e.to_string()),
                        notifications: Vec::new(),
                    });
                }
            }
        }
    }

    // Get executor instance for other queries
    // Executor is Clone and contains only Arc internally, so we can clone directly
    // without any locks - this enables true parallel execution
    let executor = server.executor.clone();

    // Create query
    let query = Query {
        cypher: request.query.clone(),
        params: request.params,
    };

    // Check cache status before execution
    let (cache_hits, cache_misses) = check_query_cache_status(&server, &request.query);

    // Track memory usage during query execution
    let initial_memory =
        nexus_core::performance::memory_tracking::QueryMemoryTracker::get_current_memory_usage()
            .ok();

    // Execute query - clone executor for concurrent execution
    // This removes the global lock bottleneck - each query gets its own executor clone
    // that shares the underlying data structures (catalog, store, indexes) via Arc
    // Use spawn_blocking to execute in a separate thread pool for true parallelism
    // No lock needed - Executor is Clone and Arc is thread-safe
    let executor_clone = executor.clone();
    let query_clone = query.clone();

    // Debug: Log thread info before spawning
    let thread_id_before = std::thread::current().id();
    tracing::debug!("Spawning blocking task from thread {:?}", thread_id_before);

    // Execute in blocking thread pool for true parallel execution
    // This allows multiple queries to run concurrently across CPU cores
    // Tokio's blocking thread pool automatically scales with CPU count
    let execution_result = match tokio::task::spawn_blocking(move || {
        let thread_id_after = std::thread::current().id();
        tracing::debug!("Executing in blocking thread {:?}", thread_id_after);

        let result = executor_clone.execute(&query_clone);
        tracing::debug!(
            "Query executed successfully in blocking thread {:?}",
            thread_id_after
        );
        result
    })
    .await
    {
        Ok(result) => result,
        Err(e) => {
            return Json(CypherResponse {
                columns: vec![],
                rows: vec![],
                execution_time_ms: start_time.elapsed().as_millis() as u64,
                error: Some(format!("Task execution error: {}", e)),
                notifications: Vec::new(),
            });
        }
    };

    // Get memory delta after execution
    let memory_usage = initial_memory.and_then(|initial| {
        nexus_core::performance::memory_tracking::QueryMemoryTracker::get_current_memory_usage()
            .ok()
            .map(|final_memory| final_memory.saturating_sub(initial))
            .filter(|&delta| delta > 1024) // Only include if > 1KB
    });

    match execution_result {
        Ok(result_set) => {
            let execution_time = start_time.elapsed();
            let execution_time_ms = execution_time.as_millis() as u64;
            let rows_count = result_set.rows.len();

            tracing::info!(
                "Query executed successfully in {}ms, {} rows returned{}",
                execution_time_ms,
                rows_count,
                memory_usage
                    .map(|m| format!(", {} bytes memory", m))
                    .unwrap_or_default()
            );

            // Record successful query execution with cache and memory metrics
            record_query_execution_with_metrics(
                &server,
                &query_for_tracking,
                execution_time,
                true,
                None,
                rows_count,
                memory_usage,
                Some(cache_hits),
                Some(cache_misses),
            );

            // Record Prometheus metrics
            let cache_hit = cache_hits > 0;
            record_prometheus_metrics(&server, execution_time_ms, true, cache_hit);

            // `_query_guard` (held in scope above) auto-completes
            // on Drop — no manual `mark_query_completed` needed.

            Json(CypherResponse {
                columns: result_set.columns,
                rows: result_set
                    .rows
                    .into_iter()
                    .map(|row| serde_json::Value::Array(row.values))
                    .collect(),
                execution_time_ms,
                error: None,
                notifications: result_set.notifications,
            })
        }
        Err(e) => {
            let execution_time = start_time.elapsed();
            let execution_time_ms = execution_time.as_millis() as u64;
            let error_msg = e.to_string();

            tracing::error!("Query execution failed: {}", error_msg);

            // Get memory delta even for failed queries (already calculated above)

            // Record failed query execution with cache and memory metrics
            record_query_execution_with_metrics(
                &server,
                &query_for_tracking,
                execution_time,
                false,
                Some(error_msg.clone()),
                0,
                memory_usage,
                Some(cache_hits),
                Some(cache_misses),
            );

            // Record Prometheus metrics
            let cache_hit = cache_hits > 0;
            record_prometheus_metrics(&server, execution_time_ms, false, cache_hit);

            // `_query_guard` (held in scope above) auto-completes
            // on Drop — no manual `mark_query_completed` needed.

            Json(CypherResponse {
                columns: vec![],
                rows: vec![],
                execution_time_ms,
                error: Some(error_msg),
                notifications: Vec::new(),
            })
        }
    }
}
