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

    /// Execute NodeByLabel operator

    /// Execute Filter operator with index optimization

    /// Execute Project operator

    /// Execute Union operator

    /// Execute CREATE operator with context from MATCH
    /// Execute a single operator and return results
    /// Execute UNWIND operator - expands a list into rows

    /// Execute CALL procedure operator

    /// Execute CREATE INDEX command
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

    /// Check if value is a duration object (has years, months, days, hours, minutes, or seconds keys)

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
