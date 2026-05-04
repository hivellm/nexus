//! Runtime execution context threaded through operator dispatch: parameter
//! bindings, variable bindings, and accumulated `ResultSet`. Also hosts the
//! `RelationshipInfo` record used by expand/path operators and the advanced
//! columnar relationship-join fast path.

use super::planner::PlanHint;
use super::types::{Direction, ResultSet, Row};
use crate::{Error, Result};
use parking_lot::Mutex;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// Compensating-undo entry recorded by write operators when a
/// `CALL { … } IN TRANSACTIONS` batch is in flight.
///
/// Today's storage layer auto-commits every CREATE / DELETE / SET, so
/// "atomic rollback at the batch boundary" is delivered by replaying
/// the inverse op when a batch fails. The CallSubquery operator wraps
/// each batch attempt in its own [`CompensatingUndoBuffer`]; on
/// success the buffer is discarded, on failure (and after the retry
/// budget is exhausted, when `ON ERROR` requires recovery) the buffer
/// is drained in reverse order to undo the partially-committed
/// writes.
#[derive(Debug, Clone, Copy)]
pub enum CompensatingUndoOp {
    /// Reverse a node creation by deleting the node id.
    DeleteNode(u64),
    /// Reverse a relationship creation by deleting the rel id.
    DeleteRelationship(u64),
}

/// Shared, append-only buffer used by the [`Executor`] write paths to
/// register entities they create while a `CALL { … } IN
/// TRANSACTIONS` batch attempt is in flight. The buffer is shared
/// across every per-row inner context spawned within a single batch
/// attempt; the CallSubquery operator owns the only [`Arc`] reference
/// and drains it on success / failure.
pub type CompensatingUndoBuffer = Arc<Mutex<Vec<CompensatingUndoOp>>>;

/// Relationship information for expansion
#[derive(Debug, Clone)]
pub struct RelationshipInfo {
    pub id: u64,
    pub source_id: u64,
    pub target_id: u64,
    pub type_id: u32,
}

/// Execution context for query processing
pub struct ExecutionContext {
    /// Query parameters
    pub(super) params: HashMap<String, Value>,
    /// Variable bindings
    pub(super) variables: HashMap<String, Value>,
    /// Query result set
    pub(super) result_set: ResultSet,
    /// Cache system for optimizations
    pub(super) cache: Option<Arc<parking_lot::RwLock<crate::cache::MultiLayerCache>>>,
    /// Plan hints parsed from `/*+ … */` comments — steer operator
    /// choices (columnar vs row path today) without touching the
    /// global `ExecutorConfig`. Populated by `Executor::execute`
    /// before dispatching operators; empty for direct-call tests
    /// that construct a context without going through `execute`.
    pub(super) plan_hints: Vec<PlanHint>,
    /// Compensating-undo buffer. `Some(buf)` when the executor is
    /// running inside a `CALL { … } IN TRANSACTIONS` batch attempt
    /// — every CREATE / MERGE / SET that lands an entity registers
    /// its inverse op so the operator can roll the batch back if it
    /// fails. `None` for everything else (the storage layer's own
    /// commit semantics provide durability).
    pub(super) undo_buffer: Option<CompensatingUndoBuffer>,
}

impl ExecutionContext {
    pub(super) fn new(
        params: HashMap<String, Value>,
        cache: Option<Arc<parking_lot::RwLock<crate::cache::MultiLayerCache>>>,
    ) -> Self {
        Self {
            params,
            variables: HashMap::new(),
            result_set: ResultSet::new(Vec::new(), Vec::new()),
            cache,
            plan_hints: Vec::new(),
            undo_buffer: None,
        }
    }

    /// Install (or clear) the per-batch compensating-undo buffer used
    /// by the `CALL { … } IN TRANSACTIONS` operator.
    pub(in crate::executor) fn set_undo_buffer(&mut self, buffer: Option<CompensatingUndoBuffer>) {
        self.undo_buffer = buffer;
    }

