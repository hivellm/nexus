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

        // Parse the query into operators
        let operators = self.parse_and_plan(&query.cypher)?;

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

        // Lazy cache warming after observing query patterns
        if let Some(ref cache) = self.shared.cache {
            let _ = cache.write().warm_cache_lazy(current_count);
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
                        } else {
                            // result_set.rows only contains aggregation results, use context.variables
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

    /// Execute CREATE pattern to create nodes and relationships
    /// Returns map of variable names to created node IDs
    pub(super) fn execute_create_pattern_with_variables(
        &self,
        pattern: &parser::Pattern,
    ) -> Result<(
        std::collections::HashMap<String, u64>,
        std::collections::HashMap<String, RelationshipInfo>,
    )> {
        let mut created_nodes: std::collections::HashMap<String, u64> =
            std::collections::HashMap::new();
        let mut created_relationships: std::collections::HashMap<String, RelationshipInfo> =
            std::collections::HashMap::new();

        // Call the original implementation
        self.execute_create_pattern_internal(
            pattern,
            &mut created_nodes,
            &mut created_relationships,
        )?;

        Ok((created_nodes, created_relationships))
    }

    /// Internal implementation of CREATE pattern execution
    pub(super) fn execute_create_pattern_internal(
        &self,
        pattern: &parser::Pattern,
        created_nodes: &mut std::collections::HashMap<String, u64>,
        created_relationships: &mut std::collections::HashMap<String, RelationshipInfo>,
    ) -> Result<()> {
        // PERFORMANCE OPTIMIZATION: Reuse shared transaction manager
        let mut tx_mgr = self.transaction_manager().lock();
        let mut tx = tx_mgr.begin_write()?;

        // Phase 1 Optimization: Cache label lookups and batch catalog updates
        let mut label_cache: std::collections::HashMap<String, u32> =
            std::collections::HashMap::new();
        let mut label_count_updates: std::collections::HashMap<u32, u32> =
            std::collections::HashMap::new();

        // Phase 1.5.2: Pre-allocate label/type IDs in batches
        // Collect all unique labels and types from the pattern first
        let mut all_labels = std::collections::HashSet::new();
        let mut all_types = std::collections::HashSet::new();

        for element in &pattern.elements {
            match element {
                parser::PatternElement::Node(node) => {
                    for label in &node.labels {
                        all_labels.insert(label.as_str());
                    }
                }
                parser::PatternElement::Relationship(rel) => {
                    for rel_type in &rel.types {
                        all_types.insert(rel_type.as_str());
                    }
                }
            }
        }

        // Batch allocate all labels in a single transaction
        if !all_labels.is_empty() {
            let labels_vec: Vec<&str> = all_labels.iter().copied().collect();
            let batch_results = self.catalog().batch_get_or_create_labels(&labels_vec)?;
            label_cache.extend(batch_results);
        }

        // Batch allocate all types in a single transaction
        if !all_types.is_empty() {
            let types_vec: Vec<&str> = all_types.iter().copied().collect();
            let batch_results = self.catalog().batch_get_or_create_types(&types_vec)?;
            label_cache.extend(batch_results); // Reuse label_cache for types too
        }

        // Use the passed-in created_nodes HashMap (don't create a new one)
        let mut last_node_id: Option<u64> = None;
        let mut skip_next_node = false; // Flag to skip node already created in relationship

        // Process pattern elements in sequence
        // Pattern alternates: Node -> Relationship -> Node -> Relationship ...
        for (i, element) in pattern.elements.iter().enumerate() {
            match element {
                parser::PatternElement::Node(node) => {
                    // Skip if this node was already created as part of the previous relationship
                    if skip_next_node {
                        skip_next_node = false;
                        continue;
                    }

                    // Phase 1.5.2: Build label bitmap with pre-allocated IDs
                    // All labels should already be in label_cache from batch allocation
                    let mut label_bits = 0u64;
                    let mut label_ids_for_update = Vec::new();
                    for label in &node.labels {
                        // Labels should already be in cache from batch allocation
                        // Fallback to individual lookup if not found (shouldn't happen, but be safe)
                        let label_id = if let Some(&id) = label_cache.get(label) {
                            id
                        } else {
                            // Fallback: individual lookup (shouldn't happen with batch allocation)
                            let id = self.catalog().get_or_create_label(label)?;
                            label_cache.insert(label.clone(), id);
                            id
                        };

                        if label_id < 64 {
                            label_bits |= 1u64 << label_id;
                        }
                        label_ids_for_update.push(label_id);
                    }

                    // Phase 1 Optimization: Pre-size properties Map to avoid reallocations
                    let properties = if let Some(props_map) = &node.properties {
                        let prop_count = props_map.properties.len();
                        let mut json_props = serde_json::Map::with_capacity(prop_count);
                        for (key, value_expr) in &props_map.properties {
                            let json_value = self.expression_to_json_value(value_expr)?;
                            json_props.insert(key.clone(), json_value);
                        }
                        tracing::debug!(
                            "execute_create_pattern_internal: creating node with variable {:?}, labels {:?}, properties={:?}",
                            node.variable,
                            node.labels,
                            serde_json::Value::Object(json_props.clone())
                        );
                        serde_json::Value::Object(json_props)
                    } else {
                        tracing::debug!(
                            "execute_create_pattern_internal: creating node with variable {:?}, labels {:?}, NO PROPERTIES",
                            node.variable,
                            node.labels
                        );
                        serde_json::Value::Null
                    };

                    // Check constraints before creating node
                    self.check_constraints(&label_ids_for_update, &properties)?;

                    // Create the node
                    let node_id = self
                        .store_mut()
                        .create_node_with_label_bits(&mut tx, label_bits, properties)?;

                    tracing::debug!(
                        "execute_create_pattern_internal: created node_id={}, variable={:?}",
                        node_id,
                        node.variable
                    );

                    // Phase 1 Optimization: Batch catalog metadata updates (defer to end)
                    for label_id in label_ids_for_update {
                        *label_count_updates.entry(label_id).or_insert(0) += 1;
                    }

                    // Store node ID if variable exists
                    if let Some(var) = &node.variable {
                        created_nodes.insert(var.clone(), node_id);
                    }

                    // Track last node for relationship creation
                    last_node_id = Some(node_id);
                }
                parser::PatternElement::Relationship(rel) => {
                    // Get source node (previous element should be a node)
                    let source_id = if i > 0 {
                        last_node_id.ok_or_else(|| {
                            Error::CypherExecution("Relationship must follow a node".to_string())
                        })?
                    } else {
                        return Err(Error::CypherExecution(
                            "Pattern must start with a node".to_string(),
                        ));
                    };

                    // Get target node (next element should be a node)
                    let target_id = if i + 1 < pattern.elements.len() {
                        if let parser::PatternElement::Node(target_node) = &pattern.elements[i + 1]
                        {
                            // Phase 1 Optimization: Build label bitmap with cached lookups
                            let mut target_label_bits = 0u64;
                            let mut target_label_ids_for_update = Vec::new();
                            for label in &target_node.labels {
                                // Use cache if available, otherwise lookup and cache
                                let label_id = if let Some(&cached_id) = label_cache.get(label) {
                                    cached_id
                                } else {
                                    let id = self.catalog().get_or_create_label(label)?;
                                    label_cache.insert(label.clone(), id);
                                    id
                                };

                                if label_id < 64 {
                                    target_label_bits |= 1u64 << label_id;
                                }
                                target_label_ids_for_update.push(label_id);
                            }

                            // Phase 1 Optimization: Pre-size properties Map
                            let target_properties = if let Some(props_map) = &target_node.properties
                            {
                                let prop_count = props_map.properties.len();
                                let mut json_props = serde_json::Map::with_capacity(prop_count);
                                for (key, value_expr) in &props_map.properties {
                                    let json_value = self.expression_to_json_value(value_expr)?;
                                    json_props.insert(key.clone(), json_value);
                                }
                                serde_json::Value::Object(json_props)
                            } else {
                                serde_json::Value::Null
                            };

                            // Create target node (we'll skip it in the next iteration)
                            let tid = self.store_mut().create_node_with_label_bits(
                                &mut tx,
                                target_label_bits,
                                target_properties,
                            )?;

                            // Phase 1 Optimization: Batch catalog metadata updates for target node
                            for label_id in target_label_ids_for_update {
                                *label_count_updates.entry(label_id).or_insert(0) += 1;
                            }

                            // Store target node ID if variable exists
                            if let Some(var) = &target_node.variable {
                                created_nodes.insert(var.clone(), tid);
                            }

                            last_node_id = Some(tid);

                            // Set flag to skip this node in the next iteration
                            skip_next_node = true;

                            tid
                        } else {
                            return Err(Error::CypherExecution(
                                "Relationship must be followed by a node".to_string(),
                            ));
                        }
                    } else {
                        return Err(Error::CypherExecution(
                            "Pattern must end with a node".to_string(),
                        ));
                    };

                    // Get relationship type
                    let rel_type = rel.types.first().ok_or_else(|| {
                        Error::CypherExecution("Relationship must have a type".to_string())
                    })?;

                    // Phase 1.5.2: Use pre-allocated type ID
                    // Type should already be in cache from batch allocation
                    // Fallback to individual lookup if not found (shouldn't happen, but be safe)
                    let type_id = if let Some(&id) = label_cache.get(rel_type) {
                        id
                    } else {
                        // Fallback: individual lookup (shouldn't happen with batch allocation)
                        let id = self.catalog().get_or_create_type(rel_type)?;
                        label_cache.insert(rel_type.to_string(), id);
                        id
                    };

                    // Phase 1 Optimization: Pre-size properties Map for relationships
                    let rel_properties = if let Some(props_map) = &rel.properties {
                        let prop_count = props_map.properties.len();
                        let mut json_props = serde_json::Map::with_capacity(prop_count);
                        for (key, value_expr) in &props_map.properties {
                            let json_value = self.expression_to_json_value(value_expr)?;
                            json_props.insert(key.clone(), json_value);
                        }
                        serde_json::Value::Object(json_props)
                    } else {
                        serde_json::Value::Null
                    };

                    // Clone properties for Phase 8 synchronization (before moving to create_relationship)
                    let rel_props_clone = rel_properties.clone();

                    // Acquire row locks on source and target nodes before creating relationship
                    let (_source_lock, _target_lock) =
                        self.acquire_relationship_locks(source_id, target_id)?;

                    // Create the relationship (locks held by guards)
                    let rel_id = self.store_mut().create_relationship(
                        &mut tx,
                        source_id,
                        target_id,
                        type_id,
                        rel_properties,
                    )?;

                    // Locks are released when guards are dropped

                    // Store relationship ID if variable exists
                    if let Some(var) = &rel.variable {
                        created_relationships.insert(
                            var.clone(),
                            RelationshipInfo {
                                id: rel_id,
                                source_id,
                                target_id,
                                type_id,
                            },
                        );
                    }

                    // Phase 8: Update RelationshipStorageManager and RelationshipPropertyIndex
                    if self.enable_relationship_optimizations {
                        if let Some(ref rel_storage) = self.shared.relationship_storage {
                            // Convert properties from JSON Value to HashMap<String, Value>
                            let mut props_map = std::collections::HashMap::new();
                            if let serde_json::Value::Object(obj) = &rel_props_clone {
                                for (key, value) in obj {
                                    props_map.insert(key.clone(), value.clone());
                                }
                            }

                            // Add relationship to specialized storage
                            if let Err(e) = rel_storage.write().create_relationship(
                                source_id,
                                target_id,
                                type_id,
                                props_map.clone(),
                            ) {
                                tracing::warn!(
                                    "Failed to update RelationshipStorageManager: {}",
                                    e
                                );
                                // Don't fail the operation, just log the warning
                            }

                            // Update property index if there are properties
                            if !props_map.is_empty() {
                                if let Some(ref prop_index) =
                                    self.shared.relationship_property_index
                                {
                                    if let Err(e) = prop_index
                                        .write()
                                        .index_properties(rel_id, type_id, &props_map)
                                    {
                                        tracing::warn!(
                                            "Failed to update RelationshipPropertyIndex: {}",
                                            e
                                        );
                                        // Don't fail the operation, just log the warning
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Commit transaction
        tx_mgr.commit(&mut tx)?;

        // Phase 1 Optimization: Batch apply catalog metadata updates (reduces I/O)
        // Convert HashMap to Vec for batch update
        let updates: Vec<(u32, u32)> = label_count_updates.into_iter().collect();
        if !updates.is_empty() {
            if let Err(e) = self.catalog().batch_increment_node_counts(&updates) {
                // Log error but don't fail the operation
                tracing::warn!("Failed to batch update node counts: {}", e);
            }
        }

        // PERFORMANCE OPTIMIZATION: Use async flush for better throughput
        // The transaction commit above ensures data integrity
        // Async flush triggers write without blocking on OS confirmation
        // Memory barrier below ensures visibility across threads
        self.store_mut().flush_async()?;

        // Update label index with created nodes
        // Scan all nodes from the store that were created (iterate based on node IDs, not variables)
        let start_node_id = if created_nodes.is_empty() {
            // If no variables were tracked, we need to find the new nodes
            // For now, just iterate over ALL nodes in the recent range
            // This is a workaround - ideally we'd track all created IDs, not just those with variables
            // For standalone CREATE without variables, we need a different approach
            // Let's assume created nodes are at the end of the node_count range
            let node_count = self.store().node_count();
            // Get the expected number of nodes created (pattern elements count)
            let expected_created = pattern
                .elements
                .iter()
                .filter(|e| matches!(e, parser::PatternElement::Node(_)))
                .count();
            if node_count as usize >= expected_created {
                node_count - expected_created as u64
            } else {
                0
            }
        } else {
            // Use the tracked nodes
            *created_nodes.values().min().unwrap_or(&0)
        };

        let end_node_id = self.store().node_count();

        for node_id in start_node_id..end_node_id {
            // Read the node to get its labels
            if let Ok(node_record) = self.store().read_node(node_id) {
                if node_record.is_deleted() {
                    continue;
                }
                let mut label_ids = Vec::new();
                for bit in 0..64 {
                    if (node_record.label_bits & (1u64 << bit)) != 0 {
                        label_ids.push(bit as u32);
                    }
                }
                if !label_ids.is_empty() {
                    self.label_index_mut().add_node(node_id, &label_ids)?;
                }
            }
        }

        Ok(())
    }

    /// Convert expression to JSON value
    pub(super) fn expression_to_json_value(&self, expr: &parser::Expression) -> Result<Value> {
        match expr {
            parser::Expression::Literal(lit) => match lit {
                parser::Literal::String(s) => Ok(Value::String(s.clone())),
                parser::Literal::Integer(i) => Ok(Value::Number((*i).into())),
                parser::Literal::Float(f) => {
                    if let Some(num) = serde_json::Number::from_f64(*f) {
                        Ok(Value::Number(num))
                    } else {
                        Err(Error::CypherExecution(format!("Invalid float: {}", f)))
                    }
                }
                parser::Literal::Boolean(b) => Ok(Value::Bool(*b)),
                parser::Literal::Null => Ok(Value::Null),
                parser::Literal::Point(p) => Ok(p.to_json_value()),
            },
            parser::Expression::Variable(_) => Err(Error::CypherExecution(
                "Variables not supported in CREATE properties".to_string(),
            )),
            _ => Err(Error::CypherExecution(
                "Complex expressions not supported in CREATE properties".to_string(),
            )),
        }
    }

    /// Check constraints before creating a node
    pub(super) fn check_constraints(
        &self,
        label_ids: &[u32],
        properties: &serde_json::Value,
    ) -> Result<()> {
        let constraint_manager = self.catalog().constraint_manager().read();

        // Check constraints for each label
        for &label_id in label_ids {
            let constraints = constraint_manager.get_constraints_for_label(label_id)?;

            for constraint in constraints {
                // Get property name
                let property_name = self
                    .catalog()
                    .get_key_name(constraint.property_key_id)?
                    .ok_or_else(|| Error::Internal("Property key not found".to_string()))?;

                let property_value = properties.as_object().and_then(|m| m.get(&property_name));

                match constraint.constraint_type {
                    crate::catalog::constraints::ConstraintType::Exists => {
                        // Property must exist (not null)
                        if property_value.is_none()
                            || property_value == Some(&serde_json::Value::Null)
                        {
                            let label_name = self
                                .catalog()
                                .get_label_name(label_id)?
                                .unwrap_or_else(|| format!("ID{}", label_id));
                            return Err(Error::ConstraintViolation(format!(
                                "EXISTS constraint violated: property '{}' must exist on nodes with label '{}'",
                                property_name, label_name
                            )));
                        }
                    }
                    crate::catalog::constraints::ConstraintType::Unique => {
                        // Property value must be unique across all nodes with this label
                        if let Some(value) = property_value {
                            let label_name = self
                                .catalog()
                                .get_label_name(label_id)?
                                .unwrap_or_else(|| format!("ID{}", label_id));

                            // Get all nodes with this label
                            let bitmap = self.label_index().get_nodes_with_labels(&[label_id])?;

                            for node_id in bitmap.iter() {
                                let node_id_u64 = node_id as u64;

                                let node_props = self.store().load_node_properties(node_id_u64)?;
                                if let Some(serde_json::Value::Object(props_map)) = node_props {
                                    if let Some(existing_value) = props_map.get(&property_name) {
                                        if existing_value == value {
                                            return Err(Error::ConstraintViolation(format!(
                                                "UNIQUE constraint violated: property '{}' value already exists on another node with label '{}'",
                                                property_name, label_name
                                            )));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Convert expression to string representation
    pub(super) fn expression_to_string(&self, expr: &parser::Expression) -> Result<String> {
        match expr {
            parser::Expression::Variable(name) => Ok(name.clone()),
            parser::Expression::PropertyAccess { variable, property } => {
                Ok(format!("{}.{}", variable, property))
            }
            parser::Expression::Literal(literal) => match literal {
                // Use single quotes for strings in filter predicates to match Cypher parser expectations
                parser::Literal::String(s) => Ok(format!("'{}'", s)),
                parser::Literal::Integer(i) => Ok(i.to_string()),
                parser::Literal::Float(f) => Ok(f.to_string()),
                parser::Literal::Boolean(b) => Ok(b.to_string()),
                parser::Literal::Null => Ok("NULL".to_string()),
                parser::Literal::Point(p) => Ok(p.to_string()),
            },
            parser::Expression::BinaryOp { left, op, right } => {
                let left_str = self.expression_to_string(left)?;
                let right_str = self.expression_to_string(right)?;
                let op_str = match op {
                    parser::BinaryOperator::Equal => "=",
                    parser::BinaryOperator::NotEqual => "!=",
                    parser::BinaryOperator::LessThan => "<",
                    parser::BinaryOperator::LessThanOrEqual => "<=",
                    parser::BinaryOperator::GreaterThan => ">",
                    parser::BinaryOperator::GreaterThanOrEqual => ">=",
                    parser::BinaryOperator::And => "AND",
                    parser::BinaryOperator::Or => "OR",
                    parser::BinaryOperator::Add => "+",
                    parser::BinaryOperator::Subtract => "-",
                    parser::BinaryOperator::Multiply => "*",
                    parser::BinaryOperator::Divide => "/",
                    parser::BinaryOperator::In => "IN",
                    _ => "?",
                };
                Ok(format!("{} {} {}", left_str, op_str, right_str))
            }
            parser::Expression::Parameter(name) => Ok(format!("${}", name)),
            _ => Ok("?".to_string()),
        }
    }

    /// Execute NodeByLabel operator

    /// Execute Filter operator with index optimization

    /// Execute Project operator

    /// Execute Union operator
    pub(super) fn execute_union(
        &self,
        context: &mut ExecutionContext,
        left: &[Operator],
        right: &[Operator],
        distinct: bool,
    ) -> Result<()> {
        // Execute left operator pipeline and collect its results
        let mut left_context = ExecutionContext::new(context.params.clone(), context.cache.clone());
        for (idx, operator) in left.iter().enumerate() {
            tracing::debug!(
                "UNION: executing left operator {}/{}: {:?}",
                idx + 1,
                left.len(),
                operator
            );
            self.execute_operator(&mut left_context, operator)?;
            tracing::debug!(
                "UNION: after left operator {}, result_set.rows={}, columns={:?}, variables={:?}",
                idx + 1,
                left_context.result_set.rows.len(),
                left_context.result_set.columns,
                left_context.variables.keys().collect::<Vec<_>>()
            );
        }

        tracing::debug!(
            "UNION: left side completed - result_set.rows={}, columns={:?}",
            left_context.result_set.rows.len(),
            left_context.result_set.columns
        );

        // Execute right operator pipeline and collect its results
        let mut right_context =
            ExecutionContext::new(context.params.clone(), context.cache.clone());
        for (idx, operator) in right.iter().enumerate() {
            tracing::debug!(
                "UNION: executing right operator {}/{}: {:?}",
                idx + 1,
                right.len(),
                operator
            );
            self.execute_operator(&mut right_context, operator)?;
            tracing::debug!(
                "UNION: after right operator {}, result_set.rows={}, columns={:?}, variables={:?}",
                idx + 1,
                right_context.result_set.rows.len(),
                right_context.result_set.columns,
                right_context.variables.keys().collect::<Vec<_>>()
            );
        }

        tracing::debug!(
            "UNION: right side completed - result_set.rows={}, columns={:?}",
            right_context.result_set.rows.len(),
            right_context.result_set.columns
        );

        // Combine results from both sides
        // Ensure results are in result_set.rows (some operators may store in variables)
        // Convert variable-based results to rows if needed
        // CRITICAL FIX: Project operator should populate result_set.rows, but if it's empty,
        // we need to materialize from variables to ensure all rows are collected for UNION
        // However, we should NOT materialize if variables only contain empty arrays (no matches found)
        if left_context.result_set.rows.is_empty() && !left_context.variables.is_empty() {
            // Check if any variable has non-empty array - if all are empty, don't materialize
            let has_non_empty_array = left_context.variables.values().any(|v| {
                match v {
                    Value::Array(arr) => !arr.is_empty(),
                    _ => true, // Non-array values should be materialized
                }
            });

            if has_non_empty_array {
                // If no rows but we have variables with data, materialize from variables
                let row_maps = self.materialize_rows_from_variables(&left_context);
                if !row_maps.is_empty() {
                    // Ensure columns are set from variables if not already set
                    if left_context.result_set.columns.is_empty() {
                        let mut columns: Vec<String> = row_maps[0].keys().cloned().collect();
                        columns.sort();
                        left_context.result_set.columns = columns;
                    }
                    self.update_result_set_from_rows(&mut left_context, &row_maps);
                }
            }
            // If all arrays are empty (no matches found), leave result_set.rows empty
        }

        if right_context.result_set.rows.is_empty() && !right_context.variables.is_empty() {
            // Check if any variable has non-empty array - if all are empty, don't materialize
            let has_non_empty_array = right_context.variables.values().any(|v| {
                match v {
                    Value::Array(arr) => !arr.is_empty(),
                    _ => true, // Non-array values should be materialized
                }
            });

            if has_non_empty_array {
                // If no rows but we have variables with data, materialize from variables
                let row_maps = self.materialize_rows_from_variables(&right_context);
                if !row_maps.is_empty() {
                    // Ensure columns are set from variables if not already set
                    if right_context.result_set.columns.is_empty() {
                        let mut columns: Vec<String> = row_maps[0].keys().cloned().collect();
                        columns.sort();
                        right_context.result_set.columns = columns;
                    }
                    self.update_result_set_from_rows(&mut right_context, &row_maps);
                }
            }
            // If all arrays are empty (no matches found), leave result_set.rows empty
        }

        // CRITICAL FIX: Ensure columns are set from result_set.rows if Project already executed
        // Project should have set columns, but verify they match the row structure
        if !left_context.result_set.rows.is_empty() && !left_context.result_set.columns.is_empty() {
            // Verify column count matches row value count
            if let Some(first_row) = left_context.result_set.rows.first() {
                if first_row.values.len() != left_context.result_set.columns.len() {
                    // Mismatch - this shouldn't happen, but log it
                    tracing::warn!(
                        "UNION: Left side column/row mismatch: {} cols, {} values",
                        left_context.result_set.columns.len(),
                        first_row.values.len()
                    );
                }
            }
        }

        if !right_context.result_set.rows.is_empty() && !right_context.result_set.columns.is_empty()
        {
            if let Some(first_row) = right_context.result_set.rows.first() {
                if first_row.values.len() != right_context.result_set.columns.len() {
                    tracing::warn!(
                        "UNION: Right side column/row mismatch: {} cols, {} values",
                        right_context.result_set.columns.len(),
                        first_row.values.len()
                    );
                }
            }
        }

        // Ensure both sides have the same columns (UNION requires matching column structure)
        // UNION requires that both sides have the same number of columns with compatible types
        // Priority: left columns > right columns > columns from RETURN items
        let columns = if !left_context.result_set.columns.is_empty() {
            left_context.result_set.columns.clone()
        } else if !right_context.result_set.columns.is_empty() {
            right_context.result_set.columns.clone()
        } else {
            // If both sides are empty, try to get columns from variables or result set rows
            // First try to get from left side variables
            let left_row_maps = self.materialize_rows_from_variables(&left_context);
            let right_row_maps = self.materialize_rows_from_variables(&right_context);

            // Get columns from row maps if available
            let mut all_columns = std::collections::HashSet::new();
            if !left_row_maps.is_empty() {
                all_columns.extend(left_row_maps[0].keys().cloned());
            }
            if !right_row_maps.is_empty() {
                all_columns.extend(right_row_maps[0].keys().cloned());
            }

            // If still empty, try variables
            if all_columns.is_empty() {
                for (var, _) in &left_context.variables {
                    all_columns.insert(var.clone());
                }
                for (var, _) in &right_context.variables {
                    all_columns.insert(var.clone());
                }
            }

            let mut cols: Vec<String> = all_columns.into_iter().collect();
            cols.sort();
            cols
        };

        // Normalize rows from both sides to use the same column order
        // CRITICAL FIX: If columns are empty but rows exist, use row order directly
        let mut left_rows = Vec::new();
        tracing::debug!(
            "UNION: left side - result_set.rows={}, columns={:?}, left_context.columns={:?}",
            left_context.result_set.rows.len(),
            columns,
            left_context.result_set.columns
        );

        if left_context.result_set.columns.is_empty() && !left_context.result_set.rows.is_empty() {
            // No columns defined - use row values as-is (shouldn't happen if Project ran correctly)
            tracing::debug!("UNION: left side has no columns, using row values as-is");
            for row in &left_context.result_set.rows {
                left_rows.push(row.clone());
            }
        } else {
            for (row_idx, row) in left_context.result_set.rows.iter().enumerate() {
                let mut normalized_values = Vec::new();
                for col in &columns {
                    if let Some(idx) = left_context
                        .result_set
                        .columns
                        .iter()
                        .position(|c| c == col)
                    {
                        if idx < row.values.len() {
                            normalized_values.push(row.values[idx].clone());
                        } else {
                            normalized_values.push(Value::Null);
                        }
                    } else {
                        normalized_values.push(Value::Null);
                    }
                }
                tracing::debug!(
                    "UNION: left row {} normalized: {:?}",
                    row_idx,
                    normalized_values
                );
                left_rows.push(Row {
                    values: normalized_values,
                });
            }
        }

        tracing::debug!("UNION: left_rows after normalization: {}", left_rows.len());

        let mut right_rows = Vec::new();
        tracing::debug!(
            "UNION: right side - result_set.rows={}, columns={:?}, right_context.columns={:?}",
            right_context.result_set.rows.len(),
            columns,
            right_context.result_set.columns
        );

        if right_context.result_set.columns.is_empty() && !right_context.result_set.rows.is_empty()
        {
            // No columns defined - use row values as-is (shouldn't happen if Project ran correctly)
            tracing::debug!("UNION: right side has no columns, using row values as-is");
            for row in &right_context.result_set.rows {
                right_rows.push(row.clone());
            }
        } else {
            for (row_idx, row) in right_context.result_set.rows.iter().enumerate() {
                let mut normalized_values = Vec::new();
                for col in &columns {
                    if let Some(idx) = right_context
                        .result_set
                        .columns
                        .iter()
                        .position(|c| c == col)
                    {
                        if idx < row.values.len() {
                            normalized_values.push(row.values[idx].clone());
                        } else {
                            normalized_values.push(Value::Null);
                        }
                    } else {
                        normalized_values.push(Value::Null);
                    }
                }
                tracing::debug!(
                    "UNION: right row {} normalized: {:?}",
                    row_idx,
                    normalized_values
                );
                right_rows.push(Row {
                    values: normalized_values,
                });
            }
        }

        tracing::debug!(
            "UNION: right_rows after normalization: {}",
            right_rows.len()
        );

        tracing::debug!(
            "UNION: left_rows={}, right_rows={}, columns={:?}",
            left_rows.len(),
            right_rows.len(),
            columns
        );

        let mut combined_rows = Vec::new();
        combined_rows.extend(left_rows);
        combined_rows.extend(right_rows);

        tracing::debug!(
            "UNION: combined_rows before dedup={}, distinct={}",
            combined_rows.len(),
            distinct
        );

        // If UNION (not UNION ALL), deduplicate results
        if distinct {
            let mut seen = std::collections::HashSet::new();
            let mut deduped_rows = Vec::new();

            for row in combined_rows {
                // Serialize row values to a string for comparison
                // Use a canonical JSON representation to ensure consistent comparison
                let row_key = serde_json::to_string(&row.values).unwrap_or_default();
                if seen.insert(row_key.clone()) {
                    deduped_rows.push(row);
                } else {
                    tracing::debug!("UNION: duplicate row removed: {}", row_key);
                }
            }
            combined_rows = deduped_rows;
            tracing::debug!("UNION: deduped_rows={}", combined_rows.len());
        }

        // Update the main context with combined results
        context.set_columns_and_rows(columns, combined_rows);
        tracing::debug!(
            "UNION: final result_set.rows={}",
            context.result_set.rows.len()
        );
        let row_maps = self.result_set_as_rows(context);
        self.update_variables_from_rows(context, &row_maps);
        Ok(())
    }

    /// Execute CREATE operator with context from MATCH
    pub(super) fn execute_create_with_context(
        &self,
        context: &mut ExecutionContext,
        pattern: &parser::Pattern,
    ) -> Result<()> {
        // Note: TransactionManager is now accessed via self.transaction_manager() (shared)
        use serde_json::Value as JsonValue;

        // CRITICAL FIX: Always try to use context.variables first for MATCH...CREATE
        // The variables contain the full node objects with _nexus_id, while result_set.rows
        // may contain only projected values (strings) without _nexus_id.
        // Only fall back to result_set.rows if variables are empty.

        tracing::debug!(
            "execute_create_with_context: variables={:?}, result_set.rows={}",
            context.variables.keys().collect::<Vec<_>>(),
            context.result_set.rows.len()
        );

        let current_rows = if !context.variables.is_empty() {
            // PERFORMANCE OPTIMIZATION: Fast-path for simple single-value variables
            // This avoids the expensive materialize_rows_from_variables() for common cases
            // like MATCH (p:Person {name: 'X'}), (c:Company {name: 'Y'}) CREATE ...
            let all_single_values = context
                .variables
                .values()
                .all(|v| !matches!(v, JsonValue::Array(_)));

            if all_single_values {
                // Fast path: directly create a single row from variables
                let mut row = std::collections::HashMap::with_capacity(context.variables.len());
                let mut has_node_ids = false;
                for (var, value) in &context.variables {
                    if let JsonValue::Object(obj) = value {
                        if obj.contains_key("_nexus_id") {
                            has_node_ids = true;
                        }
                    }
                    row.insert(var.clone(), value.clone());
                }
                if has_node_ids {
                    vec![row]
                } else if !context.result_set.rows.is_empty() {
                    // Fallback to result_set if no node IDs
                    let columns = context.result_set.columns.clone();
                    context
                        .result_set
                        .rows
                        .iter()
                        .map(|row| self.row_to_map(row, &columns))
                        .collect()
                } else {
                    vec![row]
                }
            } else {
                // Slow path: use full materialization for array variables
                let materialized = self.materialize_rows_from_variables(context);

                // Verify materialized rows have node objects with _nexus_id
                let has_node_ids = materialized.iter().any(|row| {
                    row.values().any(|v| {
                        if let JsonValue::Object(obj) = v {
                            obj.contains_key("_nexus_id")
                        } else {
                            false
                        }
                    })
                });

                if has_node_ids {
                    materialized
                } else if !context.result_set.rows.is_empty() {
                    let columns = context.result_set.columns.clone();
                    context
                        .result_set
                        .rows
                        .iter()
                        .map(|row| self.row_to_map(row, &columns))
                        .collect()
                } else {
                    materialized
                }
            }
        } else if !context.result_set.rows.is_empty() {
            // No variables - use result_set.rows
            let columns = context.result_set.columns.clone();
            let rows: Vec<_> = context
                .result_set
                .rows
                .iter()
                .map(|row| self.row_to_map(row, &columns))
                .collect();
            tracing::debug!(
                "execute_create_with_context: no variables, using {} rows from result_set.rows",
                rows.len()
            );
            rows
        } else {
            // No variables and no rows
            tracing::debug!("execute_create_with_context: no variables and no rows");
            Vec::new()
        };

        // If no rows from MATCH, nothing to create
        if current_rows.is_empty() {
            return Ok(());
        }

        // DEBUG: Print row contents to see if they contain _nexus_id
        for (idx, row) in current_rows.iter().enumerate() {}

        // PERFORMANCE OPTIMIZATION: Reuse shared transaction manager instead of creating new
        // This saves ~1-2ms per operation by avoiding TransactionManager::new() overhead
        let mut tx_mgr = self.transaction_manager().lock();
        let mut tx = tx_mgr.begin_write()?;

        // For each row in the MATCH result, create the pattern
        // PERFORMANCE OPTIMIZATION: Pre-calculate expected capacity for node_ids
        let expected_vars = pattern
            .elements
            .iter()
            .filter(|e| matches!(e, parser::PatternElement::Node(n) if n.variable.is_some()))
            .count();

        for row in current_rows.iter() {
            // Pre-allocate HashMap with expected capacity
            let mut node_ids: std::collections::HashMap<String, u64> =
                std::collections::HashMap::with_capacity(expected_vars);

            // First, resolve existing node variables from the row
            for (var_name, var_value) in row {
                if let JsonValue::Object(obj) = var_value {
                    if let Some(JsonValue::Number(id)) = obj.get("_nexus_id") {
                        if let Some(node_id) = id.as_u64() {
                            node_ids.insert(var_name.clone(), node_id);
                        }
                    }
                }
            }

            // DEBUG: Print node_ids after extraction

            // CRITICAL FIX: If no node IDs were resolved from the row and the pattern requires
            // existing nodes from MATCH, skip this row (Filter removed all valid rows)
            // This prevents CREATE from executing when Filter filtered out all rows
            if node_ids.is_empty() {
                // Check if pattern requires existing nodes (has variables that should come from MATCH)
                let pattern_requires_existing_nodes = pattern.elements.iter().any(|elem| {
                    match elem {
                        parser::PatternElement::Node(node) => {
                            if let Some(var) = &node.variable {
                                // If node has no properties or labels, it's likely from MATCH
                                // If it has properties/labels, it's a new node to create
                                node.properties.is_none() && node.labels.is_empty()
                            } else {
                                false
                            }
                        }
                        parser::PatternElement::Relationship(_) => false,
                    }
                });

                if pattern_requires_existing_nodes {
                    continue; // Skip this row - Filter removed all valid matches
                }
            }

            // Now process the pattern elements to create new nodes and relationships
            let mut last_node_var: Option<String> = None;

            for (idx, element) in pattern.elements.iter().enumerate() {
                match element {
                    parser::PatternElement::Node(node) => {
                        if let Some(var) = &node.variable {
                            if !node_ids.contains_key(var) {
                                // Create new node (not from MATCH)
                                let labels: Vec<u64> = node
                                    .labels
                                    .iter()
                                    .filter_map(|l| self.catalog().get_or_create_label(l).ok())
                                    .map(|id| id as u64)
                                    .collect();

                                let mut label_bits = 0u64;
                                for label_id in labels {
                                    label_bits |= 1u64 << label_id;
                                }

                                // Extract properties
                                let properties = if let Some(props_map) = &node.properties {
                                    JsonValue::Object(
                                        props_map
                                            .properties
                                            .iter()
                                            .filter_map(|(k, v)| {
                                                self.expression_to_json_value(v)
                                                    .ok()
                                                    .map(|val| (k.clone(), val))
                                            })
                                            .collect(),
                                    )
                                } else {
                                    JsonValue::Object(serde_json::Map::new())
                                };

                                // Create the node
                                let node_id = self
                                    .store_mut()
                                    .create_node_with_label_bits(&mut tx, label_bits, properties)?;
                                node_ids.insert(var.clone(), node_id);
                            }

                            // Track this node as the last one for relationship creation
                            last_node_var = Some(var.clone());
                        }
                    }
                    parser::PatternElement::Relationship(rel) => {
                        // Create relationship between last_node and next_node
                        if let Some(rel_type) = rel.types.first() {
                            let type_id = self.catalog().get_or_create_type(rel_type)?;

                            // Extract relationship properties
                            let properties = if let Some(props_map) = &rel.properties {
                                JsonValue::Object(
                                    props_map
                                        .properties
                                        .iter()
                                        .filter_map(|(k, v)| {
                                            self.expression_to_json_value(v)
                                                .ok()
                                                .map(|val| (k.clone(), val))
                                        })
                                        .collect(),
                                )
                            } else {
                                JsonValue::Object(serde_json::Map::new())
                            };

                            // Source is the last_node_var, target will be the next node in pattern
                            if let Some(source_var) = &last_node_var {
                                if let Some(source_id) = node_ids.get(source_var) {
                                    // Find target node (next element after this relationship)
                                    if idx + 1 < pattern.elements.len() {
                                        if let parser::PatternElement::Node(target_node) =
                                            &pattern.elements[idx + 1]
                                        {
                                            if let Some(target_var) = &target_node.variable {
                                                if let Some(target_id) = node_ids.get(target_var) {
                                                    // PERFORMANCE OPTIMIZATION: Skip row-level locking when lock-free mode is enabled
                                                    // The transaction manager mutex already provides serialization
                                                    // Row locks are only needed for concurrent writers
                                                    let _locks =
                                                        if !self.config.enable_lock_free_structures
                                                        {
                                                            Some(self.acquire_relationship_locks(
                                                                *source_id, *target_id,
                                                            )?)
                                                        } else {
                                                            None
                                                        };

                                                    // Create the relationship
                                                    let rel_id =
                                                        self.store_mut().create_relationship(
                                                            &mut tx, *source_id, *target_id,
                                                            type_id, properties,
                                                        )?;
                                                    tracing::debug!(
                                                        "execute_create_with_context: relationship created successfully, rel_id={}",
                                                        rel_id
                                                    );

                                                    // CRITICAL FIX: Populate relationship variable if specified
                                                    // This ensures that queries like CREATE (a)-[r:KNOWS]->(b) RETURN r work correctly
                                                    if let Some(rel_var) = &rel.variable {
                                                        if !rel_var.is_empty() {
                                                            let rel_info = RelationshipInfo {
                                                                id: rel_id,
                                                                source_id: *source_id,
                                                                target_id: *target_id,
                                                                type_id,
                                                            };
                                                            if let Ok(rel_value) = self
                                                                .read_relationship_as_value(
                                                                    &rel_info,
                                                                )
                                                            {
                                                                // Store relationship in context for RETURN clause
                                                                context.variables.insert(
                                                                    rel_var.clone(),
                                                                    rel_value,
                                                                );
                                                            }
                                                        }
                                                    }

                                                    // Locks are released when guards are dropped

                                                    // Relationship created successfully
                                                } else {
                                                    tracing::warn!(
                                                        "execute_create_with_context: Target node not found: var={}, available node_ids: {:?}",
                                                        target_var,
                                                        node_ids.keys().collect::<Vec<_>>()
                                                    );
                                                }
                                            } else {
                                                tracing::warn!(
                                                    "execute_create_with_context: Target node has no variable"
                                                );
                                            }
                                        } else {
                                            tracing::warn!(
                                                "execute_create_with_context: Next element is not a Node"
                                            );
                                        }
                                    } else {
                                        tracing::warn!(
                                            "execute_create_with_context: No next element after relationship"
                                        );
                                    }
                                } else {
                                    tracing::warn!(
                                        "execute_create_with_context: Source node not found: var={}, available node_ids: {:?}",
                                        source_var,
                                        node_ids.keys().collect::<Vec<_>>()
                                    );
                                }
                            } else {
                                tracing::warn!(
                                    "execute_create_with_context: No last_node_var (no source node before relationship)"
                                );
                            }
                        }
                    }
                }
            }
        }

        // Commit transaction
        tx_mgr.commit(&mut tx)?;

        // PERFORMANCE OPTIMIZATION: Use async flush instead of sync flush
        // The sync flush was costing ~15-20ms per relationship creation
        // Async flush triggers the write but doesn't wait for OS confirmation
        // Data integrity is still maintained by the transaction commit above
        // For critical durability, callers can explicitly call flush() after the query
        self.store_mut().flush_async()?;

        // Memory barrier to ensure writes are visible to subsequent reads
        // Using Acquire/Release is sufficient here since we're in single-writer context
        std::sync::atomic::fence(std::sync::atomic::Ordering::Release);

        // CRITICAL FIX: Populate result_set with created entities for CREATE without RETURN
        // Instead of clearing everything, we populate result_set with the variables we have
        // This ensures that CREATE without RETURN returns the created entities
        // If RETURN clause follows, Project operator will overwrite this
        let mut columns: Vec<String> = context.variables.keys().cloned().collect();
        columns.sort(); // Ensure consistent column order

        if !columns.is_empty() {
            let mut row_values = Vec::new();
            for col in &columns {
                if let Some(value) = context.variables.get(col) {
                    // CRITICAL FIX: Unwrap arrays to get the actual node object
                    // Variables from MATCH are arrays, but we need single objects
                    let unwrapped = match value {
                        JsonValue::Array(arr) if arr.len() == 1 => arr[0].clone(),
                        _ => value.clone(),
                    };
                    row_values.push(unwrapped);
                } else {
                    row_values.push(JsonValue::Null);
                }
            }
            context.result_set.columns = columns;
            context.result_set.rows = vec![Row { values: row_values }];
        } else {
            // No variables created - clear result_set
            context.result_set.rows.clear();
            context.result_set.columns.clear();
        }

        tracing::trace!(
            "After CREATE: result_set.columns={:?}, result_set.rows.len()={}, variables.len()={}",
            context.result_set.columns,
            context.result_set.rows.len(),
            context.variables.len()
        );

        Ok(())
    }
    /// Execute a single operator and return results
    pub(super) fn execute_operator(
        &self,
        context: &mut ExecutionContext,
        operator: &Operator,
    ) -> Result<()> {
        match operator {
            Operator::NodeByLabel { label_id, variable } => {
                let nodes = self.execute_node_by_label(*label_id)?;
                tracing::debug!(
                    "execute_operator NodeByLabel: found {} nodes for label_id {}, variable '{}'",
                    nodes.len(),
                    label_id,
                    variable
                );

                // CRITICAL FIX: Remove relationship objects from variables before creating cartesian product
                // Relationship objects have a "type" property - filter them out to avoid contamination
                context.variables.retain(|var_name, var_value| {
                    let is_relationship = if let Value::Object(obj) = var_value {
                        obj.contains_key("type") // Relationships have "type" property
                    } else if let Value::Array(arr) = var_value {
                        // Check if array contains relationship objects
                        arr.iter().any(|v| {
                            if let Value::Object(obj) = v {
                                obj.contains_key("type")
                            } else {
                                false
                            }
                        })
                    } else {
                        false
                    };
                    if is_relationship {}
                    !is_relationship // Keep only non-relationship variables
                });

                // CRITICAL FIX: Always clear result_set.rows before regenerating from variables
                // Since we are applying Cartesian product and regenerating the full state from variables,
                // the old rows in result_set are stale (partial state) and should be removed.
                context.result_set.rows.clear();

                context.variables.remove(variable);

                // CRITICAL FIX: Apply Cartesian product if there are existing variables
                // If we have existing rows (e.g. from a previous MATCH), we must cross-product
                // the new nodes with the existing rows.
                // Example: MATCH (a), (b) -> a has N rows, b has M rows -> Result N*M rows
                if !context.variables.is_empty() {
                    self.apply_cartesian_product(context, variable, nodes)?;
                } else {
                    context.set_variable(variable, Value::Array(nodes));
                }

                // CRITICAL FIX: Materialize rows from variables so Project can process them
                // This matches the behavior in the main execute loop
                let rows = self.materialize_rows_from_variables(context);
                tracing::debug!(
                    "execute_operator NodeByLabel: materialized {} rows from variables",
                    rows.len()
                );
                self.update_result_set_from_rows(context, &rows);
                tracing::debug!(
                    "execute_operator NodeByLabel: result_set now has {} rows",
                    context.result_set.rows.len()
                );
            }
            Operator::AllNodesScan { variable } => {
                let nodes = self.execute_all_nodes_scan()?;

                // CRITICAL FIX: Always clear result_set.rows before regenerating from variables
                context.result_set.rows.clear();

                // CRITICAL FIX: Apply Cartesian product if there are existing variables
                if !context.variables.is_empty() {
                    self.apply_cartesian_product(context, variable, nodes)?;
                } else {
                    context.set_variable(variable, Value::Array(nodes));
                }

                // CRITICAL FIX: Materialize rows from variables so Project can process them
                let rows = self.materialize_rows_from_variables(context);
                self.update_result_set_from_rows(context, &rows);
            }
            Operator::Filter { predicate } => {
                self.execute_filter(context, predicate)?;
            }
            Operator::OptionalFilter {
                predicate,
                optional_vars,
            } => {
                self.execute_optional_filter(context, predicate, optional_vars)?;
            }
            Operator::Expand {
                type_ids,
                direction,
                source_var,
                target_var,
                rel_var,
                optional,
            } => {
                self.execute_expand(
                    context, type_ids, *direction, source_var, target_var, rel_var, *optional,
                    None, // Cache not available at this level
                )?;
            }
            Operator::Project { items } => {
                self.execute_project(context, items)?;
            }
            Operator::With { items, distinct } => {
                self.execute_with(context, items, *distinct)?;
            }
            Operator::Limit { count } => {
                self.execute_limit(context, *count)?;
            }
            Operator::Sort { columns, ascending } => {
                self.execute_sort(context, columns, ascending)?;
            }
            Operator::Aggregate {
                group_by,
                aggregations,
                projection_items,
                source: _,
                streaming_optimized: _,
                push_down_optimized: _,
            } => {
                // Use projection items if available, otherwise call without them
                if let Some(items) = projection_items {
                    self.execute_aggregate_with_projections(
                        context,
                        group_by,
                        aggregations,
                        Some(items.as_slice()),
                    )?;
                } else {
                    self.execute_aggregate(context, group_by, aggregations)?;
                }
            }
            Operator::Union {
                left,
                right,
                distinct,
            } => {
                self.execute_union(context, left, right, *distinct)?;
            }
            Operator::Create { pattern: _ } => {
                // Note: execute_create_with_context requires &mut self
                // This method is only used internally, so we'll handle it differently
                // For now, this path shouldn't be reached as CREATE is handled in execute()
                return Err(Error::CypherExecution(
                    "CREATE operator should be handled in execute() method".to_string(),
                ));
            }
            Operator::Delete { variables } => {
                self.execute_delete(context, variables, false)?;
            }
            Operator::DetachDelete { variables } => {
                self.execute_delete(context, variables, true)?;
            }
            Operator::Join {
                left,
                right,
                join_type,
                condition,
            } => {
                self.execute_join(context, left, right, *join_type, condition.as_deref())?;
            }
            Operator::IndexScan { index_name, label } => {
                self.execute_index_scan_new(context, index_name, label)?;
            }
            Operator::Distinct { columns } => {
                self.execute_distinct(context, columns)?;
            }
            Operator::Unwind {
                expression,
                variable,
            } => {
                self.execute_unwind(context, expression, variable)?;
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
                    context, *type_id, *direction, source_var, target_var, rel_var, path_var,
                    quantifier,
                )?;
            }
            Operator::CallProcedure {
                procedure_name,
                arguments,
                yield_columns,
            } => {
                self.execute_call_procedure(
                    context,
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
                    context,
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
        Ok(())
    }

    /// Execute Join operator

    /// Extract value from a row for a given column name.
    /// Handles PropertyAccess columns (like "n.value") by extracting from the node object.
    pub(super) fn extract_value_from_row(
        &self,
        row: &Row,
        column: &str,
        columns: &[String],
    ) -> Option<Value> {
        // First try direct column lookup
        if let Some(idx) = columns.iter().position(|c| c == column) {
            if idx < row.values.len() {
                return Some(row.values[idx].clone());
            }
        }

        // If column is a PropertyAccess (like "n.value"), extract from node object
        if column.contains('.') {
            let parts: Vec<&str> = column.split('.').collect();
            if parts.len() == 2 {
                let var_name = parts[0];
                let prop_name = parts[1];

                // Find the variable in columns
                if let Some(var_idx) = columns.iter().position(|c| c == var_name) {
                    if var_idx < row.values.len() {
                        // Extract property from the node object
                        if let Value::Object(obj) = &row.values[var_idx] {
                            // Node objects can have properties directly or nested
                            if let Some(val) = obj.get(prop_name) {
                                return Some(val.clone());
                            }
                        }
                    }
                }
            }
        }

        None
    }

    /// Get the index of a column by name
    pub(super) fn get_column_index(&self, column_name: &str, columns: &[String]) -> Option<usize> {
        columns.iter().position(|col| col == column_name)
    }

    /// Evaluate a predicate expression against a node
    pub(super) fn evaluate_predicate(
        &self,
        node: &Value,
        expr: &parser::Expression,
        context: &ExecutionContext,
    ) -> Result<bool> {
        match expr {
            parser::Expression::BinaryOp { left, op, right } => {
                let left_val = self.evaluate_expression(node, left, context)?;
                let right_val = self.evaluate_expression(node, right, context)?;

                match op {
                    parser::BinaryOperator::Equal => {
                        // In Neo4j, null = null returns null (which evaluates to false in WHERE), and null = anything else returns null
                        if left_val.is_null() || right_val.is_null() {
                            Ok(false) // null comparisons in WHERE clauses evaluate to false
                        } else {
                            // Use numeric comparison for numbers to handle 1.0 == 1
                            let is_equal = self.values_equal_for_comparison(&left_val, &right_val);
                            Ok(is_equal)
                        }
                    }
                    parser::BinaryOperator::NotEqual => {
                        // In Neo4j, null <> null returns null (which evaluates to false in WHERE), and null <> anything else returns null
                        if left_val.is_null() || right_val.is_null() {
                            Ok(false) // null comparisons in WHERE clauses evaluate to false
                        } else {
                            Ok(left_val != right_val)
                        }
                    }
                    parser::BinaryOperator::LessThan => {
                        self.compare_values(&left_val, &right_val, |a, b| a < b)
                    }
                    parser::BinaryOperator::LessThanOrEqual => {
                        self.compare_values(&left_val, &right_val, |a, b| a <= b)
                    }
                    parser::BinaryOperator::GreaterThan => {
                        self.compare_values(&left_val, &right_val, |a, b| a > b)
                    }
                    parser::BinaryOperator::GreaterThanOrEqual => {
                        self.compare_values(&left_val, &right_val, |a, b| a >= b)
                    }
                    parser::BinaryOperator::And => {
                        let left_bool = self.value_to_bool(&left_val)?;
                        let right_bool = self.value_to_bool(&right_val)?;
                        Ok(left_bool && right_bool)
                    }
                    parser::BinaryOperator::Or => {
                        let left_bool = self.value_to_bool(&left_val)?;
                        let right_bool = self.value_to_bool(&right_val)?;
                        Ok(left_bool || right_bool)
                    }
                    parser::BinaryOperator::StartsWith => {
                        let left_str = self.value_to_string(&left_val);
                        let right_str = self.value_to_string(&right_val);
                        Ok(left_str.starts_with(&right_str))
                    }
                    parser::BinaryOperator::EndsWith => {
                        let left_str = self.value_to_string(&left_val);
                        let right_str = self.value_to_string(&right_val);
                        Ok(left_str.ends_with(&right_str))
                    }
                    parser::BinaryOperator::Contains => {
                        let left_str = self.value_to_string(&left_val);
                        let right_str = self.value_to_string(&right_val);
                        Ok(left_str.contains(&right_str))
                    }
                    parser::BinaryOperator::RegexMatch => {
                        let left_str = self.value_to_string(&left_val);
                        let right_str = self.value_to_string(&right_val);
                        // Use regex crate for pattern matching
                        match regex::Regex::new(&right_str) {
                            Ok(re) => Ok(re.is_match(&left_str)),
                            Err(_) => Ok(false), // Invalid regex pattern returns false
                        }
                    }
                    parser::BinaryOperator::In => {
                        // IN operator: left IN right (where right is a list)
                        // Check if left_val is in the right_val list
                        match &right_val {
                            Value::Array(list) => {
                                // Check if left_val is in the list
                                Ok(list.iter().any(|item| item == &left_val))
                            }
                            _ => {
                                // Right side is not a list, return false
                                Ok(false)
                            }
                        }
                    }
                    parser::BinaryOperator::Power => {
                        // Power operator: left ^ right
                        // For predicates, we need to return a boolean
                        // But power is a numeric operation, so we compare result to 0
                        let base = self.value_to_number(&left_val)?;
                        let exp = self.value_to_number(&right_val)?;
                        let result = base.powf(exp);
                        Ok(result != 0.0 && result.is_finite())
                    }
                    _ => Ok(false), // Other operators not implemented
                }
            }
            parser::Expression::UnaryOp { op, operand } => {
                let operand_val = self.evaluate_expression(node, operand, context)?;
                match op {
                    parser::UnaryOperator::Not => {
                        let bool_val = self.value_to_bool(&operand_val)?;
                        Ok(!bool_val)
                    }
                    _ => Ok(false),
                }
            }
            parser::Expression::IsNull { expr, negated } => {
                let value = self.evaluate_expression(node, expr, context)?;
                let is_null = value.is_null();
                Ok(if *negated { !is_null } else { is_null })
            }
            _ => {
                let result = self.evaluate_expression(node, expr, context)?;
                self.value_to_bool(&result)
            }
        }
    }

    /// Evaluate an expression against a node
    pub(super) fn evaluate_expression(
        &self,
        node: &Value,
        expr: &parser::Expression,
        context: &ExecutionContext,
    ) -> Result<Value> {
        match expr {
            parser::Expression::Variable(name) => {
                if let Some(value) = context.get_variable(name) {
                    Ok(value.clone())
                } else {
                    Ok(Value::Null)
                }
            }
            parser::Expression::PropertyAccess { variable, property } => {
                if variable == "n" || variable == "node" {
                    // Access property of the current node
                    if let Value::Object(props) = node {
                        Ok(props.get(property).cloned().unwrap_or(Value::Null))
                    } else {
                        Ok(Value::Null)
                    }
                } else {
                    // Access property of a variable
                    if let Some(Value::Object(props)) = context.get_variable(variable) {
                        Ok(props.get(property).cloned().unwrap_or(Value::Null))
                    } else {
                        Ok(Value::Null)
                    }
                }
            }
            parser::Expression::ArrayIndex { base, index } => {
                // Evaluate the base expression (should return an array)
                let base_value = self.evaluate_expression(node, base, context)?;

                // Evaluate the index expression (should return an integer)
                let index_value = self.evaluate_expression(node, index, context)?;

                // Extract index as i64
                // Handle both integer and float numbers (floats come from unary minus)
                let idx = match index_value {
                    Value::Number(n) => n
                        .as_i64()
                        .or_else(|| n.as_f64().map(|f| f as i64))
                        .unwrap_or(0),
                    _ => return Ok(Value::Null), // Invalid index type
                };

                // Access array element
                match base_value {
                    Value::Array(arr) => {
                        // Handle negative indices (Python-style)
                        let array_len = arr.len() as i64;
                        let actual_idx = if idx < 0 {
                            (array_len + idx) as usize
                        } else {
                            idx as usize
                        };

                        // Return element or null if out of bounds
                        Ok(arr.get(actual_idx).cloned().unwrap_or(Value::Null))
                    }
                    _ => Ok(Value::Null), // Base is not an array
                }
            }
            parser::Expression::ArraySlice { base, start, end } => {
                // Evaluate the base expression (should return an array)
                let base_value = self.evaluate_expression(node, base, context)?;

                match base_value {
                    Value::Array(arr) => {
                        let array_len = arr.len() as i64;

                        // Evaluate start index (default to 0)
                        let start_idx = if let Some(start_expr) = start {
                            let start_val = self.evaluate_expression(node, start_expr, context)?;
                            match start_val {
                                Value::Number(n) => {
                                    // Handle both integer and float numbers (floats come from unary minus)
                                    let idx = n
                                        .as_i64()
                                        .or_else(|| n.as_f64().map(|f| f as i64))
                                        .unwrap_or(0);
                                    // Handle negative indices
                                    if idx < 0 {
                                        ((array_len + idx).max(0)) as usize
                                    } else {
                                        idx.min(array_len) as usize
                                    }
                                }
                                _ => 0,
                            }
                        } else {
                            0
                        };

                        // Evaluate end index (default to array length)
                        let end_idx = if let Some(end_expr) = end {
                            let end_val = self.evaluate_expression(node, end_expr, context)?;
                            match end_val {
                                Value::Number(n) => {
                                    // Handle both integer and float numbers (floats come from unary minus)
                                    let idx = n
                                        .as_i64()
                                        .or_else(|| n.as_f64().map(|f| f as i64))
                                        .unwrap_or(array_len);
                                    // Handle negative indices
                                    // In Cypher, negative end index excludes that many elements from the end
                                    // e.g., [1..-1] means from index 1 to (length - 1), excluding the last element
                                    if idx < 0 {
                                        let calculated = array_len + idx;
                                        // Ensure we don't go below 0, but negative end should exclude elements
                                        if calculated <= 0 {
                                            0
                                        } else {
                                            calculated as usize
                                        }
                                    } else {
                                        idx.min(array_len) as usize
                                    }
                                }
                                _ => arr.len(),
                            }
                        } else {
                            arr.len()
                        };

                        // Return slice (empty if start >= end)
                        if start_idx <= end_idx && start_idx < arr.len() {
                            let slice = arr[start_idx..end_idx.min(arr.len())].to_vec();
                            Ok(Value::Array(slice))
                        } else {
                            Ok(Value::Array(Vec::new()))
                        }
                    }
                    _ => Ok(Value::Null), // Base is not an array
                }
            }
            parser::Expression::Literal(literal) => match literal {
                parser::Literal::String(s) => Ok(Value::String(s.clone())),
                parser::Literal::Integer(i) => Ok(Value::Number((*i).into())),
                parser::Literal::Float(f) => Ok(Value::Number(
                    serde_json::Number::from_f64(*f).unwrap_or(serde_json::Number::from(0)),
                )),
                parser::Literal::Boolean(b) => Ok(Value::Bool(*b)),
                parser::Literal::Null => Ok(Value::Null),
                parser::Literal::Point(p) => Ok(p.to_json_value()),
            },
            parser::Expression::Parameter(name) => {
                if let Some(value) = context.params.get(name) {
                    Ok(value.clone())
                } else {
                    Ok(Value::Null)
                }
            }
            parser::Expression::BinaryOp { left, op, right } => {
                let left_val = self.evaluate_expression(node, left, context)?;
                let right_val = self.evaluate_expression(node, right, context)?;

                match op {
                    parser::BinaryOperator::And => {
                        let left_bool = self.value_to_bool(&left_val)?;
                        let right_bool = self.value_to_bool(&right_val)?;
                        Ok(Value::Bool(left_bool && right_bool))
                    }
                    parser::BinaryOperator::Or => {
                        let left_bool = self.value_to_bool(&left_val)?;
                        let right_bool = self.value_to_bool(&right_val)?;
                        Ok(Value::Bool(left_bool || right_bool))
                    }
                    parser::BinaryOperator::Equal => {
                        if left_val.is_null() || right_val.is_null() {
                            Ok(Value::Null)
                        } else {
                            Ok(Value::Bool(left_val == right_val))
                        }
                    }
                    parser::BinaryOperator::NotEqual => {
                        if left_val.is_null() || right_val.is_null() {
                            Ok(Value::Null)
                        } else {
                            Ok(Value::Bool(left_val != right_val))
                        }
                    }
                    parser::BinaryOperator::LessThan => Ok(Value::Bool(
                        self.compare_values_for_sort(&left_val, &right_val)
                            == std::cmp::Ordering::Less,
                    )),
                    parser::BinaryOperator::LessThanOrEqual => Ok(Value::Bool(matches!(
                        self.compare_values_for_sort(&left_val, &right_val),
                        std::cmp::Ordering::Less | std::cmp::Ordering::Equal
                    ))),
                    parser::BinaryOperator::GreaterThan => Ok(Value::Bool(
                        self.compare_values_for_sort(&left_val, &right_val)
                            == std::cmp::Ordering::Greater,
                    )),
                    parser::BinaryOperator::GreaterThanOrEqual => Ok(Value::Bool(matches!(
                        self.compare_values_for_sort(&left_val, &right_val),
                        std::cmp::Ordering::Greater | std::cmp::Ordering::Equal
                    ))),
                    parser::BinaryOperator::Add => self.add_values(&left_val, &right_val),
                    parser::BinaryOperator::Subtract => self.subtract_values(&left_val, &right_val),
                    parser::BinaryOperator::Multiply => self.multiply_values(&left_val, &right_val),
                    parser::BinaryOperator::Divide => self.divide_values(&left_val, &right_val),
                    parser::BinaryOperator::Modulo => self.modulo_values(&left_val, &right_val),
                    parser::BinaryOperator::Power => self.power_values(&left_val, &right_val),
                    _ => Ok(Value::Null), // Other operators not implemented in evaluate_expression
                }
            }
            parser::Expression::Case {
                input,
                when_clauses,
                else_clause,
            } => {
                // Evaluate input expression if present (generic CASE)
                let input_value = if let Some(input_expr) = input {
                    Some(self.evaluate_expression(node, input_expr, context)?)
                } else {
                    None
                };

                // Evaluate WHEN clauses
                for when_clause in when_clauses {
                    let condition_value =
                        self.evaluate_expression(node, &when_clause.condition, context)?;

                    // For generic CASE: compare input with condition
                    // For simple CASE: evaluate condition as boolean
                    let matches = if let Some(ref input_val) = input_value {
                        // Generic CASE: input == condition
                        input_val == &condition_value
                    } else {
                        // Simple CASE: condition is boolean expression
                        self.value_to_bool(&condition_value)?
                    };

                    if matches {
                        return self.evaluate_expression(node, &when_clause.result, context);
                    }
                }

                // No WHEN clause matched, return ELSE or NULL
                if let Some(else_expr) = else_clause {
                    self.evaluate_expression(node, else_expr, context)
                } else {
                    Ok(Value::Null)
                }
            }
            _ => Ok(Value::Null), // Other expressions not implemented in MVP
        }
    }

    /// Compare two values for equality, handling numeric type differences (1.0 == 1)
    pub(super) fn values_equal_for_comparison(&self, left: &Value, right: &Value) -> bool {
        match (left, right) {
            (Value::Number(a), Value::Number(b)) => {
                // Compare numbers (handle int/float conversion)
                if let (Some(a_i64), Some(b_i64)) = (a.as_i64(), b.as_i64()) {
                    a_i64 == b_i64
                } else if let (Some(a_f64), Some(b_f64)) = (a.as_f64(), b.as_f64()) {
                    (a_f64 - b_f64).abs() < f64::EPSILON * 10.0
                } else {
                    false
                }
            }
            (Value::String(a), Value::String(b)) => {
                // String comparison - exact match
                a == b
            }
            (Value::String(a), Value::Number(b)) => {
                // Try to parse string as number for comparison
                if let Ok(parsed) = a.parse::<f64>() {
                    if let Some(b_f64) = b.as_f64() {
                        (parsed - b_f64).abs() < f64::EPSILON * 10.0
                    } else if let Some(b_i64) = b.as_i64() {
                        (parsed - b_i64 as f64).abs() < f64::EPSILON * 10.0
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            (Value::Number(a), Value::String(b)) => {
                // Try to parse string as number for comparison
                if let Ok(parsed) = b.parse::<f64>() {
                    if let Some(a_f64) = a.as_f64() {
                        (parsed - a_f64).abs() < f64::EPSILON * 10.0
                    } else if let Some(a_i64) = a.as_i64() {
                        (parsed - a_i64 as f64).abs() < f64::EPSILON * 10.0
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            _ => left == right,
        }
    }

    /// Compare two values using a comparison function
    pub(super) fn compare_values<F>(
        &self,
        left: &Value,
        right: &Value,
        compare_fn: F,
    ) -> Result<bool>
    where
        F: FnOnce(f64, f64) -> bool,
    {
        let left_num = self.value_to_number(left)?;
        let right_num = self.value_to_number(right)?;
        Ok(compare_fn(left_num, right_num))
    }

    /// Convert a value to a number
    pub(super) fn value_to_number(&self, value: &Value) -> Result<f64> {
        match value {
            Value::Number(n) => n.as_f64().ok_or_else(|| Error::TypeMismatch {
                expected: "number".to_string(),
                actual: "invalid number".to_string(),
            }),
            Value::String(s) => s.parse::<f64>().map_err(|_| Error::TypeMismatch {
                expected: "number".to_string(),
                actual: "string".to_string(),
            }),
            Value::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
            Value::Null => Err(Error::TypeMismatch {
                expected: "number".to_string(),
                actual: "null".to_string(),
            }),
            _ => Err(Error::TypeMismatch {
                expected: "number".to_string(),
                actual: "unknown type".to_string(),
            }),
        }
    }

    /// Convert a value to a boolean
    pub(super) fn value_to_bool(&self, value: &Value) -> Result<bool> {
        match value {
            Value::Bool(b) => Ok(*b),
            Value::Number(n) => Ok(n.as_f64().unwrap_or(0.0) != 0.0),
            Value::String(s) => Ok(!s.is_empty()),
            Value::Null => Ok(false),
            Value::Array(arr) => Ok(!arr.is_empty()),
            Value::Object(obj) => Ok(!obj.is_empty()),
        }
    }

    /// Find relationships for a node

    /// Get a column value from a node for sorting
    pub(super) fn get_column_value(&self, node: &Value, column: &str) -> Value {
        if let Value::Object(props) = node {
            if let Some(value) = props.get(column) {
                value.clone()
            } else {
                // Try to access as property access (e.g., "n.name")
                if let Some(dot_pos) = column.find('.') {
                    let var_name = &column[..dot_pos];
                    let prop_name = &column[dot_pos + 1..];

                    if let Some(Value::Object(var_props)) = props.get(var_name) {
                        if let Some(prop_value) = var_props.get(prop_name) {
                            return prop_value.clone();
                        }
                    }
                }
                Value::Null
            }
        } else {
            Value::Null
        }
    }

    /// Compare values for sorting
    pub(super) fn compare_values_for_sort(&self, a: &Value, b: &Value) -> std::cmp::Ordering {
        match (a, b) {
            (Value::Null, Value::Null) => std::cmp::Ordering::Equal,
            (Value::Null, _) => std::cmp::Ordering::Less,
            (_, Value::Null) => std::cmp::Ordering::Greater,
            (Value::Number(a_num), Value::Number(b_num)) => {
                let a_f64 = a_num.as_f64().unwrap_or(0.0);
                let b_f64 = b_num.as_f64().unwrap_or(0.0);
                a_f64
                    .partial_cmp(&b_f64)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }
            (Value::String(a_str), Value::String(b_str)) => a_str.cmp(b_str),
            (Value::Bool(a_bool), Value::Bool(b_bool)) => a_bool.cmp(b_bool),
            (Value::Array(a_arr), Value::Array(b_arr)) => match a_arr.len().cmp(&b_arr.len()) {
                std::cmp::Ordering::Equal => {
                    for (a_item, b_item) in a_arr.iter().zip(b_arr.iter()) {
                        let comparison = self.compare_values_for_sort(a_item, b_item);
                        if comparison != std::cmp::Ordering::Equal {
                            return comparison;
                        }
                    }
                    std::cmp::Ordering::Equal
                }
                other => other,
            },
            _ => {
                // Convert to strings for comparison
                let a_str = self.value_to_string(a);
                let b_str = self.value_to_string(b);
                a_str.cmp(&b_str)
            }
        }
    }

    /// Convert a value to string for comparison
    pub(super) fn value_to_string(&self, value: &Value) -> String {
        match value {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Null => "null".to_string(),
            Value::Array(arr) => format!("[{}]", arr.len()),
            Value::Object(obj) => format!("{{{}}}", obj.len()),
        }
    }
    /// Execute UNWIND operator - expands a list into rows
    pub(super) fn execute_unwind(
        &self,
        context: &mut ExecutionContext,
        expression: &str,
        variable: &str,
    ) -> Result<()> {
        // Materialize rows from variables if needed (like execute_distinct does)
        if context.result_set.rows.is_empty() && !context.variables.is_empty() {
            let rows = self.materialize_rows_from_variables(context);
            self.update_result_set_from_rows(context, &rows);
        }

        // Parse the expression string
        let mut parser_instance = parser::CypherParser::new(expression.to_string());
        let parsed_expr = parser_instance.parse_expression().map_err(|e| {
            Error::CypherSyntax(format!("Failed to parse UNWIND expression: {}", e))
        })?;

        // If no existing rows, evaluate expression once and create new rows
        if context.result_set.rows.is_empty() {
            // Evaluate expression with empty row context
            let empty_row = HashMap::new();
            let list_value =
                self.evaluate_projection_expression(&empty_row, context, &parsed_expr)?;

            // Convert to array if needed
            let list_items = match list_value {
                Value::Array(items) => items,
                Value::Null => Vec::new(), // NULL list produces no rows
                other => vec![other],      // Single value wraps into single-item list
            };

            // Add variable as column
            context.result_set.columns.push(variable.to_string());

            // Create one row per list item
            for item in list_items {
                let row = Row { values: vec![item] };
                context.result_set.rows.push(row);
            }
        } else {
            // Expand existing rows: for each existing row, evaluate expression and create N new rows
            let existing_rows = std::mem::take(&mut context.result_set.rows);
            let existing_columns = context.result_set.columns.clone();

            // Find or add variable column index
            let var_col_idx = if let Some(idx) = self.get_column_index(variable, &existing_columns)
            {
                idx
            } else {
                // Add new column
                context.result_set.columns.push(variable.to_string());
                existing_columns.len()
            };

            // For each existing row, evaluate expression and create new rows with each list item
            for existing_row in existing_rows.iter() {
                // Convert Row to HashMap for evaluation
                let row_map = self.row_to_map(existing_row, &existing_columns);

                // Evaluate expression in context of this row
                let list_value =
                    self.evaluate_projection_expression(&row_map, context, &parsed_expr)?;

                // Convert to array if needed
                let list_items = match list_value {
                    Value::Array(items) => items,
                    Value::Null => Vec::new(), // NULL list produces no rows
                    other => vec![other],      // Single value wraps into single-item list
                };

                if list_items.is_empty() {
                    // Empty list produces no rows (Cartesian product with empty set)
                    continue;
                }

                for item in &list_items {
                    let mut new_values = existing_row.values.clone();

                    // If var_col_idx equals existing length, append; otherwise replace
                    if var_col_idx >= new_values.len() {
                        new_values.resize(var_col_idx + 1, Value::Null);
                    }
                    new_values[var_col_idx] = item.clone();

                    let new_row = Row { values: new_values };
                    context.result_set.rows.push(new_row);
                }
            }
        }

        Ok(())
    }

    /// Convert Row to HashMap for expression evaluation
    pub(super) fn row_to_map(&self, row: &Row, columns: &[String]) -> HashMap<String, Value> {
        let mut map = HashMap::new();
        for (idx, col_name) in columns.iter().enumerate() {
            if let Some(value) = row.values.get(idx) {
                map.insert(col_name.clone(), value.clone());
            }
        }
        map
    }

    /// Execute new index scan operation
    pub(super) fn execute_index_scan_new(
        &self,
        context: &mut ExecutionContext,
        _index_name: &str,
        label: &str,
    ) -> Result<()> {
        // Get label ID from catalog
        let label_id = self.catalog().get_or_create_label(label)?;

        // Execute node by label scan
        let nodes = self.execute_node_by_label(label_id)?;
        context.set_variable("n", Value::Array(nodes));

        Ok(())
    }

    /// Execute LOAD CSV operator
    pub(super) fn execute_load_csv(
        &self,
        context: &mut ExecutionContext,
        url: &str,
        variable: &str,
        with_headers: bool,
        field_terminator: Option<&str>,
    ) -> Result<()> {
        use std::fs;
        use std::io::{BufRead, BufReader};

        // Extract file path from URL (file:///path/to/file.csv or file://path/to/file.csv)
        // Handle both absolute paths (file:///C:/path) and relative paths (file://path)
        // Also handle Windows paths with backslashes
        // Note: file:/// means absolute path (preserve leading slash), file:// means relative path
        let file_path_str = if url.starts_with("file:///") {
            // Absolute path: file:///path -> /path (preserve leading slash)
            let path = &url[7..];
            // On Windows, if path starts with /C:/, remove the leading / to get C:/
            // This handles file:///C:/path correctly
            #[cfg(windows)]
            {
                if path.len() >= 3
                    && path.chars().nth(0) == Some('/')
                    && path.chars().nth(1).map(|c| c.is_ascii_alphabetic()) == Some(true)
                    && path.chars().nth(2) == Some(':')
                {
                    &path[1..]
                } else {
                    path
                }
            }
            #[cfg(not(windows))]
            {
                path
            }
        } else if let Some(stripped) = url.strip_prefix("file://") {
            // Relative path: file://path -> path
            stripped
        } else {
            url
        };

        // Convert to PathBuf to handle path resolution properly
        use std::path::PathBuf;
        let path_buf = PathBuf::from(file_path_str);

        // Try to resolve the path - if it's relative or doesn't exist, try to find it
        let file_path = if path_buf.exists() {
            // Path exists, canonicalize it
            path_buf.canonicalize().unwrap_or(path_buf)
        } else if path_buf.is_relative() {
            // Relative path - try to resolve relative to current directory
            std::env::current_dir()
                .ok()
                .and_then(|cwd| {
                    let joined = cwd.join(&path_buf);
                    if joined.exists() {
                        joined.canonicalize().ok()
                    } else {
                        None
                    }
                })
                .unwrap_or(path_buf)
        } else {
            // Absolute path that doesn't exist - use as-is (will fail with proper error)
            path_buf
        };

        // Read CSV file
        let file = fs::File::open(&file_path).map_err(|e| {
            Error::Internal(format!(
                "Failed to open CSV file '{}': {}",
                file_path.display(),
                e
            ))
        })?;
        let reader = BufReader::new(file);
        let terminator = field_terminator.unwrap_or(",");
        let mut lines = reader.lines();

        // Skip header if WITH HEADERS
        let headers = if with_headers {
            if let Some(Ok(header_line)) = lines.next() {
                header_line
                    .split(terminator)
                    .map(|s| s.trim().to_string())
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        // Parse CSV rows
        let mut rows = Vec::new();
        for line_result in lines {
            let line = line_result
                .map_err(|e| Error::Internal(format!("Failed to read CSV line: {}", e)))?;

            if line.trim().is_empty() {
                continue; // Skip empty lines
            }

            let fields: Vec<String> = line
                .split(terminator)
                .map(|s| s.trim().to_string())
                .collect();

            // Convert to Value based on whether we have headers
            let row_value = if with_headers && !headers.is_empty() {
                // Create a map with header keys
                let mut row_map = serde_json::Map::new();
                for (i, header) in headers.iter().enumerate() {
                    let field_value = if i < fields.len() {
                        Value::String(fields[i].clone())
                    } else {
                        Value::Null
                    };
                    row_map.insert(header.clone(), field_value);
                }
                Value::Object(row_map)
            } else {
                // Create an array of field values
                let field_values: Vec<Value> = fields.into_iter().map(Value::String).collect();
                Value::Array(field_values)
            };

            rows.push(row_value);
        }

        // Store rows in result_set
        context.result_set.rows.clear();
        context.result_set.columns = vec![variable.to_string()];

        for row_value in rows {
            context.result_set.rows.push(Row {
                values: vec![row_value],
            });
        }

        // Also update variables for compatibility
        if !context.result_set.rows.is_empty() {
            let row_maps = self.result_set_as_rows(context);
            self.update_variables_from_rows(context, &row_maps);
        }

        Ok(())
    }

    /// Execute CALL procedure operator
    pub(super) fn execute_call_procedure(
        &self,
        context: &mut ExecutionContext,
        procedure_name: &str,
        arguments: &[parser::Expression],
        yield_columns: Option<&Vec<String>>,
    ) -> Result<()> {
        // Handle built-in db.* procedures that don't need Graph
        match procedure_name {
            "db.labels" => {
                return self.execute_db_labels_procedure(context, yield_columns);
            }
            "db.propertyKeys" => {
                return self.execute_db_property_keys_procedure(context, yield_columns);
            }
            "db.relationshipTypes" => {
                return self.execute_db_relationship_types_procedure(context, yield_columns);
            }
            "db.schema" => {
                return self.execute_db_schema_procedure(context, yield_columns);
            }
            _ => {}
        }

        // Get procedure registry (for now, create a new one - in full implementation would be shared)
        let registry = ProcedureRegistry::new();

        // Find procedure
        let procedure = registry.get(procedure_name).ok_or_else(|| {
            Error::CypherSyntax(format!("Procedure '{}' not found", procedure_name))
        })?;

        // Evaluate arguments
        let mut args_map = HashMap::new();
        for arg_expr in arguments {
            // Evaluate argument expression
            // For now, we'll use a simple evaluation - in a full implementation,
            // we'd need to evaluate expressions in the context of current rows
            let arg_value = self.evaluate_expression_in_context(context, arg_expr)?;
            // Use the expression string representation as key (simplified)
            args_map.insert("arg".to_string(), arg_value);
        }

        // Convert args_map to the format expected by procedures (HashMap<String, Value>)
        // For now, we'll create a simple graph from the current engine state
        // In a full implementation, we'd convert the entire graph from Engine
        let graph = Graph::new(); // Empty graph for now - full implementation would convert from Engine

        // Check if procedure supports streaming and use it for better memory efficiency
        let use_streaming = procedure.supports_streaming();

        if use_streaming {
            // Use streaming execution for better memory efficiency
            use std::sync::{Arc, Mutex};

            let rows = Arc::new(Mutex::new(Vec::new()));
            let columns = Arc::new(Mutex::new(Option::<Vec<String>>::None));

            let rows_clone = rows.clone();
            let columns_clone = columns.clone();

            procedure.execute_streaming(
                &graph,
                &args_map,
                Box::new(move |cols, row| {
                    // Store columns on first call
                    {
                        let mut cols_ref = columns_clone.lock().unwrap();
                        if cols_ref.is_none() {
                            *cols_ref = Some(cols.to_vec());
                        }
                    }

                    // Convert row to Row format
                    rows_clone.lock().unwrap().push(Row {
                        values: row.to_vec(),
                    });

                    Ok(())
                }),
            )?;

            let final_columns = columns.lock().unwrap().clone().ok_or_else(|| {
                Error::CypherSyntax("No columns returned from procedure".to_string())
            })?;

            // Filter columns based on YIELD clause if specified
            let filtered_columns = if let Some(yield_cols) = yield_columns {
                let mut filtered = Vec::new();
                for col in yield_cols {
                    if final_columns.iter().any(|c| c == col) {
                        filtered.push(col.clone());
                    }
                }
                filtered
            } else {
                final_columns
            };

            let final_rows = rows.lock().unwrap().clone();
            context.set_columns_and_rows(filtered_columns, final_rows);
        } else {
            // Use standard execution (collect all results first)
            let procedure_result = procedure
                .execute(&graph, &args_map)
                .map_err(|e| Error::CypherSyntax(format!("Procedure execution failed: {}", e)))?;

            // Convert procedure result to rows
            let mut rows = Vec::new();
            for procedure_row in &procedure_result.rows {
                rows.push(Row {
                    values: procedure_row.clone(),
                });
            }

            // Set columns and rows in context
            let columns = if let Some(yield_cols) = yield_columns {
                // Filter columns based on YIELD clause
                let mut filtered_columns = Vec::new();
                for col in yield_cols {
                    if procedure_result.columns.iter().any(|c| c == col) {
                        filtered_columns.push(col.clone());
                    }
                }
                filtered_columns
            } else {
                // Use all columns from procedure result
                procedure_result.columns.clone()
            };

            context.set_columns_and_rows(columns, rows);
        }

        Ok(())
    }

    /// Execute db.labels() procedure
    pub(super) fn execute_db_labels_procedure(
        &self,
        context: &mut ExecutionContext,
        yield_columns: Option<&Vec<String>>,
    ) -> Result<()> {
        // Get all labels from catalog - iterate through all label IDs
        // We'll scan from 0 to a reasonable max (or use stats)
        let mut labels = Vec::new();

        // Try to get labels by iterating through possible IDs
        // This is a workaround - ideally Catalog would have list_all_labels()
        for label_id in 0..10000u32 {
            if let Ok(Some(label_name)) = self.catalog().get_label_name(label_id) {
                labels.push(label_name);
            }
        }

        // Convert to rows
        let mut rows = Vec::new();
        for label in labels {
            rows.push(Row {
                values: vec![serde_json::Value::String(label)],
            });
        }

        // Set columns based on YIELD clause
        let columns = if let Some(yield_cols) = yield_columns {
            // Use YIELD columns if specified
            yield_cols.clone()
        } else {
            // Default column name
            vec!["label".to_string()]
        };

        context.set_columns_and_rows(columns, rows);
        Ok(())
    }

    /// Execute db.propertyKeys() procedure
    pub(super) fn execute_db_property_keys_procedure(
        &self,
        context: &mut ExecutionContext,
        yield_columns: Option<&Vec<String>>,
    ) -> Result<()> {
        // Get all property keys from catalog using public method
        let property_keys: Vec<String> = self
            .catalog()
            .list_all_keys()
            .into_iter()
            .map(|(_, name)| name)
            .collect();

        // Convert to rows
        let mut rows = Vec::new();
        for key in property_keys {
            rows.push(Row {
                values: vec![serde_json::Value::String(key)],
            });
        }

        // Set columns based on YIELD clause
        let columns = if let Some(yield_cols) = yield_columns {
            yield_cols.clone()
        } else {
            vec!["propertyKey".to_string()]
        };

        context.set_columns_and_rows(columns, rows);
        Ok(())
    }

    /// Execute db.relationshipTypes() procedure
    pub(super) fn execute_db_relationship_types_procedure(
        &self,
        context: &mut ExecutionContext,
        yield_columns: Option<&Vec<String>>,
    ) -> Result<()> {
        // Get all relationship types from catalog - iterate through possible IDs
        let mut rel_types = Vec::new();

        // Try to get types by iterating through possible IDs
        for type_id in 0..10000u32 {
            if let Ok(Some(type_name)) = self.catalog().get_type_name(type_id) {
                rel_types.push(type_name);
            }
        }

        // Convert to rows
        let mut rows = Vec::new();
        for rel_type in rel_types {
            rows.push(Row {
                values: vec![serde_json::Value::String(rel_type)],
            });
        }

        // Set columns based on YIELD clause
        let columns = if let Some(yield_cols) = yield_columns {
            yield_cols.clone()
        } else {
            vec!["relationshipType".to_string()]
        };

        context.set_columns_and_rows(columns, rows);
        Ok(())
    }

    /// Execute db.schema() procedure
    pub(super) fn execute_db_schema_procedure(
        &self,
        context: &mut ExecutionContext,
        yield_columns: Option<&Vec<String>>,
    ) -> Result<()> {
        // Get all labels and relationship types from catalog
        let mut labels = Vec::new();
        for label_id in 0..10000u32 {
            if let Ok(Some(label_name)) = self.catalog().get_label_name(label_id) {
                labels.push(label_name);
            }
        }

        let mut rel_types = Vec::new();
        for type_id in 0..10000u32 {
            if let Ok(Some(type_name)) = self.catalog().get_type_name(type_id) {
                rel_types.push(type_name);
            }
        }

        // Convert to JSON arrays
        let nodes_array: Vec<serde_json::Value> = labels
            .into_iter()
            .map(|l| serde_json::json!({"name": l}))
            .collect();
        let relationships_array: Vec<serde_json::Value> = rel_types
            .into_iter()
            .map(|t| serde_json::json!({"name": t}))
            .collect();

        // Create result row
        let rows = vec![Row {
            values: vec![
                serde_json::Value::Array(nodes_array),
                serde_json::Value::Array(relationships_array),
            ],
        }];

        // Set columns based on YIELD clause
        let columns = if let Some(yield_cols) = yield_columns {
            yield_cols.clone()
        } else {
            vec!["nodes".to_string(), "relationships".to_string()]
        };

        context.set_columns_and_rows(columns, rows);
        Ok(())
    }

    /// Execute CREATE INDEX command
    pub fn execute_create_index(
        &self,
        label: &str,
        property: &str,
        index_type: Option<&str>,
        if_not_exists: bool,
        or_replace: bool,
    ) -> Result<()> {
        let index_key = format!("{}.{}", label, property);

        // Check if index already exists
        let indexes = self.shared.spatial_indexes.read();
        let exists = indexes.contains_key(&index_key);
        drop(indexes);

        if exists {
            if if_not_exists {
                // Index exists and IF NOT EXISTS was specified - do nothing
                return Ok(());
            } else if !or_replace {
                return Err(Error::CypherExecution(format!(
                    "Index on :{}({}) already exists",
                    label, property
                )));
            }
            // OR REPLACE - will be handled by creating new index below
        }

        // Create the appropriate index type
        match index_type {
            Some("spatial") => {
                // Create spatial index (R-tree)
                let mut indexes = self.shared.spatial_indexes.write();
                if or_replace && exists {
                    // Replace existing index
                    indexes.remove(&index_key);
                }
                indexes.insert(index_key, SpatialIndex::new());
            }
            None | Some("property") => {
                // Property index - for now, just register in catalog
                // In a full implementation, this would create a B-tree index
                // For MVP, we'll just track that the index exists
                let _label_id = self.catalog().get_or_create_label(label)?;
                let _key_id = self.catalog().get_or_create_key(property)?;
                // Index is registered - actual indexing would happen during inserts
            }
            _ => {
                return Err(Error::CypherExecution(format!(
                    "Unknown index type: {}",
                    index_type.unwrap_or("unknown")
                )));
            }
        }

        Ok(())
    }

    /// Execute SHOW DATABASES command
    pub(super) fn execute_show_databases(&self) -> Result<ResultSet> {
        if let Some(db_manager_arc) = self.shared.database_manager() {
            let db_manager = db_manager_arc.read();
            let databases = db_manager.list_databases();
            let default_db = db_manager.default_database_name();

            // Neo4j-compatible columns
            let columns = vec![
                "name".to_string(),
                "type".to_string(),
                "aliases".to_string(),
                "access".to_string(),
                "address".to_string(),
                "role".to_string(),
                "writer".to_string(),
                "requestedStatus".to_string(),
                "currentStatus".to_string(),
                "statusMessage".to_string(),
                "default".to_string(),
                "home".to_string(),
                "constituents".to_string(),
            ];

            let rows: Vec<Row> = databases
                .iter()
                .map(|db| {
                    let is_default = db.name == default_db;
                    Row {
                        values: vec![
                            Value::String(db.name.clone()),
                            Value::String("standard".to_string()),
                            Value::Array(vec![]),
                            Value::String("read-write".to_string()),
                            Value::String("localhost:7687".to_string()),
                            Value::String("primary".to_string()),
                            Value::Bool(true),
                            Value::String("online".to_string()),
                            Value::String("online".to_string()),
                            Value::String("".to_string()),
                            Value::Bool(is_default),
                            Value::Bool(is_default),
                            Value::Array(vec![]),
                        ],
                    }
                })
                .collect();

            Ok(ResultSet { columns, rows })
        } else {
            // No database manager - return single default database
            let columns = vec![
                "name".to_string(),
                "type".to_string(),
                "aliases".to_string(),
                "access".to_string(),
                "address".to_string(),
                "role".to_string(),
                "writer".to_string(),
                "requestedStatus".to_string(),
                "currentStatus".to_string(),
                "statusMessage".to_string(),
                "default".to_string(),
                "home".to_string(),
                "constituents".to_string(),
            ];

            let rows = vec![Row {
                values: vec![
                    Value::String("neo4j".to_string()),
                    Value::String("standard".to_string()),
                    Value::Array(vec![]),
                    Value::String("read-write".to_string()),
                    Value::String("localhost:7687".to_string()),
                    Value::String("primary".to_string()),
                    Value::Bool(true),
                    Value::String("online".to_string()),
                    Value::String("online".to_string()),
                    Value::String("".to_string()),
                    Value::Bool(true),
                    Value::Bool(true),
                    Value::Array(vec![]),
                ],
            }];

            Ok(ResultSet { columns, rows })
        }
    }

    /// Execute CREATE DATABASE command
    pub(super) fn execute_create_database(
        &self,
        name: &str,
        if_not_exists: bool,
    ) -> Result<ResultSet> {
        if let Some(db_manager_arc) = self.shared.database_manager() {
            let db_manager = db_manager_arc.read();
            // Check if database already exists
            if db_manager.exists(name) {
                if if_not_exists {
                    // Return success without creating
                    return Ok(ResultSet {
                        columns: vec!["result".to_string()],
                        rows: vec![Row {
                            values: vec![Value::String(format!(
                                "Database '{}' already exists",
                                name
                            ))],
                        }],
                    });
                } else {
                    return Err(Error::CypherExecution(format!(
                        "Database '{}' already exists",
                        name
                    )));
                }
            }

            // Create the database
            db_manager.create_database(name)?;

            Ok(ResultSet {
                columns: vec!["result".to_string()],
                rows: vec![Row {
                    values: vec![Value::String(format!(
                        "Database '{}' created successfully",
                        name
                    ))],
                }],
            })
        } else {
            Err(Error::CypherExecution(
                "Multi-database support is not enabled. DatabaseManager not configured."
                    .to_string(),
            ))
        }
    }

    /// Execute DROP DATABASE command
    pub(super) fn execute_drop_database(&self, name: &str, if_exists: bool) -> Result<ResultSet> {
        if let Some(db_manager_arc) = self.shared.database_manager() {
            let db_manager = db_manager_arc.read();
            // Check if trying to drop default database
            if name == db_manager.default_database_name() {
                return Err(Error::CypherExecution(
                    "Cannot drop the default database".to_string(),
                ));
            }

            // Check if database exists
            if !db_manager.exists(name) {
                if if_exists {
                    // Return success without error
                    return Ok(ResultSet {
                        columns: vec!["result".to_string()],
                        rows: vec![Row {
                            values: vec![Value::String(format!(
                                "Database '{}' does not exist",
                                name
                            ))],
                        }],
                    });
                } else {
                    return Err(Error::CypherExecution(format!(
                        "Database '{}' does not exist",
                        name
                    )));
                }
            }

            // Drop the database
            db_manager.drop_database(name, if_exists)?;

            Ok(ResultSet {
                columns: vec!["result".to_string()],
                rows: vec![Row {
                    values: vec![Value::String(format!(
                        "Database '{}' dropped successfully",
                        name
                    ))],
                }],
            })
        } else {
            Err(Error::CypherExecution(
                "Multi-database support is not enabled. DatabaseManager not configured."
                    .to_string(),
            ))
        }
    }

    pub(super) fn execute_alter_database(
        &self,
        name: &str,
        read_only: Option<bool>,
        option: Option<(String, String)>,
    ) -> Result<ResultSet> {
        if let Some(db_manager_arc) = self.shared.database_manager() {
            let db_manager = db_manager_arc.read();
            // Check if database exists
            if !db_manager.exists(name) {
                return Err(Error::CypherExecution(format!(
                    "Database '{}' does not exist",
                    name
                )));
            }

            let alteration_msg = if let Some(read_only) = read_only {
                if read_only {
                    format!("Database '{}' set to READ ONLY", name)
                } else {
                    format!("Database '{}' set to READ WRITE", name)
                }
            } else if let Some((key, value)) = option {
                format!("Database '{}' option '{}' set to '{}'", name, key, value)
            } else {
                format!("Database '{}' altered successfully", name)
            };

            Ok(ResultSet {
                columns: vec!["result".to_string()],
                rows: vec![Row {
                    values: vec![Value::String(alteration_msg)],
                }],
            })
        } else {
            Err(Error::CypherExecution(
                "Multi-database support is not enabled. DatabaseManager not configured."
                    .to_string(),
            ))
        }
    }

    pub(super) fn execute_use_database(&self, name: &str) -> Result<ResultSet> {
        if let Some(db_manager_arc) = self.shared.database_manager() {
            let db_manager = db_manager_arc.read();
            // Check if database exists
            if !db_manager.exists(name) {
                return Err(Error::CypherExecution(format!(
                    "Database '{}' does not exist",
                    name
                )));
            }

            // Note: In a real implementation, this would switch the session's current database
            // For now, we just return success
            Ok(ResultSet {
                columns: vec!["result".to_string()],
                rows: vec![Row {
                    values: vec![Value::String(format!("Switched to database '{}'", name))],
                }],
            })
        } else {
            Err(Error::CypherExecution(
                "Multi-database support is not enabled. DatabaseManager not configured."
                    .to_string(),
            ))
        }
    }

    /// Evaluate an expression in the current context
    pub(super) fn evaluate_expression_in_context(
        &self,
        context: &ExecutionContext,
        expr: &parser::Expression,
    ) -> Result<Value> {
        // Simple evaluation - for literals and variables
        match expr {
            parser::Expression::Literal(parser::Literal::String(s)) => Ok(Value::String(s.clone())),
            parser::Expression::Literal(parser::Literal::Integer(i)) => {
                Ok(Value::Number((*i).into()))
            }
            parser::Expression::Literal(parser::Literal::Float(f)) => Ok(Value::Number(
                serde_json::Number::from_f64(*f).unwrap_or_else(|| 0.into()),
            )),
            parser::Expression::Literal(parser::Literal::Boolean(b)) => Ok(Value::Bool(*b)),
            parser::Expression::Literal(parser::Literal::Null) => Ok(Value::Null),
            parser::Expression::Literal(parser::Literal::Point(p)) => Ok(p.to_json_value()),
            parser::Expression::Variable(var) => context
                .get_variable(var)
                .cloned()
                .ok_or_else(|| Error::CypherSyntax(format!("Variable '{}' not found", var))),
            _ => Err(Error::CypherSyntax(
                "Complex expressions in procedure arguments not yet supported".to_string(),
            )),
        }
    }

    /// Apply Cartesian product of new values with existing variables in context
    /// This expands all existing array variables by repeating each element M times (where M is new_values.len())
    /// and creates the new variable by repeating the whole sequence N times (where N is existing row count).
    pub(super) fn apply_cartesian_product(
        &self,
        context: &mut ExecutionContext,
        new_var: &str,
        new_values: Vec<Value>,
    ) -> Result<()> {
        // 1. Determine current row count (N)
        // Find the length of the first array variable
        let current_count = context
            .variables
            .values()
            .filter_map(|v| {
                if let Value::Array(arr) = v {
                    Some(arr.len())
                } else {
                    None
                }
            })
            .max() // Use max just in case, though they should be equal
            .unwrap_or(0);

        if current_count == 0 {
            // No existing rows (or only scalars), just set the new variable
            context.set_variable(new_var, Value::Array(new_values));
            return Ok(());
        }

        let new_count = new_values.len();
        if new_count == 0 {
            // New set is empty -> Cartesian product is empty
            // Clear all variables to empty arrays
            for val in context.variables.values_mut() {
                *val = Value::Array(Vec::new());
            }
            context.set_variable(new_var, Value::Array(Vec::new()));
            return Ok(());
        }

        // 2. Expand existing variables: repeat each element M times (M = new_count)
        // We need to collect keys first to avoid borrowing issues
        let keys: Vec<String> = context.variables.keys().cloned().collect();

        for key in keys {
            if let Some(val) = context.variables.get_mut(&key) {
                if let Value::Array(arr) = val {
                    let mut new_arr = Vec::with_capacity(arr.len() * new_count);
                    for item in arr.iter() {
                        for _ in 0..new_count {
                            new_arr.push(item.clone());
                        }
                    }
                    *val = Value::Array(new_arr);
                }
            }
        }

        // 3. Expand new variable: repeat the whole sequence N times (N = current_count)
        let mut expanded_new_values = Vec::with_capacity(new_count * current_count);
        for _ in 0..current_count {
            expanded_new_values.extend(new_values.clone());
        }
        context.set_variable(new_var, Value::Array(expanded_new_values));

        Ok(())
    }

    pub(super) fn materialize_rows_from_variables(
        &self,
        context: &ExecutionContext,
    ) -> Vec<HashMap<String, Value>> {
        // TRACE: Log variables before creating cartesian product
        let mut has_relationships = false;
        let mut var_types: Vec<(String, String)> = Vec::new();
        for (var, value) in &context.variables {
            let var_type = match value {
                Value::Object(obj) => {
                    if obj.contains_key("type") {
                        has_relationships = true;
                        "RELATIONSHIP".to_string()
                    } else {
                        "NODE".to_string()
                    }
                }
                Value::Array(arr) => {
                    let has_rel = arr.iter().any(|v| {
                        if let Value::Object(obj) = v {
                            obj.contains_key("type")
                        } else {
                            false
                        }
                    });
                    if has_rel {
                        has_relationships = true;
                    }
                    format!(
                        "ARRAY({})",
                        if has_rel {
                            "HAS_RELATIONSHIPS"
                        } else {
                            "NODES_ONLY"
                        }
                    )
                }
                _ => "OTHER".to_string(),
            };
            var_types.push((var.clone(), var_type));
        }
        tracing::trace!(
            "materialize_rows_from_variables: variables={:?}, has_relationships={}, creating cartesian product",
            var_types,
            has_relationships
        );

        let mut arrays: HashMap<String, Vec<Value>> = HashMap::new();

        for (var, value) in &context.variables {
            match value {
                Value::Array(values) => {
                    // Only include non-empty arrays
                    if !values.is_empty() {
                        arrays.insert(var.clone(), values.clone());
                    }
                }
                other => {
                    // Include non-null single values
                    if !matches!(other, Value::Null) {
                        arrays.insert(var.clone(), vec![other.clone()]);
                    }
                }
            }
        }

        if arrays.is_empty() {
            return Vec::new();
        }

        // CRITICAL FIX: Implement true cartesian product instead of zip
        // When we have multiple node arrays (e.g., p1=[Alice, Bob], c2=[Acme, TechCorp]),
        // we need ALL combinations (4 rows), not just pairs (2 rows)

        // Check if all arrays have the same length and all are nodes (not single values)
        let all_same_len = arrays
            .values()
            .map(|v| v.len())
            .collect::<std::collections::HashSet<_>>()
            .len()
            == 1;
        let has_multiple_arrays = arrays.len() > 1;
        let all_multi_element = arrays.values().all(|v| v.len() > 1);

        let needs_cartesian_product = has_multiple_arrays && all_multi_element && all_same_len;

        if needs_cartesian_product {
            // TRUE CARTESIAN PRODUCT: Generate ALL combinations
            let var_names: Vec<String> = arrays.keys().cloned().collect();
            let array_values: Vec<Vec<Value>> =
                var_names.iter().map(|k| arrays[k].clone()).collect();

            // Calculate total number of combinations
            let total_combinations: usize = array_values.iter().map(|arr| arr.len()).product();

            let mut rows = Vec::new();

            // Generate all combinations using nested iteration
            let mut indices = vec![0usize; array_values.len()];

            loop {
                // Create a row from current indices
                let mut row = HashMap::new();
                for (i, var_name) in var_names.iter().enumerate() {
                    let value = array_values[i][indices[i]].clone();
                    row.insert(var_name.clone(), value);
                }
                rows.push(row);

                // Increment indices (like odometer)
                let mut carry = true;
                for i in (0..indices.len()).rev() {
                    if carry {
                        indices[i] += 1;
                        if indices[i] < array_values[i].len() {
                            carry = false;
                        } else {
                            indices[i] = 0;
                        }
                    }
                }

                // If carry is still true, we've exhausted all combinations
                if carry {
                    break;
                }
            }

            return rows;
        }

        // FALLBACK: Old zip-based logic for single arrays or mixed sizes
        let max_len = arrays
            .values()
            .map(|values| values.len())
            .max()
            .unwrap_or(0);

        if max_len == 0 {
            return Vec::new();
        }

        let mut rows = Vec::new();

        for idx in 0..max_len {
            let mut row = HashMap::new();
            let mut all_null = true;
            let mut entity_ids = Vec::new();

            for (var, values) in &arrays {
                let value = if values.len() == max_len {
                    values.get(idx).cloned().unwrap_or(Value::Null)
                } else if values.len() == 1 {
                    values.first().cloned().unwrap_or(Value::Null)
                } else {
                    // For arrays with different lengths, only use value if index exists
                    if idx < values.len() {
                        values.get(idx).cloned().unwrap_or(Value::Null)
                    } else {
                        Value::Null
                    }
                };

                // Track if row has at least one non-null value
                if !matches!(value, Value::Null) {
                    all_null = false;

                    // Extract entity ID (node or relationship) for deduplication
                    if let Value::Object(obj) = &value {
                        if let Some(Value::Number(id)) = obj.get("_nexus_id") {
                            if let Some(nid) = id.as_u64() {
                                entity_ids.push(nid);
                            }
                        }
                    }
                }

                row.insert(var.clone(), value);
            }

            // Add row if it has content and is not a duplicate
            if !all_null {
                /*
                let is_duplicate = if !entity_ids.is_empty() {
                    // Sort IDs to ensure consistent key regardless of column order
                    entity_ids.sort();
                    let key = entity_ids
                        .iter()
                        .map(|id| id.to_string())
                        .collect::<Vec<String>>()
                        .join("_");
                    !seen_row_keys.insert(key)
                } else {
                    // Fallback for rows without entities (e.g. literals) - no deduplication or full content deduplication?
                    // For now, allow all since we can't identify entities
                    false
                };

                if !is_duplicate {
                    rows.push(row);
                }
                */
                // DEBUG: Disable deduplication to see if rows are being generated
                rows.push(row);
            }
        }

        rows
    }

    pub(super) fn update_result_set_from_rows(
        &self,
        context: &mut ExecutionContext,
        rows: &[HashMap<String, Value>],
    ) {
        // TRACE: Check if input rows contain relationships
        let mut rows_with_relationships = 0;
        for row in rows {
            let has_rel = row.values().any(|value| {
                if let Value::Object(obj) = value {
                    obj.contains_key("type") // Relationships have "type" property
                } else {
                    false
                }
            });
            if has_rel {
                rows_with_relationships += 1;
            }
        }

        // CRITICAL FIX: Only use columns from rows, not from context.variables
        // Context variables may contain old/unused variables that cause null rows
        // Only include variables that are actually present in the rows
        let mut columns: std::collections::HashSet<String> = std::collections::HashSet::new();
        for row in rows {
            columns.extend(row.keys().cloned());
        }

        // Don't include variables from context - they may be stale
        // Only use what's actually in the rows

        let mut columns: Vec<String> = columns.into_iter().collect();
        columns.sort();

        // CRITICAL FIX: Deduplicate rows intelligently - consider full row content for relationship rows
        // When we have relationships (multiple rows with same source node), we need to check the full row
        // content, not just the source node ID, to avoid removing valid relationship rows
        use std::collections::HashSet;
        let mut seen_row_keys = HashSet::new();
        let mut unique_rows = Vec::new();

        for row_map in rows {
            // Collect all entity IDs (nodes and relationships) in this row
            // CRITICAL FIX: Extract all _nexus_id values, which can be from nodes or relationships
            // For relationship rows, we need to use ALL IDs (source node + target node + relationship)
            // to correctly differentiate between different relationships
            let mut all_entity_ids: Vec<u64> = Vec::new();

            // Extract all _nexus_id values from the row (both nodes and relationships have this)
            for value in row_map.values() {
                if let Value::Object(obj) = value {
                    if let Some(Value::Number(id)) = obj.get("_nexus_id") {
                        if let Some(entity_id) = id.as_u64() {
                            all_entity_ids.push(entity_id);
                        }
                    }
                }
            }

            // CRITICAL FIX: Determine deduplication key based on number of entity IDs
            // Relationship rows typically have multiple entity IDs (source node + target node + relationship)
            // Non-relationship rows have only one entity ID (just the node)
            let is_duplicate = if all_entity_ids.len() > 1 {
                // Relationship row or row with multiple entities
                // CRITICAL FIX: Find relationship ID and use it as primary key for deduplication
                // This ensures that rows with the same relationship ID are considered duplicates
                // even if they appear in different contexts (e.g., bidirectional relationships from source vs target)
                let relationship_id = row_map.values().find_map(|value| {
                    if let Value::Object(obj) = value {
                        // Relationship objects have a "type" property
                        if obj.contains_key("type") {
                            if let Some(Value::Number(nid)) = obj.get("_nexus_id") {
                                return nid.as_u64();
                            }
                        }
                    }
                    None
                });

                if let Some(rel_id) = relationship_id {
                    // CRITICAL FIX: For relationship rows, use relationship ID + variable values
                    // This ensures that rows with same relationship ID but different variable assignments
                    // are not considered duplicates (e.g., bidirectional relationships: a=778,b=779 vs a=779,b=778)
                    // Build key using relationship ID + sorted list of variable names and their node IDs
                    let mut var_entries: Vec<(String, u64)> = Vec::new();

                    for (key, value) in row_map {
                        if let Value::Object(obj) = value {
                            if let Some(Value::Number(nid)) = obj.get("_nexus_id") {
                                if let Some(entity_id) = nid.as_u64() {
                                    // Skip relationship ID
                                    if entity_id != rel_id && !obj.contains_key("type") {
                                        // This is a node variable
                                        var_entries.push((key.clone(), entity_id));
                                    }
                                }
                            }
                        }
                    }

                    // Sort variable entries by variable name for consistent key generation
                    var_entries.sort_by(|a, b| a.0.cmp(&b.0));

                    // Build deduplication key: rel_{id}_{var1}_{id1}_{var2}_{id2}...
                    let mut key_parts = vec![format!("rel_{}", rel_id)];
                    for (var_name, var_id) in &var_entries {
                        key_parts.push(format!("{}_{}", var_name, var_id));
                    }
                    let row_key = key_parts.join("_");

                    let is_dup = !seen_row_keys.insert(row_key.clone());
                    is_dup
                } else {
                    // Fallback: Can't find rel_id but have multiple entities - include variables in key
                    // This handles bidirectional relationships where we need to differentiate by variable assignment
                    let mut var_entries: Vec<(String, u64)> = Vec::new();

                    for (key, value) in row_map {
                        if let Value::Object(obj) = value {
                            if let Some(Value::Number(nid)) = obj.get("_nexus_id") {
                                if let Some(entity_id) = nid.as_u64() {
                                    // Include all entities with their variable names
                                    var_entries.push((key.clone(), entity_id));
                                }
                            }
                        }
                    }

                    // Sort by variable name for consistent key generation
                    var_entries.sort_by(|a, b| a.0.cmp(&b.0));

                    // Build key: var1_id1_var2_id2_var3_id3...
                    let key_parts: Vec<String> = var_entries
                        .iter()
                        .map(|(var_name, var_id)| format!("{}_{}", var_name, var_id))
                        .collect();
                    let row_key = key_parts.join("_");

                    let is_dup = !seen_row_keys.insert(row_key.clone());
                    is_dup
                }
            } else if let Some(first_id) = all_entity_ids.first() {
                // Non-relationship row - but check if this is from OPTIONAL MATCH (has NULL values)
                // CRITICAL FIX: For OPTIONAL MATCH NULL rows, include NULL variable names in key
                // to prevent incorrect deduplication of different source nodes
                let has_null_values = row_map.values().any(|v| matches!(v, Value::Null));

                if has_null_values {
                    // OPTIONAL MATCH NULL row - include all variable names and their values/NULL status
                    let mut var_entries: Vec<String> = Vec::new();
                    for (key, value) in row_map {
                        if let Value::Object(obj) = value {
                            if let Some(Value::Number(nid)) = obj.get("_nexus_id") {
                                if let Some(entity_id) = nid.as_u64() {
                                    var_entries.push(format!("{}_{}", key, entity_id));
                                }
                            }
                        } else if matches!(value, Value::Null) {
                            var_entries.push(format!("{}_null", key));
                        }
                    }
                    var_entries.sort();
                    let row_key = var_entries.join("_");
                    !seen_row_keys.insert(row_key)
                } else {
                    // Regular non-relationship row - use only entity ID
                    let entity_key = format!("node_{}", first_id);
                    !seen_row_keys.insert(entity_key)
                }
            } else {
                // No entity IDs found - use full row content as fallback
                let row_key = serde_json::to_string(row_map).unwrap_or_default();
                !seen_row_keys.insert(row_key)
            };

            // Only add row if it's not a duplicate
            if !is_duplicate {
                unique_rows.push(row_map.clone());
            }
        }

        tracing::debug!(
            "update_result_set_from_rows: deduplicated {} rows to {} unique rows",
            rows.len(),
            unique_rows.len()
        );

        // DEBUG: Log details of each row for debugging
        for (idx, row_map) in rows.iter().enumerate() {
            let mut all_entity_ids: Vec<u64> = Vec::new();
            for value in row_map.values() {
                if let Value::Object(obj) = value {
                    if let Some(Value::Number(id)) = obj.get("_nexus_id") {
                        if let Some(entity_id) = id.as_u64() {
                            all_entity_ids.push(entity_id);
                        }
                    }
                }
            }
            all_entity_ids.sort();
        }

        // CRITICAL FIX: Always clear result_set.rows before updating to ensure complete replacement
        // This prevents mixing old rows with new ones
        context.result_set.rows.clear();
        context.result_set.columns = columns.clone();
        context.result_set.rows = unique_rows
            .iter()
            .map(|row_map| Row {
                values: columns
                    .iter()
                    .map(|column| row_map.get(column).cloned().unwrap_or(Value::Null))
                    .collect(),
            })
            .collect();
    }

    /// Check if an expression can be evaluated without variables (only literals and operations)
    pub(super) fn can_evaluate_without_variables(&self, expr: &parser::Expression) -> bool {
        match expr {
            parser::Expression::Literal(_) => true,
            parser::Expression::Parameter(_) => true, // Parameters can be evaluated
            parser::Expression::Variable(_) => false, // Variables need context
            parser::Expression::PropertyAccess { .. } => false, // Property access needs variables
            parser::Expression::ArrayIndex { base, index } => {
                // Can evaluate if both base and index can be evaluated without variables
                self.can_evaluate_without_variables(base)
                    && self.can_evaluate_without_variables(index)
            }
            parser::Expression::ArraySlice { base, start, end } => {
                // Can evaluate if base and both indices can be evaluated without variables
                self.can_evaluate_without_variables(base)
                    && start
                        .as_ref()
                        .map(|s| self.can_evaluate_without_variables(s))
                        .unwrap_or(true)
                    && end
                        .as_ref()
                        .map(|e| self.can_evaluate_without_variables(e))
                        .unwrap_or(true)
            }
            parser::Expression::BinaryOp { left, right, .. } => {
                // Can evaluate if both operands can be evaluated
                self.can_evaluate_without_variables(left)
                    && self.can_evaluate_without_variables(right)
            }
            parser::Expression::UnaryOp { operand, .. } => {
                // Can evaluate if operand can be evaluated
                self.can_evaluate_without_variables(operand)
            }
            parser::Expression::FunctionCall { args, .. } => {
                // Can evaluate if all arguments can be evaluated
                args.iter()
                    .all(|arg| self.can_evaluate_without_variables(arg))
            }
            parser::Expression::Case {
                input,
                when_clauses,
                else_clause,
            } => {
                // Can evaluate if input (if present) and all when/else expressions can be evaluated
                let input_ok = input
                    .as_ref()
                    .map(|e| self.can_evaluate_without_variables(e))
                    .unwrap_or(true);
                let when_ok = when_clauses.iter().all(|when| {
                    self.can_evaluate_without_variables(&when.condition)
                        && self.can_evaluate_without_variables(&when.result)
                });
                let else_ok = else_clause
                    .as_ref()
                    .map(|e| self.can_evaluate_without_variables(e))
                    .unwrap_or(true);
                input_ok && when_ok && else_ok
            }
            parser::Expression::IsNull { expr, .. } => self.can_evaluate_without_variables(expr),
            parser::Expression::List(exprs) => {
                exprs.iter().all(|e| self.can_evaluate_without_variables(e))
            }
            parser::Expression::Map(map) => {
                map.values().all(|e| self.can_evaluate_without_variables(e))
            }
            parser::Expression::Exists { .. } => false, // EXISTS needs graph context
            parser::Expression::PatternComprehension { .. } => false, // Pattern needs graph context
            parser::Expression::MapProjection { .. } => false, // Map projection needs variables
            parser::Expression::ListComprehension {
                list_expression, ..
            } => {
                // List comprehension can be evaluated if the list expression can be evaluated.
                // The where_clause and transform_expression may reference the comprehension variable,
                // which is fine - it will be bound during comprehension execution.
                self.can_evaluate_without_variables(list_expression)
            }
        }
    }

    /// Check if a pattern exists in the current context
    pub(super) fn check_pattern_exists(
        &self,
        row: &HashMap<String, Value>,
        context: &ExecutionContext,
        pattern: &parser::Pattern,
    ) -> Result<bool> {
        // For EXISTS, we need to check if the pattern matches in the current context
        // This checks if nodes and relationships actually exist

        // If pattern is empty, return false
        if pattern.elements.is_empty() {
            return Ok(false);
        }

        // Get the first node from the pattern
        if let Some(parser::PatternElement::Node(first_node)) = pattern.elements.first() {
            // If the node has a variable, check if it exists in the current row/context
            if let Some(var_name) = &first_node.variable {
                // Check if variable exists in current row
                if let Some(Value::Object(obj)) = row.get(var_name) {
                    // If it's a valid node object, check relationships if pattern has them
                    if let Some(Value::Number(node_id_val)) = obj.get("_nexus_id") {
                        let node_id = node_id_val
                            .as_u64()
                            .ok_or_else(|| Error::InvalidId("Invalid node ID".to_string()))?;

                        // If pattern has only one element (just a node), it exists
                        if pattern.elements.len() == 1 {
                            return Ok(true);
                        }

                        // Pattern has relationships - actually check if they exist
                        // Look for relationship element in pattern
                        for (i, element) in pattern.elements.iter().enumerate() {
                            if let parser::PatternElement::Relationship(rel) = element {
                                // Get relationship types to match
                                let type_ids: Vec<u32> = if rel.types.is_empty() {
                                    // No types specified = match all types
                                    vec![]
                                } else {
                                    rel.types
                                        .iter()
                                        .filter_map(|t| {
                                            self.catalog().get_type_id(t).ok().flatten()
                                        })
                                        .collect()
                                };

                                // Determine direction
                                let direction = match rel.direction {
                                    parser::RelationshipDirection::Outgoing => Direction::Outgoing,
                                    parser::RelationshipDirection::Incoming => Direction::Incoming,
                                    parser::RelationshipDirection::Both => Direction::Both,
                                };

                                // Fetch relationships for this node
                                // find_relationships already filters by type_ids and direction
                                let relationships = self.find_relationships(
                                    node_id, &type_ids, direction,
                                    None, // No cache for EXISTS checks
                                )?;

                                // If no matching relationships found, pattern doesn't exist
                                if relationships.is_empty() {
                                    return Ok(false);
                                }

                                // At least one relationship exists
                                return Ok(true);
                            }
                        }

                        // No relationship element found in pattern
                        return Ok(true);
                    }
                }

                // Check if variable exists in context variables
                if let Some(Value::Array(nodes)) = context.variables.get(var_name) {
                    if !nodes.is_empty() {
                        return Ok(true);
                    }
                }
            } else {
                // No variable - pattern exists if we can find matching nodes
                // For simplicity, if no variable is specified, assume pattern might exist
                // This is a basic implementation
                return Ok(true);
            }
        }

        // Pattern doesn't match
        Ok(false)
    }

    pub(super) fn extract_property(entity: &Value, property: &str) -> Value {
        if let Value::Object(obj) = entity {
            // First check directly in the object (for nodes with flat properties)
            // This is the primary case - nodes have properties directly in the object
            if let Some(value) = obj.get(property) {
                // CRITICAL FIX: Allow _nexus_id to be returned when explicitly requested
                // Only skip truly internal properties that shouldn't be exposed
                if property == "_nexus_id" {
                    // _nexus_id is allowed and commonly used in queries
                    return value.clone();
                }
                // Skip other internal properties
                if property != "_nexus_type"
                    && property != "_source"
                    && property != "_target"
                    && property != "_element_id"
                {
                    return value.clone();
                }
            }
            // Then check if there's a nested "properties" object (for compatibility with other formats)
            if let Some(Value::Object(props)) = obj.get("properties") {
                if let Some(value) = props.get(property) {
                    return value.clone();
                }
            }
        }
        Value::Null
    }

    pub(super) fn add_values(&self, left: &Value, right: &Value) -> Result<Value> {
        // Handle null values - null + number or number + null = null (Neo4j behavior)
        if left.is_null() || right.is_null() {
            return Ok(Value::Null);
        }

        // Check if both values are strings - then concatenate
        if let (Value::String(l_str), Value::String(r_str)) = (left, right) {
            return Ok(Value::String(format!("{}{}", l_str, r_str)));
        }

        // Check if both values are arrays - then concatenate
        if let (Value::Array(l_arr), Value::Array(r_arr)) = (left, right) {
            let mut result = l_arr.clone();
            result.extend(r_arr.iter().cloned());
            return Ok(Value::Array(result));
        }

        // Check for datetime + duration arithmetic
        if let Some(result) = self.try_datetime_add(left, right)? {
            return Ok(result);
        }

        // Check for duration + duration arithmetic
        if let Some(result) = self.try_duration_add(left, right)? {
            return Ok(result);
        }

        // Otherwise, treat as numeric addition
        let l = self.value_to_number(left)?;
        let r = self.value_to_number(right)?;
        serde_json::Number::from_f64(l + r)
            .map(Value::Number)
            .ok_or_else(|| Error::TypeMismatch {
                expected: "number".to_string(),
                actual: "non-finite sum".to_string(),
            })
    }

    pub(super) fn subtract_values(&self, left: &Value, right: &Value) -> Result<Value> {
        // Handle null values - null - number or number - null = null (Neo4j behavior)
        if left.is_null() || right.is_null() {
            return Ok(Value::Null);
        }

        // Check for datetime - duration arithmetic
        if let Some(result) = self.try_datetime_subtract(left, right)? {
            return Ok(result);
        }

        // Check for datetime - datetime (returns duration)
        if let Some(result) = self.try_datetime_diff(left, right)? {
            return Ok(result);
        }

        // Check for duration - duration arithmetic
        if let Some(result) = self.try_duration_subtract(left, right)? {
            return Ok(result);
        }

        let l = self.value_to_number(left)?;
        let r = self.value_to_number(right)?;
        serde_json::Number::from_f64(l - r)
            .map(Value::Number)
            .ok_or_else(|| Error::TypeMismatch {
                expected: "number".to_string(),
                actual: "non-finite difference".to_string(),
            })
    }

    pub(super) fn multiply_values(&self, left: &Value, right: &Value) -> Result<Value> {
        // Handle null values - null * number or number * null = null (Neo4j behavior)
        if left.is_null() || right.is_null() {
            return Ok(Value::Null);
        }
        let l = self.value_to_number(left)?;
        let r = self.value_to_number(right)?;
        serde_json::Number::from_f64(l * r)
            .map(Value::Number)
            .ok_or_else(|| Error::TypeMismatch {
                expected: "number".to_string(),
                actual: "non-finite product".to_string(),
            })
    }

    pub(super) fn divide_values(&self, left: &Value, right: &Value) -> Result<Value> {
        // Handle null values - null / number or number / null = null (Neo4j behavior)
        if left.is_null() || right.is_null() {
            return Ok(Value::Null);
        }
        let l = self.value_to_number(left)?;
        let r = self.value_to_number(right)?;
        if r == 0.0 {
            return Err(Error::TypeMismatch {
                expected: "non-zero".to_string(),
                actual: "division by zero".to_string(),
            });
        }
        serde_json::Number::from_f64(l / r)
            .map(Value::Number)
            .ok_or_else(|| Error::TypeMismatch {
                expected: "number".to_string(),
                actual: "non-finite quotient".to_string(),
            })
    }

    pub(super) fn power_values(&self, left: &Value, right: &Value) -> Result<Value> {
        // Handle null values - null ^ anything or anything ^ null = null
        if left.is_null() || right.is_null() {
            return Ok(Value::Null);
        }

        let base = self.value_to_number(left)?;
        let exp = self.value_to_number(right)?;
        let result = base.powf(exp);

        serde_json::Number::from_f64(result)
            .map(Value::Number)
            .ok_or_else(|| Error::TypeMismatch {
                expected: "number".to_string(),
                actual: "non-finite power result".to_string(),
            })
    }

    pub(super) fn modulo_values(&self, left: &Value, right: &Value) -> Result<Value> {
        // Handle null values - null % anything or anything % null = null
        if left.is_null() || right.is_null() {
            return Ok(Value::Null);
        }

        let l = self.value_to_number(left)?;
        let r = self.value_to_number(right)?;

        if r == 0.0 {
            return Err(Error::TypeMismatch {
                expected: "non-zero".to_string(),
                actual: "modulo by zero".to_string(),
            });
        }

        // Use f64::rem_euclid for modulo operation
        let result = l.rem_euclid(r);

        serde_json::Number::from_f64(result)
            .map(Value::Number)
            .ok_or_else(|| Error::TypeMismatch {
                expected: "number".to_string(),
                actual: "non-finite modulo result".to_string(),
            })
    }

    /// Check if value is a duration object (has years, months, days, hours, minutes, or seconds keys)
    pub(super) fn is_duration_object(value: &Value) -> bool {
        if let Value::Object(map) = value {
            map.contains_key("years")
                || map.contains_key("months")
                || map.contains_key("days")
                || map.contains_key("hours")
                || map.contains_key("minutes")
                || map.contains_key("seconds")
        } else {
            false
        }
    }

    /// Check if value is a datetime string (RFC3339 format)
    pub(super) fn is_datetime_string(value: &Value) -> bool {
        if let Value::String(s) = value {
            chrono::DateTime::parse_from_rfc3339(s).is_ok()
                || chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").is_ok()
                || chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()
        } else {
            false
        }
    }

    /// Extract duration components as (years, months, days, hours, minutes, seconds)
    pub(super) fn extract_duration_components(value: &Value) -> (i64, i64, i64, i64, i64, i64) {
        if let Value::Object(map) = value {
            let years = map.get("years").and_then(|v| v.as_i64()).unwrap_or(0);
            let months = map.get("months").and_then(|v| v.as_i64()).unwrap_or(0);
            let days = map.get("days").and_then(|v| v.as_i64()).unwrap_or(0);
            let hours = map.get("hours").and_then(|v| v.as_i64()).unwrap_or(0);
            let minutes = map.get("minutes").and_then(|v| v.as_i64()).unwrap_or(0);
            let seconds = map.get("seconds").and_then(|v| v.as_i64()).unwrap_or(0);
            (years, months, days, hours, minutes, seconds)
        } else {
            (0, 0, 0, 0, 0, 0)
        }
    }

    /// Try to add datetime + duration
    pub(super) fn try_datetime_add(&self, left: &Value, right: &Value) -> Result<Option<Value>> {
        // datetime + duration
        if Self::is_datetime_string(left) && Self::is_duration_object(right) {
            return self.datetime_add_duration(left, right).map(Some);
        }
        // duration + datetime (commutative)
        if Self::is_duration_object(left) && Self::is_datetime_string(right) {
            return self.datetime_add_duration(right, left).map(Some);
        }
        Ok(None)
    }

    /// Try to add duration + duration
    pub(super) fn try_duration_add(&self, left: &Value, right: &Value) -> Result<Option<Value>> {
        if Self::is_duration_object(left) && Self::is_duration_object(right) {
            let (y1, mo1, d1, h1, mi1, s1) = Self::extract_duration_components(left);
            let (y2, mo2, d2, h2, mi2, s2) = Self::extract_duration_components(right);

            let mut result_map = Map::new();
            let years = y1 + y2;
            let months = mo1 + mo2;
            let days = d1 + d2;
            let hours = h1 + h2;
            let minutes = mi1 + mi2;
            let seconds = s1 + s2;

            if years != 0 {
                result_map.insert("years".to_string(), Value::Number(years.into()));
            }
            if months != 0 {
                result_map.insert("months".to_string(), Value::Number(months.into()));
            }
            if days != 0 {
                result_map.insert("days".to_string(), Value::Number(days.into()));
            }
            if hours != 0 {
                result_map.insert("hours".to_string(), Value::Number(hours.into()));
            }
            if minutes != 0 {
                result_map.insert("minutes".to_string(), Value::Number(minutes.into()));
            }
            if seconds != 0 {
                result_map.insert("seconds".to_string(), Value::Number(seconds.into()));
            }

            return Ok(Some(Value::Object(result_map)));
        }
        Ok(None)
    }

    /// Try to subtract datetime - duration
    pub(super) fn try_datetime_subtract(
        &self,
        left: &Value,
        right: &Value,
    ) -> Result<Option<Value>> {
        if Self::is_datetime_string(left) && Self::is_duration_object(right) {
            return self.datetime_subtract_duration(left, right).map(Some);
        }
        Ok(None)
    }

    /// Try to compute datetime - datetime (returns duration)
    pub(super) fn try_datetime_diff(&self, left: &Value, right: &Value) -> Result<Option<Value>> {
        if Self::is_datetime_string(left) && Self::is_datetime_string(right) {
            return self.datetime_difference(left, right).map(Some);
        }
        Ok(None)
    }

    /// Try to subtract duration - duration
    pub(super) fn try_duration_subtract(
        &self,
        left: &Value,
        right: &Value,
    ) -> Result<Option<Value>> {
        if Self::is_duration_object(left) && Self::is_duration_object(right) {
            let (y1, mo1, d1, h1, mi1, s1) = Self::extract_duration_components(left);
            let (y2, mo2, d2, h2, mi2, s2) = Self::extract_duration_components(right);

            let mut result_map = Map::new();
            let years = y1 - y2;
            let months = mo1 - mo2;
            let days = d1 - d2;
            let hours = h1 - h2;
            let minutes = mi1 - mi2;
            let seconds = s1 - s2;

            if years != 0 {
                result_map.insert("years".to_string(), Value::Number(years.into()));
            }
            if months != 0 {
                result_map.insert("months".to_string(), Value::Number(months.into()));
            }
            if days != 0 {
                result_map.insert("days".to_string(), Value::Number(days.into()));
            }
            if hours != 0 {
                result_map.insert("hours".to_string(), Value::Number(hours.into()));
            }
            if minutes != 0 {
                result_map.insert("minutes".to_string(), Value::Number(minutes.into()));
            }
            if seconds != 0 {
                result_map.insert("seconds".to_string(), Value::Number(seconds.into()));
            }

            return Ok(Some(Value::Object(result_map)));
        }
        Ok(None)
    }

    /// Add duration to datetime
    pub(super) fn datetime_add_duration(
        &self,
        datetime: &Value,
        duration: &Value,
    ) -> Result<Value> {
        let (years, months, days, hours, minutes, seconds) =
            Self::extract_duration_components(duration);

        if let Value::String(dt_str) = datetime {
            // Try RFC3339 format first
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(dt_str) {
                let mut result = dt.with_timezone(&chrono::Utc);

                // Add years and months using checked arithmetic
                if years != 0 || months != 0 {
                    let total_months = years * 12 + months;
                    let new_month = result.month() as i64 + total_months;
                    let year_offset = (new_month - 1).div_euclid(12);
                    let final_month = ((new_month - 1).rem_euclid(12) + 1) as u32;
                    let final_year = result.year() as i64 + year_offset;

                    if let Some(new_dt) = result
                        .with_year(final_year as i32)
                        .and_then(|d| d.with_month(final_month))
                    {
                        result = new_dt;
                    }
                }

                // Add days, hours, minutes, seconds
                let duration_secs = days * 86400 + hours * 3600 + minutes * 60 + seconds;
                result = result + chrono::Duration::seconds(duration_secs);

                return Ok(Value::String(result.to_rfc3339()));
            }

            // Try NaiveDateTime format
            if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(dt_str, "%Y-%m-%dT%H:%M:%S") {
                let mut result = dt;

                // Add years and months
                if years != 0 || months != 0 {
                    let total_months = years * 12 + months;
                    let new_month = result.month() as i64 + total_months;
                    let year_offset = (new_month - 1).div_euclid(12);
                    let final_month = ((new_month - 1).rem_euclid(12) + 1) as u32;
                    let final_year = result.year() as i64 + year_offset;

                    if let Some(new_dt) = result
                        .with_year(final_year as i32)
                        .and_then(|d| d.with_month(final_month))
                    {
                        result = new_dt;
                    }
                }

                // Add days, hours, minutes, seconds
                let duration_secs = days * 86400 + hours * 3600 + minutes * 60 + seconds;
                result = result + chrono::Duration::seconds(duration_secs);

                return Ok(Value::String(
                    result.format("%Y-%m-%dT%H:%M:%S").to_string(),
                ));
            }

            // Try NaiveDate format
            if let Ok(dt) = chrono::NaiveDate::parse_from_str(dt_str, "%Y-%m-%d") {
                let mut result = dt;

                // Add years and months
                if years != 0 || months != 0 {
                    let total_months = years * 12 + months;
                    let new_month = result.month() as i64 + total_months;
                    let year_offset = (new_month - 1).div_euclid(12);
                    let final_month = ((new_month - 1).rem_euclid(12) + 1) as u32;
                    let final_year = result.year() as i64 + year_offset;

                    if let Some(new_dt) = result
                        .with_year(final_year as i32)
                        .and_then(|d| d.with_month(final_month))
                    {
                        result = new_dt;
                    }
                }

                // Add days
                result = result + chrono::Duration::days(days);

                return Ok(Value::String(result.format("%Y-%m-%d").to_string()));
            }
        }

        Ok(Value::Null)
    }

    /// Subtract duration from datetime
    pub(super) fn datetime_subtract_duration(
        &self,
        datetime: &Value,
        duration: &Value,
    ) -> Result<Value> {
        let (years, months, days, hours, minutes, seconds) =
            Self::extract_duration_components(duration);

        if let Value::String(dt_str) = datetime {
            // Try RFC3339 format first
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(dt_str) {
                let mut result = dt.with_timezone(&chrono::Utc);

                // Subtract years and months
                if years != 0 || months != 0 {
                    let total_months = years * 12 + months;
                    let new_month = result.month() as i64 - total_months;
                    let year_offset = (new_month - 1).div_euclid(12);
                    let final_month = ((new_month - 1).rem_euclid(12) + 1) as u32;
                    let final_year = result.year() as i64 + year_offset;

                    if let Some(new_dt) = result
                        .with_year(final_year as i32)
                        .and_then(|d| d.with_month(final_month))
                    {
                        result = new_dt;
                    }
                }

                // Subtract days, hours, minutes, seconds
                let duration_secs = days * 86400 + hours * 3600 + minutes * 60 + seconds;
                result = result - chrono::Duration::seconds(duration_secs);

                return Ok(Value::String(result.to_rfc3339()));
            }

            // Try NaiveDateTime format
            if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(dt_str, "%Y-%m-%dT%H:%M:%S") {
                let mut result = dt;

                // Subtract years and months
                if years != 0 || months != 0 {
                    let total_months = years * 12 + months;
                    let new_month = result.month() as i64 - total_months;
                    let year_offset = (new_month - 1).div_euclid(12);
                    let final_month = ((new_month - 1).rem_euclid(12) + 1) as u32;
                    let final_year = result.year() as i64 + year_offset;

                    if let Some(new_dt) = result
                        .with_year(final_year as i32)
                        .and_then(|d| d.with_month(final_month))
                    {
                        result = new_dt;
                    }
                }

                // Subtract days, hours, minutes, seconds
                let duration_secs = days * 86400 + hours * 3600 + minutes * 60 + seconds;
                result = result - chrono::Duration::seconds(duration_secs);

                return Ok(Value::String(
                    result.format("%Y-%m-%dT%H:%M:%S").to_string(),
                ));
            }

            // Try NaiveDate format
            if let Ok(dt) = chrono::NaiveDate::parse_from_str(dt_str, "%Y-%m-%d") {
                let mut result = dt;

                // Subtract years and months
                if years != 0 || months != 0 {
                    let total_months = years * 12 + months;
                    let new_month = result.month() as i64 - total_months;
                    let year_offset = (new_month - 1).div_euclid(12);
                    let final_month = ((new_month - 1).rem_euclid(12) + 1) as u32;
                    let final_year = result.year() as i64 + year_offset;

                    if let Some(new_dt) = result
                        .with_year(final_year as i32)
                        .and_then(|d| d.with_month(final_month))
                    {
                        result = new_dt;
                    }
                }

                // Subtract days
                result = result - chrono::Duration::days(days);

                return Ok(Value::String(result.format("%Y-%m-%d").to_string()));
            }
        }

        Ok(Value::Null)
    }

    /// Compute difference between two datetimes (returns duration)
    pub(super) fn datetime_difference(&self, left: &Value, right: &Value) -> Result<Value> {
        if let (Value::String(left_str), Value::String(right_str)) = (left, right) {
            // Try RFC3339 format
            let left_dt = chrono::DateTime::parse_from_rfc3339(left_str)
                .map(|dt| dt.with_timezone(&chrono::Utc));
            let right_dt = chrono::DateTime::parse_from_rfc3339(right_str)
                .map(|dt| dt.with_timezone(&chrono::Utc));

            if let (Ok(l), Ok(r)) = (left_dt, right_dt) {
                let diff = l.signed_duration_since(r);
                let total_seconds = diff.num_seconds();

                let days = total_seconds / 86400;
                let remaining = total_seconds % 86400;
                let hours = remaining / 3600;
                let remaining = remaining % 3600;
                let minutes = remaining / 60;
                let seconds = remaining % 60;

                let mut result_map = Map::new();
                if days != 0 {
                    result_map.insert("days".to_string(), Value::Number(days.into()));
                }
                if hours != 0 {
                    result_map.insert("hours".to_string(), Value::Number(hours.into()));
                }
                if minutes != 0 {
                    result_map.insert("minutes".to_string(), Value::Number(minutes.into()));
                }
                if seconds != 0 {
                    result_map.insert("seconds".to_string(), Value::Number(seconds.into()));
                }

                return Ok(Value::Object(result_map));
            }

            // Try NaiveDate format
            let left_date = chrono::NaiveDate::parse_from_str(left_str, "%Y-%m-%d");
            let right_date = chrono::NaiveDate::parse_from_str(right_str, "%Y-%m-%d");

            if let (Ok(l), Ok(r)) = (left_date, right_date) {
                let diff = l.signed_duration_since(r);
                let days = diff.num_days();

                let mut result_map = Map::new();
                if days != 0 {
                    result_map.insert("days".to_string(), Value::Number(days.into()));
                }

                return Ok(Value::Object(result_map));
            }
        }

        Ok(Value::Null)
    }

    pub(super) fn update_variables_from_rows(
        &self,
        context: &mut ExecutionContext,
        rows: &[HashMap<String, Value>],
    ) {
        let mut arrays: HashMap<String, Vec<Value>> = HashMap::new();
        for row in rows {
            for (var, value) in row {
                arrays.entry(var.clone()).or_default().push(value.clone());
            }
        }

        context.variables.clear();

        for (var, values) in arrays {
            context.variables.insert(var, Value::Array(values));
        }
    }

    pub(super) fn evaluate_predicate_on_row(
        &self,
        row: &HashMap<String, Value>,
        context: &ExecutionContext,
        expr: &parser::Expression,
    ) -> Result<bool> {
        let value = self.evaluate_projection_expression(row, context, expr)?;
        self.value_to_bool(&value)
    }

    pub(super) fn extract_entity_id(value: &Value) -> Option<u64> {
        match value {
            Value::Object(obj) => {
                if let Some(id) = obj.get("_nexus_id").and_then(|id| id.as_u64()) {
                    Some(id)
                } else if let Some(id) = obj
                    .get("_element_id")
                    .and_then(|id| id.as_str())
                    .and_then(|s| s.parse::<u64>().ok())
                {
                    Some(id)
                } else if let Some(id_value) = obj.get("id") {
                    match id_value {
                        Value::Number(num) => num.as_u64(),
                        Value::String(s) => s.parse::<u64>().ok(),
                        _ => None,
                    }
                } else {
                    None
                }
            }
            Value::Number(num) => num.as_u64(),
            _ => None,
        }
    }

    pub(super) fn read_relationship_as_value(&self, rel: &RelationshipInfo) -> Result<Value> {
        let type_name = self
            .catalog()
            .get_type_name(rel.type_id)?
            .unwrap_or_else(|| format!("type_{}", rel.type_id));

        let properties_value = self
            .store()
            .load_relationship_properties(rel.id)?
            .unwrap_or_else(|| Value::Object(Map::new()));

        let properties_map = match properties_value {
            Value::Object(map) => map,
            other => {
                let mut map = Map::new();
                map.insert("value".to_string(), other);
                map
            }
        };

        // Add _nexus_id for internal ID extraction (e.g., for type() function)
        // Add type property to identify this as a relationship object in deduplication
        let mut rel_obj = properties_map;
        rel_obj.insert("_nexus_id".to_string(), Value::Number(rel.id.into()));
        rel_obj.insert("type".to_string(), Value::String(type_name));

        // Return only the properties as a flat object, matching Neo4j's format
        Ok(Value::Object(rel_obj))
    }

    /// Phase 2.4.2: Optimize result_set_as_rows to reduce intermediate copies
    pub(super) fn result_set_as_rows(
        &self,
        context: &ExecutionContext,
    ) -> Vec<HashMap<String, Value>> {
        // Pre-size the result vector to avoid reallocations
        let capacity = context.result_set.rows.len();
        let mut result = Vec::with_capacity(capacity);

        for row in &context.result_set.rows {
            // Pre-size HashMap based on column count
            let mut map = HashMap::with_capacity(context.result_set.columns.len());
            for (idx, column) in context.result_set.columns.iter().enumerate() {
                if idx < row.values.len() {
                    // Use reference when possible, only clone when necessary
                    map.insert(column.clone(), row.values[idx].clone());
                } else {
                    map.insert(column.clone(), Value::Null);
                }
            }
            result.push(map);
        }

        result
    }
}

impl Default for Executor {
    fn default() -> Self {
        use std::sync::{Arc, Mutex, Once};

        // Use a shared record store for tests to prevent file descriptor leaks
        static INIT: Once = Once::new();
        static SHARED_STORE: Mutex<Option<RecordStore>> = Mutex::new(None);

        let mut store_guard = SHARED_STORE.lock().unwrap();
        if store_guard.is_none() {
            let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
            let store = RecordStore::new(temp_dir.path()).expect("Failed to create record store");
            // Keep temp_dir alive by leaking it (acceptable for testing)
            std::mem::forget(temp_dir);
            *store_guard = Some(store);
        }

        let store = store_guard.as_ref().unwrap().clone();
        let catalog = Catalog::default();
        let label_index = LabelIndex::default();
        let knn_index = KnnIndex::new_default(128).expect("Failed to create default KNN index");

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
        let knn_index = KnnIndex::new_default(128).unwrap();

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

    // TODO: Add JIT and parallel execution methods after core optimizations
}
