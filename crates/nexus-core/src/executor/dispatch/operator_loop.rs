//! `Executor::execute_inner`: the per-operator dispatch loop that walks
//! the physical plan produced by [`super::planning`] and executes each
//! [`Operator`] variant against the shared [`ExecutionContext`].
//!
//! This is the single largest function in the executor — every physical
//! operator lands in exactly one match arm here — so it stays whole in
//! its own file rather than being split further.

use super::super::*;
use crate::query_cache::IntelligentQueryCache;
use crate::{Error, Result};
use serde_json::Value;
use tracing;

impl Executor {
    /// Inner execute body — see [`Self::execute`] for the wrapper that
    /// manages the planner notification sink. Marked `pub(in super::super)` so
    /// downstream sub-query operators (`call_subquery`) that want to
    /// avoid double-attaching notifications can dispatch through here
    /// directly; main callers should always go through [`Self::execute`].
    #[tracing::instrument(skip_all, level = "debug", fields(cypher = %query.cypher))]
    pub(in super::super) fn execute_inner(&self, query: &Query) -> Result<ResultSet> {
        // Increment query counter for lazy cache warming
        let current_count = self
            .query_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        // Extract `/*+ ... */` plan hints before parsing — the cleaned
        // query is what the main parser sees so the hint syntax stays
        // invisible to the rest of the Cypher front-end.
        let (cleaned_cypher, plan_hints) = planner::extract_plan_hints(&query.cypher);

        // Cluster-mode handoff: if the engine installed a pre-parsed
        // AST for this call, consume it (one-shot, so we cannot leak
        // the override into an unrelated subsequent query) and plan
        // from that AST. Otherwise parse the cypher string the
        // classic way. Without this override path, the executor
        // would re-parse `Query.cypher` and silently discard the
        // upstream label / type rewrite that produced tenant
        // isolation in the first place.
        let preparsed = self.shared.preparsed_ast_override.lock().take();
        let operators = match preparsed {
            Some(ast) => self.plan_ast(&ast)?,
            None => self.parse_and_plan_with_params(&cleaned_cypher, &query.params)?,
        };

        // TODO: JIT and Parallel execution - implement after core optimizations
        // For now, focus on proven optimizations: columnar, SIMD, caching

        // Check if this is a write query - don't cache write operations
        let is_write_query = operators.iter().any(|op| {
            matches!(
                op,
                Operator::Create { .. } | Operator::Delete { .. } | Operator::DetachDelete { .. }
            )
        });

        // Check query cache for read operations
        /*
        if !is_write_query {
            if let Some(ref cache) = self.shared.query_cache {
                let query_hash =
                    IntelligentQueryCache::generate_query_hash(&query.cypher, &query.params);
                tracing::trace!(
                    "Checking query cache for: {} (hash: {})",
                    &query.cypher,
                    query_hash
                );
                if let Some(cached_result) = cache.read().get(query_hash) {
                    // Cache hit - return cached result
                    tracing::trace!(
                        "Query cache HIT for query: {} (hash: {})",
                        &query.cypher,
                        query_hash
                    );
                    return Ok(cached_result.as_ref().clone());
                } else {
                    tracing::trace!(
                        "Query cache MISS for query: {} (hash: {})",
                        &query.cypher,
                        query_hash
                    );
                }
            } else {
                tracing::trace!("Query cache not available for query: {}", &query.cypher);
            }
        }
        */

        // Lazy cache warming after observing query patterns. Non-fatal —
        // if warming fails (e.g. transient store contention) we log a
        // warning and bump `nexus_executor_serde_fallback_total{site="warm_cache_lazy"}`
        // rather than silently dropping the error, so ops can alarm on
        // a sustained warming failure rate.
        if let Some(ref cache) = self.shared.cache {
            if let Err(e) = cache.write().warm_cache_lazy(current_count) {
                serde_metrics::record_fallback(serde_metrics::SerdeFallbackSite::WarmCacheLazy);
                tracing::warn!(
                    target: "nexus_core::executor",
                    error = %e,
                    "warm_cache_lazy failed; cache warming was skipped for query #{}",
                    current_count
                );
            }
        }

        // Columnar storage framework ready - will be activated in next phase

        // Try direct execution for simple queries (bypass operator overhead)
        if !is_write_query && self.is_simple_match_query(&query.cypher) {
            if let Ok(result) = self.execute_simple_match_directly(query) {
                tracing::trace!("Direct execution optimization used");
                return Ok(result);
            }
        }

        // Short-circuit: `MATCH (a:L1), (b:L2), ... RETURN count(*)` and
        // its `count(var)` variants don't need to materialise the full
        // cross-product. The answer is the product of each label's
        // cardinality, already tracked in the catalog's per-label
        // `node_counts` histogram. Without this, the cartesian-product
        // assembly in `apply_cartesian_product` clones every source
        // node's JSON payload N×M times before `Aggregate` discards
        // every value it just copied — 327× slower than Neo4j on the
        // bench's `traversal.cartesian_a_b` scenario.
        if !is_write_query {
            if let Some(count_rs) = self.try_short_circuit_count_cross_product(&operators)? {
                return Ok(count_rs);
            }
        }

        // Execute the plan using traditional operator-based execution
        tracing::trace!(
            "Starting query execution, creating new ExecutionContext for query: {}",
            query.cypher
        );
        let mut context = ExecutionContext::new(query.params.clone(), self.shared.cache.clone());
        context.set_plan_hints(plan_hints);
        tracing::trace!(
            "New ExecutionContext created: variables.len()={}, result_set.rows.len()={}",
            context.variables.len(),
            context.result_set.rows.len()
        );
        let mut results = Vec::new();
        let mut projection_columns: Vec<String> = Vec::new();
        // Set once a `Limit`/`Skip` has fixed the row count. The final
        // assembly below falls back to `results` when the result set is
        // empty, on the assumption that an empty set means "no operator
        // produced rows" — but `LIMIT 0` (or a `SKIP` past the end) empties
        // it *deliberately*, and without this flag that fallback resurrects
        // the pre-limit rows and the limit appears to be ignored entirely.
        let mut row_count_is_final = false;

        // Check if first operator is CREATE standalone (no MATCH before)
        // If so, execute it directly and populate result_set
        if let Some(Operator::Create {
            pattern,
            external_id_expr,
            conflict_policy,
        }) = operators.first()
        {
            let resolved_external_id = if let Some(expr) = external_id_expr.as_ref() {
                Some(self.resolve_external_id(expr, &context.params)?)
            } else {
                None
            };
            let policy = operators::create::ast_conflict_policy_to_storage(*conflict_policy);
            let existing_rows = self.materialize_rows_from_variables(&context)?;
            if existing_rows.is_empty() {
                // CREATE standalone - create nodes and relationships directly.
                //
                // A statement may carry SEVERAL consecutive CREATE clauses
                // (`CREATE (a) CREATE (b) CREATE (a)-[:R]->(b)` — standard
                // openCypher, distinct from the comma form, which arrives as
                // one pattern). The planner emits one Create operator per
                // clause; `created_node_ids` / `created_rel_ids` are threaded
                // through every clause below via `execute_create_pattern_internal`
                // (rather than each clause calling the
                // `execute_create_pattern_with_variables` wrapper, which
                // always starts from a fresh, clause-local map) so a bare
                // variable reference in a LATER clause — `(a)` with no
                // labels/properties, the only shape that reaches here once
                // `a` is already bound (re-declaring a bound variable with
                // structure is rejected earlier, at semantic validation) —
                // resolves to the node an EARLIER clause created instead of
                // minting an unbound duplicate.
                // `execute_create_pattern_internal`'s own node/relationship-target
                // arms already prefer an existing `created_node_ids` entry
                // over creating a fresh node; sharing the accumulator across
                // clauses is what makes that reuse span clause boundaries.
                let mut created_node_ids = std::collections::HashMap::new();
                let mut created_rel_ids = std::collections::HashMap::new();
                self.execute_create_pattern_internal(
                    pattern,
                    &mut created_node_ids,
                    &mut created_rel_ids,
                    resolved_external_id,
                    policy,
                    &context.params,
                )?;

                for op in operators.iter().skip(1) {
                    if let Operator::Create {
                        pattern: extra_pattern,
                        external_id_expr: extra_ext_expr,
                        conflict_policy: extra_policy,
                    } = op
                    {
                        let extra_ext_id = if let Some(expr) = extra_ext_expr.as_ref() {
                            Some(self.resolve_external_id(expr, &context.params)?)
                        } else {
                            None
                        };
                        let extra_policy =
                            operators::create::ast_conflict_policy_to_storage(*extra_policy);
                        self.execute_create_pattern_internal(
                            extra_pattern,
                            &mut created_node_ids,
                            &mut created_rel_ids,
                            extra_ext_id,
                            extra_policy,
                            &context.params,
                        )?;
                    }
                }

                // Collect all created entities (nodes and relationships)
                let mut columns: Vec<String> = created_node_ids.keys().cloned().collect();
                let mut rel_columns: Vec<String> = created_rel_ids.keys().cloned().collect();
                columns.append(&mut rel_columns);

                // A write-only statement (no `RETURN`/`WITH` downstream of
                // this CREATE) must yield an EMPTY result set per
                // openCypher — TCK `Create2[2]`/[3]/[5]-[12] all assert
                // "the result should be empty" for
                // `CREATE (a) CREATE (b) CREATE (a)-[:R]->(b)`-shaped
                // statements with bound variables and correct side effects.
                // Only synthesize `context.result_set` from the created
                // entities when a downstream row consumer actually exists:
                // `Project` (a plain RETURN), `With`, or `Aggregate` (an
                // aggregating RETURN like `RETURN count(*)` — the planner
                // only emits `Project` when there is a non-empty
                // `projection_items` list; a pure-aggregate RETURN plans as
                // a bare `Operator::Aggregate` with no `Project` at all).
                // `Project`/`With` fall back to `materialize_rows_from_variables`
                // (fed by the `context.set_variable` calls below) when
                // `result_set.rows` is empty, so skipping the synthesis
                // does not change what a RETURN/WITH-bearing statement
                // returns; it only stops the phantom row from leaking out
                // when nothing downstream consumes it.
                let has_downstream_row_consumer = operators[1..].iter().any(|op| {
                    matches!(
                        op,
                        Operator::Project { .. }
                            | Operator::With { .. }
                            | Operator::Aggregate { .. }
                    )
                });

                // Create a single row with all created entities
                if !columns.is_empty() {
                    let mut row_values = Vec::new();
                    for col in &columns {
                        if let Some(node_id) = created_node_ids.get(col) {
                            // It's a node
                            if let Ok(node_value) = self.read_node_as_value(*node_id) {
                                row_values.push(node_value.clone());
                                // Store in context variable
                                context.set_variable(col, node_value);
                            } else {
                                row_values.push(Value::Null);
                            }
                        } else if let Some(rel_info) = created_rel_ids.get(col) {
                            // It's a relationship
                            if let Ok(rel_value) = self.read_relationship_as_value(rel_info) {
                                row_values.push(rel_value.clone());
                                // Store in context variable
                                context.set_variable(col, rel_value);
                            } else {
                                row_values.push(Value::Null);
                            }
                        } else {
                            row_values.push(Value::Null);
                        }
                    }

                    if has_downstream_row_consumer && !row_values.is_empty() {
                        context.result_set.columns = columns;
                        context.result_set.rows = vec![Row { values: row_values }];
                    }
                }

                // Skip CREATE operator in loop since we already executed it
                // Continue with remaining operators (if any)
                for (_idx, operator) in operators.iter().enumerate().skip(1) {
                    match operator {
                        Operator::Project { items } => {
                            projection_columns =
                                items.iter().map(|item| item.alias.clone()).collect();
                            results = self.execute_project(&mut context, items)?;
                        }
                        Operator::With {
                            items,
                            distinct,
                            where_predicate,
                        } => {
                            self.execute_with(
                                &mut context,
                                items,
                                *distinct,
                                where_predicate.as_deref(),
                            )?;
                        }
                        Operator::Aggregate {
                            group_by,
                            aggregations,
                            projection_items,
                            output_order,
                            source: _,
                            streaming_optimized: _,
                            push_down_optimized: _,
                        } => {
                            // A pure-aggregate RETURN (`RETURN count(*)`)
                            // plans as a bare Aggregate with no Project —
                            // without this arm the `_` catch-all below
                            // silently dropped it, leaving the CREATE's
                            // own phantom row (or, pre-gating-fix, an
                            // empty result) instead of the actual count.
                            self.execute_aggregate_with_projections(
                                &mut context,
                                group_by,
                                aggregations,
                                projection_items.as_deref(),
                                output_order.as_deref(),
                            )?;
                        }
                        Operator::Limit { count } => {
                            self.execute_limit(&mut context, *count)?;
                            row_count_is_final = true;
                        }
                        Operator::Sort { columns, ascending } => {
                            self.execute_sort(&mut context, columns, ascending)?;
                        }
                        Operator::LoadCsv {
                            url,
                            variable,
                            with_headers,
                            field_terminator,
                        } => {
                            self.execute_load_csv(
                                &mut context,
                                url,
                                variable,
                                *with_headers,
                                field_terminator.as_deref(),
                            )?;
                        }
                        _ => {
                            // Other operators after CREATE standalone
                        }
                    }
                }

                // Return early with populated result_set
                let final_columns = if !context.result_set.columns.is_empty() {
                    context.result_set.columns.clone()
                } else if !projection_columns.is_empty() {
                    projection_columns
                } else {
                    vec![]
                };

                let final_rows = if !context.result_set.rows.is_empty() {
                    context.result_set.rows.clone()
                } else if !results.is_empty() {
                    results
                } else {
                    vec![]
                };

                return Ok(ResultSet::new(final_columns, final_rows));
            }
        }

        // Vectorized execution framework ready - will be activated in next phase

        // If a pipeline mixes Project and Aggregate, ensure Aggregate runs before Project.
        // We detect presence of Aggregate upfront and, if present, we will skip executing
        // Project operators until after the aggregation step. This preserves intermediate
        // row variables (e.g., relationship variable `r`) needed by aggregations like COUNT(r).
        // However, post-aggregation projections (like head(collect())) should be executed.
        let mut aggregate_executed = false;

        for (op_idx, operator) in operators.iter().enumerate() {
            // DEBUG: Print each operator as it executes
            let op_name = match operator {
                Operator::NodeByLabel { variable, .. } => format!("NodeByLabel({})", variable),
                Operator::Filter { predicate, .. } => {
                    format!("Filter({})", predicate.chars().take(40).collect::<String>())
                }
                Operator::OptionalFilter {
                    predicate,
                    optional_vars,
                    ..
                } => {
                    format!(
                        "OptionalFilter({}, vars={:?})",
                        predicate.chars().take(30).collect::<String>(),
                        optional_vars
                    )
                }
                Operator::Create { .. } => "Create".to_string(),
                Operator::Project { items } => format!("Project({} items)", items.len()),
                Operator::With {
                    items,
                    distinct,
                    where_predicate,
                } => {
                    format!(
                        "With({} items, distinct={}, has_where={})",
                        items.len(),
                        distinct,
                        where_predicate.is_some()
                    )
                }
                _ => format!("{:?}", std::mem::discriminant(operator)),
            };
            tracing::trace!("EXECUTING OPERATOR #{}: {}", op_idx, op_name);
            // Check if there's still an Aggregate operator ahead in the pipeline
            let has_aggregate_ahead = operators[op_idx + 1..]
                .iter()
                .any(|op| matches!(op, Operator::Aggregate { .. }));
            match operator {
                Operator::NodeByLabel { label_id, variable } => {
                    let nodes = self.execute_node_by_label(*label_id)?;
                    self.seed_scan_main_loop(&mut context, variable, nodes)?;
                }
                Operator::NodeIndexSeek {
                    label_id,
                    key_id,
                    value,
                    key_expression,
                    variable,
                } => {
                    if let Some(expr) = key_expression {
                        // Correlated seek: the key is row-local (e.g. `r.s`
                        // from `UNWIND $rows AS r MATCH (a:P {id: r.s})`).
                        // Seek per driving row instead of scanning the
                        // label and cross-joining — see
                        // `phase0_fix-correlated-predicate-index-seek` §3.
                        self.execute_correlated_index_seek(
                            &mut context,
                            *label_id,
                            *key_id,
                            expr,
                            variable,
                        )?;
                    } else {
                        // Read-side index seek: only nodes whose indexed
                        // property equals `value` seed the scan
                        // (O(matches)), instead of a full label scan.
                        // Residual `Filter` operators still run for full
                        // correctness. The downstream seeding (cartesian
                        // product / UNWIND cross-product / materialize) is
                        // shared with NodeByLabel via `seed_scan_main_loop`.
                        let nodes = self.execute_node_index_seek(*label_id, *key_id, value)?;
                        self.seed_scan_main_loop(&mut context, variable, nodes)?;
                    }
                }
                Operator::NodeIndexRangeSeek {
                    label_id,
                    key_id,
                    op,
                    value,
                    variable,
                } => {
                    // Read-side range seek on a single-property B-tree index;
                    // residual Filter operators still run for full correctness.
                    let nodes =
                        self.execute_node_index_range_seek(*label_id, *key_id, *op, value)?;
                    self.seed_scan_main_loop(&mut context, variable, nodes)?;
                }
                Operator::NodeIndexInSeek {
                    label_id,
                    key_id,
                    values,
                    variable,
                } => {
                    // Read-side `IN`-list seek: a union of point lookups on the
                    // single-property B-tree index, seeded exactly like the
                    // other scans.
                    let nodes = self.execute_node_index_in_seek(*label_id, *key_id, values)?;
                    self.seed_scan_main_loop(&mut context, variable, nodes)?;
                }
                Operator::NodeIndexPrefixSeek {
                    label_id,
                    key_id,
                    prefix,
                    variable,
                } => {
                    // Read-side `STARTS WITH` seek: the contiguous prefix run
                    // of the single-property B-tree index.
                    let nodes = self.execute_node_index_prefix_seek(*label_id, *key_id, prefix)?;
                    self.seed_scan_main_loop(&mut context, variable, nodes)?;
                }
                Operator::NodeIndexParamSeek {
                    label_id,
                    key_id,
                    parameter,
                    variable,
                } => {
                    // Read-side `$parameter` equality seek: the key comes from
                    // the query envelope, so no driving rows are required.
                    let nodes = self
                        .execute_node_index_param_seek(&context, *label_id, *key_id, parameter)?;
                    self.seed_scan_main_loop(&mut context, variable, nodes)?;
                }
                Operator::AllNodesScan { variable } => {
                    let nodes = self.execute_all_nodes_scan()?;
                    context.variables.remove(variable);

                    // CRITICAL FIX: Apply Cartesian product if there are existing variables
                    if !context.variables.is_empty() {
                        self.apply_cartesian_product(&mut context, variable, nodes)?;
                    } else {
                        context.set_variable(variable, Value::Array(nodes));
                    }
                    let rows = self.materialize_rows_from_variables(&context)?;
                    self.update_result_set_from_rows(&mut context, &rows);
                }
                Operator::Filter {
                    predicate,
                    predicate_ast,
                } => {
                    self.execute_filter(&mut context, predicate, predicate_ast.as_deref())?;
                }
                Operator::OptionalFilter {
                    predicate,
                    predicate_ast,
                    optional_vars,
                } => {
                    self.execute_optional_filter(
                        &mut context,
                        predicate,
                        predicate_ast.as_deref(),
                        optional_vars,
                    )?;
                }
                Operator::Expand {
                    type_ids,
                    direction,
                    source_var,
                    target_var,
                    rel_var,
                    optional,
                    target_labels,
                    iso_scope,
                } => {
                    // Advanced JOIN algorithms framework ready - using traditional expand for now
                    self.execute_expand(
                        &mut context,
                        type_ids,
                        *direction,
                        source_var,
                        target_var,
                        rel_var,
                        *optional,
                        target_labels,
                        *iso_scope,
                        None, // Cache not available at this level
                    )?;
                }
                Operator::Project { items } => {
                    projection_columns = items.iter().map(|item| item.alias.clone()).collect();
                    // Check if Project contains collect argument items (__collect_arg_*)
                    // If so, we must NOT defer - these need to be evaluated before Aggregate
                    let has_collect_args = items
                        .iter()
                        .any(|item| item.alias.starts_with("__collect_arg_"));
                    if has_aggregate_ahead && !has_collect_args {
                        // Defer Project until after Aggregate to keep source columns (e.g., `r`) available.
                        // Aggregation operator will produce the correct final columns/rows.
                        tracing::trace!(
                            "Deferring Project ({} items) because Aggregate exists later in pipeline",
                            items.len()
                        );
                    } else {
                        // Execute Project - either no Aggregate in pipeline, or this is post-aggregation projection
                        tracing::trace!(
                            "Executing Project ({} items), aggregate_executed={}",
                            items.len(),
                            aggregate_executed
                        );
                        results = self.execute_project(&mut context, items)?;
                    }
                }
                Operator::With {
                    items,
                    distinct,
                    where_predicate,
                } => {
                    self.execute_with(&mut context, items, *distinct, where_predicate.as_deref())?;
                }
                Operator::Limit { count } => {
                    self.execute_limit(&mut context, *count)?;
                    row_count_is_final = true;
                }
                Operator::Skip { count } => {
                    self.execute_skip(&mut context, *count)?;
                    row_count_is_final = true;
                }
                Operator::Sort { columns, ascending } => {
                    self.execute_sort(&mut context, columns, ascending)?;
                }
                Operator::Aggregate {
                    group_by,
                    aggregations,
                    projection_items,
                    output_order,
                    source: _,
                    streaming_optimized: _,
                    push_down_optimized: _,
                } => {
                    // Use projection items from the operator itself
                    self.execute_aggregate_with_projections(
                        &mut context,
                        group_by,
                        aggregations,
                        projection_items.as_deref(),
                        output_order.as_deref(),
                    )?;
                    aggregate_executed = true;
                }
                Operator::Union {
                    left,
                    right,
                    distinct,
                } => {
                    self.execute_union(&mut context, left, right, *distinct)?;
                }
                Operator::Create {
                    pattern,
                    external_id_expr,
                    conflict_policy,
                } => {
                    let resolved_external_id = if let Some(expr) = external_id_expr.as_ref() {
                        Some(self.resolve_external_id(expr, &context.params)?)
                    } else {
                        None
                    };
                    let policy =
                        operators::create::ast_conflict_policy_to_storage(*conflict_policy);
                    // The standalone-CREATE fast path above already executed
                    // the plan's FIRST operator when it is a Create — but only
                    // that one. This guard used to test `operators.first()`
                    // for ANY Create in the loop, which skipped every
                    // subsequent Create operator too: `CREATE (a) CREATE (b)`
                    // silently created only `a` (parity harness case 02c).
                    // Skip strictly the operator the fast path consumed.
                    if op_idx == 0 {
                        continue;
                    }

                    // Check if there are existing rows from MATCH
                    // CRITICAL FIX: For MATCH...CREATE, we need to preserve variables even after Filter
                    // because CREATE needs the matched nodes. If result_set.rows is empty (e.g., after RETURN count(*)),
                    // we must use context.variables which should still contain the matched nodes.
                    tracing::trace!(
                        "CREATE operator: checking for existing rows. result_set.rows={}, variables={:?}",
                        context.result_set.rows.len(),
                        context.variables.keys().collect::<Vec<_>>()
                    );

                    let existing_rows = if !context.result_set.rows.is_empty() {
                        // Convert result_set.rows to HashMap format
                        let columns = context.result_set.columns.clone();
                        let rows: Vec<_> = context
                            .result_set
                            .rows
                            .iter()
                            .map(|row| self.row_to_map(row, &columns))
                            .collect();

                        tracing::trace!(
                            "CREATE operator: converted {} rows from result_set.rows, columns={:?}",
                            rows.len(),
                            columns
                        );

                        // Check if rows contain node variables (not just aggregation results)
                        let has_node_variables = rows
                            .iter()
                            .any(|row| row.values().any(|v| crate::executor::is_node_value(v)));

                        tracing::trace!(
                            "CREATE operator: has_node_variables={}",
                            has_node_variables
                        );

                        if has_node_variables {
                            rows
                        } else if context.variables.is_empty() {
                            // No node variables on the rows AND no
                            // pre-existing bindings in context — the rows
                            // came from UNWIND / WITH / a plain
                            // projection, so every row is a distinct
                            // iteration of CREATE. Pass them through;
                            // `execute_create_with_context` walks per row
                            // and resolves property expressions (like
                            // `{id: id}` referencing the UNWIND variable)
                            // against the row bindings.
                            tracing::trace!(
                                "CREATE operator: using {} scalar rows from result_set (UNWIND / WITH projection)",
                                rows.len()
                            );
                            rows
                        } else {
                            // result_set.rows only contains aggregation
                            // or projection scalars; context.variables is
                            // the real binding source (e.g. MATCH (n)
                            // RETURN count(*) CREATE ...).
                            tracing::trace!(
                                "CREATE operator: result_set.rows has no node variables, materializing from variables"
                            );
                            self.materialize_rows_from_variables(&context)?
                        }
                    } else {
                        // No rows in result_set - materialize from variables
                        tracing::trace!(
                            "CREATE operator: result_set.rows is empty, materializing from variables"
                        );
                        let materialized = self.materialize_rows_from_variables(&context)?;
                        tracing::trace!(
                            "CREATE operator: materialized {} rows from variables",
                            materialized.len()
                        );
                        materialized
                    };

                    if existing_rows.is_empty() {
                        // CRITICAL FIX: Don't execute CREATE standalone when Filter removed all rows
                        // This happens when Filter incorrectly evaluates predicates and removes valid rows
                        // Instead, skip CREATE to avoid creating wrong relationships
                        tracing::warn!(
                            "CREATE operator: existing_rows is empty, skipping CREATE. result_set.rows={}, variables={:?}",
                            context.result_set.rows.len(),
                            context.variables.keys().collect::<Vec<_>>()
                        );
                        continue;
                    }

                    tracing::trace!(
                        "CREATE operator: found {} existing rows from MATCH, proceeding with CREATE",
                        existing_rows.len()
                    );

                    // A write-only `MATCH ... CREATE` (no `RETURN`/`WITH`
                    // after this Create in the plan) must return an EMPTY
                    // result set — only synthesize one when a downstream
                    // Project, With, or Aggregate operator exists to
                    // consume it (a pure-aggregate RETURN like `RETURN
                    // count(*)` plans as a bare `Operator::Aggregate`, no
                    // `Project`; see the standalone-CREATE fast path's
                    // matching comment above). Safe in both directions
                    // here regardless of which way the gate falls: unlike
                    // the fast path, this main loop actually executes the
                    // following `Operator::Aggregate` normally, and
                    // `execute_aggregate_with_projections`
                    // (`operators/aggregate/core.rs`) falls back to
                    // `materialize_rows_from_variables` off
                    // `context.variables` whenever `result_set.columns`
                    // doesn't look like a MATCH projection — so it
                    // overwrites whatever this CREATE left in
                    // `result_set` either way.
                    let has_downstream_row_consumer = operators[op_idx + 1..].iter().any(|op| {
                        matches!(
                            op,
                            Operator::Project { .. }
                                | Operator::With { .. }
                                | Operator::Aggregate { .. }
                        )
                    });

                    // CREATE with MATCH context - use existing implementation
                    self.execute_create_with_context(
                        &mut context,
                        pattern,
                        resolved_external_id,
                        policy,
                        has_downstream_row_consumer,
                    )?;

                    // If a RETURN/WITH follows, Project/With will handle
                    // (and overwrite) the result_set; otherwise it stays
                    // empty per the gating above.
                }
                Operator::Delete { variables } => {
                    self.execute_delete(&mut context, variables, false)?;
                }
                Operator::DetachDelete { variables } => {
                    self.execute_delete(&mut context, variables, true)?;
                }
                Operator::Join {
                    left,
                    right,
                    join_type,
                    condition,
                } => {
                    self.execute_join(&mut context, left, right, *join_type, condition.as_deref())?;
                }
                Operator::IndexScan { index_name, label } => {
                    self.execute_index_scan_new(&mut context, index_name, label)?;
                }
                Operator::Distinct { columns } => {
                    self.execute_distinct(&mut context, columns)?;
                }
                Operator::Unwind {
                    expression,
                    variable,
                } => {
                    self.execute_unwind(&mut context, expression, variable)?;
                }
                Operator::VariableLengthPath {
                    type_ids,
                    direction,
                    source_var,
                    target_var,
                    rel_var,
                    path_var,
                    quantifier,
                } => {
                    self.execute_variable_length_path(
                        &mut context,
                        type_ids,
                        *direction,
                        source_var,
                        target_var,
                        rel_var,
                        path_var,
                        quantifier,
                    )?;
                }
                Operator::QuantifiedExpand {
                    source_var,
                    target_var,
                    hops,
                    inner_nodes,
                    inner_where,
                    min_length,
                    max_length,
                    optional,
                    mode,
                } => {
                    self.execute_quantified_expand(
                        &mut context,
                        source_var,
                        target_var,
                        hops,
                        inner_nodes,
                        inner_where.as_ref(),
                        *min_length,
                        *max_length,
                        *optional,
                        *mode,
                    )?;
                }
                Operator::CallProcedure {
                    procedure_name,
                    arguments,
                    yield_columns,
                } => {
                    self.execute_call_procedure(
                        &mut context,
                        procedure_name,
                        arguments,
                        yield_columns.as_ref(),
                    )?;
                }
                Operator::LoadCsv {
                    url,
                    variable,
                    with_headers,
                    field_terminator,
                } => {
                    self.execute_load_csv(
                        &mut context,
                        url,
                        variable,
                        *with_headers,
                        field_terminator.as_deref(),
                    )?;
                }
                Operator::CreateIndex {
                    label,
                    property,
                    index_type,
                    if_not_exists,
                    or_replace,
                } => {
                    self.execute_create_index(
                        label,
                        property,
                        index_type.as_deref(),
                        *if_not_exists,
                        *or_replace,
                    )?;
                    // Return empty result set for CREATE INDEX
                    context.result_set = ResultSet::new(
                        vec!["index".to_string()],
                        vec![Row {
                            values: vec![Value::String(format!(
                                "{}.{}.{}",
                                label,
                                property,
                                index_type.as_deref().unwrap_or("property")
                            ))],
                        }],
                    );
                }
                Operator::ShowDatabases => {
                    context.result_set = self.execute_show_databases()?;
                }
                Operator::CreateDatabase {
                    name,
                    if_not_exists,
                } => {
                    context.result_set = self.execute_create_database(name, *if_not_exists)?;
                }
                Operator::DropDatabase { name, if_exists } => {
                    context.result_set = self.execute_drop_database(name, *if_exists)?;
                }
                Operator::AlterDatabase {
                    name,
                    read_only,
                    option,
                } => {
                    context.result_set =
                        self.execute_alter_database(name, *read_only, option.clone())?;
                }
                Operator::UseDatabase { name } => {
                    context.result_set = self.execute_use_database(name)?;
                }
                &Operator::HashJoin { .. } => {
                    return Err(Error::Internal(
                        "HashJoin operator not implemented".to_string(),
                    ));
                }
                Operator::CompositeBtreeSeek {
                    label,
                    variable,
                    prefix,
                } => {
                    self.execute_composite_btree_seek(&mut context, label, variable, prefix)?;
                }
                Operator::CallSubquery {
                    inner_query,
                    in_transactions,
                    batch_size,
                    concurrency,
                    on_error,
                    status_var,
                    import_list,
                } => {
                    self.execute_call_subquery(
                        &mut context,
                        inner_query,
                        *in_transactions,
                        *batch_size,
                        *concurrency,
                        on_error,
                        status_var.as_deref(),
                        import_list.as_deref(),
                    )?;
                }
                Operator::SpatialSeek {
                    index_id,
                    variable,
                    mode,
                } => {
                    self.execute_spatial_seek(&mut context, index_id, variable, mode)?;
                }
                Operator::EnsureNullRowIfEmpty { vars } => {
                    // phase8_optional-match-empty-driver: if the
                    // pipeline produced zero rows for a top-level
                    // OPTIONAL MATCH, emit one row with the
                    // listed vars bound to NULL so downstream
                    // Project / OptionalFilter / aggregations see
                    // the Neo4j-shape "OPTIONAL = always at least
                    // one row" contract.
                    if context.result_set.rows.is_empty() && results.is_empty() {
                        for v in vars {
                            context.set_variable(v, serde_json::Value::Null);
                        }
                        context
                            .result_set
                            .rows
                            .push(crate::executor::types::Row { values: Vec::new() });
                        tracing::debug!(
                            "EnsureNullRowIfEmpty (mod): emitted NULL fallback row for vars {:?}",
                            vars
                        );
                    }
                }
            }
        }

        let final_columns = if !context.result_set.columns.is_empty() {
            context.result_set.columns.clone()
        } else if !projection_columns.is_empty() {
            projection_columns
        } else {
            vec![]
        };

        let final_rows = if !context.result_set.rows.is_empty() {
            context.result_set.rows.clone()
        } else if row_count_is_final {
            // A `Limit`/`Skip` emptied the result set on purpose — that empty
            // set is the answer, not evidence that nothing ran.
            vec![]
        } else if !results.is_empty() {
            results
        } else {
            vec![]
        };

        let result_set = ResultSet::new(final_columns, final_rows);

        // Cache the result for read operations
        if !is_write_query {
            if let Some(ref cache) = self.shared.query_cache {
                // Calculate execution time for cache TTL calculation
                let execution_time_ms = 10; // TODO: Measure actual execution time

                let cache_result = cache.write().put(
                    &query.cypher,
                    &query.params,
                    result_set.clone(),
                    execution_time_ms,
                );

                match cache_result {
                    Ok(_) => tracing::trace!(
                        "Query cached successfully: {} (hash: {})",
                        &query.cypher,
                        IntelligentQueryCache::generate_query_hash(&query.cypher, &query.params)
                    ),
                    Err(e) => tracing::warn!("Failed to cache query: {}", e),
                }
            }
        }

        Ok(result_set)
    }
}