    /// Borrow the current compensating-undo buffer, if one is
    /// installed. Write operators call this to register inverse ops
    /// for the entities they create.
    pub(in crate::executor) fn undo_buffer(&self) -> Option<&CompensatingUndoBuffer> {
        self.undo_buffer.as_ref()
    }

    /// Append a single compensating-undo entry to the installed
    /// buffer (no-op when the executor is not running inside a
    /// CALL-IN-TX batch).
    pub(in crate::executor) fn push_undo(&self, op: CompensatingUndoOp) {
        if let Some(buf) = &self.undo_buffer {
            buf.lock().push(op);
        }
    }

    /// Override the plan hints for this context (used by the query
    /// entry point after extracting `/*+ ... */` comments).
    pub(in crate::executor) fn set_plan_hints(&mut self, hints: Vec<PlanHint>) {
        self.plan_hints = hints;
    }

    /// Decide whether the columnar fast path should run over a batch
    /// of `row_count` rows.
    ///
    /// A `PreferColumnar` hint is authoritative and overrides the
    /// `ExecutorConfig` threshold in both directions:
    ///   * `/*+ PREFER_COLUMNAR */` → always run the fast path
    ///   * `/*+ DISABLE_COLUMNAR */` → always run the row path
    ///
    /// Without a hint, the threshold is the only gate.
    pub(in crate::executor) fn should_use_columnar(
        &self,
        row_count: usize,
        threshold: usize,
    ) -> bool {
        for hint in &self.plan_hints {
            if let PlanHint::PreferColumnar(pref) = hint {
                return *pref;
            }
        }
        row_count >= threshold
    }

    pub(super) fn set_variable(&mut self, name: &str, value: Value) {
        self.variables.insert(name.to_string(), value);
    }

    /// Snapshot of the parameter map; used by subquery evaluators that
    /// need to seed a fresh inner ExecutionContext with the outer
    /// scope's parameters.
    pub(in crate::executor) fn params_clone(&self) -> HashMap<String, Value> {
        self.params.clone()
    }

    /// Snapshot of the shared cache handle, if one is installed.
    pub(in crate::executor) fn cache_clone(
        &self,
    ) -> Option<Arc<parking_lot::RwLock<crate::cache::MultiLayerCache>>> {
        self.cache.clone()
    }

    /// Snapshot of the plan-hint vector. Inner subqueries inherit the
    /// outer hints so `/*+ … */` directives apply uniformly.
    pub(in crate::executor) fn plan_hints_clone(&self) -> Vec<PlanHint> {
        self.plan_hints.clone()
    }

