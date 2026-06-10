//! Top-level Cypher execution pipeline: `execute_cypher`, `execute_cypher_with_params`,
//! `execute_cypher_with_context`, `execute_cypher_dispatch`, `execute_cypher_ast`,
//! EXPLAIN/PROFILE, and supporting helpers. Extracted from `engine/mod.rs`.

use super::Engine;
use crate::{Error, Result, executor};
use serde_json::Value;
use std::collections::HashMap;

impl Engine {
    /// Execute a Cypher query with no tenant scoping — the
    /// pre-cluster-mode entry point. Standalone deployments and the
    /// internal test suite use this directly.
    pub fn execute_cypher(&mut self, query: &str) -> Result<executor::ResultSet> {
        self.execute_cypher_with_context(query, None, crate::cluster::TenantIsolationMode::None)
    }

    /// Execute a Cypher query with a client-supplied parameter map.
    ///
    /// The parameters are made visible to every write-path operator
    /// through `self.current_params` for the duration of the call and
    /// cleared on exit (RAII guard, so panics and early-return errors
    /// still release the slot). Currently consumed by the write-side
    /// dynamic-label resolver
    /// ([`dynamic_labels::resolve_labels`]); read-side operators
    /// receive parameters through the existing
    /// [`executor::Query::params`] path.
    pub fn execute_cypher_with_params(
        &mut self,
        query: &str,
        params: HashMap<String, Value>,
    ) -> Result<executor::ResultSet> {
        // Install the parameter map on `self.current_params` for the
        // duration of the call. A RAII guard can't borrow `self`
        // because we also need `&mut self` for the nested call, so
        // we clear manually after — wrapping the call in a closure
        // lets us route both Ok and Err through the same cleanup
        // path without the borrow-checker conflict.
        self.current_params = params;
        let result = self.execute_cypher_with_context(
            query,
            None,
            crate::cluster::TenantIsolationMode::None,
        );
        self.current_params.clear();
        result
    }

