//! Quantified-path-pattern (QPP) operator building and the legacy-rewrite
//! feature flag.

use super::*;

/// point) leaves the legacy plan in place.
pub(super) fn recognise_order_by_distance(
    ob: &crate::executor::parser::OrderByClause,
) -> Option<(String, String, f64, f64)> {
    let item = ob.items.first()?;
    if !matches!(
        item.direction,
        crate::executor::parser::SortDirection::Ascending
    ) {
        return None;
    }
    let (name, args) = match &item.expression {
        Expression::FunctionCall { name, args } => (name.as_str(), args),
        _ => return None,
    };
    if !matches!(name, "distance" | "point.distance") {
        return None;
    }
    let (variable, property) = match args.first()? {
        Expression::PropertyAccess { variable, property } => (variable.clone(), property.clone()),
        _ => return None,
    };
    let (cx, cy) = extract_point_literal(args.get(1)?)?;
    Some((variable, property, cx, cy))
}

/// Read a non-negative integer literal as `usize`. Negative or
/// non-integer LIMIT values fall through to the legacy plan so the
/// planner never invents a coordinate the user didn't specify.
pub(super) fn extract_usize_literal(expr: &Expression) -> Option<usize> {
    match expr {
        Expression::Literal(crate::executor::parser::Literal::Integer(i)) if *i >= 0 => {
            Some(*i as usize)
        }
        _ => None,
    }
}

/// Extract a 4-tuple `(min_x, min_y, max_x, max_y)` from a bbox
/// expression of the shape `{bottomLeft: point(...), topRight:
/// point(...)}` literal map. Returns `None` for parameter / non-
/// literal forms — the planner falls back to `NodeByLabel + Filter`
/// because it can't see the coordinates at plan time.
pub(super) fn extract_bbox_literal(expr: &Expression) -> Option<(f64, f64, f64, f64)> {
    let map = match expr {
        Expression::Map(m) => m,
        _ => return None,
    };
    let bl = map.get("bottomLeft")?;
    let tr = map.get("topRight")?;
    let (min_x, min_y) = extract_point_literal(bl)?;
    let (max_x, max_y) = extract_point_literal(tr)?;
    Some((min_x, min_y, max_x, max_y))
}

/// Extract a `(x, y)` pair from a `point({x: <num>, y: <num>})`
/// literal expression. Returns `None` for any other shape so the
/// planner falls back to the legacy plan instead of guessing.
pub(super) fn extract_point_literal(expr: &Expression) -> Option<(f64, f64)> {
    match expr {
        Expression::Literal(crate::executor::parser::Literal::Point(p)) => Some((p.x, p.y)),
        // `point({x: 1, y: 2})` parses as a function call when the
        // parser can't fold it into a literal at parse time.
        Expression::FunctionCall { name, args } if name.eq_ignore_ascii_case("point") => {
            let inner = match args.first()? {
                Expression::Map(m) => m,
                _ => return None,
            };
            let x = extract_f64_literal(inner.get("x")?)?;
            let y = extract_f64_literal(inner.get("y")?)?;
            Some((x, y))
        }
        _ => None,
    }
}

/// Read a numeric literal as `f64`. Unlike the projection
/// evaluator's path, this never coerces strings — the rewriter
/// only matches when the planner can pin down the coordinate at
/// plan time.
pub(super) fn extract_f64_literal(expr: &Expression) -> Option<f64> {
    match expr {
        Expression::Literal(crate::executor::parser::Literal::Float(f)) => Some(*f),
        Expression::Literal(crate::executor::parser::Literal::Integer(i)) => Some(*i as f64),
        _ => None,
    }
}

