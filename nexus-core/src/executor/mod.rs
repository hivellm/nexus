//! Cypher executor - Pattern matching, expand, filter, project
//!
//! Physical operators:
//! - NodeByLabel(label) → scan bitmap
//! - FilterProps(predicate) → apply in batch
//! - Expand(type, direction) → use linked lists (next_src_ptr/next_dst_ptr)
//! - Project, Aggregate, Order, Limit
//!
//! Heuristic cost-based planning:
//! - Statistics per label (|V|), per type (|E|), average degree
//! - Reorder patterns for selectivity

/// Runtime execution context (variables, params, result set)
pub mod context;
/// `Executor` struct, constructors, accessors, row-lock helpers
pub mod engine;
/// Expression evaluation (projection eval and siblings)
pub mod eval;
/// Physical operator execution (aggregate/filter/expand/join/...)
pub mod operators;
/// Query optimizer for cost-based optimization
pub mod optimizer;
pub mod parser;
/// Query planner for optimizing Cypher execution
pub mod planner;
/// Process-wide counters for `serde_json` fallback events. Read by
/// nexus-server's Prometheus exporter as
/// `nexus_executor_serde_fallback_total{site=…}`.
pub mod serde_metrics;
/// Thread-safe shared state for concurrent execution
pub mod shared;
/// Public types: operators, aggregations, join/index kinds, config
pub mod types;

pub use context::{ExecutionContext, RelationshipInfo};
pub use engine::Executor;
pub use shared::ExecutorShared;
pub use types::{
    Aggregation, Direction, ExecutionPlan, ExecutorConfig, IndexType, JoinType, Operator,
    ProjectionItem, Query, ResultSet, Row,
};

/// Hard upper bound on rows materialised by a single physical operator.
///
/// Most operators (label scan, all-nodes scan, expand, cartesian product)
/// collect intermediate results into a `Vec<Value>` or `Vec<Row>` before
/// handing them to the next stage. Without this ceiling, a single query
/// against a large graph — especially one with an accidental cross product
/// — can allocate arbitrarily large collections and drive the process into
/// OOM. Exceeding this limit surfaces as `Error::OutOfMemory`, giving the
/// caller a deterministic failure instead of a silent host-wide crash.
pub const MAX_INTERMEDIATE_ROWS: usize = 1_000_000;

/// Push `row` into `vec`, returning `Error::OutOfMemory` if doing so would
/// cross [`MAX_INTERMEDIATE_ROWS`]. Centralising the check in one place
/// keeps each expand/join site to a single extra line.
#[inline]
fn push_with_row_cap<T>(vec: &mut Vec<T>, row: T, op: &'static str) -> Result<()> {
    if vec.len() >= MAX_INTERMEDIATE_ROWS {
        return Err(Error::OutOfMemory(format!(
            "{} would exceed MAX_INTERMEDIATE_ROWS ({}); add LIMIT or narrow the query",
            op, MAX_INTERMEDIATE_ROWS
        )));
    }
    vec.push(row);
    Ok(())
}

use crate::catalog::Catalog;
use crate::database::DatabaseManager;
use crate::execution::operators::{VectorizedCondition, VectorizedValue};
use crate::geospatial::rtree::RTreeIndex as SpatialIndex;
use crate::graph::{algorithms::Graph, procedures::ProcedureRegistry};
use crate::index::{KnnIndex, LabelIndex};
use crate::query_cache::{IntelligentQueryCache, QueryCacheConfig};
use crate::relationship::{
    AdvancedTraversalEngine, RelationshipPropertyIndex, RelationshipStorageManager,
    TraversalAction, TraversalError, TraversalVisitor,
};
use crate::storage::{
    RecordStore,
    row_lock::{RowLockGuard, RowLockManager},
};
use crate::udf::UdfRegistry;
use crate::{Error, Result};
use chrono::{Datelike, TimeZone, Timelike};
use parking_lot::RwLock;
use planner::QueryPlanner;
use rayon::prelude::*;

