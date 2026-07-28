//! `Executor::execute` (public entrypoint) and the `seed_scan_main_loop`
//! helper shared by every label / index-seek scan operator dispatched
//! from [`super::operator_loop`].

use super::super::*;
use crate::Result;
use serde_json::Value;
use tracing;

impl Executor {
    /// Execute a Cypher query.
    ///
    /// Takes `&self` so clones can execute concurrently; all mutable state
    /// lives behind `Arc`/`RwLock` inside [`ExecutorShared`].
    ///
    /// Seed a scan variable in the main `execute` loop from an already
    /// materialised node list, applying the same cross-product / UNWIND
    /// fan-out / variable-materialisation logic regardless of whether the
    /// nodes came from a `NodeByLabel` full scan or a `NodeIndexSeek`
    /// point lookup. Extracted so both operators share one code path.
    ///
    /// `pub(super)`: called from `execute_inner`'s scan operator arms in
    /// the sibling `super::operator_loop` submodule.
    pub(super) fn seed_scan_main_loop(
        &self,
        context: &mut ExecutionContext,
        variable: &str,
        nodes: Vec<Value>,
    ) -> Result<()> {
        // CRITICAL FIX: Only clear result_set.rows if this is the first scan
        // For subsequent scan operators (comma-separated MATCH patterns),
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
            self.apply_cartesian_product(context, variable, nodes)?;
            // `apply_cartesian_product` has ALIGNED every array variable into
            // columns of the same product length (index `i` = one output row).
            // Falling through to the shared `materialize_rows_from_variables`
            // below would hit its `needs_cartesian_product` branch and RE-cross
            // these already-aligned columns into `N^k` rows (a two-pattern
            // `MATCH` over an 8-node label with 6 driving rows explodes 384
            // aligned rows to 384^3 ≈ 56.6M ≈ 13 GB, freezing the host). Zip the
            // aligned columns directly instead.
            // (phase0_fix-materialize-recrosses-aligned-columns)
            handled_cross_product = true;
            let rows = self.materialize_aligned_rows(context);
            self.update_result_set_from_rows(context, &rows);
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
            let mut expanded_nodes = Vec::with_capacity(existing_rows.len() * nodes.len());
            for _ in &existing_rows {
                expanded_nodes.extend(nodes.clone());
            }
            context.set_variable(variable, Value::Array(expanded_nodes));

            // Expand existing column values in variables too
            for (col_idx, col_name) in existing_columns.iter().enumerate() {
                let mut expanded_values = Vec::with_capacity(existing_rows.len() * nodes.len());
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

            tracing::trace!(
                "scan seed: cross-product with existing rows: {} x {} = {} rows",
                existing_rows.len(),
                nodes.len(),
                context.result_set.rows.len()
            );
        } else {
            context.set_variable(variable, Value::Array(nodes));
        }

        // Only materialize and update if we didn't already handle cross-product above
        if !handled_cross_product {
            let rows = self.materialize_rows_from_variables(context)?;
            self.update_result_set_from_rows(context, &rows);
        }
        Ok(())
    }

    /// Wraps [`Self::execute_inner`] to manage the per-thread planner
    /// notification sink: clear before planning so a panic-aborted
    /// prior call cannot leak its diagnostics into this query, then
    /// drain after the result is built and attach the notifications
    /// to the returned `ResultSet`. The thread-local sink is shared
    /// with [`crate::executor::planner::queries::stash_planner_notifications`]
    /// which the planner's per-call accumulator flushes into right
    /// before the planner is dropped.
    ///
    /// This is also **one of three** canonicalization points for typed
    /// temporal values (see `eval::temporal_value`): every row this call
    /// returns has any `_nexus_temporal_type`-tagged intermediate value —
    /// including ones nested inside a list or map — canonicalized to its
    /// ISO-8601 string before the `ResultSet` leaves the executor. This
    /// call covers the HTTP layer's read-only lock-free fast path
    /// (`lock_free_executor.execute(&query)`), `Engine::dispatch`'s
    /// fallback/standalone-CREATE branches, and PROFILE/EXPLAIN's internal
    /// re-execution — exactly the shape the openCypher TCK's
    /// `expressions/temporal` scenarios use (bare `RETURN`/`WITH`, no write
    /// clauses).
    ///
    /// It is **not** the only canonicalization point, and callers must not
    /// assume it is: `crate::engine`'s write path (`MERGE`/`SET`/`REMOVE`/
    /// `FOREACH`, dispatched through `Engine::execute_write_query`) builds
    /// its own inline `RETURN` result via
    /// `engine::write_exec::return_builder::{build_return_result,
    /// build_return_result_with_rels}`, which reads node/relationship
    /// properties straight out of storage and never calls this method —
    /// those two functions canonicalize independently. The property-value
    /// *storage* boundary (`CREATE`'s
    /// `executor::operators::create::Executor::resolve_property_expr_for_create`)
    /// is a fourth, separate point: it canonicalizes before a value is
    /// ever written to a node/relationship record, so a tagged value never
    /// reaches disk in the first place for that write path.
    pub fn execute(&self, query: &Query) -> Result<ResultSet> {
        // Drain (and discard) any stale notifications from a prior
        // panic-aborted call before planning the new query. Equivalent
        // to a clear, but reuses the existing drain helper.
        let _stale = planner::queries::drain_pending_planner_notifications();

        let mut result = self.execute_inner(query)?;

        for row in &mut result.rows {
            for value in &mut row.values {
                eval::temporal_value::canonicalize_value_in_place(value);
            }
        }

        // Attach planner-level diagnostics produced for this call.
        // Vec is empty in the hot path (no unindexed access), so this
        // is a near-zero-cost append.
        let notes = planner::queries::drain_pending_planner_notifications();
        if !notes.is_empty() {
            result.notifications.extend(notes);
        }
        Ok(result)
    }
}
