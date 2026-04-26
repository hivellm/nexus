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
//! `ERR_CALL_IN_TX_CONCURRENCY_UNSUPPORTED`. The savepoint-bracketed
//! atomic-rollback behaviour from §3 of the design doc is the next
//! follow-up — slice-2 ships the ON ERROR + REPORT STATUS surface
//! that gates the operator's user-visible contract.
//!
//! See `phase6_opencypher-subquery-transactions/design.md` for the
//! full target execution model.

use super::super::context::ExecutionContext;
use super::super::engine::Executor;
use super::super::parser;
use super::super::types::{Operator, ResultSet, Row};
use crate::{Error, Result};
use chrono::Utc;
use serde_json::Value;

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
    pub(in crate::executor) fn execute_call_subquery(
        &self,
        context: &mut ExecutionContext,
        inner_query: &parser::CypherQuery,
        in_transactions: bool,
        batch_size: Option<usize>,
        concurrency: Option<usize>,
        on_error: &parser::OnErrorPolicy,
        status_var: Option<&str>,
    ) -> Result<()> {
        // Multi-worker `IN CONCURRENT TRANSACTIONS` requires per-worker
        // MVCC isolation that today's single-writer storage layer does
        // not provide. The grammar emits `Some(1)` for the single-
        // worker variant (parser path), which is permitted; anything
        // larger is refused here — slice-3 of the task tracks the
        // sharded / per-worker isolation lift.
        if matches!(concurrency, Some(n) if n > 1) {
            return Err(Error::executor(
                "ERR_CALL_IN_TX_CONCURRENCY_UNSUPPORTED: multi-worker \
                 IN CONCURRENT TRANSACTIONS requires per-worker MVCC \
                 isolation; only single-worker is currently supported"
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
            );
        }

        let batch_n = batch_size.unwrap_or(DEFAULT_BATCH_SIZE).max(1);
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
            // not leak into a successful retry.
            for attempt in 0..=max_attempts {
                let mut batch_rows: Vec<Row> = Vec::new();
                let mut batch_inner_columns_seen: Vec<String> = inner_columns_seen.clone();
                let mut batch_err: Option<Error> = None;

                for outer_row in batch {
                    let mut inner_ctx =
                        match self.build_inner_ctx(context, outer_columns, outer_row) {
                            Ok(c) => c,
                            Err(e) => {
                                batch_err = Some(e);
                                break;
                            }
                        };
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

    /// Read-only inner: every outer row drives one inner invocation;
    /// inner failures propagate to the outer query.
    fn run_call_subquery_serial(
        &self,
        context: &mut ExecutionContext,
        inner_operators: &[Operator],
        inner_has_return: bool,
        outer_columns: &[String],
        driver_rows: &[Row],
    ) -> Result<()> {
        let mut joined_columns: Vec<String> = outer_columns.to_vec();
        let mut joined_rows: Vec<Row> = Vec::new();
        let mut inner_columns_seen: Vec<String> = Vec::new();

        for outer_row in driver_rows {
            let mut inner_ctx = self.build_inner_ctx(context, outer_columns, outer_row)?;
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
    /// the outer scope's parameters and variable bindings projected
    /// from the current outer row.
    fn build_inner_ctx(
        &self,
        outer: &ExecutionContext,
        outer_columns: &[String],
        outer_row: &Row,
    ) -> Result<ExecutionContext> {
        // ExecutionContext is not Clone; manually rebuild it with the
        // outer's params + cache + plan hints, then import variable
        // bindings.
        let mut inner = ExecutionContext::new(outer.params.clone(), outer.cache.clone());
        inner.set_plan_hints(outer.plan_hints.clone());

        // Import every outer variable. The lexical-scope tree upgrade
        // (slice-4) narrows this to a declared import-list per Cypher
        // 25 scoping; until then we match the legacy `WITH a, b CALL
        // { … }` semantic where the entire outer scope is visible.
        for (k, v) in &outer.variables {
            inner.set_variable(k, v.clone());
        }

        // Project the current outer row's columns into single-value
        // bindings so the inner sees scalars rather than the outer's
        // multi-row arrays.
        for (idx, col) in outer_columns.iter().enumerate() {
            if let Some(v) = outer_row.values.get(idx) {
                inner.set_variable(col, v.clone());
            }
        }

        // Seed the inner result_set with the single driving outer row
        // so row-driven inner operators (UNWIND, projection) have
        // somewhere to start.
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
