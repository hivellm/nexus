//! WHERE-form index-seek lowering family: AND-conjunct flatten/rebuild
//! helpers, the `where_equality_index_seek_for` dispatcher, and the
//! individual seek-operand lowerings it tries in turn (equality, range,
//! `IN`, `STARTS WITH`, `$parameter` equality). `literal_property_value` is
//! the single source of truth for which plan-time literal types are
//! indexable, shared by the range/`IN`/equality operand lowerings below.

use super::*;

/// Convert a plan-time expression into the `PropertyValue` the property
/// index is keyed on, or `None` when it is not a scalar literal the planner
/// can seek with. `$parameter`s, `null`, points, and computed expressions all
/// return `None` — the seek-operand helpers treat that as "cannot lift, keep
/// the scan". Single source of truth for every literal-keyed seek so the
/// equality, range, and `IN` lifts can never drift apart on which literal
/// types are indexable.
fn literal_property_value(expr: &Expression) -> Option<crate::index::PropertyValue> {
    match expr {
        Expression::Literal(Literal::String(s)) => {
            Some(crate::index::PropertyValue::String(s.clone()))
        }
        Expression::Literal(Literal::Integer(i)) => Some(crate::index::PropertyValue::Integer(*i)),
        Expression::Literal(Literal::Float(f)) => Some(crate::index::PropertyValue::Float(*f)),
        Expression::Literal(Literal::Boolean(b)) => Some(crate::index::PropertyValue::Boolean(*b)),
        _ => None,
    }
}

impl<'a> QueryPlanner<'a> {
    /// Flatten a WHERE-clause expression into its top-level AND-conjuncts,
    /// recursing through nested `AND`s (`a AND b AND c` yields `[a, b,
    /// c]`). A non-`AND` expression (including one rooted in `OR`, since
    /// splitting an `OR`'s branches would change its meaning) yields
    /// itself as the sole conjunct.
    pub(super) fn flatten_and_conjuncts(expr: &Expression, out: &mut Vec<Expression>) {
        if let Expression::BinaryOp {
            left,
            op: BinaryOperator::And,
            right,
        } = expr
        {
            Self::flatten_and_conjuncts(left, out);
            Self::flatten_and_conjuncts(right, out);
        } else {
            out.push(expr.clone());
        }
    }

    /// Rebuild an AND-conjunction `Expression` from its conjuncts — the
    /// inverse of [`Self::flatten_and_conjuncts`]. Returns `None` for an
    /// empty list (every conjunct on that WHERE-clause entry was lifted
    /// into a seek by [`Self::where_equality_index_seek_for`], so the
    /// caller should drop the entry rather than emit an empty/always-true
    /// Filter).
    pub(super) fn rebuild_and_conjunction(conjuncts: Vec<Expression>) -> Option<Expression> {
        let mut iter = conjuncts.into_iter();
        let first = iter.next()?;
        Some(iter.fold(first, |acc, next| Expression::BinaryOp {
            left: Box::new(acc),
            op: BinaryOperator::And,
            right: Box::new(next),
        }))
    }

