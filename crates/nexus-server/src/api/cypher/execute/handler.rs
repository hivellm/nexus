//! `execute_cypher` — the main Cypher query HTTP handler. Drives the
//! planner, runs the executor, handles write paths (CREATE / MERGE /
//! SET / REMOVE / DELETE / FOREACH), tracks metrics, and builds the
//! JSON response.

use super::super::*;

/// Emit a write-operation audit log entry for the `/cypher` CREATE/MERGE
/// path.
///
/// Mirrors the `WriteOperationParams` / `AuditResult` shape
/// `write_ops.rs` used to build per-clause, but records a single
/// query-level entry instead: `Engine::execute_cypher_with_params` is
/// the one write entry point (phase1_http-merge-rel-and-set-rel-parity,
/// `docs/nexus/04-write-path-unification.md` Step 2) and does not expose
/// per-node/per-relationship hooks the way the old hand-rolled
/// `write_ops.rs` interpreter did. A query-level success/failure record
/// still answers the audit question that matters — who ran this write,
/// when, and did it succeed.
///
/// A failure to persist the audit entry itself does not fail the
/// request (fail-open, per `docs/security/SECURITY_AUDIT.md`) but is
/// never silently swallowed: it goes through
/// [`nexus_core::auth::record_audit_log_failure`], which bumps the
/// `audit_log_failures_total` counter and emits a `tracing::error!`.
async fn log_write_audit(
    server: &NexusServer,
    actor_info: &(Option<String>, Option<String>, Option<String>),
    operation_type: &str,
    cypher_query: &str,
    result: nexus_core::auth::AuditResult,
    failure_context: &'static str,
) {
    let (actor_user_id, actor_username, api_key_id) = actor_info.clone();
    if let Err(e) = server
        .audit_logger
        .log_write_operation(nexus_core::auth::WriteOperationParams {
            actor_user_id,
            actor_username,
            api_key_id,
            operation_type: operation_type.to_string(),
            entity_type: "PATTERN".to_string(),
            entity_id: None,
            cypher_query: Some(cypher_query.to_string()),
            result,
        })
        .await
    {
        nexus_core::auth::record_audit_log_failure(failure_context, &e);
    }
}

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

    // Explicit transaction commands (BEGIN / COMMIT / ROLLBACK / SAVEPOINT)
    // must reach the ENGINE's session-transaction machinery. Without this
    // branch they fall through to the bare executor clone at the bottom of
    // this handler, which silently no-ops them (HTTP 200, empty result) —
    // so BEGIN never opened a transaction, ROLLBACK never rolled anything
    // back, and COMMIT/ROLLBACK without BEGIN "succeeded". Found by manual
    // Docker validation of phase6_fix-rollback-executor-created-nodes.
    {
        use nexus_core::executor::parser::Clause;
        let has_tx_cmd = ast.clauses.iter().any(|c| {
            matches!(
                c,
                Clause::BeginTransaction
                    | Clause::CommitTransaction
                    | Clause::RollbackTransaction
                    | Clause::Savepoint(_)
                    | Clause::RollbackToSavepoint(_)
                    | Clause::ReleaseSavepoint(_)
            )
        });
        if has_tx_cmd {
            let mut engine = server.engine.write().await;
            let execution_time = start_time.elapsed().as_millis() as u64;
            return match engine.execute_cypher(&request.query) {
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

    // Route on the parsed AST instead of query text (write-path
    // unification Step 3, `docs/nexus/04-write-path-unification.md`;
    // `api::cypher::routing` module docs explain the shared predicate in
    // full). `routing::first_write_kind` finds the first `CREATE`/`MERGE`
    // clause anywhere in `ast.clauses`; `routing::needs_engine_interception`
    // is the broader "must this reach the engine at all" check (MATCH /
    // CREATE / DELETE / MERGE / SET / REMOVE / FOREACH). Clause-typed
    // routing — unlike the former `query_upper.starts_with(...)`
    // heuristics — naturally excludes every DDL form (`CREATE INDEX`,
    // `CREATE SPATIAL INDEX`, `CREATE CONSTRAINT`, ...): those parse into
    // their own `Clause::CreateIndex`/`Clause::CreateConstraint`/etc.
    // variants, never `Clause::Create`, so they still fall through to the
    // executor path below with no separate string-based DDL exclusion
    // list needed. It also fixes bug L1 — a query whose write clause
    // isn't the first token (a leading `//` comment, a lowercase
    // `create`, or `MATCH (a),(b) CREATE (a)-[r]->(b)`) is now routed to
    // the engine instead of silently falling through to the read-only
    // executor.
    let write_kind = routing::first_write_kind(&ast);

    if let Some(operation_type) = write_kind {
        let failure_context = if operation_type == "MERGE" {
            "http_write_merge"
        } else {
            "http_write_create"
        };

        // Route through the engine's single write entry point — the
        // same call the MATCH/UNWIND branches above use — instead of
        // the `write_ops.rs` fork, which never learned relationship
        // semantics (MERGE-rel, SET on a relationship variable,
        // same-statement `RETURN r.prop` all silently dropped data;
        // see phase1_http-merge-rel-and-set-rel-parity proposal.md).
        // phase8_neo4j-concurrency-gaps §3 — `ast` was already parsed
        // above (outside this write lock) for routing; use the
        // pre-parsed-AST entry point instead of
        // `execute_cypher_with_params` so the exclusive lock's
        // critical section no longer pays for a second parse of the
        // same query text. See `Engine::execute_cypher_ast_with_params`'s
        // doc comment.
        let mut engine_guard = server.engine.write().await;
        let dispatch_result = engine_guard.execute_cypher_ast_with_params(
            &ast,
            &request.query,
            request.params.clone(),
        );
        // Release the write lock before the (async) audit-log call —
        // auditing never touches the engine, and holding a write lock
        // across an `.await` unnecessarily serializes unrelated writes.
        drop(engine_guard);

        let execution_time = start_time.elapsed().as_millis() as u64;
        return match dispatch_result {
            Ok(result_set) => {
                tracing::info!(
                    "{} query executed successfully in {}ms, {} rows returned",
                    operation_type,
                    execution_time,
                    result_set.rows.len()
                );
                log_write_audit(
                    &server,
                    &actor_info,
                    operation_type,
                    &request.query,
                    nexus_core::auth::AuditResult::Success,
                    failure_context,
                )
                .await;

                Json(CypherResponse {
                    columns: result_set.columns,
                    rows: result_set
                        .rows
                        .into_iter()
                        .map(|row| serde_json::Value::Array(row.values))
                        .collect(),
                    execution_time_ms: execution_time,
                    error: None,
                    notifications: result_set.notifications,
                })
            }
            Err(e) => {
                tracing::error!("{} query execution failed: {}", operation_type, e);
                log_write_audit(
                    &server,
                    &actor_info,
                    operation_type,
                    &request.query,
                    nexus_core::auth::AuditResult::Failure {
                        error: e.to_string(),
                    },
                    failure_context,
                )
                .await;

                Json(CypherResponse {
                    columns: vec![],
                    rows: vec![],
                    execution_time_ms: execution_time,
                    error: Some(e.to_string()),
                    notifications: Vec::new(),
                })
            }
        };
    }

    // Any remaining engine-intercepted clause (MATCH reads, or a write
    // clause combination `first_write_kind` didn't classify as CREATE/MERGE,
    // e.g. a bare `MATCH ... SET`/`MATCH ... DELETE`) still needs the
    // engine rather than the lock-free executor below.
    if routing::needs_engine_interception(&ast) {
        // phase5_lock-free-read-path: carve the pure-read subset of this
        // bucket (MATCH / OPTIONAL MATCH / WITH / UNWIND / ... with no
        // write clause, no DDL, not an explicit transaction command —
        // see `routing::is_read_only`) out of the exclusive engine lock
        // that every other clause here still needs. Bottleneck #1 in
        // `docs/nexus/03-performance.md`: every MATCH query used to take
        // `server.engine.write().await` for its entire parse+plan+execute
        // duration, serializing all reads against each other server-wide.
        //
        // An in-progress explicit transaction (`BEGIN TRANSACTION` not yet
        // COMMIT/ROLLBACK-ed) must still route through the engine so a
        // read sees that session's own uncommitted writes
        // (read-your-own-writes) — the lock-free executor clone below can
        // only ever reflect the last COMMIT/ROLLBACK's
        // `Engine::refresh_executor()` snapshot, never an in-flight
        // transaction's staged state.
        if routing::is_read_only(&ast) {
            // Acquire a SHARED read lock just long enough to (a) clone
            // `Engine::executor` — the same instance
            // `Engine::refresh_executor()` replaces with a fresh
            // snapshot after every commit/rollback/standalone-write, so
            // the clone taken here is guaranteed at least as fresh as
            // the last write that finished before this `.read().await`
            // was granted — and (b) check whether the autocommit
            // "default" session (the only session HTTP requests ever
            // use; see `Engine::execute_transaction_commands`) has an
            // open explicit transaction. `Executor` clones are cheap —
            // a thin wrapper around `Arc`'d shared state (see
            // `executor::engine::Executor::clone`) — and, unlike the
            // exclusive `.write().await` this branch used to take for
            // every MATCH, an arbitrary number of readers can hold
            // `.read().await` concurrently: this no longer serializes
            // reads against each other.
            let (lock_free_executor, in_explicit_tx) = {
                let engine_guard = server.engine.read().await;
                let in_tx = engine_guard
                    .session_manager
                    .get_session(&"default".to_string())
                    .map(|session| session.has_active_transaction())
                    .unwrap_or(false);
                (engine_guard.executor.clone(), in_tx)
            };

            if !in_explicit_tx {
                let query = Query {
                    cypher: request.query.clone(),
                    params: request.params.clone(),
                };

                let execution_result =
                    match tokio::task::spawn_blocking(move || lock_free_executor.execute(&query))
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

                let execution_time_ms = start_time.elapsed().as_millis() as u64;
                return match execution_result {
                    Ok(result_set) => {
                        tracing::info!(
                            "Read-only query executed via lock-free path in {}ms, {} rows returned",
                            execution_time_ms,
                            result_set.rows.len()
                        );
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
                        tracing::error!("Read-only query execution failed: {}", e);
                        Json(CypherResponse {
                            columns: vec![],
                            rows: vec![],
                            execution_time_ms,
                            error: Some(e.to_string()),
                            notifications: Vec::new(),
                        })
                    }
                };
            }
            // Else: an explicit transaction is open on the "default"
            // session — fall through to the engine-locked path below so
            // this read observes that transaction's own uncommitted
            // writes.
        }

        {
            // Use the engine's execute_cypher method which uses its internal executor.
            // phase8_neo4j-concurrency-gaps §3 — reuse the `ast` parsed
            // above instead of re-parsing inside the exclusive write
            // lock; see `Engine::execute_cypher_ast_with_params`.
            let mut engine_guard = server.engine.write().await;
            match engine_guard.execute_cypher_ast_with_params(
                &ast,
                &request.query,
                request.params.clone(),
            ) {
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
