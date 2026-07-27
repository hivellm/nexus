//! Static semantic-analysis pass run after parsing and before planning.
//! Rejects queries that are structurally parseable but violate openCypher
//! scoping rules (referencing a variable bound nowhere in the query).
//!
//! # Design: conservative over-collection
//!
//! The pass must never reject a currently-valid query — a false positive
//! here is a user-visible regression. To guarantee that, the binder set is
//! deliberately *over-collected*: every construct that could bind a name
//! anywhere in the query contributes to a single flat set, and a reference
//! is flagged only when its name appears in **no** binder position at all.
//! Over-collecting can only ever *miss* a real error (a safe false
//! negative — e.g. a variable projected away by an intervening `WITH` is
//! still treated as in scope); it can never invent one. Per-clause scope
//! narrowing (which would catch the `WITH`-projected-away cases) is a
//! deliberate later refinement.
//!
//! Queries that contain constructs whose scoping this pass does not yet
//! fully model (`UNION`, `CALL {…}` subqueries, `CALL` procedures,
//! `LOAD CSV`, and every DDL/admin/transaction statement) are skipped
//! outright rather than risk a false positive.

use std::collections::HashSet;

use crate::executor::parser::ast::{
    Clause, CypherQuery, Expression, ForeachClause, MatchClause, Pattern, PatternElement,
    ReturnItem, UnwindClause,
};

/// In-band sentinel the parser pushes as the first argument of an aggregate
/// call with `DISTINCT` (e.g. `count(DISTINCT n)`); never a real variable.
const DISTINCT_MARKER: &str = "__DISTINCT__";

/// Validate a parsed query. Returns `Err` for a semantic violation, carrying
/// an openCypher CamelCase detail token in the message so the conformance
/// runner can classify it.
pub fn validate(query: &CypherQuery) -> crate::Result<()> {
    // Only queries composed entirely of clause kinds whose scoping this
    // pass fully models are checked; anything else is passed through
    // untouched (see module docs).
    if !is_fully_modeled(query) {
        return Ok(());
    }

    check_variable_type_conflicts(query)?;
    check_variable_already_bound(query)?;

    let mut binders = HashSet::new();
    collect_query_binders(query, &mut binders);

    check_query_references(query, &binders)
}

/// True when every clause is one whose variable scoping this pass models.
/// `UNION`, subqueries, procedures, `LOAD CSV` and DDL/admin/transaction
/// statements introduce scoping (or lack of it) this increment does not
/// handle, so their presence makes the whole query un-checked.
fn is_fully_modeled(query: &CypherQuery) -> bool {
    query.clauses.iter().all(|c| {
        matches!(
            c,
            Clause::Match(_)
                | Clause::Create(_)
                | Clause::Merge(_)
                | Clause::Set(_)
                | Clause::Delete(_)
                | Clause::Remove(_)
                | Clause::With(_)
                | Clause::Unwind(_)
                | Clause::Where(_)
                | Clause::Return(_)
                | Clause::OrderBy(_)
                | Clause::Limit(_)
                | Clause::Skip(_)
                | Clause::Foreach(_)
        )
    })
}

// ── Variable type conflicts ────────────────────────────────────────────

/// Reject a variable bound as a node in one place and as a relationship in
/// another (`MATCH (a) MATCH ()-[a]-()`). A Cypher variable has exactly one
/// entity type, so a name appearing in both the node-var and relationship-var
/// sets of a query's top-level patterns is always a genuine conflict — this
/// cannot false-positive on any valid query (`(a)-[r]->(a)` reuses `a` as the
/// same node, never as a relationship). Only top-level `MATCH`/`CREATE`/`MERGE`
/// patterns are inspected; expression-embedded patterns (`EXISTS { … }`,
/// pattern comprehensions) introduce inner scopes and are left to a later
/// refinement.
fn check_variable_type_conflicts(query: &CypherQuery) -> crate::Result<()> {
    let mut node_vars = HashSet::new();
    let mut rel_vars = HashSet::new();
    for clause in &query.clauses {
        match clause {
            Clause::Match(m) => {
                collect_typed_pattern_vars(&m.pattern, &mut node_vars, &mut rel_vars)
            }
            Clause::Create(c) => {
                collect_typed_pattern_vars(&c.pattern, &mut node_vars, &mut rel_vars)
            }
            Clause::Merge(m) => {
                collect_typed_pattern_vars(&m.pattern, &mut node_vars, &mut rel_vars)
            }
            _ => {}
        }
    }
    for name in &node_vars {
        if rel_vars.contains(name) {
            return Err(variable_type_conflict(name));
        }
    }
    Ok(())
}

