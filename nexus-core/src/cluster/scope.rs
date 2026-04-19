//! AST-level catalog-name rewriter for
//! [`TenantIsolationMode::CatalogPrefix`].
//!
//! Given a parsed `CypherQuery`, a `UserNamespace`, and an
//! isolation mode, walks the AST once and rewrites every
//! user-visible label / relationship-type string into the
//! namespaced form the catalog will register under. After this
//! pass runs, the rest of the pipeline (planner, catalog,
//! executor, indexes) is oblivious to tenancy — every catalog
//! lookup is already scoped to the caller's tenant.
//!
//! This commit covers first-class identifiers — `NodePattern.labels`
//! and `RelationshipPattern.types`. Property key prefixing is a
//! larger surface (SetItem, RemoveItem, PropertyMap, PropertyAccess,
//! Expression::Map, nested Exists patterns) and lands in a follow-up
//! — without it there is a *listing*-level leak (`SHOW PROPERTY KEYS`
//! exposes other tenants' key names) but no data-level leak (record
//! reads still go through label-scoped indexes that ARE isolated).
//!
//! Idempotence: `scope_query` is a no-op if the input is already
//! namespaced (names starting with `ns:`). This matters because the
//! rewriter is wired into `execute_cypher`, and nested `execute_cypher`
//! calls (rare but possible during schema migration) would otherwise
//! double-prefix.

use crate::executor::parser::ast::{
    Clause, CypherQuery, NodePattern, Pattern, PatternElement, RelationshipPattern, RemoveItem,
    SetItem,
};

use super::config::TenantIsolationMode;
use super::namespace::UserNamespace;

/// Rewrite every label / relationship-type reference in `query`
/// with the `CatalogPrefix` form of `ns`. No-op on
/// [`TenantIsolationMode::None`].
///
/// Mutates in place — returns nothing. Callers own the `query`;
/// if an immutable view is required, clone first.
pub fn scope_query(query: &mut CypherQuery, ns: &UserNamespace, mode: TenantIsolationMode) {
    if !should_rewrite(mode) {
        return;
    }

    for clause in &mut query.clauses {
        scope_clause(clause, ns);
    }
}

/// Whether the given isolation mode asks for catalog rewriting.
/// Exposed as a tiny helper so callers can short-circuit allocation
/// before cloning a query; inlined in the hot path below.
#[inline]
pub fn should_rewrite(mode: TenantIsolationMode) -> bool {
    matches!(mode, TenantIsolationMode::CatalogPrefix)
}

fn scope_clause(clause: &mut Clause, ns: &UserNamespace) {
    match clause {
        Clause::Match(m) => scope_pattern(&mut m.pattern, ns),
        Clause::Create(c) => scope_pattern(&mut c.pattern, ns),
        Clause::Merge(m) => scope_pattern(&mut m.pattern, ns),
        Clause::Set(s) => {
            for item in &mut s.items {
                if let SetItem::Label { label, .. } = item {
                    scope_label_in_place(label, ns);
                }
            }
        }
        Clause::Remove(r) => {
            for item in &mut r.items {
                if let RemoveItem::Label { label, .. } = item {
                    scope_label_in_place(label, ns);
                }
            }
        }
        // Other clauses (Return, With, OrderBy, Unwind, ...) carry
        // expressions only — no direct catalog-name strings. They
        // become relevant once property keys are covered; skip for now.
        _ => {}
    }
}

fn scope_pattern(pattern: &mut Pattern, ns: &UserNamespace) {
    for element in &mut pattern.elements {
        match element {
            PatternElement::Node(node) => scope_node_pattern(node, ns),
            PatternElement::Relationship(rel) => scope_relationship_pattern(rel, ns),
        }
    }
}

fn scope_node_pattern(node: &mut NodePattern, ns: &UserNamespace) {
    for label in &mut node.labels {
        scope_label_in_place(label, ns);
    }
}

fn scope_relationship_pattern(rel: &mut RelationshipPattern, ns: &UserNamespace) {
    for ty in &mut rel.types {
        scope_label_in_place(ty, ns);
    }
}

