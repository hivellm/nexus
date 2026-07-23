//! Unindexed-property-access notification helpers: the public entry point and
//! free functions used by both the planner pre-pass and the engine write path.

use super::notifications::{UnindexedAccessClause, planner_warn_interval, warn_log_state};
use super::*;

/// Public entry point: compute `Nexus.Performance.UnindexedPropertyAccess`
/// notifications for a parsed `CypherQuery` against a given
/// `(Catalog, PropertyIndex)` pair. Returns the notifications in
/// emission order (deduplicated within the call) and drives the same
/// rate-limited WARN log mirror that the planner uses.
///
/// Two callers:
///   - The planner pre-pass during `QueryPlanner::plan_query` —
///     covers MATCH/READ paths that go through `Executor::execute`.
///   - `Engine::execute_write_query` — covers MERGE/SET/REMOVE/FOREACH
///     paths that bypass the planner entirely. Without this hook the
///     ingest pathology that drove this feature (every Cortex
///     `MERGE (n:Artifact { natural_key: ... })`) would ship without
///     a notification because the write path never builds a plan.
pub fn compute_unindexed_property_access_notifications(
    catalog: &Catalog,
    prop_idx: &crate::index::PropertyIndex,
    query: &CypherQuery,
) -> Vec<Notification> {
    let mut out: Vec<Notification> = Vec::new();

    // First pass: collect (variable -> first_label) bindings across
    // every MATCH/MERGE pattern in the query so the WHERE walker can
    // resolve `n.prop` references back to a label.
    let mut var_label: HashMap<String, String> = HashMap::new();
    for clause in &query.clauses {
        match clause {
            Clause::Match(mc) => collect_node_var_labels(&mc.pattern, &mut var_label),
            Clause::Merge(mc) => collect_node_var_labels(&mc.pattern, &mut var_label),
            _ => {}
        }
    }

    // Second pass: emit one notification per offending (label, prop)
    // pair, deduplicated within `out`.
    for clause in &query.clauses {
        match clause {
            Clause::Match(mc) => {
                emit_unindexed_for_pattern_into(
                    catalog,
                    prop_idx,
                    &mc.pattern,
                    UnindexedAccessClause::Match,
                    &mut out,
                );
                if let Some(where_clause) = &mc.where_clause {
                    emit_unindexed_for_where_into(
                        catalog,
                        prop_idx,
                        &where_clause.expression,
                        &var_label,
                        UnindexedAccessClause::Match,
                        &mut out,
                    );
                }
            }
            Clause::Merge(mc) => {
                emit_unindexed_for_pattern_into(
                    catalog,
                    prop_idx,
                    &mc.pattern,
                    UnindexedAccessClause::Merge,
                    &mut out,
                );
            }
            Clause::Where(wc) => {
                emit_unindexed_for_where_into(
                    catalog,
                    prop_idx,
                    &wc.expression,
                    &var_label,
                    UnindexedAccessClause::Match,
                    &mut out,
                );
            }
            _ => {}
        }
    }

    out
}

/// Free-function form of `QueryPlanner::emit_unindexed_for_pattern`
/// — pushes into a caller-provided `&mut Vec<Notification>` instead
/// of the planner's per-call accumulator. Used by both the planner
/// (via the pre-pass) and the engine write path.
fn emit_unindexed_for_pattern_into(
    catalog: &Catalog,
    prop_idx: &crate::index::PropertyIndex,
    pattern: &Pattern,
    clause: UnindexedAccessClause,
    out: &mut Vec<Notification>,
) {
    for el in &pattern.elements {
        if let PatternElement::Node(node) = el {
            let Some(label_name) = node.labels.first() else {
                continue;
            };
            let Some(properties) = &node.properties else {
                continue;
            };
            let Ok(label_id) = catalog.get_label_id(label_name) else {
                continue;
            };
            for (prop_name, _value) in &properties.properties {
                match catalog.get_key_id(prop_name) {
                    Ok(key_id) => {
                        // §3.3: the inline property-map form now plans a
                        // genuine per-row `NodeIndexSeek` (§2/§3) for both
                        // constant and row-local (`r.s`) values, so this
                        // path no longer distinguishes them — silence
                        // whenever an index exists. `CorrelatedPropertyPredicate`
                        // only remains for the WHERE `=` form
                        // (`emit_unindexed_for_where_into`), which does not
                        // yet seek per row.
                        if !prop_idx.has_index(label_id, key_id) {
                            record_unindexed_into(
                                label_id, key_id, label_name, prop_name, clause, out,
                            );
                        }
                    }
                    // An un-interned property key cannot have an index (the
                    // catalog does not intern keys on the CREATE/MERGE
                    // write path), so this is by definition unindexed
                    // access — emit. `u32::MAX` is a sentinel for the WARN
                    // rate-limiter only; `out` dedups by label/prop name.
                    Err(_) => {
                        record_unindexed_into(
                            label_id,
                            u32::MAX,
                            label_name,
                            prop_name,
                            clause,
                            out,
                        );
                    }
                }
            }
        }
    }
}