    /// WHERE-form counterpart to [`Self::node_index_seek_for`]: looks for
    /// a top-level `variable.prop = <constant>` conjunct across
    /// `residual` — the AND-conjunct-decomposed working copy of the
    /// query's WHERE clauses, built once in [`Self::plan_execution_strategy`]
    /// — whose `(label_id, prop)` pair has a registered single-property
    /// index. On a match, removes the consumed conjunct from `residual`
    /// (dropping the whole entry once it has no conjuncts left) and
    /// returns the `NodeIndexSeek` operator to emit in place of
    /// `NodeByLabel`. Returns `None` when no such conjunct exists, mirroring
    /// `node_index_seek_for`'s "caller falls back to `NodeByLabel`" contract.
    ///
    /// Only inspects entries with an EMPTY `optional_vars` — lifting a
    /// conjunct out of what would become an `OptionalFilter` would change
    /// OPTIONAL MATCH semantics (a failed predicate there nulls the
    /// optional variables rather than dropping the row), so OPTIONAL
    /// MATCH WHERE clauses are left untouched and keep going through the
    /// existing `OptionalFilter` path.
    pub(super) fn where_equality_index_seek_for(
        &self,
        variable: &str,
        label_id: u32,
        residual: &mut [(Vec<Expression>, Vec<String>)],
    ) -> Option<Operator> {
        self.property_index?;
        for (conjuncts, optional_vars) in residual.iter_mut() {
            if !optional_vars.is_empty() {
                continue;
            }
            for i in 0..conjuncts.len() {
                if let Some(seek) = self
                    .where_equality_seek_operand(&conjuncts[i], variable, label_id)
                    .or_else(|| self.where_range_seek_operand(&conjuncts[i], variable, label_id))
                    .or_else(|| self.where_in_seek_operand(&conjuncts[i], variable, label_id))
                    .or_else(|| self.where_prefix_seek_operand(&conjuncts[i], variable, label_id))
                {
                    conjuncts.remove(i);
                    return Some(seek);
                }
            }
        }
        // Second pass, and only once no plan-time-literal conjunct anywhere in
        // `residual` could be lifted: a `$parameter` equality seek. It ranks
        // last because its key is unknown until execution, so it may still
        // fall back to a scan — a literal seek is always the better plan.
        // NOTE: the conjunct is deliberately NOT removed. `NodeIndexParamSeek`
        // degrades to a full label scan for a list/map or missing parameter,
        // and the retained `Filter` is what keeps that fallback correct.
        for (conjuncts, optional_vars) in residual.iter() {
            if !optional_vars.is_empty() {
                continue;
            }
            for conjunct in conjuncts.iter() {
                if let Some(seek) = self.where_param_seek_operand(conjunct, variable, label_id) {
                    return Some(seek);
                }
            }
        }
        None
    }

    /// If `conjunct` is a top-level range comparison `variable.prop > | >= | <
    /// | <= <literal>` (or the mirrored `<literal> <op> variable.prop`) and
    /// `(label_id, prop)` has a single-property index, return the
    /// `NodeIndexRangeSeek` to emit in its place. The B-tree supports range and
    /// prefix scans natively, so this only lifts the plan; results are
    /// unchanged. `CONTAINS` (an unanchored substring match, which no ordered
    /// index can seek) still falls back to a full scan.
    fn where_range_seek_operand(
        &self,
        conjunct: &Expression,
        variable: &str,
        label_id: u32,
    ) -> Option<Operator> {
        let prop_idx = self.property_index?;
        let Expression::BinaryOp { left, op, right } = conjunct else {
            return None;
        };
        let base = match op {
            BinaryOperator::GreaterThan => RangeSeekOp::Gt,
            BinaryOperator::GreaterThanOrEqual => RangeSeekOp::Ge,
            BinaryOperator::LessThan => RangeSeekOp::Lt,
            BinaryOperator::LessThanOrEqual => RangeSeekOp::Le,
            _ => return None,
        };
        // `prop <op> literal` keeps `op`; `literal <op> prop` mirrors it.
        let mirror = |o: RangeSeekOp| match o {
            RangeSeekOp::Gt => RangeSeekOp::Lt,
            RangeSeekOp::Ge => RangeSeekOp::Le,
            RangeSeekOp::Lt => RangeSeekOp::Gt,
            RangeSeekOp::Le => RangeSeekOp::Ge,
        };
        let (property, value_expr, seek_op) = match (left.as_ref(), right.as_ref()) {
            (
                Expression::PropertyAccess {
                    variable: v,
                    property,
                },
                other,
            ) if v == variable => (property, other, base),
            (
                other,
                Expression::PropertyAccess {
                    variable: v,
                    property,
                },
            ) if v == variable => (property, other, mirror(base)),
            _ => return None,
        };
        let key_id = self.catalog.get_key_id(property).ok()?;
        if !prop_idx.has_index(label_id, key_id) {
            return None;
        }
        let value = literal_property_value(value_expr)?;
        Some(Operator::NodeIndexRangeSeek {
            label_id,
            key_id,
            op: seek_op,
            value,
            variable: variable.to_string(),
        })
    }

