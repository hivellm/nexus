//! Parser integration tests. Attached via `#[cfg(test)] mod tests;` in
//! the parent; all private parser helpers are visible here as pub(super).

#![allow(unused_imports)]
use super::*;

// ── shared helper functions ──────────────────────────────────────────────────

/// Walks the first MATCH clause in `q` and returns the QuantifiedGroup
/// embedded inside it, panicking if not found.
pub(super) fn qpp_group_of(q: &CypherQuery) -> &QuantifiedGroup {
    let Clause::Match(mc) = &q.clauses[0] else {
        panic!("expected MATCH, got {:?}", q.clauses[0]);
    };
    mc.pattern
        .elements
        .iter()
        .find_map(|e| {
            if let PatternElement::QuantifiedGroup(g) = e {
                Some(g)
            } else {
                None
            }
        })
        .expect("expected a QuantifiedGroup in pattern")
}

/// Returns the first `CallSubqueryClause` in `q`, panicking if absent.
pub(super) fn call_tx_clause(q: &CypherQuery) -> &CallSubqueryClause {
    q.clauses
        .iter()
        .find_map(|c| {
            if let Clause::CallSubquery(s) = c {
                Some(s)
            } else {
                None
            }
        })
        .expect("expected a CallSubqueryClause")
}

/// Returns the first `CreateIndexClause` in `q`, panicking if absent.
pub(super) fn first_create_index(q: &CypherQuery) -> &CreateIndexClause {
    for c in &q.clauses {
        if let Clause::CreateIndex(ix) = c {
            return ix;
        }
    }
    panic!("expected a CreateIndex clause in: {q:?}");
}

/// Returns the first `CreateClause` in `q`, panicking if absent.
pub(super) fn first_create_clause(q: &CypherQuery) -> &CreateClause {
    match &q.clauses[0] {
        Clause::Create(c) => c,
        other => panic!("expected CREATE clause, got {:?}", other),
    }
}

// ── submodules ───────────────────────────────────────────────────────────────

mod clauses;
mod ddl;
mod expressions;
mod external_ids;
mod patterns;
mod tokens;