    /// Execute a Cypher query, optionally rewriting catalog-visible
    /// names to the tenant's namespaced form before planning.
    ///
    /// `ctx = None` or `mode = None` short-circuits to the
    /// pre-cluster-mode behaviour — the AST is not touched and the
    /// catalog sees unprefixed names, preserving standalone
    /// compatibility. When cluster mode is active and the
    /// `CatalogPrefix` isolation mode is selected, every label and
    /// relationship-type string in the parsed AST is rewritten
    /// through [`cluster::scope::scope_query`] so the catalog ends
    /// up with distinct IDs per tenant — data isolation follows
    /// transparently through the existing planner and storage.
    ///
    /// This is the single integration point for Phase 2 multi-tenant
    /// scoping. Every other code path inside the engine stays
    /// tenant-oblivious.
    ///
    /// [`cluster::scope::scope_query`]: crate::cluster::scope::scope_query
    pub fn execute_cypher_with_context(
        &mut self,
        query: &str,
        ctx: Option<&crate::cluster::UserContext>,
        mode: crate::cluster::TenantIsolationMode,
    ) -> Result<executor::ResultSet> {
        // Parse query to check if it contains CREATE or DELETE clauses
        let mut parser = executor::parser::CypherParser::new(query.to_string());
        let mut ast = parser.parse()?;

        // phase6_opencypher-advanced-types §6 — honour a leading
        // `GRAPH[name]` preamble. With a `DatabaseManager` wired to
        // the executor, the target database is resolved and either
        // served in place (when it matches the manager's default
        // name) or routed to the owning engine. Without a manager,
        // the scope cannot be resolved and we surface
        // `ERR_GRAPH_NOT_FOUND`.
        if let Some(requested) = ast.graph_scope.clone() {
            match crate::engine::graph_scope::resolve(self, &requested)? {
                crate::engine::graph_scope::ScopedDispatch::AcceptHere => {
                    // Fall through — the rest of this function runs
                    // against `self`, the correct engine.
                }
                crate::engine::graph_scope::ScopedDispatch::Route(target) => {
                    // Strip the preamble from the text query so the
                    // target engine doesn't loop on its own scope
                    // resolver. Parameters and cluster context flow
                    // through verbatim.
                    let cleaned = super::strip_graph_preamble(query);
                    let mut target_engine = target.write();
                    return target_engine.execute_cypher_with_context(&cleaned, ctx, mode);
                }
            }
        }

        // Cluster-mode scope rewrite. When a UserContext is present
        // AND the isolation mode asks for catalog-level prefixing,
        // rewrite every label / relationship-type in place, then
        // stash the rewritten AST as a one-shot override on the
        // executor. The executor's `execute()` consumes the override
        // exactly once (via `.take()`), so downstream call sites that
        // build a `Query { cypher: query.to_string(), .. }` don't
        // have to pass the scoped AST explicitly — it rides a
        // side-channel on `ExecutorShared`. Without this, the
        // executor's internal re-parse would silently discard the
        // tenant scope.
        //
        // Standalone deployments hit `should_rewrite(None) == false`
        // and the entire block is a no-op — no clone, no mutex take.
        let mut override_installed = false;
        if let Some(user_ctx) = ctx {
            if crate::cluster::scope::should_rewrite(mode) {
                crate::cluster::scope::scope_query(&mut ast, user_ctx.namespace(), mode);
                self.executor
                    .install_preparsed_ast_override(Some(ast.clone()));
                override_installed = true;
            }
        }
        // Ensure the one-shot override slot is cleared even if an
        // early-return path (EXPLAIN, PROFILE, admin command) skips
        // the normal executor.execute() that would consume it. A
        // stale override left on the slot would corrupt the NEXT
        // caller's query — fatal in cluster mode, so the cleanup
        // path uses an RAII guard. The guard owns a clone of the
        // executor (cheap — `Executor` is a thin newtype around
        // `Arc`'d `ExecutorShared`), which side-steps a borrow-
        // checker collision with the `&mut self` methods called
        // further down.
        struct OverrideGuard {
            executor: executor::Executor,
            active: bool,
        }
        impl Drop for OverrideGuard {
            fn drop(&mut self) {
                if self.active {
                    self.executor.install_preparsed_ast_override(None);
                }
            }
        }
        let _override_guard = OverrideGuard {
            executor: self.executor.clone(),
            active: override_installed,
        };

        // Cluster-mode write-path quota gate (Phase 4 §13). Fires
        // only when BOTH a UserContext AND a QuotaProvider are
        // installed — standalone deployments short-circuit on the
        // `has_quota_provider` check and never touch the provider.
        //
        // The check uses `check_storage(ns, 0)`: "is this tenant
        // already at or past its storage ceiling?". We don't yet
        // know the exact byte cost of the query (that's knowable
        // only after planning + partial execution), so the gate is
        // deliberately conservative — an already-exhausted tenant
        // can't grow further, but a tenant right at the edge may
        // sneak one more write in before the post-write
        // `record_usage` pushes them over. That's the right
        // trade-off for a first cut: never reject a write that
        // fits, always reject one that definitely does not.
        let is_write = crate::cluster::scope::is_write_query(&ast);
        if is_write {
            if let (Some(user_ctx), Some(provider)) = (ctx, self.quota_provider.as_ref()) {
                let decision = provider.check_storage(user_ctx.namespace(), 0);
                if let crate::cluster::QuotaDecision::Deny { reason, .. } = decision {
                    return Err(Error::QuotaExceeded(reason));
                }
            }
        }

        // Run the actual dispatch. We separate the post-execution
        // usage-recording step from the dispatch itself so every
        // success path feeds through a single bookkeeping point —
        // there are ~8 `return Ok(...)` sites inside the dispatcher
        // and instrumenting each individually is brittle.
        let dispatch_result = self.execute_cypher_dispatch(&ast, query);

        // Post-write usage charge (Phase 4 §13 / §14.1). Runs once,
        // after a successful write, once the RAII override guard
        // has had its chance to clear state on the error path.
        //
        // `storage_bytes` is a rough fixed heuristic for the first
        // cut: every write charges a baseline of 256 bytes against
        // the tenant. Accurate per-operation accounting (exact
        // record bytes written) needs the planner to thread a size
        // hint back up, which is tracked as a follow-up — under-
        // reporting is safer than over-reporting for the first
        // deployment.
        if is_write && dispatch_result.is_ok() {
            if let (Some(user_ctx), Some(provider)) = (ctx, self.quota_provider.as_ref()) {
                provider.record_usage(
                    user_ctx.namespace(),
                    crate::cluster::UsageDelta {
                        storage_bytes: 256,
                        requests: 1,
                    },
                );
            }
        }

        dispatch_result
    }