/// Sort a pattern's variables into node names and relationship names.
fn collect_typed_pattern_vars(
    pattern: &Pattern,
    node_vars: &mut HashSet<String>,
    rel_vars: &mut HashSet<String>,
) {
    for element in &pattern.elements {
        collect_typed_element_vars(element, node_vars, rel_vars);
    }
}

fn collect_typed_element_vars(
    element: &PatternElement,
    node_vars: &mut HashSet<String>,
    rel_vars: &mut HashSet<String>,
) {
    match element {
        PatternElement::Node(n) => {
            if let Some(v) = &n.variable {
                node_vars.insert(v.clone());
            }
        }
        PatternElement::Relationship(r) => {
            if let Some(v) = &r.variable {
                rel_vars.insert(v.clone());
            }
        }
        PatternElement::QuantifiedGroup(g) => {
            for inner in &g.inner {
                collect_typed_element_vars(inner, node_vars, rel_vars);
            }
        }
    }
}

// ── Variable already bound (CREATE re-declaration) ─────────────────────

/// Reject a `CREATE` that re-declares an already-bound variable by giving it
/// labels or a property map (`MATCH (a) CREATE (a {x: 1})`,
/// `CREATE (n:Foo) CREATE (n:Bar)-[:R]->()`). Adding structure to a node whose
/// variable is already bound is never legal; a bare reference endpoint
/// (`MATCH (a) CREATE (a)-[:R]->(b)`) carries no labels/properties and is left
/// alone.
///
/// The check requires a monotonic scope so "already bound" is exact, so it
/// bails out entirely when the query contains a `WITH` (which can project a
/// variable out of scope, making a later `CREATE (a:X)` a legal fresh bind).
/// The bare standalone `CREATE (a)` form is intentionally not flagged here.
fn check_variable_already_bound(query: &CypherQuery) -> crate::Result<()> {
    if query.clauses.iter().any(|c| matches!(c, Clause::With(_))) {
        return Ok(());
    }

    let mut bound: HashSet<String> = HashSet::new();
    for clause in &query.clauses {
        match clause {
            Clause::Create(c) => check_create_pattern_rebind(&c.pattern, &mut bound)?,
            Clause::Match(m) => add_pattern_vars(&m.pattern, &mut bound),
            Clause::Merge(m) => add_pattern_vars(&m.pattern, &mut bound),
            Clause::Unwind(u) => {
                bound.insert(u.variable.clone());
            }
            Clause::Foreach(f) => {
                bound.insert(f.variable.clone());
            }
            _ => {}
        }
    }
    Ok(())
}

/// Walk a `CREATE` pattern in order, flagging any node that re-declares an
/// already-bound variable with structure, then recording each variable so a
/// later element in the same pattern (`(n:Foo)-[]->(), (n:Bar)-[]->()`) is
/// caught too.
fn check_create_pattern_rebind(
    pattern: &Pattern,
    bound: &mut HashSet<String>,
) -> crate::Result<()> {
    if let Some(path) = &pattern.path_variable {
        bound.insert(path.clone());
    }
    for element in &pattern.elements {
        check_create_element_rebind(element, bound)?;
    }
    Ok(())
}

fn check_create_element_rebind(
    element: &PatternElement,
    bound: &mut HashSet<String>,
) -> crate::Result<()> {
    match element {
        PatternElement::Node(n) => {
            if let Some(v) = &n.variable {
                let has_structure =
                    !n.labels.is_empty() || n.properties.is_some() || n.external_id_expr.is_some();
                if has_structure && bound.contains(v) {
                    return Err(variable_already_bound(v));
                }
                bound.insert(v.clone());
            }
        }
        PatternElement::Relationship(r) => {
            if let Some(v) = &r.variable {
                bound.insert(v.clone());
            }
        }
        PatternElement::QuantifiedGroup(g) => {
            for inner in &g.inner {
                check_create_element_rebind(inner, bound)?;
            }
        }
    }
    Ok(())
}