/// Free-function form of `QueryPlanner::emit_unindexed_for_where`.
fn emit_unindexed_for_where_into(
    catalog: &Catalog,
    prop_idx: &crate::index::PropertyIndex,
    expr: &Expression,
    var_label: &HashMap<String, String>,
    clause: UnindexedAccessClause,
    out: &mut Vec<Notification>,
) {
    match expr {
        Expression::BinaryOp { left, op, right } => {
            if matches!(op, BinaryOperator::Equal) {
                // `candidate` is the node-side operand (resolvable via
                // `var_label`); `other` is whatever it is being compared
                // against. Left is preferred when both sides are property
                // accesses, matching the pre-existing resolution order.
                let candidate = match (left.as_ref(), right.as_ref()) {
                    (Expression::PropertyAccess { variable, property }, other) => {
                        Some((variable, property, other))
                    }
                    (other, Expression::PropertyAccess { variable, property }) => {
                        Some((variable, property, other))
                    }
                    _ => None,
                };
                if let Some((variable, property, other)) = candidate
                    && let Some(label_name) = var_label.get(variable)
                    && let Ok(label_id) = catalog.get_label_id(label_name)
                    && let Ok(key_id) = catalog.get_key_id(property)
                {
                    if !prop_idx.has_index(label_id, key_id) {
                        record_unindexed_into(label_id, key_id, label_name, property, clause, out);
                    } else if is_correlated_where_operand(other, var_label) {
                        record_correlated_predicate_into(
                            label_id, key_id, label_name, property, clause, out,
                        );
                    }
                }
            } else if matches!(
                op,
                BinaryOperator::GreaterThan
                    | BinaryOperator::GreaterThanOrEqual
                    | BinaryOperator::LessThan
                    | BinaryOperator::LessThanOrEqual
                    | BinaryOperator::In
                    | BinaryOperator::StartsWith
                    | BinaryOperator::Contains
            ) {
                // Range and `IN` predicates DO seek on an indexed property
                // (`where_range_seek_operand` / `where_in_seek_operand` in
                // `strategy.rs`), provided the compared-against operand is a
                // plan-time literal. `CONTAINS` — an unanchored substring
                // match no ordered index can seek — still full scans even
                // with an index, so it keeps firing regardless of
                // `has_index`.
                // `property_on_left` matters: `IN` and `STARTS WITH` only seek
                // with the property on the LEFT (`'abc' STARTS WITH n.name`
                // asks the opposite question), while a range comparison seeks
                // either way because the planner mirrors the operator.
                let candidate = match (left.as_ref(), right.as_ref()) {
                    (Expression::PropertyAccess { variable, property }, other) => {
                        Some((variable, property, other, true))
                    }
                    (other, Expression::PropertyAccess { variable, property }) => {
                        Some((variable, property, other, false))
                    }
                    _ => None,
                };
                if let Some((variable, property, other, property_on_left)) = candidate
                    && let Some(label_name) = var_label.get(variable)
                    && let Ok(label_id) = catalog.get_label_id(label_name)
                    && let Ok(key_id) = catalog.get_key_id(property)
                {
                    let already_indexed = prop_idx.has_index(label_id, key_id);
                    // An indexed pair whose predicate shape AND operand the
                    // planner can lift produces a real seek — notifying there
                    // would claim a full scan that does not happen.
                    if !(already_indexed
                        && seekable_predicate_operand(*op, other, property_on_left))
                    {
                        record_unindexed_predicate_shape_into(
                            label_id,
                            key_id,
                            label_name,
                            property,
                            already_indexed,
                            clause,
                            out,
                        );
                    }
                }
            }
            emit_unindexed_for_where_into(catalog, prop_idx, left, var_label, clause, out);
            emit_unindexed_for_where_into(catalog, prop_idx, right, var_label, clause, out);
        }
        Expression::UnaryOp { operand, .. } => {
            emit_unindexed_for_where_into(catalog, prop_idx, operand, var_label, clause, out);
        }
        _ => {}
    }
}

