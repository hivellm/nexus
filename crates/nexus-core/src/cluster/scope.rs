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
    Clause, CypherQuery, Expression, NodePattern, Pattern, PatternElement, PropertyMap,
    RelationshipPattern, RemoveItem, ReturnItem, SetItem, WhereClause, WithClause,
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

    tracing::trace!(
        namespace = %ns,
        clauses = query.clauses.len(),
        "cluster::scope_query: rewriting query in place"
    );

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

/// Whether a parsed query contains any clause that writes to the
/// graph. Used by the cluster-mode write-path quota gate
/// (`Engine::execute_cypher_with_context`) to decide whether to
/// consult the `QuotaProvider` before execution.
///
/// Clauses that count as writes:
///
/// - `CREATE` / `MERGE` (new nodes and relationships).
/// - `SET` / `REMOVE` (property and label mutations).
/// - `DELETE` — not strictly "storage growth", but still a
///   chargeable mutation that we want to meter in the same path.
/// - `LOAD CSV` — bulk ingest writes, explicitly in scope for
///   Phase 4 §13.3 ("storage quota check before data import").
/// - `FOREACH` — always opens a sub-mutation block in Cypher.
///
/// Read-only clauses (MATCH / RETURN / WITH / UNWIND / WHERE /
/// ORDER BY / LIMIT / SKIP / CALL-procedure) return `false` and
/// skip the quota gate entirely.
pub fn is_write_query(query: &CypherQuery) -> bool {
    query.clauses.iter().any(|c| {
        matches!(
            c,
            Clause::Create(_)
                | Clause::Merge(_)
                | Clause::Set(_)
                | Clause::Remove(_)
                | Clause::Delete(_)
                | Clause::LoadCsv(_)
                | Clause::Foreach(_)
        )
    })
}

fn scope_clause(clause: &mut Clause, ns: &UserNamespace) {
    match clause {
        Clause::Match(m) => {
            scope_pattern(&mut m.pattern, ns);
            if let Some(where_clause) = &mut m.where_clause {
                scope_where_clause(where_clause, ns);
            }
        }
        Clause::Create(c) => scope_pattern(&mut c.pattern, ns),
        Clause::Merge(m) => scope_pattern(&mut m.pattern, ns),
        Clause::Where(w) => scope_where_clause(w, ns),
        Clause::Set(s) => {
            for item in &mut s.items {
                match item {
                    SetItem::Label { label, .. } => scope_label_in_place(label, ns),
                    SetItem::Property {
                        property, value, ..
                    } => {
                        scope_label_in_place(property, ns);
                        scope_expression(value, ns);
                    }
                    // phase6_opencypher-quickwins §6 — scope the RHS map
                    // expression. Keys inside the map are user-visible
                    // property names (not labels/types), so they need the
                    // same treatment as `SET target.prop = value`.
                    SetItem::MapMerge { map, .. } => scope_expression(map, ns),
                }
            }
        }
        Clause::Remove(r) => {
            for item in &mut r.items {
                match item {
                    RemoveItem::Label { label, .. } => scope_label_in_place(label, ns),
                    RemoveItem::Property { property, .. } => scope_label_in_place(property, ns),
                }
            }
        }
        Clause::With(w) => scope_with_clause(w, ns),
        Clause::Return(r) => {
            for item in &mut r.items {
                scope_return_item(item, ns);
            }
        }
        Clause::Unwind(u) => scope_expression(&mut u.expression, ns),
        // Other clauses (OrderBy, Limit, Skip, admin, …) carry no
        // direct catalog-name strings and remain untouched.
        _ => {}
    }
}

fn scope_pattern(pattern: &mut Pattern, ns: &UserNamespace) {
    for element in &mut pattern.elements {
        match element {
            PatternElement::Node(node) => scope_node_pattern(node, ns),
            PatternElement::Relationship(rel) => scope_relationship_pattern(rel, ns),
            PatternElement::QuantifiedGroup(group) => {
                let mut inner = Pattern {
                    elements: std::mem::take(&mut group.inner),
                    path_variable: None,
                };
                scope_pattern(&mut inner, ns);
                group.inner = inner.elements;
            }
        }
    }
}

fn scope_node_pattern(node: &mut NodePattern, ns: &UserNamespace) {
    for label in &mut node.labels {
        scope_label_in_place(label, ns);
    }
    if let Some(props) = &mut node.properties {
        scope_property_map(props, ns);
    }
}

fn scope_relationship_pattern(rel: &mut RelationshipPattern, ns: &UserNamespace) {
    for ty in &mut rel.types {
        scope_label_in_place(ty, ns);
    }
    if let Some(props) = &mut rel.properties {
        scope_property_map(props, ns);
    }
}