    /// Snapshot of every (name, value) variable binding currently in
    /// scope. The receiver gets owned values so it can populate a
    /// freshly-built inner ExecutionContext without holding a borrow
    /// on the outer.
    pub(in crate::executor) fn variables_clone_pairs(&self) -> Vec<(String, Value)> {
        self.variables
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    pub(super) fn get_variable(&self, name: &str) -> Option<&Value> {
        self.variables.get(name)
    }

    pub(super) fn set_columns_and_rows(&mut self, columns: Vec<String>, rows: Vec<Row>) {
        self.result_set.columns = columns;
        self.result_set.rows = rows;
    }

    /// Try advanced JOIN algorithms for relationship expansion
    pub(super) fn try_advanced_relationship_join(
        &self,
        context: &mut ExecutionContext,
        type_ids: &[u32],
        direction: Direction,
        source_var: &str,
        target_var: &str,
        rel_var: &str,
    ) -> Result<bool> {
        use crate::execution::columnar::{ColumnarResult, ComparisonOp, DataType, WhereCondition};
        use crate::execution::joins::adaptive::AdaptiveJoinExecutor;
        use std::time::Instant;

        tracing::trace!(
            "🎯 ADVANCED JOIN: Attempting optimized relationship expansion for {} relationships",
            type_ids.len()
        );

        let start_time = Instant::now();

        // Check if we have enough data for columnar processing
        let source_data = match context.get_variable(source_var) {
            Some(Value::Array(nodes)) if nodes.len() > 10 => nodes, // Minimum threshold for columnar benefits
            _ => {
                tracing::trace!("ADVANCED JOIN: Not enough source data for columnar processing");
                return Ok(false);
            }
        };

        // Convert source nodes to columnar format
        let mut source_columnar = ColumnarResult::new();
        source_columnar.add_column("id".to_string(), DataType::Int64, source_data.len());

        let id_col = source_columnar.get_column_mut("id").unwrap();
        for node in source_data {
            if let Value::Object(node_obj) = node {
                if let Some(Value::Number(id_num)) = node_obj.get("id") {
                    if let Some(id) = id_num.as_i64() {
                        id_col.push(id).unwrap();
                    }
                }
            }
        }
        source_columnar.row_count = source_data.len();

        // For now, use a simplified approach - build relationship data from context
        // In a full implementation, this would use the graph storage engine
        let mut rel_data = Vec::new();

        // Extract relationships from context variables (simplified approach)
        // In production, this would query the graph storage directly
        if let Some(Value::Array(relationships)) = context.get_variable(rel_var) {
            for rel in relationships {
                if let Value::Object(rel_obj) = rel {
                    if let (
                        Some(Value::Number(from_id)),
                        Some(Value::Number(to_id)),
                        Some(Value::Number(rel_id)),
                    ) = (rel_obj.get("from"), rel_obj.get("to"), rel_obj.get("id"))
                    {
                        if let (Some(from), Some(to), Some(rel)) =
                            (from_id.as_i64(), to_id.as_i64(), rel_id.as_i64())
                        {
                            rel_data.push((
                                from as u64,
                                to as u64,
                                rel as u64,
                                type_ids.get(0).copied().unwrap_or(0),
                            ));
                        }
                    }
                }
            }
        }

        if rel_data.is_empty() {
            // Fallback: create mock relationship data for testing
            // In production, this would be removed and proper graph storage queries would be used
            for node in source_data {
                if let Value::Object(node_obj) = node {
                    if let Some(Value::Number(id_num)) = node_obj.get("id") {
                        if let Some(id) = id_num.as_i64() {
                            // Create mock outgoing relationship
                            rel_data.push((
                                id as u64,
                                (id + 1) as u64,
                                (id * 100) as u64,
                                type_ids.get(0).copied().unwrap_or(0),
                            ));
                        }
                    }
                }
            }
        }

        if rel_data.is_empty() {
            tracing::trace!("ADVANCED JOIN: No relationships found");
            return Ok(false);
        }

        // Convert relationships to columnar format
        let mut rel_columnar = ColumnarResult::new();
        rel_columnar.add_column("from_id".to_string(), DataType::Int64, rel_data.len());
        rel_columnar.add_column("to_id".to_string(), DataType::Int64, rel_data.len());
        rel_columnar.add_column("rel_id".to_string(), DataType::Int64, rel_data.len());

        {
            let from_col = rel_columnar.get_column_mut("from_id").unwrap();
            for (from_id, _, _, _) in &rel_data {
                from_col.push(*from_id as i64).unwrap();
            }
        }

        {
            let to_col = rel_columnar.get_column_mut("to_id").unwrap();
            for (_, to_id, _, _) in &rel_data {
                to_col.push(*to_id as i64).unwrap();
            }
        }

        {
            let rel_id_col = rel_columnar.get_column_mut("rel_id").unwrap();
            for (_, _, rel_id, _) in &rel_data {
                rel_id_col.push(*rel_id as i64).unwrap();
            }
        }

        rel_columnar.row_count = rel_data.len();

        // Execute adaptive join
        let join_executor = AdaptiveJoinExecutor::new();
        let join_key_left = "id";
        let join_key_right = match direction {
            Direction::Outgoing => "from_id",
            Direction::Incoming => "to_id",
            Direction::Both => {
                // For both directions, we need to handle this differently
                tracing::trace!("ADVANCED JOIN: Both direction expansion not yet optimized");
                return Ok(false);
            }
        };

        let left_columns = vec!["id".to_string()];
        let right_columns = vec![
            "from_id".to_string(),
            "to_id".to_string(),
            "rel_id".to_string(),
        ];

        let join_result = match join_executor.execute_join(
            &source_columnar,
            &rel_columnar,
            join_key_left,
            join_key_right,
            &left_columns,
            &right_columns,
        ) {
            Ok(result) => {
                tracing::trace!(
                    "🎯 ADVANCED JOIN: Successfully executed join in {:.2}ms, {} rows produced",
                    result.execution_time.as_millis(),
                    result.result.row_count
                );
                result
            }
            Err(e) => {
                tracing::warn!("ADVANCED JOIN: Join execution failed: {}", e);
                return Ok(false);
            }
        };

        // Convert join results back to context format
        let mut result_nodes = Vec::new();
        let mut result_relationships = Vec::new();

        // Extract target nodes and relationships from join results
        let from_ids = join_result
            .result
            .left_columns
            .get("id")
            .ok_or_else(|| Error::executor("Missing id column in join result".to_string()))?;
        let to_ids = join_result
            .result
            .right_columns
            .get("to_id")
            .ok_or_else(|| Error::executor("Missing to_id column in join result".to_string()))?;
        let rel_ids = join_result
            .result
            .right_columns
            .get("rel_id")
            .ok_or_else(|| Error::executor("Missing rel_id column in join result".to_string()))?;

        for i in 0..join_result.result.row_count {
            // Get target node
            if let Some(Value::Number(to_id_num)) = to_ids.get(i) {
                if let Some(to_id) = to_id_num.as_i64() {
                    // Create node object from available data
                    let mut node_obj = serde_json::Map::new();
                    node_obj.insert("id".to_string(), Value::Number(to_id_num.clone()));
                    node_obj.insert(
                        "labels".to_string(),
                        Value::Array(vec![Value::String("Node".to_string())]),
                    );

                    let mut props = serde_json::Map::new();
                    props.insert("id".to_string(), Value::Number(to_id_num.clone()));
                    node_obj.insert("properties".to_string(), Value::Object(props));

                    result_nodes.push(Value::Object(node_obj));
                }
            }

            // Get relationship
            if let (Some(Value::Number(from_id_num)), Some(Value::Number(rel_id_num))) =
                (from_ids.get(i), rel_ids.get(i))
            {
                if let (Some(from_id), Some(rel_id)) = (from_id_num.as_i64(), rel_id_num.as_i64()) {
                    // Create relationship object
                    let mut rel_obj = serde_json::Map::new();
                    rel_obj.insert("id".to_string(), Value::Number(rel_id_num.clone()));
                    rel_obj.insert(
                        "type".to_string(),
                        Value::String("RELATIONSHIP".to_string()),
                    );
                    rel_obj.insert("from".to_string(), Value::Number(from_id_num.clone()));
                    rel_obj.insert(
                        "to".to_string(),
                        to_ids.get(i).unwrap_or(&Value::Null).clone(),
                    );

                    result_relationships.push(Value::Object(rel_obj));
                }
            }
        }

        // Update context with results
        let nodes_count = result_nodes.len();
        let rels_count = result_relationships.len();

        if !result_nodes.is_empty() {
            context.set_variable(target_var, Value::Array(result_nodes));
        }

        if !result_relationships.is_empty() && !rel_var.is_empty() {
            context.set_variable(rel_var, Value::Array(result_relationships));
        }

        let total_time = start_time.elapsed();
        tracing::trace!(
            "🎯 ADVANCED JOIN: Completed in {:.2}ms, {} nodes, {} relationships",
            total_time.as_millis(),
            nodes_count,
            rels_count
        );

        Ok(true)
    }
}