/// Build a `QuantifiedExpand` operator from a `QuantifiedGroup`
/// that survived the slice-1 lowering. Returns
/// `ERR_QPP_NOT_IMPLEMENTED` for shapes the slice-2 operator
/// itself cannot handle yet (multi-hop body, no relationship
/// inside the body, …) so the user gets a clean recoverable
/// error instead of silently wrong rows.
///
/// `prev_node_var` follows the same convention as the surrounding
/// `add_relationship_operators` loop: the planner threads it across
/// pattern elements so each operator knows which outer variable
/// holds its source node. The function updates it in place to point
/// at the QPP's target binding before returning, so a follow-up
/// element in the pattern keeps chaining correctly.
pub(super) fn build_quantified_expand_operator(
    group: &QuantifiedGroup,
    prev_node_var: &mut Option<String>,
    pattern_elements: &[PatternElement],
    idx: usize,
    is_optional: bool,
    tmp_var_counter: &mut usize,
    catalog: &Catalog,
) -> Result<Operator> {
    // Body shape: alternating Node, Relationship, Node, Relationship,
    // …, Node. Length must be odd and ≥ 3. The outer `MATCH (a)( body
    // ){m,n}(b)` always wraps `body` in the parentheses; the inner
    // node patterns are the boundary nodes between hops.
    if group.inner.len() < 3 || group.inner.len() % 2 == 0 {
        return Err(Error::CypherExecution(format!(
            "ERR_QPP_NOT_IMPLEMENTED: QPP body must be Node, \
             Relationship, Node, … (alternating, odd length ≥ 3); \
             got {} elements",
            group.inner.len(),
        )));
    }

    // Walk the body splitting node specs and hop specs apart.
    let mut node_specs: Vec<NodePattern> = Vec::new();
    let mut hop_patterns: Vec<&RelationshipPattern> = Vec::new();
    for (i, element) in group.inner.iter().enumerate() {
        if i % 2 == 0 {
            match element {
                PatternElement::Node(n) => node_specs.push(n.clone()),
                _ => {
                    return Err(Error::CypherExecution(
                        "ERR_QPP_NOT_IMPLEMENTED: QPP body even-index \
                         elements must be node patterns"
                            .to_string(),
                    ));
                }
            }
        } else {
            match element {
                PatternElement::Relationship(r) => {
                    if r.quantifier.is_some() {
                        return Err(Error::CypherExecution(
                            "ERR_QPP_NOT_IMPLEMENTED: stacking a \
                             relationship quantifier inside a QPP body \
                             is not yet supported"
                                .to_string(),
                        ));
                    }
                    hop_patterns.push(r);
                }
                _ => {
                    return Err(Error::CypherExecution(
                        "ERR_QPP_NOT_IMPLEMENTED: QPP body odd-index \
                         elements must be relationship patterns"
                            .to_string(),
                    ));
                }
            }
        }
    }

    // Resolve the source variable from the pattern context. The
    // surrounding loop fills `prev_node_var` from the previous
    // element (a Node), so it is always populated when the QPP
    // arm fires for a well-formed pattern.
    let source_var = prev_node_var.clone().ok_or_else(|| {
        Error::CypherExecution(
            "ERR_QPP_INTERNAL: quantified path pattern has no \
                 outer source node — every QPP must follow a node \
                 pattern"
                .to_string(),
        )
    })?;

    // Resolve the target variable from the next element in the
    // outer pattern. If the pattern ends with the QPP (no trailing
    // boundary node), generate a temporary variable so the operator
    // has somewhere to bind the last reached node.
    let target_var = if idx + 1 < pattern_elements.len() {
        if let PatternElement::Node(n) = &pattern_elements[idx + 1] {
            n.variable.clone().unwrap_or_else(|| {
                let v = format!("__qpp_target_{}", *tmp_var_counter);
                *tmp_var_counter += 1;
                v
            })
        } else {
            let v = format!("__qpp_target_{}", *tmp_var_counter);
            *tmp_var_counter += 1;
            v
        }
    } else {
        let v = format!("__qpp_target_{}", *tmp_var_counter);
        *tmp_var_counter += 1;
        v
    };

    let hops: Vec<crate::executor::types::QppHopSpec> = hop_patterns
        .iter()
        .map(|rel| {
            let direction = match rel.direction {
                RelationshipDirection::Outgoing => Direction::Outgoing,
                RelationshipDirection::Incoming => Direction::Incoming,
                RelationshipDirection::Both => Direction::Both,
            };
            let type_ids: Vec<u32> = rel
                .types
                .iter()
                .filter_map(|name| {
                    catalog
                        .get_type_id(name)
                        .ok()
                        .flatten()
                        .or_else(|| catalog.get_or_create_type(name).ok())
                })
                .collect();
            crate::executor::types::QppHopSpec {
                type_ids,
                direction,
                var: rel.variable.clone(),
                properties: rel.properties.clone(),
            }
        })
        .collect();

    let inner_nodes: Vec<crate::executor::types::QppNodeSpec> = node_specs
        .iter()
        .map(|n| crate::executor::types::QppNodeSpec {
            var: n.variable.clone(),
            labels: n.labels.clone(),
            properties: n.properties.clone(),
        })
        .collect();

    // Push the new binding so the next iteration of the surrounding
    // loop chains onto the QPP's target.
    *prev_node_var = Some(target_var.clone());

    let (min_length, max_length) = quantifier_to_bounds(&group.quantifier);

    Ok(Operator::QuantifiedExpand {
        source_var,
        target_var,
        hops,
        inner_nodes,
        inner_where: group.where_clause.clone(),
        min_length,
        max_length,
        optional: is_optional,
        mode: group.mode,
    })
}