    /// Internal dispatcher — the original body of
    /// [`Self::execute_cypher_with_context`] minus the cluster-mode
    /// pre-check and post-record. Split out so the outer function
    /// can bracket every success path with a single
    /// `record_usage` call instead of instrumenting each of the
    /// ~8 `return Ok(...)` sites inside.
    fn execute_cypher_dispatch(
        &mut self,
        ast: &executor::parser::CypherQuery,
        query: &str,
    ) -> Result<executor::ResultSet> {
        // Check for EXPLAIN command
        if let Some(executor::parser::Clause::Explain(explain_clause)) = ast.clauses.first() {
            // Use stored query string if available, otherwise convert from AST
            let query_str = explain_clause
                .query_string
                .clone()
                .unwrap_or_else(|| self.query_to_string(&explain_clause.query));
            return self.execute_explain_with_string(&explain_clause.query, &query_str);
        }

        // Check for PROFILE command
        if let Some(executor::parser::Clause::Profile(profile_clause)) = ast.clauses.first() {
            // Use stored query string if available, otherwise convert from AST
            let query_str = profile_clause
                .query_string
                .clone()
                .unwrap_or_else(|| self.query_to_string(&profile_clause.query));
            return self.execute_profile_with_string(&profile_clause.query, &query_str);
        }

        // Check for administrative commands that need special handling
        // These commands (CREATE/DROP DATABASE, SHOW DATABASES, USE DATABASE) should be handled at server level
        // as Engine doesn't have access to DatabaseManager
        let has_admin_db_cmd = ast.clauses.iter().any(|c| {
            matches!(
                c,
                executor::parser::Clause::CreateDatabase(_)
                    | executor::parser::Clause::DropDatabase(_)
                    | executor::parser::Clause::ShowDatabases
                    | executor::parser::Clause::UseDatabase(_)
            )
        });

        if has_admin_db_cmd {
            return Err(Error::CypherExecution(
                "Database management commands (CREATE/DROP DATABASE, SHOW DATABASES, USE DATABASE) must be executed at server level".to_string(),
            ));
        }

        // Check for transaction commands
        let has_begin = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::BeginTransaction));
        let has_commit = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::CommitTransaction));
        let has_rollback = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::RollbackTransaction));
        // phase6_opencypher-advanced-types §5 — route savepoint
        // statements through the transaction-command path so they
        // share session resolution and return a uniform `status`
        // column.
        let has_savepoint_cmd = ast.clauses.iter().any(|c| {
            matches!(
                c,
                executor::parser::Clause::Savepoint(_)
                    | executor::parser::Clause::RollbackToSavepoint(_)
                    | executor::parser::Clause::ReleaseSavepoint(_)
            )
        });

        if has_begin || has_commit || has_rollback || has_savepoint_cmd {
            return self.execute_transaction_commands(ast, None);
        }

        // Check for index management commands
        let has_create_index = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::CreateIndex(_)));
        let has_drop_index = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::DropIndex(_)));

        if has_create_index || has_drop_index {
            return self.execute_index_commands(ast);
        }

        // Check for constraint management commands
        let has_create_constraint = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::CreateConstraint(_)));
        let has_drop_constraint = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::DropConstraint(_)));

        if has_create_constraint || has_drop_constraint {
            return self.execute_constraint_commands(ast);
        }

        // Check for function management commands
        let has_show_functions = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::ShowFunctions));
        let has_show_constraints = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::ShowConstraints));
        let has_create_function = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::CreateFunction(_)));
        let has_drop_function = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::DropFunction(_)));

        if has_show_functions || has_show_constraints || has_create_function || has_drop_function {
            return self.execute_function_commands(ast);
        }

        // Check for user management commands (should be handled at server level)
        let has_user_cmd = ast.clauses.iter().any(|c| {
            matches!(
                c,
                executor::parser::Clause::ShowUsers
                    | executor::parser::Clause::CreateUser(_)
                    | executor::parser::Clause::Grant(_)
                    | executor::parser::Clause::Revoke(_)
            )
        });

        if has_user_cmd {
            return Err(Error::CypherExecution(
                "User management commands (SHOW USERS, CREATE USER, GRANT, REVOKE) must be executed at server level".to_string(),
            ));
        }

        // Check if query contains CREATE or DELETE
        let has_create = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::Create(_)));
        let has_delete = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::Delete(_)));
        let has_merge = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::Merge(_)));
        let has_set_clause = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::Set(_)));
        let has_remove_clause = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::Remove(_)));
        let has_foreach = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::Foreach(_)));
        let has_match = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::Match(_)));
        // phase6 §8 — a CREATE clause binds node variables too, so
        // `CREATE (n) WITH n DELETE n` (the bench's create-delete
        // cycle) is legal per openCypher even with no MATCH.
        let has_create_bound_vars = ast.clauses.iter().any(|c| {
            if let executor::parser::Clause::Create(cc) = c {
                cc.pattern.elements.iter().any(|el| {
                    if let executor::parser::PatternElement::Node(node) = el {
                        node.variable.is_some()
                    } else {
                        false
                    }
                })
            } else {
                false
            }
        });

        // Handle DELETE (with or without MATCH)
        if has_delete {
            let deleted_count = if has_match || has_create_bound_vars {
                // MATCH ... DELETE or CREATE ... DELETE: execute the
                // upstream pattern first, then DELETE with results.
                self.execute_match_delete_query(ast)?
            } else {
                // Standalone DELETE won't work without an upstream
                // binding. DELETE n with no MATCH / CREATE / WITH to
                // produce `n` is genuinely invalid.
                return Err(Error::CypherSyntax(
                    "DELETE requires an upstream MATCH, CREATE, or WITH".to_string(),
                ));
            };
            self.refresh_executor()?;

            // Check if there's a RETURN clause after DELETE
            let return_clause_opt = ast.clauses.iter().find_map(|c| {
                if let executor::parser::Clause::Return(rc) = c {
                    Some(rc)
                } else {
                    None
                }
            });

            if let Some(return_clause) = return_clause_opt {
                // Check if RETURN contains count aggregation
                let mut is_count_only = false;
                let mut count_alias = "count".to_string();

                if return_clause.items.len() == 1 {
                    let executor::parser::ReturnItem { expression, alias } =
                        &return_clause.items[0];
                    if let executor::parser::Expression::FunctionCall { name, args: _ } = expression
                    {
                        if name.to_lowercase() == "count" {
                            is_count_only = true;
                            count_alias = alias.clone().unwrap_or_else(|| "count".to_string());
                        }
                    }
                }

                if is_count_only {
                    // Return count of deleted nodes
                    return Ok(executor::ResultSet::new(
                        vec![count_alias],
                        vec![executor::Row {
                            values: vec![serde_json::Value::Number(deleted_count.into())],
                        }],
                    ));
                } else {
                    // phase6 §8.2 — build an AST for the RETURN tail and
                    // install it as the executor's preparsed-AST override.
                    // Previously this path round-tripped the full AST
                    // through `query_to_string`, whose `format!("{:?}",
                    // clause)` implementation emits the Rust debug shape
                    // (`Create(CreateClause { pattern: ... })`), not
                    // valid Cypher. The executor then re-parsed that
                    // gibberish and failed with a mid-token syntax
                    // error. By handing the executor a pre-built AST
                    // we skip the re-parse entirely, so the CREATE +
                    // DELETE + RETURN shape (bench's
                    // `write.create_delete_cycle`) executes cleanly.
                    let tail_ast = executor::parser::CypherQuery {
                        clauses: vec![executor::parser::Clause::Return(return_clause.clone())],
                        params: ast.params.clone(),
                        graph_scope: ast.graph_scope.clone(),
                    };
                    struct OverrideGuard {
                        executor: executor::Executor,
                    }
                    impl Drop for OverrideGuard {
                        fn drop(&mut self) {
                            self.executor.install_preparsed_ast_override(None);
                        }
                    }
                    self.executor.install_preparsed_ast_override(Some(tail_ast));
                    let _guard = OverrideGuard {
                        executor: self.executor.clone(),
                    };
                    let query_obj = executor::Query {
                        cypher: String::new(),
                        params: ast.params.clone(),
                    };
                    return self.executor.execute(&query_obj);
                }
            } else {
                // No RETURN clause - return count of deleted nodes
                return Ok(executor::ResultSet::new(
                    vec!["count".to_string()],
                    vec![executor::Row {
                        values: vec![serde_json::Value::Number(deleted_count.into())],
                    }],
                ));
            }
        }

        // Handle MERGE / SET / REMOVE / FOREACH write queries before falling back to read executor
        if has_merge || has_set_clause || has_remove_clause || has_foreach {
            let result = self.execute_write_query(ast)?;
            return Ok(result);
        }

        // If query has CREATE (with or without MATCH), handle via Engine for persistence
        if has_create {
            if has_match {
                // MATCH ... CREATE: execute MATCH first, then CREATE with results
                let result = self.execute_match_create_query(ast, Some(query))?;

                // CRITICAL: Sync executor's store back to engine's storage
                // The executor has a cloned store, so changes need to be synced back
                self.storage = self.executor.get_store();

                // NOTE: Do NOT call refresh_executor() here!
                // The caller should call refresh_executor() explicitly when ready
                // This allows batching multiple CREATE statements before refreshing

                return Ok(result);
            }

            // Standalone CREATE - execute through executor only (not through Engine)
            // This prevents duplicate node creation
            // The executor will handle CREATE internally
            // Just refresh after to see changes. Attach the scoped AST
            // via `preparsed_ast` so cluster-mode label rewrites survive
            // the executor's parse step.
            //
            // phase6_opencypher-advanced-types §2 — if the CREATE
            // pattern contains a `:$param` dynamic-label sentinel, we
            // can't hand it to the executor's CREATE operator because
            // that path would register `"$ident"` as a literal label
            // in the catalog. Instead, route through the engine's
            // own write path which resolves the sentinel against
            // `self.current_params` before reaching the catalog.
            let has_dynamic_labels = ast.clauses.iter().any(|c| {
                if let executor::parser::Clause::Create(cc) = c {
                    cc.pattern.elements.iter().any(|e| {
                        if let executor::parser::PatternElement::Node(n) = e {
                            crate::engine::dynamic_labels::contains_dynamic(&n.labels)
                        } else {
                            false
                        }
                    })
                } else {
                    false
                }
            });
            if has_dynamic_labels {
                self.execute_create_via_engine(ast)?;
                return Ok(executor::ResultSet::new(
                    vec!["status".to_string()],
                    vec![executor::Row {
                        values: vec![serde_json::Value::String("ok".to_string())],
                    }],
                ));
            }
            let query_obj = executor::Query {
                cypher: query.to_string(),
                params: self.current_params.clone(),
            };
            // Watermark for typed property-index maintenance: the executor
            // CREATE path writes storage + the label index but NOT the
            // typed property B-tree, so a freshly created node was
            // invisible to `find_exact`/NodeIndexSeek (and a follow-up
            // MATCH {prop} SET silently no-opped). Index the id range the
            // executor allocates (exact under the single-writer model) —
            // same write-set source as the #15 scoped-commit maintenance.
            let pre_create_node_count = self.storage.node_count();
            let result = self.executor.execute(&query_obj)?;

            // CRITICAL: Sync executor's store back to engine's storage
            self.storage = self.executor.get_store();

            self.index_typed_properties_for_new_nodes(pre_create_node_count);

            // Refresh executor to see the changes (only if not in transaction)
            let session_id = "default";
            let in_transaction = {
                let session = self.session_manager.get_session(&session_id.to_string());
                session.map(|s| s.has_active_transaction()).unwrap_or(false)
            };

            if !in_transaction {
                self.refresh_executor()?;
            }

            return Ok(result);
        }

        // Execute the query normally. Attach the scoped AST so the
        // cluster-mode label rewrite (performed at the top of this
        // function) survives the executor's re-parse.
        let query_obj = executor::Query {
            cypher: query.to_string(),
            params: self.current_params.clone(),
        };
        self.executor.execute(&query_obj)
    }

    /// Execute EXPLAIN command - returns execution plan without executing query
    pub(super) fn execute_explain_with_string(
        &mut self,
        query: &executor::parser::CypherQuery,
        query_str: &str,
    ) -> Result<executor::ResultSet> {
        // Use the query AST directly if it has clauses, otherwise parse the string
        let operators = if !query.clauses.is_empty() {
            // Use the planner directly with the AST
            let mut planner = executor::planner::QueryPlanner::new(
                &self.catalog,
                &self.indexes.label_index,
                &self.indexes.knn_index,
            )
            .with_rtree(self.indexes.rtree.clone());
            planner.plan_query(query)?
        } else {
            // Fallback: parse and plan from string
            self.executor.parse_and_plan(query_str)?
        };

        // Format plan as JSON for return
        let plan_json = serde_json::json!({
            "plan": {
                "operators": operators.iter().map(|op| {
                    serde_json::json!({
                        "type": format!("{:?}", op),
                        "description": format!("{:?}", op)
                    })
                }).collect::<Vec<_>>()
            },
            "estimated_cost": "N/A", // Would need cost estimation
            "estimated_rows": "N/A"  // Would need row estimation
        });

        Ok(executor::ResultSet::new(
            vec!["plan".to_string()],
            vec![executor::Row {
                values: vec![plan_json],
            }],
        ))
    }

    /// Execute PROFILE command - executes query and returns execution statistics
    pub(super) fn execute_profile_with_string(
        &mut self,
        query: &executor::parser::CypherQuery,
        query_str: &str,
    ) -> Result<executor::ResultSet> {
        use std::time::Instant;

        let start_time = Instant::now();

        // Use the query AST directly if it has clauses, otherwise parse the string
        let operators = if !query.clauses.is_empty() {
            // Use the planner directly with the AST
            let mut planner = executor::planner::QueryPlanner::new(
                &self.catalog,
                &self.indexes.label_index,
                &self.indexes.knn_index,
            )
            .with_rtree(self.indexes.rtree.clone());
            planner.plan_query(query)?
        } else {
            // Fallback: parse and plan from string
            self.executor.parse_and_plan(query_str)?
        };

        // Execute the query
        let result = self.execute_cypher_internal(query_str)?;

        let execution_time = start_time.elapsed();

        // Format profile as JSON
        let profile_json = serde_json::json!({
            "plan": {
                "operators": operators.iter().map(|op| {
                    serde_json::json!({
                        "type": format!("{:?}", op),
                        "description": format!("{:?}", op)
                    })
                }).collect::<Vec<_>>()
            },
            "execution_time_ms": execution_time.as_millis(),
            "execution_time_us": execution_time.as_micros(),
            "rows_returned": result.rows.len(),
            "columns_returned": result.columns.len()
        });

        Ok(executor::ResultSet::new(
            vec!["profile".to_string()],
            vec![executor::Row {
                values: vec![profile_json],
            }],
        ))
    }

    /// Convert CypherQuery AST to string representation
    pub(super) fn query_to_string(&self, query: &executor::parser::CypherQuery) -> String {
        // Simple conversion - in production would need proper formatting
        // For now, reconstruct from clauses
        let mut parts = Vec::new();
        for clause in &query.clauses {
            parts.push(format!("{:?}", clause));
        }
        parts.join(" ")
    }

    /// Internal method to execute Cypher query (used by PROFILE)
    pub(super) fn execute_cypher_internal(&mut self, query: &str) -> Result<executor::ResultSet> {
        // Re-parse and execute (avoiding infinite recursion with EXPLAIN/PROFILE)
        let mut parser = executor::parser::CypherParser::new(query.to_string());
        let ast = parser.parse()?;

        // Execute normally (but skip EXPLAIN/PROFILE checks)
        self.execute_cypher_ast(&ast)
    }

    /// Execute Cypher AST (internal, used to avoid EXPLAIN/PROFILE recursion)
    pub(super) fn execute_cypher_ast(
        &mut self,
        ast: &executor::parser::CypherQuery,
    ) -> Result<executor::ResultSet> {
        // Check for administrative commands that need special handling
        let has_admin_db_cmd = ast.clauses.iter().any(|c| {
            matches!(
                c,
                executor::parser::Clause::CreateDatabase(_)
                    | executor::parser::Clause::DropDatabase(_)
                    | executor::parser::Clause::ShowDatabases
                    | executor::parser::Clause::UseDatabase(_)
            )
        });

        if has_admin_db_cmd {
            return Err(Error::CypherExecution(
                "Database management commands (CREATE/DROP DATABASE, SHOW DATABASES, USE DATABASE) must be executed at server level".to_string(),
            ));
        }

        // Check for transaction commands
        let has_begin = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::BeginTransaction));
        let has_commit = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::CommitTransaction));
        let has_rollback = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::RollbackTransaction));
        let has_savepoint_cmd = ast.clauses.iter().any(|c| {
            matches!(
                c,
                executor::parser::Clause::Savepoint(_)
                    | executor::parser::Clause::RollbackToSavepoint(_)
                    | executor::parser::Clause::ReleaseSavepoint(_)
            )
        });

        if has_begin || has_commit || has_rollback || has_savepoint_cmd {
            return self.execute_transaction_commands(ast, None);
        }

        // Check for index management commands
        let has_create_index = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::CreateIndex(_)));
        let has_drop_index = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::DropIndex(_)));

        if has_create_index || has_drop_index {
            return self.execute_index_commands(ast);
        }

        // Check for constraint management commands
        let has_create_constraint = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::CreateConstraint(_)));
        let has_drop_constraint = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::DropConstraint(_)));

        if has_create_constraint || has_drop_constraint {
            return self.execute_constraint_commands(ast);
        }

        // Check for function management commands
        let has_show_functions = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::ShowFunctions));
        let has_create_function = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::CreateFunction(_)));
        let has_drop_function = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::DropFunction(_)));

        if has_show_functions || has_create_function || has_drop_function {
            return self.execute_function_commands(ast);
        }

        // Check for LOAD CSV commands
        let has_load_csv = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::LoadCsv(_)));

        if has_load_csv {
            return self.execute_load_csv_commands(ast);
        }

        // Check for CALL subquery commands
        let has_call_subquery = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::CallSubquery(_)));

        if has_call_subquery {
            return self.execute_call_subquery_commands(ast);
        }

        // Check for user management commands (should be handled at server level)
        let has_user_cmd = ast.clauses.iter().any(|c| {
            matches!(
                c,
                executor::parser::Clause::ShowUsers
                    | executor::parser::Clause::CreateUser(_)
                    | executor::parser::Clause::Grant(_)
                    | executor::parser::Clause::Revoke(_)
            )
        });

        if has_user_cmd {
            return Err(Error::CypherExecution(
                "User management commands (SHOW USERS, CREATE USER, GRANT, REVOKE) must be executed at server level".to_string(),
            ));
        }

        // Check if query contains CREATE or DELETE
        let has_create = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::Create(_)));
        let has_delete = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::Delete(_)));
        let has_merge = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::Merge(_)));
        let has_set_clause = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::Set(_)));
        let has_remove_clause = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::Remove(_)));
        let has_foreach = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::Foreach(_)));
        let has_match = ast
            .clauses
            .iter()
            .any(|c| matches!(c, executor::parser::Clause::Match(_)));
        // phase6 §8 — CREATE-bound variables satisfy DELETE's context
        // requirement too, matching openCypher semantics.
        let has_create_bound_vars = ast.clauses.iter().any(|c| {
            if let executor::parser::Clause::Create(cc) = c {
                cc.pattern.elements.iter().any(|el| {
                    if let executor::parser::PatternElement::Node(node) = el {
                        node.variable.is_some()
                    } else {
                        false
                    }
                })
            } else {
                false
            }
        });

        // Handle DELETE (with or without MATCH)
        if has_delete {
            let deleted_count = if has_match || has_create_bound_vars {
                // MATCH ... DELETE or CREATE ... DELETE: execute the
                // upstream pattern first, then DELETE with results.
                self.execute_match_delete_query(ast)?
            } else {
                return Err(Error::CypherSyntax(
                    "DELETE requires an upstream MATCH, CREATE, or WITH".to_string(),
                ));
            };
            self.refresh_executor()?;

            // Check if there's a RETURN clause after DELETE
            let return_clause_opt = ast.clauses.iter().find_map(|c| {
                if let executor::parser::Clause::Return(rc) = c {
                    Some(rc)
                } else {
                    None
                }
            });

            if let Some(return_clause) = return_clause_opt {
                // Check if RETURN contains count aggregation
                let mut is_count_only = false;
                let mut count_alias = "count".to_string();

                if return_clause.items.len() == 1 {
                    let executor::parser::ReturnItem { expression, alias } =
                        &return_clause.items[0];
                    if let executor::parser::Expression::FunctionCall { name, args: _ } = expression
                    {
                        if name.to_lowercase() == "count" {
                            is_count_only = true;
                            count_alias = alias.clone().unwrap_or_else(|| "count".to_string());
                        }
                    }
                }

                if is_count_only {
                    // Return count of deleted nodes
                    return Ok(executor::ResultSet::new(
                        vec![count_alias],
                        vec![executor::Row {
                            values: vec![serde_json::Value::Number(deleted_count.into())],
                        }],
                    ));
                } else {
                    // If there's a RETURN clause with other expressions, let the executor handle it
                    // The executor will process the RETURN, but since nodes are deleted,
                    // it will likely return empty results or handle it appropriately
                    let query_obj = executor::Query {
                        cypher: self.query_to_string(ast),
                        params: ast.params.clone(),
                    };
                    return self.executor.execute(&query_obj);
                }
            } else {
                // No RETURN clause - return count of deleted nodes
                return Ok(executor::ResultSet::new(
                    vec!["count".to_string()],
                    vec![executor::Row {
                        values: vec![serde_json::Value::Number(deleted_count.into())],
                    }],
                ));
            }
        }

        // Handle MERGE / SET / REMOVE / FOREACH write queries before falling back to read executor
        if has_merge || has_set_clause || has_remove_clause || has_foreach {
            let result = self.execute_write_query(ast)?;
            return Ok(result);
        }

        // If query has CREATE (with or without MATCH), handle via Engine for persistence
        if has_create {
            if has_match {
                // MATCH ... CREATE: execute MATCH first, then CREATE with results
                let result = self.execute_match_create_query(ast, None)?;

                // CRITICAL: Sync executor's store back to engine's storage
                self.storage = self.executor.get_store();

                // Refresh executor to see the changes
                self.refresh_executor()?;

                return Ok(result);
            } else {
                // Standalone CREATE
                self.execute_create_query(ast)?;
            }

            // Refresh executor to see the changes
            self.refresh_executor()?;
        }

        // Execute the query normally
        let query_obj = executor::Query {
            cypher: self.query_to_string(ast),
            params: std::collections::HashMap::new(),
        };
        self.executor.execute(&query_obj)
    }
}