/// Record every node/relationship/path variable a pattern binds (used to seed
/// the "already bound" set from `MATCH`/`MERGE` clauses; no checking).
fn add_pattern_vars(pattern: &Pattern, bound: &mut HashSet<String>) {
    if let Some(path) = &pattern.path_variable {
        bound.insert(path.clone());
    }
    for element in &pattern.elements {
        add_element_vars(element, bound);
    }
}

fn add_element_vars(element: &PatternElement, bound: &mut HashSet<String>) {
    match element {
        PatternElement::Node(n) => {
            if let Some(v) = &n.variable {
                bound.insert(v.clone());
            }
        }
        PatternElement::Relationship(r) => {
            if let Some(v) = &r.variable {
                bound.insert(v.clone());
            }
        }
        PatternElement::QuantifiedGroup(g) => {
            for inner in &g.inner {
                add_element_vars(inner, bound);
            }
        }
    }
}

// ── Binder collection ──────────────────────────────────────────────────

/// Add every variable name bound anywhere in `query` to `binders`.
fn collect_query_binders(query: &CypherQuery, binders: &mut HashSet<String>) {
    for clause in &query.clauses {
        collect_clause_binders(clause, binders);
    }
}

fn collect_clause_binders(clause: &Clause, binders: &mut HashSet<String>) {
    match clause {
        Clause::Match(MatchClause {
            pattern,
            where_clause,
            ..
        }) => {
            collect_pattern_binders(pattern, binders);
            if let Some(w) = where_clause {
                collect_expr_binders(&w.expression, binders);
            }
        }
        Clause::Create(c) => collect_pattern_binders(&c.pattern, binders),
        Clause::Merge(m) => {
            collect_pattern_binders(&m.pattern, binders);
            if let Some(sc) = &m.on_create {
                for item in &sc.items {
                    collect_set_item_binders(item, binders);
                }
            }
            if let Some(sc) = &m.on_match {
                for item in &sc.items {
                    collect_set_item_binders(item, binders);
                }
            }
        }
        Clause::With(w) => {
            collect_projection_binders(&w.items, binders);
            if let Some(cond) = &w.where_clause {
                collect_expr_binders(&cond.expression, binders);
            }
        }
        Clause::Return(r) => collect_projection_binders(&r.items, binders),
        Clause::Unwind(UnwindClause {
            expression,
            variable,
        }) => {
            binders.insert(variable.clone());
            collect_expr_binders(expression, binders);
        }
        Clause::Foreach(ForeachClause {
            variable,
            list_expression,
            ..
        }) => {
            binders.insert(variable.clone());
            collect_expr_binders(list_expression, binders);
        }
        Clause::Where(w) => collect_expr_binders(&w.expression, binders),
        Clause::OrderBy(o) => {
            for item in &o.items {
                collect_expr_binders(&item.expression, binders);
            }
        }
        Clause::Limit(l) => collect_expr_binders(&l.count, binders),
        Clause::Skip(s) => collect_expr_binders(&s.count, binders),
        Clause::Set(sc) => {
            for item in &sc.items {
                collect_set_item_binders(item, binders);
            }
        }
        // DELETE / REMOVE bind nothing; other clause kinds are excluded by
        // `is_fully_modeled` before we ever get here.
        _ => {}
    }
}

/// A `WITH`/`RETURN` item introduces its **alias** as a new name (referenced
/// downstream, e.g. by a later clause or `ORDER BY`). A bare, unaliased item
/// (`RETURN a`) does NOT bind — `a` is a reference that must already be bound
/// at its origin site; treating it as a binder here would mask the very
/// undefined-variable references this pass exists to catch.
fn collect_projection_binders(items: &[ReturnItem], binders: &mut HashSet<String>) {
    for item in items {
        if let Some(alias) = &item.alias {
            binders.insert(alias.clone());
        }
        collect_expr_binders(&item.expression, binders);
    }
}

