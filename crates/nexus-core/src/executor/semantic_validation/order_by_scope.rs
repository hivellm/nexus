//! What an `ORDER BY` after a `WITH` is allowed to name.
//!
//! ```cypher
//! MATCH (n) WITH n.num1 AS foo ORDER BY count(1) RETURN foo   -- InvalidAggregation
//! WITH 1 AS a, 3 AS c WITH a, 1 AS z WITH a ORDER BY c RETURN a  -- UndefinedVariable: c
//! ```
//!
//! Both ran and answered before this check. The reference checker cannot catch
//! the second: it validates against the binders of the *whole* query, where
//! `c` is bound perfectly well — just not any longer where the `ORDER BY`
//! reads it. Scope, not existence, is the question.
//!
//! Two things make the rule looser than it first looks, and getting either
//! wrong rejects valid Cypher:
//!
//! - **The visible names are the `WITH`'s output *plus* its input.** Sorting by
//!   something the `WITH` consumed but did not forward is legal —
//!   `WITH a, x + y AS sum WITH a, z AS m ORDER BY sum` sorts by `sum`, which
//!   the second `WITH` drops. Only a name that is in neither set is undefined.
//! - **An aggregate is judged per sub-expression, not per key.** The illegal
//!   thing is an aggregation the projection never computed; wrapping a
//!   projected one in more arithmetic is fine —
//!   `WITH avg(p.age) AS avg ORDER BY $x + avg(p.age) - 1000` is valid, and
//!   comparing the whole key against the projection would reject it.

use std::collections::HashSet;

use super::{
    Clause, CypherQuery, Expression, Pattern, PatternElement, ReturnItem, child_exprs,
    is_aggregate_name,
};

/// Reject an `ORDER BY` that aggregates something its projection did not
/// compute, or that names something no longer in scope.
///
/// Only `ORDER BY` after a `WITH` is checked. A `RETURN`'s `ORDER BY` obeys
/// the same idea but with a wider input scope that this pass does not model,
/// so it is left alone rather than risk rejecting `RETURN n.name ORDER BY
/// n.age`.
pub(super) fn check_order_by_scope(query: &CypherQuery) -> crate::Result<()> {
    // Names bound and still visible at this point in the clause list.
    let mut scope: HashSet<String> = HashSet::new();
    // Set once a WITH is seen: (what it may sort by, what it projected).
    let mut open_with: Option<(HashSet<String>, &Vec<ReturnItem>)> = None;

    for clause in &query.clauses {
        match clause {
            Clause::Match(m) => {
                collect_pattern_names(&m.pattern, &mut scope);
                open_with = None;
            }
            Clause::Create(c) => {
                collect_pattern_names(&c.pattern, &mut scope);
                open_with = None;
            }
            Clause::Merge(m) => {
                collect_pattern_names(&m.pattern, &mut scope);
                open_with = None;
            }
            Clause::Unwind(u) => {
                scope.insert(u.variable.clone());
                open_with = None;
            }
            Clause::With(w) => {
                let projected = projected_names(&w.items);
                // The ORDER BY attached to this WITH sees both sides of the
                // projection; the clause *after* it sees only the output.
                let mut visible = scope.clone();
                visible.extend(projected.iter().cloned());
                open_with = Some((visible, &w.items));
                scope = projected;
            }
            Clause::OrderBy(o) => {
                if let Some((visible, items)) = &open_with {
                    for sort_item in &o.items {
                        check_sort_key(&sort_item.expression, items, visible)?;
                    }
                }
            }
            // SKIP/LIMIT sit inside a WITH's tail without closing it.
            Clause::Skip(_) | Clause::Limit(_) => {}
            _ => open_with = None,
        }
    }
    Ok(())
}

fn check_sort_key(
    expr: &Expression,
    items: &[ReturnItem],
    visible: &HashSet<String>,
) -> crate::Result<()> {
    for aggregate in aggregates_in(expr) {
        if !is_projected(aggregate, items) {
            return Err(invalid_aggregation());
        }
    }
    check_names(expr, items, visible)
}

/// Every aggregate call in the key, including nested ones.
fn aggregates_in(expr: &Expression) -> Vec<&Expression> {
    let mut found = Vec::new();
    if let Expression::FunctionCall { name, .. } = expr {
        if is_aggregate_name(name) {
            found.push(expr);
        }
    }
    for child in child_exprs(expr) {
        found.extend(aggregates_in(child));
    }
    found
}

fn check_names(
    expr: &Expression,
    items: &[ReturnItem],
    visible: &HashSet<String>,
) -> crate::Result<()> {
    // A key the projection computed verbatim needs no further checking — its
    // parts were legal where the projection evaluated them.
    if is_projected(expr, items) {
        return Ok(());
    }
    if let Expression::Variable(name) = expr {
        if !visible.contains(name) {
            return Err(undefined_variable(name));
        }
        return Ok(());
    }
    for child in child_exprs(expr) {
        check_names(child, items, visible)?;
    }
    Ok(())
}

/// Names the `WITH` puts in scope: each item's alias, or the variable itself
/// when the item is a bare pass-through.
fn projected_names(items: &[ReturnItem]) -> HashSet<String> {
    items
        .iter()
        .filter_map(|item| match (&item.alias, &item.expression) {
            (Some(alias), _) => Some(alias.clone()),
            (None, Expression::Variable(v)) => Some(v.clone()),
            _ => None,
        })
        .collect()
}

/// True when the projection contains this exact expression, aliased or not.
/// Compared structurally via the debug rendering — the AST does not implement
/// `PartialEq`, and "is this the same expression the projection computed" is a
/// syntactic question.
fn is_projected(expr: &Expression, items: &[ReturnItem]) -> bool {
    let target = format!("{expr:?}");
    items
        .iter()
        .any(|item| format!("{:?}", item.expression) == target)
}

fn collect_pattern_names(pattern: &Pattern, out: &mut HashSet<String>) {
    for name in pattern.path_variables() {
        out.insert(name.clone());
    }
    for element in &pattern.elements {
        collect_element_names(element, out);
    }
}

fn collect_element_names(element: &PatternElement, out: &mut HashSet<String>) {
    match element {
        PatternElement::Node(n) => {
            if let Some(v) = &n.variable {
                out.insert(v.clone());
            }
        }
        PatternElement::Relationship(r) => {
            if let Some(v) = &r.variable {
                out.insert(v.clone());
            }
        }
        PatternElement::QuantifiedGroup(g) => {
            for inner in &g.inner {
                collect_element_names(inner, out);
            }
        }
    }
}

fn invalid_aggregation() -> crate::Error {
    crate::Error::CypherSyntax(
        "InvalidAggregation: ORDER BY cannot introduce an aggregation the projection \
         it sorts did not compute"
            .to_string(),
    )
}

fn undefined_variable(name: &str) -> crate::Error {
    crate::Error::CypherSyntax(format!(
        "UndefinedVariable: variable `{name}` is not in scope for this ORDER BY"
    ))
}
