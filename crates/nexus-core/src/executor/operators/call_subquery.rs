//! `CALL { … }` subquery operator (phase6_opencypher-subquery-transactions
//! slice-1).
//!
//! Executes the inner subquery once per outer row, then concatenates
//! the inner rows into the outer result set (Neo4j-compatible CALL
//! semantics: each outer row joins to the rows produced by its inner
//! invocation).
//!
//! ## Slice-1 scope
//!
//! Slice-1 covers the **read-only inner** path — the inner subquery
//! may issue MATCH/RETURN/UNWIND/WITH/WHERE clauses, including nested
//! aggregations, and the operator joins each outer row against the
//! inner rows it produced. Standalone `CALL { MATCH … RETURN … }` (no
//! preceding outer driver) is also covered via an empty driver-row
//! seed.
//!
//! Write-bearing inner subqueries (`CALL { CREATE … }`, `MERGE`,
//! `DELETE`, `SET`) and `IN TRANSACTIONS` semantics rely on the
//! re-entrant executor refactor tracked under slice-2; the operator
//! returns `ERR_CALL_SUBQUERY_WRITE_INNER_UNSUPPORTED` rather than
//! silently producing inconsistent state.
//!
//! See `phase6_opencypher-subquery-transactions/design.md` for the
//! full target execution model.

use super::super::context::ExecutionContext;
use super::super::engine::Executor;
use super::super::parser;
use super::super::types::{Operator, ResultSet, Row};
use crate::{Error, Result};

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
        _batch_size: Option<usize>,
        concurrency: Option<usize>,
        on_error: &parser::OnErrorPolicy,
        status_var: Option<&str>,
    ) -> Result<()> {
        // Slice-2 ships the IN TRANSACTIONS / ON ERROR / REPORT
        // STATUS variants once the executor's re-entrant write path
        // exists. Refusing them here is the documented contract — we
        // would otherwise emit silently-inconsistent results.
        if in_transactions
            || concurrency.is_some()
            || status_var.is_some()
            || !matches!(on_error, parser::OnErrorPolicy::Fail)
        {
            return Err(Error::executor(
                "ERR_CALL_IN_TX_PENDING_SLICE2: CALL { … } IN TRANSACTIONS / \
                 REPORT STATUS / ON ERROR are tracked under slice-2 of \
                 phase6_opencypher-subquery-transactions; the executor's \
                 re-entrant write path needs to land before this \
                 operator can wrap an inner CREATE / DELETE / MERGE / \
                 SET in a per-batch boundary"
                    .to_string(),
            ));
        }

        // Reject write-bearing inner clauses up-front. The dispatcher
        // does not yet route CREATE/MERGE/DELETE/SET through the
        // sub-execution context cleanly (write paths bypass dispatch
        // and live inside the top-level `execute()` loop). Slice-2
        // unifies these.
        if inner_subquery_has_writes(inner_query) {
            return Err(Error::executor(
                "ERR_CALL_SUBQUERY_WRITE_INNER_UNSUPPORTED: CALL { … } with \
                 a write-bearing inner subquery (CREATE / MERGE / DELETE / \
                 SET) requires the re-entrant executor refactor tracked \
                 under slice-2 of phase6_opencypher-subquery-transactions"
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

        self.run_call_subquery_serial(
            context,
            &inner_operators,
            inner_has_return,
            &outer_columns,
            &driver_rows,
        )
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

/// Walk the inner AST looking for a write-bearing clause. Slice-1
/// refuses these because the executor's write paths short-circuit
/// the dispatch loop and assume top-level `execute()` is driving
/// them — calling them from a sub-execution context can leave the
/// catalog and label index out of sync. Slice-2 unifies the write
/// paths through dispatch.
fn inner_subquery_has_writes(query: &parser::CypherQuery) -> bool {
    fn clause_has_writes(clause: &parser::Clause) -> bool {
        match clause {
            parser::Clause::Create(_) | parser::Clause::Merge(_) | parser::Clause::Delete(_) => {
                true
            }
            parser::Clause::CallSubquery(c) => c.query.clauses.iter().any(clause_has_writes),
            _ => false,
        }
    }
    query.clauses.iter().any(clause_has_writes)
}