fn collect_set_item_binders(
    item: &crate::executor::parser::ast::SetItem,
    binders: &mut HashSet<String>,
) {
    // SET never introduces a new variable, but its RHS expressions may carry
    // inline binding constructs (list comprehensions etc.) whose variables
    // must be counted so their inner references are not flagged.
    match item {
        crate::executor::parser::ast::SetItem::Property { value, .. } => {
            collect_expr_binders(value, binders)
        }
        crate::executor::parser::ast::SetItem::MapMerge { map, .. } => {
            collect_expr_binders(map, binders)
        }
        crate::executor::parser::ast::SetItem::Replace { value, .. } => {
            collect_expr_binders(value, binders)
        }
        crate::executor::parser::ast::SetItem::Label { .. } => {}
    }
}

/// Add the variables bound by a pattern: every node/relationship variable,
/// the variables inside a quantified group, and the named-path variable.
fn collect_pattern_binders(pattern: &Pattern, binders: &mut HashSet<String>) {
    if let Some(path) = &pattern.path_variable {
        binders.insert(path.clone());
    }
    for element in &pattern.elements {
        collect_pattern_element_binders(element, binders);
    }
}

fn collect_pattern_element_binders(element: &PatternElement, binders: &mut HashSet<String>) {
    match element {
        PatternElement::Node(n) => {
            if let Some(v) = &n.variable {
                binders.insert(v.clone());
            }
        }
        PatternElement::Relationship(r) => {
            if let Some(v) = &r.variable {
                binders.insert(v.clone());
            }
        }
        PatternElement::QuantifiedGroup(g) => {
            for inner in &g.inner {
                collect_pattern_element_binders(inner, binders);
            }
        }
    }
}

/// Recurse through an expression, adding any variables bound by inline
/// constructs: list/pattern comprehensions, the `any`/`all`/`none`/`single`
/// list predicates (whose bound name is the first argument, a string
/// literal), `EXISTS { … }` patterns, and `COLLECT { … }` subqueries.
fn collect_expr_binders(expr: &Expression, binders: &mut HashSet<String>) {
    use crate::executor::parser::ast::Literal;
    match expr {
        Expression::ListComprehension {
            variable,
            list_expression,
            where_clause,
            transform_expression,
        } => {
            binders.insert(variable.clone());
            collect_expr_binders(list_expression, binders);
            if let Some(w) = where_clause {
                collect_expr_binders(w, binders);
            }
            if let Some(t) = transform_expression {
                collect_expr_binders(t, binders);
            }
        }
        Expression::PatternComprehension {
            pattern,
            where_clause,
            transform_expression,
        } => {
            collect_pattern_binders(pattern, binders);
            if let Some(w) = where_clause {
                collect_expr_binders(w, binders);
            }
            if let Some(t) = transform_expression {
                collect_expr_binders(t, binders);
            }
        }
        Expression::Exists {
            pattern,
            where_clause,
        } => {
            collect_pattern_binders(pattern, binders);
            if let Some(w) = where_clause {
                collect_expr_binders(w, binders);
            }
        }
        Expression::CollectSubquery { inner } => collect_query_binders(inner, binders),
        Expression::FunctionCall { name, args } => {
            // `any`/`all`/`none`/`single(x IN list WHERE pred)` are lowered to
            // three args: [Literal::String("x"), list, predicate]. The bound
            // name `x` is referenced as a `Variable` inside the predicate.
            if matches!(
                name.to_lowercase().as_str(),
                "any" | "all" | "none" | "single"
            ) {
                if let Some(Expression::Literal(Literal::String(var))) = args.first() {
                    binders.insert(var.clone());
                }
            }
            for a in args {
                collect_expr_binders(a, binders);
            }
        }
        Expression::BinaryOp { left, right, .. } => {
            collect_expr_binders(left, binders);
            collect_expr_binders(right, binders);
        }
        Expression::UnaryOp { operand, .. } => collect_expr_binders(operand, binders),
        Expression::ArrayIndex { base, index } => {
            collect_expr_binders(base, binders);
            collect_expr_binders(index, binders);
        }
        Expression::ArraySlice { base, start, end } => {
            collect_expr_binders(base, binders);
            if let Some(s) = start {
                collect_expr_binders(s, binders);
            }
            if let Some(e) = end {
                collect_expr_binders(e, binders);
            }
        }
        Expression::IsNull { expr, .. } => collect_expr_binders(expr, binders),
        Expression::Case {
            input,
            when_clauses,
            else_clause,
        } => {
            if let Some(i) = input {
                collect_expr_binders(i, binders);
            }
            for w in when_clauses {
                collect_expr_binders(&w.condition, binders);
                collect_expr_binders(&w.result, binders);
            }
            if let Some(e) = else_clause {
                collect_expr_binders(e, binders);
            }
        }
        Expression::List(items) => {
            for i in items {
                collect_expr_binders(i, binders);
            }
        }
        Expression::Map(entries) => {
            for v in entries.values() {
                collect_expr_binders(v, binders);
            }
        }
        Expression::MapProjection { source, .. } => collect_expr_binders(source, binders),
        // Leaves and non-binding forms.
        Expression::Literal(_)
        | Expression::Variable(_)
        | Expression::PropertyAccess { .. }
        | Expression::Parameter(_) => {}
    }
}

