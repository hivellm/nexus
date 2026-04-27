//! `CALL { … }` subquery operator
//! (phase6_opencypher-subquery-transactions slice-1 + slice-2).
//!
//! Executes the inner subquery once per outer row, joining each outer
//! row against the inner rows it produced (Neo4j 5.x CALL semantics).
//!
//! ## Coverage
//!
//! - **Read-only inner** (slice-1): MATCH / RETURN / UNWIND / WITH /
//!   WHERE clauses, including nested aggregations. Standalone `CALL {
//!   MATCH … RETURN … }` runs against a single empty driver row.
//! - **Write-bearing inner** (slice-1 lift + slice-2): `CALL {
//!   CREATE … }` flows through the dispatch path; the
//!   `Operator::Create` arm picks `execute_create_pattern_with_variables`
//!   for the empty-scope case and `execute_create_with_context` when
//!   the outer scope carries row-level bindings (e.g. `UNWIND … AS i
//!   CALL { WITH i CREATE (n {x: i}) }`).
//! - **`IN TRANSACTIONS`** (slice-2): batches `batch_size` outer rows
//!   per "commit boundary" and applies the `on_error` recovery policy
//!   — Fail / Continue / Break / Retry n. With `REPORT STATUS AS s`
//!   the operator emits one MAP-typed row per batch under the
//!   declared name, with keys `started`, `committed`, `rowsProcessed`,
//!   `err`.
//!
//! Multi-worker `IN CONCURRENT TRANSACTIONS` (slice-3) requires
//! per-worker MVCC isolation that the single-writer storage layer
//! does not yet provide; the operator refuses worker counts > 1 with
//! `ERR_CALL_IN_TX_CONCURRENCY_UNSUPPORTED`.
//!
//! ## Atomic-rollback (§3)
//!
//! Each batch attempt installs a per-attempt
//! [`CompensatingUndoBuffer`] (`Arc<Mutex<Vec<CompensatingUndoOp>>>`)
//! into every per-row inner [`ExecutionContext`]. The CREATE write
//! paths register `DeleteNode` / `DeleteRelationship` inverse ops
//! against that buffer for every entity they mint. On failure (or
//! before a retry attempt) the operator drains the buffer in
//! reverse order and replays each inverse op, restoring the catalog
//! to its pre-batch state. Successful batches discard the buffer
//! without replay. The mechanism is best-effort: individual undo
//! failures are logged via `tracing` and do not propagate, since
//! the user-visible error has already been captured at the batch
//! level.
//!
//! See `phase6_opencypher-subquery-transactions/design.md` for the
//! full target execution model.

use super::super::context::{CompensatingUndoOp, ExecutionContext};
use super::super::engine::Executor;
use super::super::parser;
use super::super::types::{Operator, ResultSet, Row};
use crate::{Error, Result};
use chrono::Utc;
use parking_lot::Mutex;
use serde_json::Value;
use std::sync::Arc;

/// Default per-batch row count when the user writes `IN TRANSACTIONS`
/// without an explicit `OF N ROWS` clause. Mirrors Neo4j 5.x.
const DEFAULT_BATCH_SIZE: usize = 1000;