    /// If `conjunct` is a top-level `variable.prop IN [<literals>]` and
    /// `(label_id, prop)` has a single-property index, return the
    /// `NodeIndexInSeek` to emit in its place — one point seek per element,
    /// bitmap-OR'd at execution time. The union is exactly the predicate's
    /// match set (a node satisfies `prop IN list` iff its property equals one
    /// of the listed values), so the caller drops the consumed conjunct.
    ///
    /// Bails out — leaving the full scan — when ANY element is not a plan-time
    /// scalar literal: a `$parameter` or a computed element would make the
    /// seek an UNDER-approximation, and the lifted conjunct is no longer
    /// around to correct it. `NULL` elements are the one exception: a null
    /// element can only ever make the comparison `null`, never `true`
    /// (`1 IN [1, null]` is true only because of the `1`), so dropping them
    /// preserves the match set exactly. A list that is empty — or all nulls —
    /// lifts to a seek with no values, which correctly matches nothing.
    fn where_in_seek_operand(
        &self,
        conjunct: &Expression,
        variable: &str,
        label_id: u32,
    ) -> Option<Operator> {
        let prop_idx = self.property_index?;
        let Expression::BinaryOp {
            left,
            op: BinaryOperator::In,
            right,
        } = conjunct
        else {
            return None;
        };
        // Only `prop IN list` seeks — the mirrored `list IN prop` is a
        // containment test on a list-valued property, a different predicate.
        let Expression::PropertyAccess {
            variable: v,
            property,
        } = left.as_ref()
        else {
            return None;
        };
        if v != variable {
            return None;
        }
        let Expression::List(elements) = right.as_ref() else {
            return None;
        };
        let key_id = self.catalog.get_key_id(property).ok()?;
        if !prop_idx.has_index(label_id, key_id) {
            return None;
        }
        let mut values = Vec::with_capacity(elements.len());
        for element in elements {
            if matches!(element, Expression::Literal(Literal::Null)) {
                continue;
            }
            values.push(literal_property_value(element)?);
        }
        Some(Operator::NodeIndexInSeek {
            label_id,
            key_id,
            values,
            variable: variable.to_string(),
        })
    }

    /// If `conjunct` is a top-level `variable.prop STARTS WITH '<literal>'`
    /// and `(label_id, prop)` has a single-property index, return the
    /// `NodeIndexPrefixSeek` to emit in its place — the contiguous run of
    /// string keys sharing the prefix. `STARTS WITH` is false for every
    /// non-string value, and `find_prefix` only ever returns string keys, so
    /// the seek is exactly the predicate's match set and the caller drops the
    /// consumed conjunct.
    ///
    /// Not mirrored: `'literal' STARTS WITH n.prop` asks whether the LITERAL
    /// starts with the property, which no prefix run on `prop` can answer.
    /// A `$parameter` prefix has no plan-time value and keeps the scan.
    fn where_prefix_seek_operand(
        &self,
        conjunct: &Expression,
        variable: &str,
        label_id: u32,
    ) -> Option<Operator> {
        let prop_idx = self.property_index?;
        let Expression::BinaryOp {
            left,
            op: BinaryOperator::StartsWith,
            right,
        } = conjunct
        else {
            return None;
        };
        let Expression::PropertyAccess {
            variable: v,
            property,
        } = left.as_ref()
        else {
            return None;
        };
        if v != variable {
            return None;
        }
        let Expression::Literal(Literal::String(prefix)) = right.as_ref() else {
            return None;
        };
        let key_id = self.catalog.get_key_id(property).ok()?;
        if !prop_idx.has_index(label_id, key_id) {
            return None;
        }
        Some(Operator::NodeIndexPrefixSeek {
            label_id,
            key_id,
            prefix: prefix.clone(),
            variable: variable.to_string(),
        })
    }