// ── Reference checking ─────────────────────────────────────────────────

/// Walk the read-position expressions of every clause and reject any
/// variable reference whose name is not in `binders`.
fn check_query_references(query: &CypherQuery, binders: &HashSet<String>) -> crate::Result<()> {
    for clause in &query.clauses {
        check_clause_references(clause, binders)?;
    }
    Ok(())
}

fn check_clause_references(clause: &Clause, binders: &HashSet<String>) -> crate::Result<()> {
    match clause {
        Clause::Match(MatchClause { where_clause, .. }) => {
            if let Some(w) = where_clause {
                check_expr_references(&w.expression, binders)?;
            }
        }
        Clause::With(w) => {
            for item in &w.items {
                check_expr_references(&item.expression, binders)?;
            }
            if let Some(cond) = &w.where_clause {
                check_expr_references(&cond.expression, binders)?;
            }
        }
        Clause::Return(r) => {
            for item in &r.items {
                check_expr_references(&item.expression, binders)?;
            }
        }
        Clause::Where(w) => check_expr_references(&w.expression, binders)?,
        Clause::OrderBy(o) => {
            for item in &o.items {
                check_expr_references(&item.expression, binders)?;
            }
        }
        Clause::Limit(l) => check_expr_references(&l.count, binders)?,
        Clause::Skip(s) => check_expr_references(&s.count, binders)?,
        Clause::Unwind(u) => check_expr_references(&u.expression, binders)?,
        Clause::Foreach(f) => check_expr_references(&f.list_expression, binders)?,
        // Write-clause target references (SET/DELETE/REMOVE/CREATE/MERGE) are
        // validated by a later increment; pattern property maps are not
        // reference-checked here.
        _ => {}
    }
    Ok(())
}