/// Whether `(op, other)` is a predicate the planner actually lifts to an
/// index seek when the `(label, property)` pair is indexed — the exact
/// mirror of the `where_*_seek_operand` helpers in `strategy.rs`. Keeps the
/// notification honest: it must fire when the query really full scans, and
/// stay silent when it seeks.
///
/// Range comparisons seek against a scalar literal threshold in either
/// operand order (the planner mirrors the operator); `IN` seeks a literal
/// list and `STARTS WITH` a literal prefix, both only with the property on
/// the left (`property_on_left`). `CONTAINS` is an unanchored substring match
/// — no ordered index can seek it, so it is never seekable. A `$parameter`
/// operand on any of these shapes has no plan-time value, so it still full
/// scans.
fn seekable_predicate_operand(
    op: BinaryOperator,
    other: &Expression,
    property_on_left: bool,
) -> bool {
    let is_scalar_literal = |e: &Expression| {
        matches!(
            e,
            Expression::Literal(
                Literal::String(_) | Literal::Integer(_) | Literal::Float(_) | Literal::Boolean(_)
            )
        )
    };
    match op {
        BinaryOperator::GreaterThan
        | BinaryOperator::GreaterThanOrEqual
        | BinaryOperator::LessThan
        | BinaryOperator::LessThanOrEqual => is_scalar_literal(other),
        BinaryOperator::In => match other {
            Expression::List(elements) => {
                property_on_left
                    && elements.iter().all(|e| {
                        is_scalar_literal(e) || matches!(e, Expression::Literal(Literal::Null))
                    })
            }
            _ => false,
        },
        BinaryOperator::StartsWith => {
            property_on_left && matches!(other, Expression::Literal(Literal::String(_)))
        }
        _ => false,
    }
}

/// Whether the non-node-side operand of a WHERE `=` predicate is
/// correlated (row-local): a bare variable, or a property access whose
/// variable is *not* one of the matched node variables in `var_label`.
/// A property access on another matched node (e.g. `a.id = b.id`) is a
/// join predicate, not a per-driving-row value, so it is deliberately
/// excluded here.
fn is_correlated_where_operand(operand: &Expression, var_label: &HashMap<String, String>) -> bool {
    match operand {
        Expression::Variable(_) => true,
        Expression::PropertyAccess { variable, .. } => !var_label.contains_key(variable),
        _ => false,
    }
}