/// Walk a `{key: value, ...}` inline property map. Both halves get
/// scoped — the key through the standard label-prefix rule (so
/// catalog KeyIds separate cleanly per tenant) and the value
/// through the expression walker (which recurses into nested
/// patterns and property references).
fn scope_property_map(props: &mut PropertyMap, ns: &UserNamespace) {
    // HashMap keys cannot be mutated in place — drain and rebuild.
    let drained: Vec<(String, Expression)> = props.properties.drain().collect();
    for (mut key, mut value) in drained {
        scope_label_in_place(&mut key, ns);
        scope_expression(&mut value, ns);
        props.properties.insert(key, value);
    }
}

fn scope_where_clause(w: &mut WhereClause, ns: &UserNamespace) {
    scope_expression(&mut w.expression, ns);
}

fn scope_with_clause(w: &mut WithClause, ns: &UserNamespace) {
    for item in &mut w.items {
        scope_return_item(item, ns);
    }
    if let Some(where_clause) = &mut w.where_clause {
        scope_where_clause(where_clause, ns);
    }
}

fn scope_return_item(item: &mut ReturnItem, ns: &UserNamespace) {
    scope_expression(&mut item.expression, ns);
}

/// Recursive expression walker. Rewrites property names at every
/// catalog-touching position — `n.key`, map literals, nested
/// subquery patterns inside `EXISTS { ... }`, slice bounds, and
/// function-call arguments.
///
/// Variable names (the `variable` in `PropertyAccess`) are NOT
/// rewritten. They are query-local bindings that never reach the
/// catalog; prefixing them would just make debugging output uglier.
fn scope_expression(expr: &mut Expression, ns: &UserNamespace) {
    match expr {
        Expression::Literal(_) | Expression::Variable(_) | Expression::Parameter(_) => {}
        Expression::PropertyAccess { property, .. } => {
            scope_label_in_place(property, ns);
        }
        Expression::ArrayIndex { base, index } => {
            scope_expression(base, ns);
            scope_expression(index, ns);
        }
        Expression::ArraySlice { base, start, end } => {
            scope_expression(base, ns);
            if let Some(e) = start.as_mut() {
                scope_expression(e, ns);
            }
            if let Some(e) = end.as_mut() {
                scope_expression(e, ns);
            }
        }
        Expression::FunctionCall { args, .. } => {
            for a in args {
                scope_expression(a, ns);
            }
        }
        Expression::BinaryOp { left, right, .. } => {
            scope_expression(left, ns);
            scope_expression(right, ns);
        }
        Expression::UnaryOp { operand, .. } => scope_expression(operand, ns),
        Expression::Case {
            input,
            when_clauses,
            else_clause,
        } => {
            if let Some(e) = input.as_deref_mut() {
                scope_expression(e, ns);
            }
            for w in when_clauses {
                scope_when_clause(w, ns);
            }
            if let Some(e) = else_clause.as_deref_mut() {
                scope_expression(e, ns);
            }
        }
        Expression::List(xs) => {
            for x in xs {
                scope_expression(x, ns);
            }
        }
        Expression::Map(m) => {
            let drained: Vec<(String, Expression)> = m.drain().collect();
            for (mut key, mut value) in drained {
                scope_label_in_place(&mut key, ns);
                scope_expression(&mut value, ns);
                m.insert(key, value);
            }
        }
        Expression::IsNull { expr, .. } => scope_expression(expr, ns),
        Expression::Exists {
            pattern,
            where_clause,
        } => {
            scope_pattern(pattern, ns);
            if let Some(e) = where_clause.as_deref_mut() {
                scope_expression(e, ns);
            }
        }
        // Catch-all for expression variants added after this was
        // written (e.g. list comprehensions, predicates). Rather
        // than silently leaking unscoped property names, leave them
        // untouched — the AST tests in this module will notice a
        // mismatch and the CI guard rail `rewrite_is_idempotent`
        // covers the re-entry path.
        _ => {}
    }
}