/// Flag `Variable` / `PropertyAccess` references whose base name is unbound.
/// Recurses through every sub-expression; inline-bound names (comprehension /
/// predicate variables) are already present in `binders`, so their inner
/// references pass.
fn check_expr_references(expr: &Expression, binders: &HashSet<String>) -> crate::Result<()> {
    match expr {
        Expression::Variable(name) => {
            // `__DISTINCT__` is an in-band marker the parser injects as the
            // first argument of an aggregate call (`count(DISTINCT n)`), not a
            // real variable reference — skip it.
            if name != DISTINCT_MARKER && !binders.contains(name) {
                return Err(undefined_variable(name));
            }
        }
        Expression::PropertyAccess { variable, .. } => {
            if !binders.contains(variable) {
                return Err(undefined_variable(variable));
            }
        }
        Expression::BinaryOp { left, right, .. } => {
            check_expr_references(left, binders)?;
            check_expr_references(right, binders)?;
        }
        Expression::UnaryOp { operand, .. } => check_expr_references(operand, binders)?,
        Expression::ArrayIndex { base, index } => {
            check_expr_references(base, binders)?;
            check_expr_references(index, binders)?;
        }
        Expression::ArraySlice { base, start, end } => {
            check_expr_references(base, binders)?;
            if let Some(s) = start {
                check_expr_references(s, binders)?;
            }
            if let Some(e) = end {
                check_expr_references(e, binders)?;
            }
        }
        Expression::FunctionCall { args, .. } => {
            for a in args {
                check_expr_references(a, binders)?;
            }
        }
        Expression::IsNull { expr, .. } => check_expr_references(expr, binders)?,
        Expression::Case {
            input,
            when_clauses,
            else_clause,
        } => {
            if let Some(i) = input {
                check_expr_references(i, binders)?;
            }
            for w in when_clauses {
                check_expr_references(&w.condition, binders)?;
                check_expr_references(&w.result, binders)?;
            }
            if let Some(e) = else_clause {
                check_expr_references(e, binders)?;
            }
        }
        Expression::List(items) => {
            for i in items {
                check_expr_references(i, binders)?;
            }
        }
        Expression::Map(entries) => {
            for v in entries.values() {
                check_expr_references(v, binders)?;
            }
        }
        Expression::ListComprehension {
            list_expression,
            where_clause,
            transform_expression,
            ..
        } => {
            check_expr_references(list_expression, binders)?;
            if let Some(w) = where_clause {
                check_expr_references(w, binders)?;
            }
            if let Some(t) = transform_expression {
                check_expr_references(t, binders)?;
            }
        }
        Expression::PatternComprehension {
            where_clause,
            transform_expression,
            ..
        } => {
            // The pattern itself binds names (already in `binders`); its
            // property-map expressions are not reference-checked this
            // increment. The filter/transform may reference outer names.
            if let Some(w) = where_clause {
                check_expr_references(w, binders)?;
            }
            if let Some(t) = transform_expression {
                check_expr_references(t, binders)?;
            }
        }
        Expression::MapProjection { source, .. } => check_expr_references(source, binders)?,
        // Leaves and inner-scoped subqueries are not reference-checked here.
        Expression::Literal(_)
        | Expression::Parameter(_)
        | Expression::Exists { .. }
        | Expression::CollectSubquery { .. } => {}
    }
    Ok(())
}

fn undefined_variable(name: &str) -> crate::Error {
    crate::Error::CypherSyntax(format!(
        "UndefinedVariable: variable `{name}` is not defined in this scope"
    ))
}

fn variable_type_conflict(name: &str) -> crate::Error {
    crate::Error::CypherSyntax(format!(
        "VariableTypeConflict: variable `{name}` is used as both a node and a relationship"
    ))
}