impl Executor {
    /// Driver for `Operator::CallSubquery` (slice-1).
    ///
    /// `inner_query` is the AST captured at parse time and re-used
    /// across outer rows. The other fields mirror the
    /// `CallSubqueryClause` AST node and gate which execution path
    /// runs.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::executor) fn execute_call_subquery(
        &self,
        context: &mut ExecutionContext,
        inner_query: &parser::CypherQuery,
        in_transactions: bool,
        batch_size: Option<usize>,
        concurrency: Option<usize>,
        on_error: &parser::OnErrorPolicy,
        status_var: Option<&str>,
        import_list: Option<&[String]>,
    ) -> Result<()> {
        // `IN CONCURRENT TRANSACTIONS` resolves the parser's `Some(0)`
        // sentinel against the executor's `cypher_concurrency` config
        // (default 4). `Some(n)` with `n >= 2` requests an explicit
        // worker count. Serial CALL stays `None`. Single-worker
        // concurrent (`Some(1)`) collapses to the serial in-transactions
        // path so we don't pay thread-pool overhead for an effectively
        // sequential plan.
        let resolved_workers: Option<usize> = match concurrency {
            None => None,
            Some(0) => Some(self.config.cypher_concurrency.max(1)),
            Some(n) => Some(n),
        };
        let needs_concurrent_pool = matches!(resolved_workers, Some(n) if n > 1);
        if needs_concurrent_pool && !in_transactions {
            return Err(Error::executor(
                "ERR_CALL_IN_TX_INVALID_STATE: IN CONCURRENT TRANSACTIONS requires \
                 the IN TRANSACTIONS clause"
                    .to_string(),
            ));
        }
        // Defensive consistency checks — the §2 parser validation
        // already rejects `REPORT STATUS` / non-default `ON ERROR`
        // outside `IN TRANSACTIONS`, but a hand-built operator could
        // bypass that. Refuse the impossible combinations so the
        // executor stays self-validating.
        if status_var.is_some() && !in_transactions {
            return Err(Error::executor(
                "ERR_CALL_IN_TX_INVALID_STATE: REPORT STATUS AS <var> requires \
                 IN TRANSACTIONS"
                    .to_string(),
            ));
        }
        if !matches!(on_error, parser::OnErrorPolicy::Fail) && !in_transactions {
            return Err(Error::executor(
                "ERR_CALL_IN_TX_INVALID_STATE: ON ERROR clause requires \
                 IN TRANSACTIONS"
                    .to_string(),
            ));
        }

        // Snapshot outer driver. The inner runs against a fresh
        // result_set; we rebuild the outer's columns + rows after the
        // join so downstream operators see the joined view.
        let outer_columns = context.result_set.columns.clone();
        let outer_rows = std::mem::take(&mut context.result_set.rows);

        // Plan inner once. The inner AST is stable across outer rows,
        // so a single plan is correct and avoids per-row planner
        // overhead.
        let inner_operators = self.plan_ast(inner_query)?;
        let inner_has_return = inner_query
            .clauses
            .iter()
            .any(|c| matches!(c, parser::Clause::Return(_)));

        // Empty-driver edge case. Cypher 5.x semantics — runs the
        // inner once with an empty driving row when the outer
        // produced no rows AND has no bound variables (typical of a
        // standalone `CALL { MATCH (n) RETURN n }`).
        let driver_rows = if outer_rows.is_empty() && context.variables.is_empty() {
            vec![Row { values: Vec::new() }]
        } else {
            outer_rows
        };

        if !in_transactions {
            return self.run_call_subquery_serial(
                context,
                &inner_operators,
                inner_has_return,
                &outer_columns,
                &driver_rows,
                import_list,
            );
        }

        let batch_n = batch_size.unwrap_or(DEFAULT_BATCH_SIZE).max(1);

        if needs_concurrent_pool {
            // Safe by the `needs_concurrent_pool` guard above.
            let workers = resolved_workers.unwrap_or(1).max(2);
            return self.run_call_subquery_concurrent(
                context,
                inner_query,
                &inner_operators,
                inner_has_return,
                &outer_columns,
                &driver_rows,
                batch_n,
                workers,
                on_error,
                status_var,
                import_list,
            );
        }

        self.run_call_subquery_in_transactions(
            context,
            inner_query,
            &inner_operators,
            inner_has_return,
            &outer_columns,
            &driver_rows,
            batch_n,
            on_error,
            status_var,
            import_list,
        )
    }

    /// Transactional execution: groups of `batch_size` outer rows are
    /// processed under the `on_error` recovery policy, with an
    /// optional `status_var` driving per-batch reporting rows.
    ///
    /// Slice-2 surface (this method):
    ///
    /// - `Fail` (default): first error aborts the outer query.
    /// - `Continue`: log + skip the failing batch, keep going. With
    ///   `REPORT STATUS` the failure produces a `(committed=false,
    ///   err=…)` status row.
    /// - `Break`: stop processing further batches; emit collected
    ///   status rows so far (when `REPORT STATUS` is set).
    /// - `Retry n`: retry the failing batch up to `n` extra times
    ///   before escalating to `Fail`.
    ///
    /// The "transaction commit boundary" today is a per-batch
    /// row-by-row execution — the storage layer auto-commits each
    /// CREATE/DELETE/SET, so partial-batch durability matches the
    /// non-transactional path. The savepoint-bracketed atomic-rollback
    /// behaviour described in §3 of the task design is the next
    /// follow-up; this slice ships the ON ERROR + REPORT STATUS
    /// surface that gates the operator's user-visible contract.
    #[allow(clippy::too_many_arguments)]
    fn run_call_subquery_in_transactions(
        &self,
        context: &mut ExecutionContext,
        inner_query: &parser::CypherQuery,
        inner_operators: &[Operator],
        inner_has_return: bool,
        outer_columns: &[String],
        driver_rows: &[Row],
        batch_size: usize,
        on_error: &parser::OnErrorPolicy,
        status_var: Option<&str>,
        import_list: Option<&[String]>,
    ) -> Result<()> {
        // §2.3 already rejects RETURN inside the inner when REPORT
        // STATUS is set; guard defensively.
        if status_var.is_some()
            && inner_query
                .clauses
                .iter()
                .any(|c| matches!(c, parser::Clause::Return(_)))
        {
            return Err(Error::executor(
                "ERR_CALL_IN_TX_RETURN_WITH_STATUS: the inner subquery cannot \
                 declare RETURN when REPORT STATUS AS <var> is set"
                    .to_string(),
            ));
        }

        // Output buffers. Two distinct shapes share the same operator:
        //   * status_var set → output rows are single-column status
        //     reports under the user's bound name; the column value
        //     is a MAP `{started, committed, rowsProcessed, err}` so
        //     downstream `s.committed` PropertyAccess resolves.
        //   * status_var none → output rows are the inner-join view.
        let mut joined_columns: Vec<String> = if let Some(name) = status_var {
            vec![name.to_string()]
        } else {
            outer_columns.to_vec()
        };
        let mut joined_rows: Vec<Row> = Vec::new();
        let mut inner_columns_seen: Vec<String> = Vec::new();

        let max_attempts = match on_error {
            parser::OnErrorPolicy::Retry { max_attempts } => *max_attempts,
            _ => 0,
        };

        for batch in driver_rows.chunks(batch_size) {
            let started = Utc::now().to_rfc3339();
            let mut last_err: Option<Error> = None;
            let mut committed_rows: Vec<Row> = Vec::new();
            let mut succeeded = false;

            // Try-loop: 1 baseline attempt + up to `max_attempts`
            // retries. Buffers are rebuilt on each attempt so a
            // partially populated buffer from a previous attempt does
            // not leak into a successful retry. Each attempt also
            // gets its own compensating-undo buffer (shared across
            // every per-row inner context spawned within the
            // attempt); on failure we replay it in reverse order to
            // unwind the partially-committed writes.
            for attempt in 0..=max_attempts {
                let mut batch_rows: Vec<Row> = Vec::new();
                let mut batch_inner_columns_seen: Vec<String> = inner_columns_seen.clone();
                let mut batch_err: Option<Error> = None;
                let undo_buffer: Arc<Mutex<Vec<CompensatingUndoOp>>> =
                    Arc::new(Mutex::new(Vec::new()));

                for outer_row in batch {
                    let mut inner_ctx = match self.build_inner_ctx(
                        context,
                        outer_columns,
                        outer_row,
                        import_list,
                    ) {
                        Ok(c) => c,
                        Err(e) => {
                            batch_err = Some(e);
                            break;
                        }
                    };
                    inner_ctx.set_undo_buffer(Some(undo_buffer.clone()));
                    let mut row_err: Option<Error> = None;
                    for op in inner_operators {
                        if let Err(e) = self.execute_operator(&mut inner_ctx, op) {
                            row_err = Some(e);
                            break;
                        }
                    }
                    if let Some(e) = row_err {
                        batch_err = Some(e);
                        break;
                    }
                    if status_var.is_none() {
                        if let Err(e) = self.merge_inner_into_joined(
                            outer_row,
                            &mut inner_ctx,
                            &mut joined_columns,
                            &mut batch_rows,
                            &mut batch_inner_columns_seen,
                            inner_has_return,
                        ) {
                            batch_err = Some(e);
                            break;
                        }
                    }
                }

                if batch_err.is_none() {
                    succeeded = true;
                    if status_var.is_none() {
                        committed_rows = batch_rows;
                        inner_columns_seen = batch_inner_columns_seen;
                    }
                    break;
                }

                // Failed attempt — replay the compensating-undo
                // buffer so the next attempt (or the final FAIL /
                // CONTINUE / BREAK branch below) sees a consistent
                // catalog. Best-effort: any individual undo
                // failure is logged but not propagated; the
                // batch-level error already in `batch_err` is the
                // user-visible reason for the rollback.
                self.replay_compensating_undo(&undo_buffer);
                last_err = batch_err;
                if attempt < max_attempts {
                    tracing::warn!(
                        target: "nexus_core::executor::call_in_tx",
                        attempt = attempt + 1,
                        max = max_attempts,
                        err = %last_err.as_ref().map(|e| e.to_string()).unwrap_or_default(),
                        "CALL {{ ... }} IN TRANSACTIONS retrying batch"
                    );
                }
            }

            if !succeeded {
                let err_text = last_err
                    .as_ref()
                    .map(|e| e.to_string())
                    .unwrap_or_else(|| "unknown error".to_string());
                match on_error {
                    parser::OnErrorPolicy::Fail | parser::OnErrorPolicy::Retry { .. } => {
                        return Err(last_err.unwrap_or_else(|| {
                            Error::executor(
                                "ERR_CALL_IN_TX_BATCH_FAILED: batch failed without an \
                                 error payload"
                                    .to_string(),
                            )
                        }));
                    }
                    parser::OnErrorPolicy::Continue => {
                        if status_var.is_some() {
                            joined_rows.push(Row {
                                values: build_status_row(
                                    &started,
                                    false,
                                    batch.len(),
                                    Some(&err_text),
                                ),
                            });
                        }
                        continue;
                    }
                    parser::OnErrorPolicy::Break => {
                        if status_var.is_some() {
                            joined_rows.push(Row {
                                values: build_status_row(
                                    &started,
                                    false,
                                    batch.len(),
                                    Some(&err_text),
                                ),
                            });
                        }
                        break;
                    }
                }
            }

            if status_var.is_some() {
                joined_rows.push(Row {
                    values: build_status_row(&started, true, batch.len(), None),
                });
            } else {
                joined_rows.extend(committed_rows);
            }
        }

        context.result_set = ResultSet {
            columns: joined_columns,
            rows: joined_rows,
        };
        Ok(())
    }

    /// `IN CONCURRENT TRANSACTIONS` worker pool driver
    /// (phase6_opencypher-subquery-transactions §6).
    ///
    /// Splits the outer driver rows round-robin across `workers`
    /// scoped threads. Each worker holds its own
    /// [`ExecutionContext`] shadow and runs the same per-batch
    /// pipeline as the serial in-transactions path against its
    /// shard. Workers prepare batches in parallel; commits serialise
    /// through the storage layer's single-writer `RwLock` (so the
    /// MVCC invariant is preserved without the engine needing
    /// per-worker isolation).
    ///
    /// Result merging:
    ///
    /// - When `status_var` is set, every worker emits status rows
    ///   under the declared name; the outer result is the
    ///   concatenation of all workers' status streams in completion
    ///   order (Cypher 25 leaves order undefined under concurrency).
    /// - When `status_var` is `None`, joined inner rows from each
    ///   worker accumulate into a single output buffer; column
    ///   shape comes from the first worker that produces a row.
    /// - Errors: the first failing worker wins; remaining workers
    ///   keep running (they can't be cancelled mid-batch without
    ///   risking partial undo state) but their results are
    ///   discarded once a fatal error is observed. The captured
    ///   error is returned to the caller and the joined result is
    ///   left untouched on the outer context.
    #[allow(clippy::too_many_arguments)]
    fn run_call_subquery_concurrent(
        &self,
        context: &mut ExecutionContext,
        inner_query: &parser::CypherQuery,
        inner_operators: &[Operator],
        inner_has_return: bool,
        outer_columns: &[String],
        driver_rows: &[Row],
        batch_size: usize,
        workers: usize,
        on_error: &parser::OnErrorPolicy,
        status_var: Option<&str>,
        import_list: Option<&[String]>,
    ) -> Result<()> {
        // Round-robin shard the driver rows across workers. We clone
        // each row once into the shard; rows are typically small (a
        // single Value vector) so the up-front allocation is cheap
        // compared to the cross-thread parallelism.
        let mut shards: Vec<Vec<Row>> = (0..workers).map(|_| Vec::new()).collect();
        for (idx, row) in driver_rows.iter().enumerate() {
            shards[idx % workers].push(row.clone());
        }

        // Snapshot inputs each worker needs to rebuild its shadow
        // outer context. ExecutionContext is intentionally not
        // Clone, so we capture only the fields a worker requires
        // and reconstruct via `ExecutionContext::new` + setters.
        let outer_columns_owned: Vec<String> = outer_columns.to_vec();
        let import_list_owned: Option<Vec<String>> = import_list.map(|v| v.to_vec());
        let status_var_owned: Option<String> = status_var.map(String::from);
        let inner_query_owned = inner_query.clone();
        let inner_ops_owned = inner_operators.to_vec();
        let on_error_owned = on_error.clone();
        let params_owned = context.params.clone();
        let cache_owned = context.cache.clone();
        let plan_hints_owned = context.plan_hints.clone();
        let variables_owned: Vec<(String, serde_json::Value)> = context
            .variables
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        let combined_rows: Arc<Mutex<Vec<Row>>> = Arc::new(Mutex::new(Vec::new()));
        let combined_columns: Arc<Mutex<Option<Vec<String>>>> = Arc::new(Mutex::new(None));
        let first_error: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

        std::thread::scope(|scope| {
            for shard in shards.into_iter().filter(|s| !s.is_empty()) {
                let exec = self.clone();
                let inner_query = inner_query_owned.clone();
                let inner_ops = inner_ops_owned.clone();
                let outer_cols = outer_columns_owned.clone();
                let import_list = import_list_owned.clone();
                let on_error = on_error_owned.clone();
                let status_var = status_var_owned.clone();
                let params = params_owned.clone();
                let cache = cache_owned.clone();
                let plan_hints = plan_hints_owned.clone();
                let variables = variables_owned.clone();
                let combined_rows = combined_rows.clone();
                let combined_columns = combined_columns.clone();
                let first_error = first_error.clone();

                scope.spawn(move || {
                    let mut shadow = ExecutionContext::new(params, cache);
                    shadow.set_plan_hints(plan_hints);
                    for (k, v) in &variables {
                        shadow.set_variable(k, v.clone());
                    }
                    shadow.set_columns_and_rows(outer_cols.clone(), shard.clone());

                    let res = exec.run_call_subquery_in_transactions(
                        &mut shadow,
                        &inner_query,
                        &inner_ops,
                        inner_has_return,
                        &outer_cols,
                        &shard,
                        batch_size,
                        &on_error,
                        status_var.as_deref(),
                        import_list.as_deref(),
                    );

                    match res {
                        Ok(()) => {
                            let mut cols_lock = combined_columns.lock();
                            if cols_lock.is_none() && !shadow.result_set.columns.is_empty() {
                                *cols_lock = Some(shadow.result_set.columns.clone());
                            }
                            drop(cols_lock);
                            let mut rows_lock = combined_rows.lock();
                            rows_lock.extend(shadow.result_set.rows);
                        }
                        Err(e) => {
                            let mut slot = first_error.lock();
                            if slot.is_none() {
                                *slot = Some(e.to_string());
                            }
                        }
                    }
                });
            }
        });

        if let Some(err_text) = first_error.lock().take() {
            return Err(Error::executor(err_text));
        }

        let cols = combined_columns
            .lock()
            .clone()
            .unwrap_or_else(|| match status_var {
                Some(name) => vec![name.to_string()],
                None => outer_columns_owned.clone(),
            });
        let rows = std::mem::take(&mut *combined_rows.lock());
        context.result_set = ResultSet {
            columns: cols,
            rows,
        };
        Ok(())
    }

    /// Read-only inner: every outer row drives one inner invocation;
    /// inner failures propagate to the outer query.
    #[allow(clippy::too_many_arguments)]
    fn run_call_subquery_serial(
        &self,
        context: &mut ExecutionContext,
        inner_operators: &[Operator],
        inner_has_return: bool,
        outer_columns: &[String],
        driver_rows: &[Row],
        import_list: Option<&[String]>,
    ) -> Result<()> {
        let mut joined_columns: Vec<String> = outer_columns.to_vec();
        let mut joined_rows: Vec<Row> = Vec::new();
        let mut inner_columns_seen: Vec<String> = Vec::new();

        for outer_row in driver_rows {
            let mut inner_ctx =
                self.build_inner_ctx(context, outer_columns, outer_row, import_list)?;
            for op in inner_operators {
                self.execute_operator(&mut inner_ctx, op)?;
            }
            self.merge_inner_into_joined(
                outer_row,
                &mut inner_ctx,
                &mut joined_columns,
                &mut joined_rows,
                &mut inner_columns_seen,
                inner_has_return,
            )?;
        }

        context.result_set = ResultSet {
            columns: joined_columns,
            rows: joined_rows,
        };
        Ok(())
    }

    /// Construct the inner subquery's `ExecutionContext`, importing
    /// the outer scope's parameters and the variable bindings
    /// projected from the current outer row.
    ///
    /// `import_list` controls scope visibility (phase6 §8):
    ///
    /// * `Some(&["a", "b"])` — Cypher 25 scoped form. Only `a` and
    ///   `b` from the outer scope (and only the matching outer-row
    ///   columns) are visible inside the inner.
    /// * `Some(&[])` — empty import list. The inner sees a fresh
    ///   scope with NO outer bindings.
    /// * `None` — legacy form. Every outer variable + every column
    ///   of the current outer row is visible (matches the old
    ///   `WITH … CALL { … }` semantic).
    /// Drain the per-attempt compensating-undo buffer in reverse
    /// order, applying each inverse op against the storage layer.
    /// Errors from individual undo ops are logged via `tracing` and
    /// otherwise ignored — the caller has already captured a
    /// batch-level error and the role of this routine is best-effort
    /// state restoration.
    fn replay_compensating_undo(&self, buffer: &Arc<Mutex<Vec<CompensatingUndoOp>>>) {
        let mut ops = buffer.lock();
        while let Some(op) = ops.pop() {
            let outcome = match op {
                CompensatingUndoOp::DeleteNode(node_id) => self.store_mut().delete_node(node_id),
                CompensatingUndoOp::DeleteRelationship(rel_id) => {
                    self.store_mut().delete_rel(rel_id)
                }
            };
            if let Err(e) = outcome {
                tracing::warn!(
                    target: "nexus_core::executor::call_in_tx",
                    op = ?op,
                    err = %e,
                    "compensating undo failed; continuing best-effort rollback"
                );
            }
        }
    }

    fn build_inner_ctx(
        &self,
        outer: &ExecutionContext,
        outer_columns: &[String],
        outer_row: &Row,
        import_list: Option<&[String]>,
    ) -> Result<ExecutionContext> {
        // ExecutionContext is not Clone; manually rebuild it with the
        // outer's params + cache + plan hints, then import variable
        // bindings under the scope-narrowing rule.
        let mut inner = ExecutionContext::new(outer.params.clone(), outer.cache.clone());
        inner.set_plan_hints(outer.plan_hints.clone());

        let import_filter = |name: &str| -> bool {
            match import_list {
                Some(list) => list.iter().any(|v| v == name),
                None => true,
            }
        };

        for (k, v) in &outer.variables {
            if import_filter(k) {
                inner.set_variable(k, v.clone());
            }
        }

        // Project the current outer row's columns into single-value
        // bindings so the inner sees scalars rather than the outer's
        // multi-row arrays. Filter through the same import rule.
        for (idx, col) in outer_columns.iter().enumerate() {
            if !import_filter(col) {
                continue;
            }
            if let Some(v) = outer_row.values.get(idx) {
                inner.set_variable(col, v.clone());
            }
        }

        // Seed the inner result_set with the single driving outer row
        // so row-driven inner operators (UNWIND, projection) have
        // somewhere to start. The seed always keeps the full row
        // shape — narrowing happens at the variable-binding layer,
        // not at the row-buffer layer, so downstream operators that
        // walk `result_set.rows` (e.g. CREATE) still see a non-empty
        // driver to iterate over.
        inner.set_columns_and_rows(outer_columns.to_vec(), vec![outer_row.clone()]);
        Ok(inner)
    }

    /// Merge the inner `result_set` into the running joined buffer.
    /// Errors out if successive inner invocations disagree on column
    /// shape — Cypher requires rectangular results.
    fn merge_inner_into_joined(
        &self,
        outer_row: &Row,
        inner_ctx: &mut ExecutionContext,
        joined_columns: &mut Vec<String>,
        joined_rows: &mut Vec<Row>,
        inner_columns_seen: &mut Vec<String>,
        inner_has_return: bool,
    ) -> Result<()> {
        if !inner_ctx.result_set.columns.is_empty() && inner_columns_seen.is_empty() {
            *inner_columns_seen = inner_ctx.result_set.columns.clone();
            for c in inner_columns_seen.iter() {
                if !joined_columns.contains(c) {
                    joined_columns.push(c.clone());
                }
            }
        } else if !inner_ctx.result_set.columns.is_empty()
            && inner_ctx.result_set.columns != *inner_columns_seen
        {
            return Err(Error::executor(
                "ERR_CALL_INNER_COLUMN_DRIFT: inner subquery produced different \
                 RETURN columns across outer rows"
                    .to_string(),
            ));
        }

        if inner_ctx.result_set.rows.is_empty() {
            // Inner emitted no rows. With a RETURN clause this is an
            // inner-join miss; without one (pure side-effect form, not
            // reachable in slice-1's read-only mode), the outer row
            // would pass through.
            if !inner_has_return {
                joined_rows.push(outer_row.clone());
            }
            return Ok(());
        }

        for inner_row in std::mem::take(&mut inner_ctx.result_set.rows) {
            let mut merged = outer_row.values.clone();
            merged.extend(inner_row.values);
            joined_rows.push(Row { values: merged });
        }
        Ok(())
    }
}

/// Build the per-batch status row emitted under `REPORT STATUS AS <var>`.
/// The row carries a single value — a MAP keyed by `started` (STRING,
/// RFC-3339), `committed` (BOOLEAN), `rowsProcessed` (INTEGER) and
/// `err` (STRING?) — bound under the user-declared variable name. A
/// downstream `RETURN s.committed AS … ` then resolves `committed`
/// via property access on the map.
fn build_status_row(
    started: &str,
    committed: bool,
    rows_processed: usize,
    err: Option<&str>,
) -> Vec<Value> {
    let mut map = serde_json::Map::with_capacity(4);
    map.insert("started".to_string(), Value::String(started.to_string()));
    map.insert("committed".to_string(), Value::Bool(committed));
    map.insert(
        "rowsProcessed".to_string(),
        Value::Number(serde_json::Number::from(rows_processed as u64)),
    );
    map.insert(
        "err".to_string(),
        match err {
            Some(s) => Value::String(s.to_string()),
            None => Value::Null,
        },
    );
    vec![Value::Object(map)]
}