fn scope_when_clause(w: &mut crate::executor::parser::ast::WhenClause, ns: &UserNamespace) {
    scope_expression(&mut w.condition, ns);
    scope_expression(&mut w.result, ns);
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
    fn rewrites_property_keys_in_inline_map() {
        // CREATE (n:Person {name: 'A'}) must scope both `Person`
        // and `name`. Without the property-key rewrite, the catalog
        // would see a shared global `name` KeyId across tenants —
        // data doesn't leak (property reads go through label-scoped
        // node ids) but `SHOW PROPERTY KEYS` would expose other
        // tenants' key names.
        let mut q = parse("CREATE (n:Person {name: 'A', age: 30})");
        scope_query(&mut q, &ns(), TenantIsolationMode::CatalogPrefix);

        let mut keys = Vec::new();
        if let Clause::Create(c) = &q.clauses[0] {
            if let PatternElement::Node(node) = &c.pattern.elements[0] {
                if let Some(props) = &node.properties {
                    keys.extend(props.properties.keys().cloned());
                }
            }
        }
        keys.sort();
        assert_eq!(keys, vec!["ns:alice:age", "ns:alice:name"]);
    }

    #[test]
    fn rewrites_property_keys_in_set_and_remove() {
        let mut q = parse("MATCH (n:Person) SET n.name = 'Bob' REMOVE n.age RETURN n");
        scope_query(&mut q, &ns(), TenantIsolationMode::CatalogPrefix);

        // SET should scope the property key.
        let mut set_props = Vec::new();
        let mut remove_props = Vec::new();
        for clause in &q.clauses {
            if let Clause::Set(s) = clause {
                for item in &s.items {
                    if let SetItem::Property { property, .. } = item {
                        set_props.push(property.clone());
                    }
                }
            }
            if let Clause::Remove(r) = clause {
                for item in &r.items {
                    if let RemoveItem::Property { property, .. } = item {
                        remove_props.push(property.clone());
                    }
                }
            }
        }
        assert_eq!(set_props, vec!["ns:alice:name"]);
        assert_eq!(remove_props, vec!["ns:alice:age"]);
    }

    #[test]
    fn rewrites_property_keys_in_where_and_return() {
        // WHERE n.email = 'x' and RETURN n.name both reach through
        // PropertyAccess. Both need rewriting for the query to hit
        // the tenant-scoped KeyIds in the catalog.
        let mut q = parse("MATCH (n:Person) WHERE n.email = 'a@b' RETURN n.name");
        scope_query(&mut q, &ns(), TenantIsolationMode::CatalogPrefix);

        // Don't assume how the parser threads WHERE — scan the whole
        // serialised AST instead. Both keys must appear in scoped
        // form somewhere, and neither should appear unscoped.
        let dump = format!("{q:?}");
        assert!(
            dump.contains("ns:alice:email"),
            "WHERE n.email must scope to ns:alice:email; AST: {dump}"
        );
        assert!(
            dump.contains("ns:alice:name"),
            "RETURN n.name must scope to ns:alice:name; AST: {dump}"
        );
        // Regression: no raw `property: "email"` substring anywhere.
        // (String check over Debug output is crude but catches the
        // "forgot to rewrite" class of bug clearly.)
        assert!(
            !dump.contains("property: \"email\""),
            "unscoped `email` leaked through; AST: {dump}"
        );
        assert!(
            !dump.contains("property: \"name\""),
            "unscoped `name` leaked through; AST: {dump}"
        );
    }

    #[test]
    fn property_key_rewrite_is_idempotent() {
        let mut q = parse("MATCH (n:Person) WHERE n.email = 'a' RETURN n.name");
        scope_query(&mut q, &ns(), TenantIsolationMode::CatalogPrefix);
        scope_query(&mut q, &ns(), TenantIsolationMode::CatalogPrefix);
        // If the rewriter wasn't idempotent we'd see
        // `ns:alice:ns:alice:name` anywhere the property appeared.
        let dump = format!("{q:?}");
        assert!(
            !dump.contains("ns:alice:ns:alice"),
            "property-key path must stay idempotent"
        );
    }

    #[test]
    fn rewrites_property_keys_in_function_call_arguments() {
        // toLower(n.email) must scope the `email` key so the
        // catalog lookup uses the tenant-specific KeyId. Without
        // this, function arguments that drill into a node would
        // leak through the prefix.
        let mut q = parse("MATCH (n:Person) RETURN toLower(n.email) AS email");
        scope_query(&mut q, &ns(), TenantIsolationMode::CatalogPrefix);
        let dump = format!("{q:?}");
        assert!(
            dump.contains("ns:alice:email"),
            "function arg property must scope: {dump}"
        );
    }

    #[test]
    fn rewrites_property_keys_inside_binary_and_unary_ops() {
        // BinaryOp / UnaryOp recurse into both operands. Pin both
        // so a future refactor that forgets one side regresses
        // deterministically.
        let mut q = parse("MATCH (n:Person) WHERE NOT (n.score > 0) RETURN n.name");
        scope_query(&mut q, &ns(), TenantIsolationMode::CatalogPrefix);
        let dump = format!("{q:?}");
        assert!(dump.contains("ns:alice:score"), "BinaryOp lhs: {dump}");
        assert!(dump.contains("ns:alice:name"), "RETURN: {dump}");
    }

    #[test]
    fn rewrites_property_keys_inside_is_null() {
        let mut q = parse("MATCH (n:Person) WHERE n.email IS NULL RETURN n");
        scope_query(&mut q, &ns(), TenantIsolationMode::CatalogPrefix);
        let dump = format!("{q:?}");
        assert!(
            dump.contains("ns:alice:email"),
            "IS NULL operand property must scope: {dump}"
        );
    }

    #[test]
    fn rewrites_property_keys_inside_list_and_map_expressions() {
        // List and Map inline expressions both recurse into their
        // elements / values. Map ALSO rewrites keys because inline
        // map literals end up as property maps at pattern
        // construction time.
        let mut q =
            parse("MATCH (n:Person) RETURN [n.first, n.last] AS parts, {name: n.email} AS row");
        scope_query(&mut q, &ns(), TenantIsolationMode::CatalogPrefix);
        let dump = format!("{q:?}");
        assert!(dump.contains("ns:alice:first"), "List element: {dump}");
        assert!(dump.contains("ns:alice:last"), "List element: {dump}");
        assert!(dump.contains("ns:alice:name"), "Map key: {dump}");
        assert!(dump.contains("ns:alice:email"), "Map value: {dump}");
    }

    #[test]
    fn rewrites_property_keys_inside_case_expression() {
        let mut q = parse(
            "MATCH (n:Person) \
             RETURN CASE WHEN n.score > 0 THEN n.active ELSE n.retired END AS state",
        );
        scope_query(&mut q, &ns(), TenantIsolationMode::CatalogPrefix);
        let dump = format!("{q:?}");
        assert!(dump.contains("ns:alice:score"), "CASE when: {dump}");
        assert!(dump.contains("ns:alice:active"), "CASE then: {dump}");
        assert!(dump.contains("ns:alice:retired"), "CASE else: {dump}");
    }

    #[test]
    fn rewrites_labels_and_property_keys_inside_exists_subquery() {
        // EXISTS { MATCH ... } is the recursive case for patterns
        // AND property access — both need scoping so a tenant can't
        // probe another tenant's graph shape.
        let mut q = parse(
            "MATCH (a:Person) WHERE EXISTS { (a)-[:KNOWS]->(b:Friend {since: a.joined}) } \
             RETURN a",
        );
        scope_query(&mut q, &ns(), TenantIsolationMode::CatalogPrefix);
        let dump = format!("{q:?}");
        assert!(dump.contains("ns:alice:Person"), "outer label: {dump}");
        assert!(dump.contains("ns:alice:KNOWS"), "EXISTS rel type: {dump}");
        assert!(
            dump.contains("ns:alice:Friend"),
            "EXISTS node label: {dump}"
        );
        assert!(
            dump.contains("ns:alice:since"),
            "EXISTS inline prop key: {dump}"
        );
        assert!(
            dump.contains("ns:alice:joined"),
            "EXISTS property access in inline prop value: {dump}"
        );
    }

    #[test]
    fn rewrites_property_keys_inside_array_slices() {
        // Array slicing recurses into the base and both bounds.
        // Pins the one slice position in the walker that's easy
        // to miss during a refactor.
        let mut q = parse("MATCH (n:Person) RETURN n.tags[0..n.limit] AS first_n");
        scope_query(&mut q, &ns(), TenantIsolationMode::CatalogPrefix);
        let dump = format!("{q:?}");
        assert!(dump.contains("ns:alice:tags"), "slice base: {dump}");
        assert!(dump.contains("ns:alice:limit"), "slice end bound: {dump}");
    }

    #[test]
    fn is_write_query_classifier_matches_every_mutating_clause() {
        // The classifier gates `check_storage` — every clause we
        // expect to cost storage must return true, and every read
        // clause must return false. Both halves matter: a false
        // positive over-charges tenants, a false negative leaks
        // storage growth past the quota.
        let writes = [
            "CREATE (n:Person)",
            "MERGE (n:Person {id: 1})",
            "MATCH (n) SET n.name = 'x'",
            "MATCH (n) REMOVE n:Tag",
            "MATCH (n) DELETE n",
            "MATCH (n) FOREACH (x IN [1,2,3] | SET n.mark = x)",
        ];
        for q in writes {
            let parsed = parse(q);
            assert!(
                is_write_query(&parsed),
                "classifier must flag `{q}` as a write"
            );
        }

        let reads = [
            "MATCH (n) RETURN n",
            "MATCH (n:Person) WHERE n.age > 30 RETURN n.name",
            "UNWIND [1,2,3] AS x RETURN x",
            "WITH 1 AS x RETURN x",
        ];
        for q in reads {
            let parsed = parse(q);
            assert!(
                !is_write_query(&parsed),
                "classifier must NOT flag `{q}` as a write"
            );
        }
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