fn variable_already_bound(name: &str) -> crate::Error {
    crate::Error::CypherSyntax(format!(
        "VariableAlreadyBound: variable `{name}` is already bound and cannot be \
         re-declared with labels or properties"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::parser::CypherParser;

    fn run(query: &str) -> crate::Result<()> {
        let ast = CypherParser::new(query.to_string())
            .parse()
            .unwrap_or_else(|e| panic!("query must parse: {query}: {e}"));
        validate(&ast)
    }

    /// A reference to a variable bound nowhere is rejected as a
    /// SyntaxError carrying the `UndefinedVariable` token.
    fn assert_undefined(query: &str) {
        let err = run(query).expect_err(&format!("expected UndefinedVariable for: {query}"));
        assert_eq!(
            format!("{:?}", err.opencypher_kind()),
            "SyntaxError",
            "{query} must classify as SyntaxError"
        );
        assert!(
            err.to_string().contains("UndefinedVariable"),
            "{query}: message must contain UndefinedVariable, got: {err}"
        );
    }

    /// A structurally valid query must pass validation untouched — a
    /// rejection here would be a user-visible regression.
    fn assert_ok(query: &str) {
        run(query).unwrap_or_else(|e| panic!("valid query wrongly rejected: {query}: {e}"));
    }

    #[test]
    fn undefined_variable_in_return_is_rejected() {
        assert_undefined("MATCH (a) RETURN b");
        assert_undefined("MATCH (a) RETURN a, b");
    }

    #[test]
    fn undefined_variable_in_where_is_rejected() {
        assert_undefined("MATCH (a) WHERE c.name = 'x' RETURN a");
    }

    #[test]
    fn bound_pattern_variables_pass() {
        assert_ok("MATCH (a)-[r]->(b) RETURN a, r, b");
        assert_ok("MATCH (a) WHERE a.age > 1 RETURN a");
        assert_ok("MATCH p = (a)-[r]->(b) RETURN p");
    }

    #[test]
    fn projection_and_unwind_binders_pass() {
        assert_ok("MATCH (a) WITH a AS x RETURN x");
        assert_ok("UNWIND [1, 2] AS n RETURN n");
        assert_ok("MATCH (a) CREATE (b) RETURN b");
    }

    #[test]
    fn distinct_aggregate_marker_is_not_a_variable() {
        // `count(DISTINCT a)` lowers to a synthetic `__DISTINCT__` arg that
        // must not be mistaken for an undefined variable.
        assert_ok("MATCH (a) RETURN count(DISTINCT a) AS c");
    }

    #[test]
    fn inline_binding_expressions_pass() {
        // List comprehension and list-predicate variables are bound locally.
        assert_ok("MATCH (a) RETURN [x IN [1, 2] | x] AS l");
        assert_ok("MATCH (a)-[r]->(b) RETURN any(x IN [1, 2] WHERE x > 0) AS y");
    }

    /// A SyntaxError carrying the `VariableTypeConflict` token.
    fn assert_type_conflict(query: &str) {
        let err = run(query).expect_err(&format!("expected VariableTypeConflict for: {query}"));
        assert_eq!(
            format!("{:?}", err.opencypher_kind()),
            "SyntaxError",
            "{query} must classify as SyntaxError"
        );
        assert!(
            err.to_string().contains("VariableTypeConflict"),
            "{query}: message must contain VariableTypeConflict, got: {err}"
        );
    }

    #[test]
    fn node_variable_reused_as_relationship_is_rejected() {
        assert_type_conflict("MATCH (a) MATCH ()-[a]-() RETURN a");
    }

    #[test]
    fn reusing_a_variable_as_the_same_type_passes() {
        // `a` is the same node at both ends — legal, not a type conflict.
        assert_ok("MATCH (a)-[r]->(a) RETURN a");
        // `a` bound as a node in two separate patterns — legal.
        assert_ok("MATCH (a) MATCH (a)-[r]->(b) RETURN a, b");
    }

    /// A SyntaxError carrying the `VariableAlreadyBound` token.
    fn assert_already_bound(query: &str) {
        let err = run(query).expect_err(&format!("expected VariableAlreadyBound for: {query}"));
        assert_eq!(
            format!("{:?}", err.opencypher_kind()),
            "SyntaxError",
            "{query} must classify as SyntaxError"
        );
        assert!(
            err.to_string().contains("VariableAlreadyBound"),
            "{query}: message must contain VariableAlreadyBound, got: {err}"
        );
    }

    #[test]
    fn create_redeclaring_a_bound_variable_with_structure_is_rejected() {
        // Adding properties to an already-bound node.
        assert_already_bound("MATCH (a) CREATE (a {name: 'foo'}) RETURN a");
        // Re-declaring across two CREATE clauses with a new label.
        assert_already_bound("CREATE (n:Foo) CREATE (n:Bar)-[:OWNS]->(:Dog)");
    }

    #[test]
    fn create_referencing_a_bound_variable_as_endpoint_passes() {
        // Bare reference endpoints carry no structure — legal.
        assert_ok("MATCH (a) CREATE (a)-[:R]->(b) RETURN b");
        assert_ok("MATCH (a), (b) CREATE (a)-[:KNOWS]->(b)");
        // Fresh variables with labels are not re-declarations.
        assert_ok("CREATE (a:X)-[:R]->(b:Y)");
    }

    #[test]
    fn queries_with_unmodeled_clauses_are_skipped() {
        // UNION scoping is not modeled yet — the pass bails out rather than
        // risk a false positive, so this must not error.
        assert_ok("MATCH (a) RETURN a UNION MATCH (b) RETURN b");
    }
}