/// Convert a parser-side `RelationshipQuantifier` into the
/// `(min, max)` pair the executor consumes. Matches
/// `execute_variable_length_path` desugaring so identical
/// quantifiers behave identically across the two operators.
pub(super) fn quantifier_to_bounds(q: &RelationshipQuantifier) -> (usize, usize) {
    match q {
        RelationshipQuantifier::ZeroOrMore => (0, usize::MAX),
        RelationshipQuantifier::OneOrMore => (1, usize::MAX),
        RelationshipQuantifier::ZeroOrOne => (0, 1),
        RelationshipQuantifier::Exact(n) => (*n, *n),
        RelationshipQuantifier::Range(min, max) => (*min, *max),
    }
}

#[allow(dead_code)]
fn _qpp_ensure_propertymap_unused(_: &PropertyMap, _: &RelationshipPattern) {}

/// Process-wide flag controlling whether the planner rewrites
/// legacy `*m..n` quantifiers to `Operator::QuantifiedExpand`
/// (slice-3b §6.5). The flag is read once per planner invocation
/// from a `OnceLock<AtomicBool>` whose initial value comes from
/// `NEXUS_QPP_REWRITE_LEGACY` (set / non-empty / not "0" → on).
///
/// Why a static instead of `std::env::var` per call: tests run in
/// parallel and each one needs to flip the flag without leaking
/// state across threads. An `AtomicBool` makes the flag observable
/// per process but unset between tests via
/// `set_legacy_var_length_rewrite_enabled(false)` in `Drop` /
/// teardown — which `std::env` can't deliver because env reads /
/// writes are not synchronised across threads on Windows.
fn qpp_legacy_rewrite_flag() -> &'static std::sync::atomic::AtomicBool {
    use std::sync::OnceLock;
    use std::sync::atomic::AtomicBool;
    static FLAG: OnceLock<AtomicBool> = OnceLock::new();
    FLAG.get_or_init(|| {
        let initial = std::env::var("NEXUS_QPP_REWRITE_LEGACY")
            .ok()
            .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes"))
            .unwrap_or(false);
        AtomicBool::new(initial)
    })
}

/// Read the slice-3b §6.5 rewrite flag. See
/// `qpp_legacy_rewrite_flag` for the rationale.
pub(crate) fn qpp_legacy_rewrite_enabled() -> bool {
    qpp_legacy_rewrite_flag().load(std::sync::atomic::Ordering::Relaxed)
}

/// Flip the slice-3b §6.5 rewrite flag for the current process.
/// Tests use this to exercise both the default-off and rewrite-on
/// branches without touching the env. The flag is `pub(crate)`
/// because external callers should configure the rewrite via the
/// `NEXUS_QPP_REWRITE_LEGACY` env var at startup, not by poking
/// process state at runtime.
pub(crate) fn set_qpp_legacy_rewrite_enabled(enabled: bool) {
    qpp_legacy_rewrite_flag().store(enabled, std::sync::atomic::Ordering::Relaxed);
}
