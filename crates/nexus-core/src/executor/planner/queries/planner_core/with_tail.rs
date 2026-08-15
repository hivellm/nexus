//! `ORDER BY` / `SKIP` / `LIMIT` written after a `WITH`.
//!
//! The parser flattens these into standalone clauses that merely *follow*
//! the `WITH` in the clause list, so the planner used to fold them into the
//! same query-wide `order_by_clause` / `skip_count` / `limit_count` slots the
//! final `RETURN` writes to. Two things went wrong with that:
//!
//! - the operators landed at the **end** of the pipeline, so anything the
//!   next clause did — an aggregation above all — saw the *unlimited* stream
//!   (`UNWIND [1,2,3,4,5] AS x WITH x LIMIT 2 RETURN count(x)` answered 5);
//! - a single slot means the later clause overwrites the earlier one, so of
//!   `WITH x LIMIT 2 ... RETURN x LIMIT 4` only the 4 survived.
//!
//! Both are fixed by keeping each `WITH`'s tail with the `WITH` it belongs to
//! and lowering it into `Sort` → `Skip` → `Limit` immediately after that
//! `WITH`'s operator — openCypher's own clause order.

use super::super::*;
use crate::executor::operators::project::ORDER_BY_HIDDEN_KEY_PREFIX;

/// The `ORDER BY` / `SKIP` / `LIMIT` that follow one `WITH`.
#[derive(Debug, Default, Clone)]
pub(super) struct WithTail {
    /// Sort keys as written, plus a per-key ascending flag. Resolved against
    /// the `WITH`'s projection at lowering time.
    pub(super) order_by: Option<(Vec<String>, Vec<bool>)>,
    pub(super) skip: Option<usize>,
    pub(super) limit: Option<usize>,
}

impl WithTail {
    pub(super) fn is_empty(&self) -> bool {
        self.order_by.is_none() && self.skip.is_none() && self.limit.is_none()
    }
}

impl QueryPlanner<'_> {
    /// Lower a `WITH`'s tail into the operators that must run directly after
    /// that `WITH`, in openCypher order: `ORDER BY`, then `SKIP`, then
    /// `LIMIT`.
    ///
    /// `projection_items` is the `WITH`'s own projection and may be extended
    /// here: `Sort` reads columns off the result set by name, so a sort key
    /// the `WITH` does not project (`WITH p ORDER BY p.age` projects only
    /// `p`) has to be carried across the projection as a hidden column.
    /// [`ORDER_BY_HIDDEN_KEY_PREFIX`] marks those, and `execute_sort` drops
    /// them again once the rows are ordered — the same mechanism the
    /// `RETURN` path uses in `resolve_order_by_columns`.
    ///
    /// `hidden_sort_keys` is the caller's permission to do that. Two callers
    /// withhold it: a `DISTINCT` projection, where an extra column changes
    /// which rows count as duplicates (silently altering the row set to make
    /// sorting convenient is the worse failure), and an aggregating `WITH`,
    /// whose projection belongs to the `Aggregate` operator and is not this
    /// vector. A key that cannot be carried resolves to its own name and
    /// simply finds no column to sort by.
    pub(super) fn lower_with_tail(
        &self,
        tail: &WithTail,
        hidden_sort_keys: bool,
        projection_items: &mut Vec<ProjectionItem>,
    ) -> Vec<Operator> {
        let mut lowered = Vec::new();

        if let Some((columns, ascending)) = &tail.order_by {
            let columns =
                self.resolve_with_order_by_columns(columns, hidden_sort_keys, projection_items);
            lowered.push(Operator::Sort {
                columns,
                ascending: ascending.clone(),
            });
        }
        if let Some(skip) = tail.skip {
            lowered.push(Operator::Skip { count: skip });
        }
        if let Some(limit) = tail.limit {
            lowered.push(Operator::Limit { count: limit });
        }

        lowered
    }

    /// Map each sort key onto the column name it will have after the `WITH`
    /// projection runs, appending a hidden projection item for any key the
    /// projection does not already carry.
    fn resolve_with_order_by_columns(
        &self,
        columns: &[String],
        hidden_sort_keys: bool,
        projection_items: &mut Vec<ProjectionItem>,
    ) -> Vec<String> {
        // What the projection already exposes: the alias of each item, plus
        // the source expression that produced it (`WITH p.age AS a ORDER BY
        // p.age` must find `a`).
        let mut expression_to_alias: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        let mut aliases: std::collections::HashSet<String> = std::collections::HashSet::new();
        for item in projection_items.iter() {
            if let Ok(expr_str) = self.expression_to_string(&item.expression) {
                expression_to_alias.insert(expr_str, item.alias.clone());
            }
            aliases.insert(item.alias.clone());
        }

        let mut hidden = Vec::new();
        let resolved = columns
            .iter()
            .enumerate()
            .map(|(idx, col)| {
                if aliases.contains(col) {
                    return col.clone();
                }
                if let Some(alias) = expression_to_alias.get(col) {
                    return alias.clone();
                }
                if !hidden_sort_keys {
                    return col.clone();
                }
                let mut parser = crate::executor::parser::CypherParser::new(col.clone());
                match parser.parse_expression() {
                    Ok(expression) => {
                        let alias = format!("{ORDER_BY_HIDDEN_KEY_PREFIX}with{idx}");
                        hidden.push(ProjectionItem {
                            expression,
                            alias: alias.clone(),
                        });
                        alias
                    }
                    Err(_) => col.clone(),
                }
            })
            .collect();

        projection_items.extend(hidden);
        resolved
    }
}