/// Core rewrite. Idempotent: already-namespaced strings pass
/// through unchanged, so nested `execute_cypher` calls never
/// double-prefix the same name.
fn scope_label_in_place(name: &mut String, ns: &UserNamespace) {
    if UserNamespace::is_namespaced_catalog_name(name) {
        return;
    }
    *name = ns.catalog_name(name);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::parser::CypherParser;

    fn parse(q: &str) -> CypherQuery {
        CypherParser::new(q.to_string()).parse().expect("parse ok")
    }

    fn ns() -> UserNamespace {
        UserNamespace::new("alice").unwrap()
    }

    fn collect_labels(q: &CypherQuery) -> Vec<String> {
        let mut out = Vec::new();
        for clause in &q.clauses {
            let pattern = match clause {
                Clause::Match(m) => Some(&m.pattern),
                Clause::Create(c) => Some(&c.pattern),
                Clause::Merge(m) => Some(&m.pattern),
                _ => None,
            };
            if let Some(pat) = pattern {
                for el in &pat.elements {
                    if let PatternElement::Node(n) = el {
                        out.extend(n.labels.iter().cloned());
                    }
                    if let PatternElement::Relationship(r) = el {
                        out.extend(r.types.iter().cloned());
                    }
                }
            }
            if let Clause::Set(s) = clause {
                for item in &s.items {
                    if let SetItem::Label { label, .. } = item {
                        out.push(label.clone());
                    }
                }
            }
            if let Clause::Remove(r) = clause {
                for item in &r.items {
                    if let RemoveItem::Label { label, .. } = item {
                        out.push(label.clone());
                    }
                }
            }
        }
        out
    }

    #[test]
    fn rewrites_node_labels_on_match() {
        let mut q = parse("MATCH (n:Person) RETURN n");
        scope_query(&mut q, &ns(), TenantIsolationMode::CatalogPrefix);
        assert_eq!(collect_labels(&q), vec!["ns:alice:Person"]);
    }

    #[test]
    fn rewrites_multiple_labels_on_node() {
        let mut q = parse("MATCH (n:Person:Employee) RETURN n");
        scope_query(&mut q, &ns(), TenantIsolationMode::CatalogPrefix);
        assert_eq!(
            collect_labels(&q),
            vec!["ns:alice:Person", "ns:alice:Employee"]
        );
    }

    #[test]
    fn rewrites_relationship_types() {
        let mut q = parse("MATCH (a:Person)-[:KNOWS]->(b:Person) RETURN a, b");
        scope_query(&mut q, &ns(), TenantIsolationMode::CatalogPrefix);
        let labels = collect_labels(&q);
        assert!(
            labels.contains(&"ns:alice:Person".to_string()),
            "labels: {labels:?}"
        );
        assert!(
            labels.contains(&"ns:alice:KNOWS".to_string()),
            "types: {labels:?}"
        );
    }

    #[test]
    fn rewrites_on_create() {
        let mut q = parse("CREATE (n:Person {name: 'Alice'})");
        scope_query(&mut q, &ns(), TenantIsolationMode::CatalogPrefix);
        assert_eq!(collect_labels(&q), vec!["ns:alice:Person"]);
    }

    #[test]
    fn rewrites_on_merge() {
        let mut q = parse("MERGE (n:Person {email: 'alice@example.com'})");
        scope_query(&mut q, &ns(), TenantIsolationMode::CatalogPrefix);
        assert_eq!(collect_labels(&q), vec!["ns:alice:Person"]);
    }

    #[test]
    fn rewrites_on_set_label() {
        let mut q = parse("MATCH (n) SET n:Person RETURN n");
        scope_query(&mut q, &ns(), TenantIsolationMode::CatalogPrefix);
        assert!(
            collect_labels(&q).contains(&"ns:alice:Person".to_string()),
            "labels: {:?}",
            collect_labels(&q)
        );
    }

    #[test]
    fn rewrites_on_remove_label() {
        let mut q = parse("MATCH (n:Person) REMOVE n:Employee RETURN n");
        scope_query(&mut q, &ns(), TenantIsolationMode::CatalogPrefix);
        let labels = collect_labels(&q);
        assert!(
            labels.contains(&"ns:alice:Person".to_string()),
            "labels: {labels:?}"
        );
        assert!(
            labels.contains(&"ns:alice:Employee".to_string()),
            "labels: {labels:?}"
        );
    }

    #[test]
    fn none_mode_is_a_true_no_op() {
        // Regression guard: standalone deployments must pay zero
        // rewrite cost. A single assertion on the label string is
        // enough to catch "accidentally added a trailing space"
        // kind of regressions.
        let mut q = parse("MATCH (n:Person) RETURN n");
        scope_query(&mut q, &ns(), TenantIsolationMode::None);
        assert_eq!(collect_labels(&q), vec!["Person"]);
    }

    #[test]
    fn rewrite_is_idempotent() {
        // Nested execute_cypher call sites can legitimately see a
        // query that was already scoped. The rewriter must detect
        // the `ns:` tag and pass through instead of producing
        // `ns:alice:ns:alice:Person`.
        let mut q = parse("MATCH (n:Person) RETURN n");
        scope_query(&mut q, &ns(), TenantIsolationMode::CatalogPrefix);
        scope_query(&mut q, &ns(), TenantIsolationMode::CatalogPrefix);
        assert_eq!(collect_labels(&q), vec!["ns:alice:Person"]);
    }

    #[test]
    fn tenants_get_distinct_catalog_names() {
        // Core isolation claim: two tenants running the literal
        // same query produce distinct scoped strings. Their
        // catalog lookups therefore hit distinct IDs and their
        // label bitmap indexes stay separate.
        let mut q_alice = parse("MATCH (n:Person) RETURN n");
        let mut q_bob = parse("MATCH (n:Person) RETURN n");
        let alice = UserNamespace::new("alice").unwrap();
        let bob = UserNamespace::new("bob").unwrap();
        scope_query(&mut q_alice, &alice, TenantIsolationMode::CatalogPrefix);
        scope_query(&mut q_bob, &bob, TenantIsolationMode::CatalogPrefix);
        assert_ne!(collect_labels(&q_alice), collect_labels(&q_bob));
        assert_eq!(collect_labels(&q_alice), vec!["ns:alice:Person"]);
        assert_eq!(collect_labels(&q_bob), vec!["ns:bob:Person"]);
    }
}