    /// If `conjunct` is a top-level equality between `variable.prop` and a
    /// `$parameter` (either operand order) and `(label_id, prop)` has a
    /// single-property index, return the `NodeIndexParamSeek` to emit in place
    /// of the label scan. The seek key is resolved from the query envelope at
    /// execution time, so — unlike `NodeIndexSeek`'s correlated
    /// `key_expression` path — it needs no driving rows and works as the first
    /// scan of the query, which is where `WHERE n.prop = $x` lands.
    ///
    /// The caller must KEEP the conjunct as a residual `Filter`: a parameter
    /// bound to a list/map, or absent from the envelope, makes the operator
    /// fall back to a full label scan, and only the retained predicate can
    /// narrow that back down to the right rows.
    fn where_param_seek_operand(
        &self,
        conjunct: &Expression,
        variable: &str,
        label_id: u32,
    ) -> Option<Operator> {
        let prop_idx = self.property_index?;
        let Expression::BinaryOp {
            left,
            op: BinaryOperator::Equal,
            right,
        } = conjunct
        else {
            return None;
        };
        let (property, parameter) = match (left.as_ref(), right.as_ref()) {
            (
                Expression::PropertyAccess {
                    variable: v,
                    property,
                },
                Expression::Parameter(parameter),
            )
            | (
                Expression::Parameter(parameter),
                Expression::PropertyAccess {
                    variable: v,
                    property,
                },
            ) if v == variable => (property, parameter),
            _ => return None,
        };
        let key_id = self.catalog.get_key_id(property).ok()?;
        if !prop_idx.has_index(label_id, key_id) {
            return None;
        }
        Some(Operator::NodeIndexParamSeek {
            label_id,
            key_id,
            parameter: parameter.clone(),
            variable: variable.to_string(),
        })
    }

    /// If `conjunct` is a top-level equality `variable.prop = <constant>`
    /// (or the mirrored `<constant> = variable.prop`), and `(label_id,
    /// prop)` has a registered single-property index, return the
    /// `NodeIndexSeek` operator to emit in its place.
    ///
    /// "Constant" here means a plan-time LITERAL only — `$parameter`
    /// values are deliberately excluded. The planner has no access to
    /// bound parameter values (they are supplied at execution time), and
    /// `NodeIndexSeek`'s `key_expression`-driven correlated-seek path
    /// (`execute_correlated_index_seek`) requires driving rows to
    /// already exist in the pipeline — the common case a bare `WHERE
    /// n.prop = $x` lowers to is the FIRST scan of the query, where no
    /// driving rows exist yet, so routing a parameter through that path
    /// would silently return zero rows instead of seeking. Parameter
    /// equality is lifted by `where_param_seek_operand` instead, into a
    /// `NodeIndexParamSeek` that resolves the key from the query envelope.
    ///
    /// SCOPE: LITERAL EQUALITY ONLY. The other lifted predicate shapes live
    /// in sibling helpers — range in `where_range_seek_operand`, `IN` in
    /// `where_in_seek_operand`, `STARTS WITH` in `where_prefix_seek_operand`,
    /// `$parameter` equality in `where_param_seek_operand`. `CONTAINS` is
    /// lifted by none of them (no ordered index can seek an unanchored
    /// substring) and stays a full scan, made observable via the
    /// `Nexus.Performance.UnindexedPropertyAccess` notification
    /// (`unindexed.rs`).
    fn where_equality_seek_operand(
        &self,
        conjunct: &Expression,
        variable: &str,
        label_id: u32,
    ) -> Option<Operator> {
        let prop_idx = self.property_index?;
        let Expression::BinaryOp {
            left,
            op: BinaryOperator::Equal,
            right,
        } = conjunct
        else {
            return None;
        };
        // Resolve which side is the `var.prop` operand and which is the
        // candidate constant. Left is preferred when both sides are
        // property accesses, matching `unindexed.rs`'s resolution order.
        let (property, value_expr) = match (left.as_ref(), right.as_ref()) {
            (
                Expression::PropertyAccess {
                    variable: v,
                    property,
                },
                other,
            ) if v == variable => (property, other),
            (
                other,
                Expression::PropertyAccess {
                    variable: v,
                    property,
                },
            ) if v == variable => (property, other),
            _ => return None,
        };
        let key_id = self.catalog.get_key_id(property).ok()?;
        if !prop_idx.has_index(label_id, key_id) {
            return None;
        }
        // null / point / parameter / non-literal: not indexable at plan
        // time — see the doc comment above.
        let value = literal_property_value(value_expr)?;
        Some(Operator::NodeIndexSeek {
            label_id,
            key_id,
            value,
            key_expression: None,
            variable: variable.to_string(),
        })
    }
}
