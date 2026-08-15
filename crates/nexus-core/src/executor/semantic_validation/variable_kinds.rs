//! One variable, one kind of thing.
//!
//! openCypher binds a variable to a node, a relationship, a path, or a plain
//! value, and a name may not change kind within a scope. Every shape below is
//! a `VariableTypeConflict`, and every one of them used to run and answer:
//!
//! ```cypher
//! MATCH p = (p)-[]-() RETURN p              -- path vs node, one pattern
//! MATCH r = ()-[]-() MATCH ()-[r]-() RETURN r  -- path vs relationship, across MATCHes
//! WITH true AS n MATCH (n) RETURN n         -- value vs node
//! ```
//!
//! The node-vs-relationship half of this rule already lived in the parent
//! module against a pair of `HashSet`s. Paths and values need a third and
//! fourth kind, and a set per kind stops scaling there — this tracks one kind
//! per name instead, which also makes the first binding (the one to blame in
//! the error) obvious.

use std::collections::HashMap;

use super::{
    Clause, CypherQuery, Expression, Literal, Pattern, PatternElement, variable_type_conflict,
};

/// What a variable is bound to. `Value` covers everything that is not a graph
/// entity — a `WITH` projection, an `UNWIND` element, a literal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VarKind {
    Node,
    Relationship,
    Path,
    Value,
}

/// Reject any variable bound to two different kinds of thing.
///
/// Scope note: a `WITH` ends the previous scope, so bindings are dropped at
/// that boundary — `MATCH (n) WITH n AS x MATCH (x)` is not a conflict, and
/// re-binding a name after a `WITH` is how legitimate Cypher renames things.
/// What `WITH` does introduce is a `Value` binding for each of its aliases,
/// which is what makes `WITH true AS n MATCH (n)` illegal.
pub(super) fn check_variable_kind_conflicts(query: &CypherQuery) -> crate::Result<()> {
    let mut kinds: HashMap<String, VarKind> = HashMap::new();

    for clause in &query.clauses {
        match clause {
            Clause::Match(m) => bind_pattern(&m.pattern, &mut kinds)?,
            Clause::Create(c) => bind_pattern(&c.pattern, &mut kinds)?,
            Clause::Merge(m) => bind_pattern(&m.pattern, &mut kinds)?,
            Clause::With(w) => {
                // The scope cut: only what this WITH projects survives.
                // Carrying the old kinds across wholesale would flag the
                // ordinary `MATCH (n) WITH n RETURN n` as a conflict.
                let mut projected = HashMap::new();
                for item in &w.items {
                    match (&item.alias, &item.expression) {
                        // A non-null literal pins the alias to a plain value:
                        // `WITH true AS n MATCH (n)` is the conflict.
                        (Some(alias), expr) if is_non_null_literal(expr) => {
                            projected.insert(alias.clone(), VarKind::Value);
                        }
                        // Anything else aliased has a type we cannot read off
                        // the source — leave it unbound rather than guess.
                        // `null` in particular inhabits every type, so
                        // `WITH null AS a OPTIONAL MATCH (a)` is legal.
                        (Some(_), _) => {}
                        // `WITH n` passes the binding through unchanged — it
                        // is still the node it was.
                        (None, Expression::Variable(v)) => {
                            if let Some(kind) = kinds.get(v) {
                                projected.insert(v.clone(), *kind);
                            }
                        }
                        _ => {}
                    }
                }
                kinds = projected;
            }
            _ => {}
        }
    }
    Ok(())
}

/// True for a literal that is definitely not a graph entity.
///
/// `null` is excluded on purpose: it is a member of every type, so binding it
/// to a name says nothing about what that name may later be matched as.
fn is_non_null_literal(expr: &Expression) -> bool {
    matches!(expr, Expression::Literal(lit) if !matches!(lit, Literal::Null))
}

fn bind_pattern(pattern: &Pattern, kinds: &mut HashMap<String, VarKind>) -> crate::Result<()> {
    for name in pattern.path_variables() {
        bind(name, VarKind::Path, kinds)?;
    }
    for element in &pattern.elements {
        bind_element(element, kinds)?;
    }
    Ok(())
}

fn bind_element(
    element: &PatternElement,
    kinds: &mut HashMap<String, VarKind>,
) -> crate::Result<()> {
    match element {
        PatternElement::Node(n) => {
            if let Some(v) = &n.variable {
                bind(v, VarKind::Node, kinds)?;
            }
        }
        PatternElement::Relationship(r) => {
            if let Some(v) = &r.variable {
                bind(v, VarKind::Relationship, kinds)?;
            }
        }
        PatternElement::QuantifiedGroup(g) => {
            for inner in &g.inner {
                bind_element(inner, kinds)?;
            }
        }
    }
    Ok(())
}

/// Record `name` as `kind`, or fail if it is already something else.
fn bind(name: &str, kind: VarKind, kinds: &mut HashMap<String, VarKind>) -> crate::Result<()> {
    match kinds.get(name) {
        Some(existing) if *existing != kind => Err(variable_type_conflict(name)),
        _ => {
            kinds.insert(name.to_string(), kind);
            Ok(())
        }
    }
}
