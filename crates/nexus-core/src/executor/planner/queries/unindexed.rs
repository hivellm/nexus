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
                let candidate = match (left.as_ref(), right.as_ref()) {
                    (Expression::PropertyAccess { variable, property }, _) => {
                        Some((variable, property))
                    }
                    (_, Expression::PropertyAccess { variable, property }) => {
                        Some((variable, property))
                    }
                    _ => None,
                };
                if let Some((variable, property)) = candidate
                    && let Some(label_name) = var_label.get(variable)
                    && let Ok(label_id) = catalog.get_label_id(label_name)
                    && let Ok(key_id) = catalog.get_key_id(property)
                    && !prop_idx.has_index(label_id, key_id)
                {
                    record_unindexed_into(label_id, key_id, label_name, property, clause, out);
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