/// Free-function form of `QueryPlanner::record_unindexed_property_access`
/// — pushes into a caller-provided `&mut Vec<Notification>`,
/// deduplicates within that vec, and drives the same rate-limited
/// WARN log mirror.
fn record_unindexed_into(
    label_id: u32,
    key_id: u32,
    label_name: &str,
    property_name: &str,
    clause: UnindexedAccessClause,
    out: &mut Vec<Notification>,
) {
    let code = "Nexus.Performance.UnindexedPropertyAccess";

    // Per-call dedup so MATCH + MERGE on the same (label, prop) pair
    // produce a single notification.
    if out.iter().any(|n| {
        n.code == code && n.title.contains(label_name) && n.description.contains(property_name)
    }) {
        return;
    }

    let suggested_ddl = format!("CREATE INDEX FOR (n:{label_name}) ON (n.{property_name})");
    let title = format!("Unindexed property access on :{label_name}({property_name})");
    let description = format!(
        "{clause} selects nodes by `:{label_name}` with a property predicate on \
         `{property_name}`, but no property index covers this pair. The planner \
         falls back to a full label scan plus property comparison, which is \
         O(N) over every `:{label_name}` node. Create the recommended index to \
         switch to an O(log N) index seek: `{suggested_ddl}`.",
    );

    out.push(Notification {
        code: code.to_string(),
        title,
        description,
        severity: NotificationSeverity::Information,
        category: NotificationCategory::Performance,
    });

    // Rate-limited WARN log — shared across planner and engine paths
    // via the process-global `warn_log_state()`.
    let now = Instant::now();
    let interval = planner_warn_interval();
    let mut should_log = true;
    if let Ok(mut state) = warn_log_state().lock() {
        if let Some(last) = state.get(&(label_id, key_id)) {
            if now.duration_since(*last) < interval {
                should_log = false;
            }
        }
        if should_log {
            state.insert((label_id, key_id), now);
        }
    }
    if should_log {
        tracing::warn!(
            code = code,
            label = label_name,
            property = property_name,
            clause = %clause,
            suggested = %suggested_ddl,
            "unindexed property access on :{}({}) — {}",
            label_name,
            property_name,
            suggested_ddl,
        );
    }
}

/// Records `Nexus.Performance.UnindexedPropertyAccess` for a WHERE
/// predicate that still ends up on a full scan: either the pair has no index
/// at all, or the predicate is one the planner cannot lift even with one —
/// `CONTAINS` (an unanchored substring match no ordered index can seek), and
/// any range / `IN` / `STARTS WITH` whose compared-against operand is not a
/// plan-time literal. The caller (`emit_unindexed_for_where_into`) filters out
/// the shapes that DO seek via `seekable_predicate_operand`, so reaching this
/// function means a scan really happens.
///
/// `already_indexed` selects between the two message bodies so the
/// notification never claims "no index" when one actually exists, and never
/// suggests creating an index that would not change the plan.
fn record_unindexed_predicate_shape_into(
    label_id: u32,
    key_id: u32,
    label_name: &str,
    property_name: &str,
    already_indexed: bool,
    clause: UnindexedAccessClause,
    out: &mut Vec<Notification>,
) {
    let code = "Nexus.Performance.UnindexedPropertyAccess";

    // Per-call dedup, same contract as `record_unindexed_into`.
    if out.iter().any(|n| {
        n.code == code && n.title.contains(label_name) && n.description.contains(property_name)
    }) {
        return;
    }

    let title = format!("Unindexed property access on :{label_name}({property_name})");
    let description = if already_indexed {
        format!(
            "{clause} selects nodes by `:{label_name}` with a property predicate on \
             `{property_name}` that cannot be served by the existing index — either \
             an unanchored `CONTAINS` match, which no ordered index can seek, or a \
             comparison against a value that is not known at plan time. Equality, \
             range, `IN`, and `STARTS WITH` against a literal (and `=` against a \
             `$parameter`) all seek `:{label_name}({property_name})`'s index. The \
             query falls back to a full label scan plus property comparison, which \
             is O(N) over every `:{label_name}` node.",
        )
    } else {
        let suggested_ddl = format!("CREATE INDEX FOR (n:{label_name}) ON (n.{property_name})");
        format!(
            "{clause} selects nodes by `:{label_name}` with a property predicate on \
             `{property_name}`, but no property index covers this pair. The planner \
             falls back to a full label scan plus property comparison, which is \
             O(N) over every `:{label_name}` node. Create the recommended index to \
             switch to an O(log N) index seek: `{suggested_ddl}`.",
        )
    };

    out.push(Notification {
        code: code.to_string(),
        title,
        description,
        severity: NotificationSeverity::Information,
        category: NotificationCategory::Performance,
    });

    // Rate-limited WARN log — shared across planner and engine paths via
    // the process-global `warn_log_state()`.
    let now = Instant::now();
    let interval = planner_warn_interval();
    let mut should_log = true;
    if let Ok(mut state) = warn_log_state().lock() {
        if let Some(last) = state.get(&(label_id, key_id)) {
            if now.duration_since(*last) < interval {
                should_log = false;
            }
        }
        if should_log {
            state.insert((label_id, key_id), now);
        }
    }
    if should_log {
        tracing::warn!(
            code = code,
            label = label_name,
            property = property_name,
            clause = %clause,
            already_indexed = already_indexed,
            "unindexed-shaped property access on :{}({}) — planner cannot seek \
             this predicate (unanchored CONTAINS, or a non-plan-time operand)",
            label_name,
            property_name,
        );
    }
}