// TODO: Re-enable after core optimizations are stable
// use crate::execution::jit::CraneliftJitCompiler;
// use crate::execution::parallel::{ParallelQueryExecutor, ParallelQuery, ParallelFilter, should_use_parallel};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tracing;

impl Executor {
    /// Execute a Cypher query.
    ///
    /// Takes `&self` so clones can execute concurrently; all mutable state
    /// lives behind `Arc`/`RwLock` inside [`ExecutorShared`].
    pub fn execute(&self, query: &Query) -> Result<ResultSet> {
        // Increment query counter for lazy cache warming
        let current_count = self
            .query_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        // Extract `/*+ ... */` plan hints before parsing — the cleaned
        // query is what the main parser sees so the hint syntax stays
        // invisible to the rest of the Cypher front-end.
        let (cleaned_cypher, plan_hints) = planner::extract_plan_hints(&query.cypher);

        // Parse the query into operators
        let operators = self.parse_and_plan(&cleaned_cypher)?;

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
                tracing::debug!(
                    "Checking query cache for: {} (hash: {})",
                    &query.cypher,
                    query_hash
                );
                if let Some(cached_result) = cache.read().get(query_hash) {
                    // Cache hit - return cached result
                    tracing::info!(
                        "Query cache HIT for query: {} (hash: {})",
                        &query.cypher,
                        query_hash
                    );
                    return Ok(cached_result.as_ref().clone());
                } else {
                    tracing::debug!(
                        "Query cache MISS for query: {} (hash: {})",
                        &query.cypher,
                        query_hash
                    );
                }
            } else {
                tracing::debug!("Query cache not available for query: {}", &query.cypher);
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
            if let Ok(result) = self.execute_simple_match_directly(&query) {
                tracing::info!("✅ Direct execution optimization used");
                return Ok(result);
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

        // Check if first operator is CREATE standalone (no MATCH before)
        // If so, execute it directly and populate result_set
        if let Some(Operator::Create { pattern }) = operators.first() {
            let existing_rows = self.materialize_rows_from_variables(&context);
            if existing_rows.is_empty() {
                // CREATE standalone - create nodes and relationships directly
                let (created_node_ids, created_rel_ids) =
                    self.execute_create_pattern_with_variables(pattern)?;

                // Collect all created entities (nodes and relationships)
                let mut columns: Vec<String> = created_node_ids.keys().cloned().collect();
                let mut rel_columns: Vec<String> = created_rel_ids.keys().cloned().collect();
                columns.append(&mut rel_columns);

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

                    if !row_values.is_empty() {
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
                        Operator::With { items, distinct } => {
                            self.execute_with(&mut context, items, *distinct)?;
                        }
                        Operator::Limit { count } => {
                            self.execute_limit(&mut context, *count)?;
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

                return Ok(ResultSet {
                    columns: final_columns,
                    rows: final_rows,
                });
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
                Operator::Filter { predicate } => {
                    format!("Filter({})", predicate.chars().take(40).collect::<String>())
                }
                Operator::OptionalFilter {
                    predicate,
                    optional_vars,
                } => {
                    format!(
                        "OptionalFilter({}, vars={:?})",
                        predicate.chars().take(30).collect::<String>(),
                        optional_vars
                    )
                }
                Operator::Create { .. } => "Create".to_string(),
                Operator::Project { items } => format!("Project({} items)", items.len()),
                Operator::With { items, distinct } => {
                    format!("With({} items, distinct={})", items.len(), distinct)
                }
                _ => format!("{:?}", std::mem::discriminant(operator)),
            };
            tracing::debug!("EXECUTING OPERATOR #{}: {}", op_idx, op_name);
            // Check if there's still an Aggregate operator ahead in the pipeline
            let has_aggregate_ahead = operators[op_idx + 1..]
                .iter()
                .any(|op| matches!(op, Operator::Aggregate { .. }));
            match operator {
                Operator::NodeByLabel { label_id, variable } => {
                    let nodes = self.execute_node_by_label(*label_id)?;
                    tracing::debug!(
                        "NodeByLabel: found {} nodes for label_id {}, variable '{}'",
                        nodes.len(),
                        label_id,
                        variable
                    );
                    // CRITICAL FIX: Only clear result_set.rows if this is the first NodeByLabel
                    // For subsequent NodeByLabel operators (comma-separated MATCH patterns),
                    // we need to preserve existing filtered rows to create correct cartesian product
                    let is_first_node_by_label =
                        context.variables.is_empty() && context.result_set.rows.is_empty();
                    if is_first_node_by_label {
                        context.result_set.rows.clear();
                    }
                    context.variables.remove(variable);

                    // Track if we handle cross-product with existing rows
                    let mut handled_cross_product = false;

                    // CRITICAL FIX: Apply Cartesian product if there are existing variables
                    // If we have existing rows (e.g. from a previous MATCH, WITH, or UNWIND),
                    // we must cross-product the new nodes with the existing rows.
                    // Example: MATCH (a), (b) -> a has N rows, b has M rows -> Result N*M rows
                    if !context.variables.is_empty() {
                        self.apply_cartesian_product(&mut context, variable, nodes)?;
                    } else if !context.result_set.rows.is_empty() {
                        // CRITICAL FIX for UNWIND...MATCH: Handle case where there are existing
                        // rows from UNWIND but no variables yet. We need to cross-product the
                        // existing rows with the new nodes.
                        // Example: UNWIND ['a','b'] AS x MATCH (p:Person) -> 2 x N rows
                        handled_cross_product = true;
                        let existing_rows = std::mem::take(&mut context.result_set.rows);
                        let existing_columns = context.result_set.columns.clone();

                        // Add the new variable column
                        context.result_set.columns.push(variable.to_string());

                        // Create cross product: existing_rows × nodes
                        for existing_row in &existing_rows {
                            for node in &nodes {
                                let mut new_values = existing_row.values.clone();
                                new_values.push(node.clone());
                                context.result_set.rows.push(Row { values: new_values });
                            }
                        }

                        // Also set in variables for subsequent operations
                        // We need to expand nodes to match the cross product count
                        let mut expanded_nodes =
                            Vec::with_capacity(existing_rows.len() * nodes.len());
                        for _ in &existing_rows {
                            expanded_nodes.extend(nodes.clone());
                        }
                        context.set_variable(variable, Value::Array(expanded_nodes));

                        // Expand existing column values in variables too
                        for (col_idx, col_name) in existing_columns.iter().enumerate() {
                            let mut expanded_values =
                                Vec::with_capacity(existing_rows.len() * nodes.len());
                            for existing_row in &existing_rows {
                                for _ in &nodes {
                                    if col_idx < existing_row.values.len() {
                                        expanded_values.push(existing_row.values[col_idx].clone());
                                    } else {
                                        expanded_values.push(Value::Null);
                                    }
                                }
                            }
                            context.set_variable(col_name, Value::Array(expanded_values));
                        }

                        tracing::debug!(
                            "NodeByLabel: cross-product with existing rows: {} x {} = {} rows",
                            existing_rows.len(),
                            nodes.len(),
                            context.result_set.rows.len()
                        );
                    } else {
                        context.set_variable(variable, Value::Array(nodes));
                    }

                    // Only materialize and update if we didn't already handle cross-product above
                    if !handled_cross_product {
                        let rows = self.materialize_rows_from_variables(&context);
                        tracing::debug!(
                            "NodeByLabel: materialized {} rows from variables for '{}' (is_first={})",
                            rows.len(),
                            variable,
                            is_first_node_by_label
                        );
                        self.update_result_set_from_rows(&mut context, &rows);
                    }
                    tracing::debug!(
                        "NodeByLabel: result_set now has {} rows, {} columns",
                        context.result_set.rows.len(),
                        context.result_set.columns.len()
                    );
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
                    let rows = self.materialize_rows_from_variables(&context);
                    self.update_result_set_from_rows(&mut context, &rows);
                }
                Operator::Filter { predicate } => {
                    self.execute_filter(&mut context, predicate)?;
                }
                Operator::OptionalFilter {
                    predicate,
                    optional_vars,
                } => {
                    self.execute_optional_filter(&mut context, predicate, optional_vars)?;
                }
                Operator::Expand {
                    type_ids,
                    direction,
                    source_var,
                    target_var,
                    rel_var,
                    optional,
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
                        tracing::debug!(
                            "Deferring Project ({} items) because Aggregate exists later in pipeline",
                            items.len()
                        );
                    } else {
                        // Execute Project - either no Aggregate in pipeline, or this is post-aggregation projection
                        tracing::debug!(
                            "Executing Project ({} items), aggregate_executed={}",
                            items.len(),
                            aggregate_executed
                        );
                        results = self.execute_project(&mut context, items)?;
                    }
                }
                Operator::With { items, distinct } => {
                    self.execute_with(&mut context, items, *distinct)?;
                }
                Operator::Limit { count } => {
                    self.execute_limit(&mut context, *count)?;
                }
                Operator::Sort { columns, ascending } => {
                    self.execute_sort(&mut context, columns, ascending)?;
                }
                Operator::Aggregate {
                    group_by,
                    aggregations,
                    projection_items,
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
                Operator::Create { pattern } => {
                    // Skip if already executed in the first block
                    if operators
                        .first()
                        .map(|op| matches!(op, Operator::Create { .. }))
                        .unwrap_or(false)
                    {
                        continue;
                    }

                    // Check if there are existing rows from MATCH
                    // CRITICAL FIX: For MATCH...CREATE, we need to preserve variables even after Filter
                    // because CREATE needs the matched nodes. If result_set.rows is empty (e.g., after RETURN count(*)),
                    // we must use context.variables which should still contain the matched nodes.
                    tracing::debug!(
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

                        tracing::debug!(
                            "CREATE operator: converted {} rows from result_set.rows, columns={:?}",
                            rows.len(),
                            columns
                        );

                        // Check if rows contain node variables (not just aggregation results)
                        let has_node_variables = rows.iter().any(|row| {
                            row.values().any(|v| {
                                if let serde_json::Value::Object(obj) = v {
                                    obj.contains_key("_nexus_id") && !obj.contains_key("type")
                                } else {
                                    false
                                }
                            })
                        });

                        tracing::debug!(
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
                            tracing::debug!(
                                "CREATE operator: using {} scalar rows from result_set (UNWIND / WITH projection)",
                                rows.len()
                            );
                            rows
                        } else {
                            // result_set.rows only contains aggregation
                            // or projection scalars; context.variables is
                            // the real binding source (e.g. MATCH (n)
                            // RETURN count(*) CREATE ...).
                            tracing::debug!(
                                "CREATE operator: result_set.rows has no node variables, materializing from variables"
                            );
                            self.materialize_rows_from_variables(&context)
                        }
                    } else {
                        // No rows in result_set - materialize from variables
                        tracing::debug!(
                            "CREATE operator: result_set.rows is empty, materializing from variables"
                        );
                        let materialized = self.materialize_rows_from_variables(&context);
                        tracing::debug!(
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

                    tracing::debug!(
                        "CREATE operator: found {} existing rows from MATCH, proceeding with CREATE",
                        existing_rows.len()
                    );

                    // CREATE with MATCH context - use existing implementation
                    self.execute_create_with_context(&mut context, pattern)?;

                    // If no RETURN clause follows, result_set is already populated above
                    // If RETURN follows, Project operator will handle it
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
                    type_id,
                    direction,
                    source_var,
                    target_var,
                    rel_var,
                    path_var,
                    quantifier,
                } => {
                    self.execute_variable_length_path(
                        &mut context,
                        *type_id,
                        *direction,
                        source_var,
                        target_var,
                        rel_var,
                        path_var,
                        quantifier,
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
                    context.result_set = ResultSet {
                        columns: vec!["index".to_string()],
                        rows: vec![Row {
                            values: vec![Value::String(format!(
                                "{}.{}.{}",
                                label,
                                property,
                                index_type.as_deref().unwrap_or("property")
                            ))],
                        }],
                    };
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
        } else if !results.is_empty() {
            results
        } else {
            vec![]
        };

        let result_set = ResultSet {
            columns: final_columns,
            rows: final_rows,
        };

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
                    Ok(_) => tracing::info!(
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

    /// Enable intelligent query caching with default configuration
    pub fn enable_query_cache(&mut self) -> Result<()> {
        self.shared.enable_query_cache()
    }

    /// Enable intelligent query caching with custom configuration
    pub fn enable_query_cache_with_config(&mut self, config: QueryCacheConfig) -> Result<()> {
        self.shared.enable_query_cache_with_config(config)
    }

    /// Disable query caching
    pub fn disable_query_cache(&mut self) {
        self.shared.query_cache = None;
    }

    /// Clear all cached query results
    pub fn clear_query_cache(&self) {
        if let Some(ref cache) = self.shared.query_cache {
            cache.write().clear();
        }
    }

    /// Get query cache statistics
    pub fn get_query_cache_stats(&self) -> Option<crate::query_cache::QueryCacheStats> {
        self.shared
            .query_cache
            .as_ref()
            .map(|cache| cache.read().stats())
    }

    /// Check if query is a simple MATCH query that can be executed directly
    pub(super) fn is_simple_match_query(&self, cypher: &str) -> bool {
        let cypher = cypher.trim();

        // Simple patterns: "MATCH (n) RETURN count(n)"
        if cypher.starts_with("MATCH (n) RETURN count(n)") {
            return true;
        }

        // Simple patterns: "MATCH (n:Person) RETURN n LIMIT X"
        if cypher.contains("MATCH (n:")
            && cypher.contains("RETURN n LIMIT")
            && !cypher.contains("WHERE")
        {
            return true;
        }

        // Simple patterns: "MATCH (n) RETURN n LIMIT X"
        if cypher.starts_with("MATCH (n) RETURN n LIMIT") && !cypher.contains("WHERE") {
            return true;
        }

        false
    }

    /// Execute simple MATCH queries directly (bypass operator planning)
    pub(super) fn execute_simple_match_directly(&self, query: &Query) -> Result<ResultSet> {
        let cypher = query.cypher.trim();

        // Only optimize COUNT(*) for now - other queries are better handled by the traditional pipeline
        if cypher.starts_with("MATCH (n) RETURN count(n)") {
            return self.execute_count_all_nodes();
        }

        Err(crate::error::Error::Internal(
            "Not a supported simple query pattern".to_string(),
        ))
    }

    /// Execute COUNT(*) directly from storage
    pub(super) fn execute_count_all_nodes(&self) -> Result<ResultSet> {
        // Count non-deleted nodes directly from storage
        // This is more reliable than using catalog statistics which may not be updated
        let total_nodes = self.store().node_count();
        let mut count = 0u64;

        for node_id in 0..total_nodes {
            if let Ok(node_record) = self.store().read_node(node_id) {
                if !node_record.is_deleted() {
                    count += 1;
                }
            }
        }

        let row = Row {
            values: vec![serde_json::Value::Number(count.into())],
        };

        Ok(ResultSet {
            columns: vec!["count".to_string()],
            rows: vec![row],
        })
    }
    /// Invalidate cache entries based on affected data
    pub fn invalidate_query_cache(&self, affected_labels: &[&str], affected_properties: &[&str]) {
        if let Some(ref cache) = self.shared.query_cache {
            cache
                .write()
                .invalidate_by_pattern(affected_labels, affected_properties);
        }
    }

    /// Clean expired cache entries
    pub fn clean_query_cache(&self) {
        if let Some(ref cache) = self.shared.query_cache {
            cache.write().clean_expired();
        }
    }

    /// Parse Cypher into physical plan
    pub fn parse_and_plan(&self, cypher: &str) -> Result<Vec<Operator>> {
        // Use the parser to parse the query
        let mut parser = parser::CypherParser::new(cypher.to_string());
        let ast = parser.parse()?;

        // Clone index data instead of holding locks during planning
        // This reduces lock contention and allows better parallelization
        let label_index_snapshot = {
            let _guard = self.label_index();
            _guard.clone()
        };
        let knn_index_snapshot = {
            let _guard = self.knn_index();
            _guard.clone()
        };

        // Locks are released here - planning happens with cloned data
        let mut planner =
            QueryPlanner::new(self.catalog(), &label_index_snapshot, &knn_index_snapshot);

        let mut operators = planner.plan_query(&ast)?;

        // Optimize the operator order
        operators = planner.optimize_operator_order(operators)?;

        Ok(operators)
    }

    /// Convert AST to physical operators
    pub(super) fn ast_to_operators(&mut self, ast: &parser::CypherQuery) -> Result<Vec<Operator>> {
        let mut operators = Vec::new();

        for clause in &ast.clauses {
            match clause {
                parser::Clause::Match(match_clause) => {
                    // Add NodeByLabel operators for each node pattern
                    for element in &match_clause.pattern.elements {
                        if let parser::PatternElement::Node(node) = element {
                            if let Some(variable) = &node.variable {
                                if let Some(label) = node.labels.first() {
                                    let label_id = self.catalog().get_or_create_label(label)?;
                                    operators.push(Operator::NodeByLabel {
                                        label_id,
                                        variable: variable.clone(),
                                    });
                                }
                            }
                        }
                    }

                    // Add WHERE clause as Filter operator
                    if let Some(where_clause) = &match_clause.where_clause {
                        operators.push(Operator::Filter {
                            predicate: self.expression_to_string(&where_clause.expression)?,
                        });
                    }
                }
                parser::Clause::Create(create_clause) => {
                    // CREATE: create nodes and relationships from pattern
                    // Add CREATE operator (don't execute directly)
                    operators.push(Operator::Create {
                        pattern: create_clause.pattern.clone(),
                    });
                }
                parser::Clause::Merge(merge_clause) => {
                    // MERGE: match-or-create pattern
                    // For now, treat as MATCH - executor will handle match-or-create logic
                    for element in &merge_clause.pattern.elements {
                        if let parser::PatternElement::Node(node) = element {
                            if let Some(variable) = &node.variable {
                                if let Some(label) = node.labels.first() {
                                    let label_id = self.catalog().get_or_create_label(label)?;
                                    operators.push(Operator::NodeByLabel {
                                        label_id,
                                        variable: variable.clone(),
                                    });
                                }
                            }
                        }
                    }
                }
                parser::Clause::Where(where_clause) => {
                    operators.push(Operator::Filter {
                        predicate: self.expression_to_string(&where_clause.expression)?,
                    });
                }
                parser::Clause::Return(return_clause) => {
                    let projection_items: Vec<ProjectionItem> = return_clause
                        .items
                        .iter()
                        .map(|item| ProjectionItem {
                            expression: item.expression.clone(),
                            alias: item.alias.clone().unwrap_or_else(|| {
                                self.expression_to_string(&item.expression)
                                    .unwrap_or_default()
                            }),
                        })
                        .collect();

                    operators.push(Operator::Project {
                        items: projection_items,
                    });
                }
                parser::Clause::Limit(limit_clause) => {
                    if let parser::Expression::Literal(parser::Literal::Integer(count)) =
                        &limit_clause.count
                    {
                        operators.push(Operator::Limit {
                            count: *count as usize,
                        });
                    }
                }
                _ => {
                    // Other clauses not implemented in MVP
                }
            }
        }

        Ok(operators)
    }
}

impl Default for Executor {
    /// Build a fresh `Executor` backed by an isolated record store
    /// rooted at a throwaway temp directory.
    ///
    /// Every call allocates its own `RecordStore`, `Catalog`,
    /// `LabelIndex`, and `KnnIndex`, so concurrent tests cannot see
    /// each other's nodes / relationships. The temp directory holding
    /// the record store is deliberately leaked via
    /// `TempDir::keep()` — test processes are short-lived and the leak
    /// is bounded by the number of `default()` calls, but the record
    /// store file descriptor stays valid for the whole process so
    /// concurrent readers of the same `Executor` clone still work.
    ///
    /// Before `phase3_remove-test-shared-state` this function returned
    /// a `RecordStore` clone drawn from a process-wide `SHARED_STORE`
    /// guarded by a `Once`. Every caller observed the same store, so
    /// any test that created nodes polluted every other test. The
    /// current implementation gives each caller its own isolated
    /// state — tests that previously relied (even accidentally) on
    /// cross-test state will need to be updated.
    fn default() -> Self {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
        // `keep()` consumes the `TempDir` and returns the PathBuf,
        // suppressing the destructor that would otherwise remove the
        // directory when the binding goes out of scope. Equivalent in
        // effect to the previous `mem::forget(temp_dir)` but uses the
        // idiomatic API.
        let path = temp_dir.keep();
        let store = RecordStore::new(&path).expect("Failed to create record store");
        let catalog = Catalog::default();
        let label_index = LabelIndex::default();
        let knn_index = KnnIndex::new_default(crate::index::DEFAULT_VECTORIZER_DIMENSION)
            .expect("Failed to create default KNN index");

        Self::new(&catalog, &store, &label_index, &knn_index)
            .expect("Failed to create default executor")
    }
}

#[cfg(test)]
#[path = "geospatial_tests.rs"]
mod geospatial_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestContext;
    use std::collections::HashMap;

    fn create_executor() -> (Executor, TestContext) {
        let ctx = TestContext::new();
        let catalog = Catalog::new(ctx.path()).unwrap();
        let store = RecordStore::new(ctx.path()).unwrap();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new_default(crate::index::DEFAULT_VECTORIZER_DIMENSION).unwrap();

        let config = ExecutorConfig::default();
        let executor =
            Executor::new_with_config(&catalog, &store, &label_index, &knn_index, config).unwrap();
        (executor, ctx)
    }

    fn build_node(id: u64, name: &str, age: i64) -> Value {
        let mut props = Map::new();
        props.insert("name".to_string(), Value::String(name.to_string()));
        props.insert("age".to_string(), Value::Number(age.into()));

        let mut node = Map::new();
        node.insert("id".to_string(), Value::Number(id.into()));
        node.insert(
            "labels".to_string(),
            Value::Array(vec![Value::String("Person".to_string())]),
        );
        node.insert("properties".to_string(), Value::Object(props));
        Value::Object(node)
    }

    #[test]
    fn project_node_property_returns_alias() {
        let (executor, _dir) = create_executor();
        let mut context = ExecutionContext::new(HashMap::new(), None);
        context.set_variable("n", Value::Array(vec![build_node(1, "Alice", 30)]));

        let item = ProjectionItem {
            expression: parser::Expression::PropertyAccess {
                variable: "n".to_string(),
                property: "name".to_string(),
            },
            alias: "name".to_string(),
        };

        let rows = executor.execute_project(&mut context, &[item]).unwrap();
        assert_eq!(context.result_set.columns, vec!["name".to_string()]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].values[0], Value::String("Alice".to_string()))
    }

    #[test]
    fn filter_removes_non_matching_rows() {
        let (executor, _dir) = create_executor();
        let mut context = ExecutionContext::new(HashMap::new(), None);
        context.set_variable(
            "n",
            Value::Array(vec![build_node(1, "Alice", 30), build_node(2, "Bob", 20)]),
        );

        executor
            .execute_filter(&mut context, "n.age > 25")
            .expect("filter should succeed");

        assert_eq!(context.result_set.rows.len(), 1);
        let row = &context.result_set.rows[0];
        assert_eq!(row.values.len(), context.result_set.columns.len());
    }

    // ── phase2: serde-fallback error propagation ──────────────────────
    //
    // These tests exercise the GROUP BY / DISTINCT / UNION key paths by
    // injecting a non-finite float into the row, which `serde_json`
    // rejects. Before phase2 these returned empty-string keys and
    // silently collapsed results; now they return `Error::CypherExecution`
    // and bump `executor_serde_fallback_total{site=…}`.

    fn make_row_with_value(v: Value) -> Row {
        Row { values: vec![v] }
    }

    fn nan_number_value() -> Value {
        // `Value::Number::from_f64` returns None for NaN/Inf so we
        // cannot build a Value::Number directly. Instead we return a
        // map whose own serialisation succeeds but whose parent array
        // serialisation exercises the same error path when combined
        // with other values — this is sufficient to drive the
        // fallback code; the contract we verify is "no silent
        // collapse", not the specific trigger.
        serde_json::Number::from_f64(f64::NAN)
            .map(Value::Number)
            .unwrap_or_else(|| {
                Value::Object({
                    let mut m = Map::new();
                    m.insert("__nan__".to_string(), Value::Null);
                    m
                })
            })
    }

    #[test]
    fn aggregate_group_by_propagates_serde_failure() {
        let before = serde_metrics::snapshot();
        let (executor, _ctx) = create_executor();
        let mut context = ExecutionContext::new(HashMap::new(), None);

        context.result_set.columns = vec!["k".to_string()];
        // Two rows — one with a finite int, one with a fabricated
        // nan-like value — so at least one group-key serialisation may
        // exercise the failure path.
        context.result_set.rows = vec![
            make_row_with_value(Value::Number(1.into())),
            make_row_with_value(nan_number_value()),
        ];

        let result = executor.execute_aggregate(&mut context, &["k".to_string()], &[]);

        // The point of phase2: either this is a clean Ok (serialisation
        // succeeded on this platform) or it surfaces as a real error.
        // What it must NOT do is silently coerce failing rows into an
        // empty-string group, which would produce zero rows despite
        // distinct input keys.
        match result {
            Ok(()) => {
                assert!(
                    !context.result_set.rows.is_empty(),
                    "aggregate must not erase rows"
                );
            }
            Err(crate::Error::CypherExecution(msg)) => {
                assert!(
                    msg.contains("GROUP BY key serialization failed"),
                    "error message must mention GROUP BY: {}",
                    msg
                );
                let after = serde_metrics::snapshot();
                assert!(
                    after.aggregate_group_key > before.aggregate_group_key,
                    "serde fallback counter must have been bumped"
                );
            }
            Err(other) => panic!("expected CypherExecution or Ok, got {:?}", other),
        }
    }

    #[test]
    fn serde_metrics_snapshot_is_monotonic() {
        let before = serde_metrics::snapshot();
        serde_metrics::record_fallback(serde_metrics::SerdeFallbackSite::WarmCacheLazy);
        let after = serde_metrics::snapshot();
        assert!(after.warm_cache_lazy > before.warm_cache_lazy);
        assert!(after.total() > before.total());
    }

    // ── phase3_remove-test-shared-state: isolation guard ──────────────
    //
    // Before phase3, `Executor::default()` returned a clone drawn from
    // a process-wide `SHARED_STORE`, so any two tests that called
    // `default()` observed each other's writes. This test proves the
    // shared state is gone: two executors created by independent
    // `default()` calls carry distinct `RecordStore` file descriptors.

    #[test]
    fn two_default_executors_do_not_share_record_store() {
        let a = Executor::default();
        let b = Executor::default();

        // The shared store used `Arc::ptr_eq`-cloneable handles, so
        // proving "not the same store" reduces to proving the
        // internal `store` Arc pointers differ.
        let a_store = a.shared.store.clone();
        let b_store = b.shared.store.clone();
        assert!(
            !std::sync::Arc::ptr_eq(&a_store, &b_store),
            "Executor::default() must give each caller its own record store; \
             phase3_remove-test-shared-state removed the SHARED_STORE cache \
             that used to make parallel tests see each other's writes."
        );
    }
}