/// Records `Nexus.Performance.CorrelatedPropertyPredicate` — a distinct
/// notification from `record_unindexed_into` for the case where a
/// `(label, property)` pair *is* indexed, but the predicate value is
/// row-local (evaluated per driving row, e.g. from `UNWIND`/`WITH`
/// rather than a plan-time constant) and the current planner cannot use
/// the index seek for a per-row key. The remedy here is not "create an
/// index" — one already exists — it is a planner limitation tracked
/// separately.
fn record_correlated_predicate_into(
    label_id: u32,
    key_id: u32,
    label_name: &str,
    property_name: &str,
    clause: UnindexedAccessClause,
    out: &mut Vec<Notification>,
) {
    let code = "Nexus.Performance.CorrelatedPropertyPredicate";

    // Per-call dedup so MATCH + MERGE on the same (label, prop) pair
    // produce a single notification.
    if out.iter().any(|n| {
        n.code == code && n.title.contains(label_name) && n.description.contains(property_name)
    }) {
        return;
    }

    let title = format!(
        "Correlated property predicate on :{label_name}({property_name}) is not index-backed"
    );
    let description = format!(
        "{clause} selects nodes by `:{label_name}` with a property predicate on \
         `{property_name}` whose value is row-local — evaluated per driving row \
         (e.g. from `UNWIND` or an earlier `WITH` binding) rather than known at \
         plan time. An index on `:{label_name}({property_name})` already exists, \
         but the current planner cannot use it to seek a per-row key, so it falls \
         back to a full label scan per driving row, which is O(rows × N) over \
         every `:{label_name}` node. Creating another index will not help — this \
         is a planner limitation, not a missing index.",
    );

    out.push(Notification {
        code: code.to_string(),
        title,
        description,
        severity: NotificationSeverity::Information,
        category: NotificationCategory::Performance,
    });

    // Rate-limited WARN log — shared across planner and engine paths
    // via the process-global `warn_log_state()`.
    let now = Instant::now();
    let interval = planner_warn_interval();
    let mut should_log = true;
    if let Ok(mut state) = warn_log_state().lock() {
        if let Some(last) = state.get(&(label_id, key_id)) {
            if now.duration_since(*last) < interval {
                should_log = false;
            }
        }
        if should_log {
            state.insert((label_id, key_id), now);
        }
    }
    if should_log {
        tracing::warn!(
            code = code,
            label = label_name,
            property = property_name,
            clause = %clause,
            "correlated property predicate on :{}({}) cannot use the existing index — \
             value is evaluated per driving row",
            label_name,
            property_name,
        );
    }
}

/// Helper: walk a pattern and record `node.variable -> first_label`
/// for every node element that has both. Used by the WHERE walker to
/// resolve `n.prop` references back to a label without re-running the
/// full planner. Free function (not a method) so the `&mut self`
/// borrow held by `scan_unindexed_property_access` does not collide
/// with the immutable pattern walk.
fn collect_node_var_labels(pattern: &Pattern, out: &mut HashMap<String, String>) {
    for el in &pattern.elements {
        if let PatternElement::Node(node) = el
            && let Some(var) = &node.variable
            && let Some(label) = node.labels.first()
        {
            out.entry(var.clone()).or_insert_with(|| label.clone());
        }
    }
}
