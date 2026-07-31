//! CREATE property-expression resolution shared by the standalone and
//! row-aware CREATE paths, plus the NOT NULL/uniqueness constraint check
//! run before insert.
//!
//! - `resolve_property_expr_for_create` — row-aware path (an upstream
//!   MATCH/UNWIND row exists): literal fast path, else evaluate +
//!   canonicalize against that row.
//! - `resolve_standalone_create_property` — no row exists yet (bare
//!   `CREATE (...)`, no preceding MATCH/UNWIND): same fast path, else
//!   evaluate + canonicalize against an empty row — except for a bare
//!   `Variable`/`PropertyAccess`, which is structurally unresolvable with
//!   no row at all and stays a hard rejection.
//! - `expression_to_json_value` — static literal/parameter-only
//!   conversion; the fast path both resolvers above try first.
//! - `expression_to_string` — string rendering used by filter-predicate
//!   construction.
//! - `check_constraints` — EXISTS/UNIQUE constraint enforcement.

use super::super::super::engine::Executor;
use super::super::super::parser;
use crate::{Error, Result};
use serde_json::Value;

impl Executor {
    /// Resolve a `CREATE` property expression against the current
    /// row (`row`) using the row-aware projection evaluator when the
    /// expression references a variable, and falling back to the
    /// static literal-only [`Self::expression_to_json_value`] for
    /// pure-literal expressions.
    ///
    /// Returns the resolved value, or the underlying evaluation error
    /// if neither path can produce one. Callers that want the legacy
    /// "skip failing keys silently" behaviour can `.ok()` the result.
    pub(in crate::executor) fn resolve_property_expr_for_create(
        &self,
        expr: &parser::Expression,
        row: &std::collections::HashMap<String, Value>,
        params: &std::collections::HashMap<String, Value>,
    ) -> Result<Value> {
        // Static-literal (and `$param`) fast path. Keeps the hot
        // CREATE-with-literal case as cheap as before this lift. No
        // canonicalization is needed on this branch: it's not that a
        // literal/parameter value can never syntactically carry the
        // `_nexus_temporal_type` tag key (a client-supplied `$param` could
        // hand-craft one, the same marker-collision hazard as
        // `_nexus_rel_type`) — it's that no *executor-produced* value
        // reaches this branch. Canonicalization here exists to collapse
        // the executor's own tagged intermediate representation, not to
        // sanitize arbitrary client input.
        if let Ok(v) = self.expression_to_json_value(expr, params) {
            return Ok(v);
        }
        // Row-aware path — resolves Variable / PropertyAccess / etc.
        // against the row scope. Build a temporary inner ctx carrying the
        // request's parameters so `$param` references inside complex
        // expressions resolve too (the ctx used to be built EMPTY, which
        // made every parameterized property unresolvable here — G1).
        let inner_ctx = super::super::super::context::ExecutionContext::new(params.clone(), None);
        let mut value = self.evaluate_projection_expression(row, &inner_ctx, expr)?;
        // A property value built from a `FunctionCall` — e.g.
        // `CREATE (n {d: duration({days: 1})})` — reaches here via the
        // shared projection evaluator, which returns the tagged
        // intermediate temporal shape (`{"_nexus_temporal_type": ...}`),
        // not the canonical ISO string. Storage is a durability boundary
        // the executor's own projection-boundary canonicalization
        // (`Executor::execute`) never sees: a tagged value written here
        // would persist as a raw JSON object on disk, corrupt any index
        // built over it, and read back un-rendered on every later MATCH.
        // Canonicalize before the value ever reaches `create_node*`.
        super::super::super::eval::temporal_value::canonicalize_value_in_place(&mut value);
        Ok(value)
    }

    /// Resolve a **standalone** `CREATE` property expression — no upstream
    /// `MATCH`/`UNWIND` row exists yet, so unlike
    /// [`Self::resolve_property_expr_for_create`] there is no `row` to
    /// evaluate a variable reference against.
    ///
    /// Tries the static literal/parameter fast path first
    /// ([`Self::expression_to_json_value`]); when that fails for an
    /// expression shape that fundamentally *cannot* resolve without a row
    /// — a bare `Variable` or `PropertyAccess`, both of which reference a
    /// name no earlier clause in a standalone CREATE could ever have bound
    /// — the original rejection from the fast path is returned unchanged
    /// rather than silently evaluating to `Null` against an empty row
    /// (the row-aware evaluator's "unbound in this row" contract, correct
    /// for MATCH/OPTIONAL MATCH projections but wrong here: a name that
    /// cannot be bound *anywhere* in a standalone CREATE is a genuine
    /// error, not an absent value — see TCK `Create1[20]`).
    ///
    /// Every other expression shape (function calls such as `date(...)` /
    /// `duration(...)` / `toUpper(...)`, arithmetic, list/map literals,
    /// CASE, …) is evaluated through the same row-aware evaluator +
    /// temporal canonicalization `resolve_property_expr_for_create` uses,
    /// just with an empty row — reusing the real evaluator instead of
    /// hand-rolling a parallel one inside `expression_to_json_value`.
    pub(in crate::executor) fn resolve_standalone_create_property(
        &self,
        expr: &parser::Expression,
        params: &std::collections::HashMap<String, Value>,
    ) -> Result<Value> {
        let literal_err = match self.expression_to_json_value(expr, params) {
            Ok(v) => return Ok(v),
            Err(e) => e,
        };
        if matches!(
            expr,
            parser::Expression::Variable(_) | parser::Expression::PropertyAccess { .. }
        ) {
            return Err(literal_err);
        }
        let empty_row = std::collections::HashMap::new();
        let inner_ctx = super::super::super::context::ExecutionContext::new(params.clone(), None);
        let mut value = self.evaluate_projection_expression(&empty_row, &inner_ctx, expr)?;
        super::super::super::eval::temporal_value::canonicalize_value_in_place(&mut value);
        Ok(value)
    }

    /// Convert expression to JSON value. `params` resolves
    /// `Expression::Parameter` (`$name`) against the request's bound
    /// parameters — without it, `CREATE (n {x: $v})` routed through the
    /// executor errored with "Complex expressions not supported in CREATE
    /// properties" even though every other write path resolves parameters
    /// (write-path unification, G1).
    pub(in crate::executor) fn expression_to_json_value(
        &self,
        expr: &parser::Expression,
        params: &std::collections::HashMap<String, Value>,
    ) -> Result<Value> {
        match expr {
            parser::Expression::Literal(lit) => match lit {
                parser::Literal::String(s) => Ok(Value::String(s.clone())),
                parser::Literal::Integer(i) => Ok(Value::Number((*i).into())),
                parser::Literal::Float(f) => {
                    if let Some(num) = serde_json::Number::from_f64(*f) {
                        Ok(Value::Number(num))
                    } else {
                        Err(Error::CypherExecution(format!("Invalid float: {}", f)))
                    }
                }
                parser::Literal::Boolean(b) => Ok(Value::Bool(*b)),
                parser::Literal::Null => Ok(Value::Null),
                parser::Literal::Point(p) => Ok(p.to_json_value()),
            },
            parser::Expression::Parameter(name) => params.get(name).cloned().ok_or_else(|| {
                Error::CypherExecution(format!("Parameter `${name}` was not provided"))
            }),
            parser::Expression::Variable(_) => Err(Error::CypherExecution(
                "Variables not supported in CREATE properties".to_string(),
            )),
            // phase7 §4.7 — a leading `-`/`+` on a numeric literal parses as a
            // UnaryOp; constant-fold it so `CREATE (:T {v: -7})` works like
            // Neo4j instead of hitting the "complex expression" reject below.
            parser::Expression::UnaryOp { .. } => match expr.fold_signed_numeric_literal() {
                Some(lit) => {
                    self.expression_to_json_value(&parser::Expression::Literal(lit), params)
                }
                None => Err(Error::CypherExecution(
                    "Complex expressions not supported in CREATE properties".to_string(),
                )),
            },
            _ => Err(Error::CypherExecution(
                "Complex expressions not supported in CREATE properties".to_string(),
            )),
        }
    }

    /// Check constraints before creating a node
    pub(in crate::executor) fn check_constraints(
        &self,
        label_ids: &[u32],
        properties: &serde_json::Value,
    ) -> Result<()> {
        let constraint_manager = self.catalog().constraint_manager().read();

        // Check constraints for each label
        for &label_id in label_ids {
            let constraints = constraint_manager.get_constraints_for_label(label_id)?;

            for constraint in constraints {
                // Get property name
                let property_name = self
                    .catalog()
                    .get_key_name(constraint.property_key_id)?
                    .ok_or_else(|| Error::Internal("Property key not found".to_string()))?;

                let property_value = properties.as_object().and_then(|m| m.get(&property_name));

                match constraint.constraint_type {
                    crate::catalog::constraints::ConstraintType::Exists => {
                        // Property must exist (not null)
                        if property_value.is_none()
                            || property_value == Some(&serde_json::Value::Null)
                        {
                            let label_name = self
                                .catalog()
                                .get_label_name(label_id)?
                                .unwrap_or_else(|| format!("ID{}", label_id));
                            return Err(Error::ConstraintViolation(format!(
                                "EXISTS constraint violated: property '{}' must exist on nodes with label '{}'",
                                property_name, label_name
                            )));
                        }
                    }
                    crate::catalog::constraints::ConstraintType::Unique => {
                        // Property value must be unique across all nodes with this label
                        if let Some(value) = property_value {
                            let label_name = self
                                .catalog()
                                .get_label_name(label_id)?
                                .unwrap_or_else(|| format!("ID{}", label_id));

                            // Get all nodes with this label
                            let bitmap = self.label_index().get_nodes_with_labels(&[label_id])?;

                            for node_id in bitmap.iter() {
                                let node_id_u64 = node_id as u64;

                                let node_props = self.store().load_node_properties(node_id_u64)?;
                                if let Some(serde_json::Value::Object(props_map)) = node_props {
                                    if let Some(existing_value) = props_map.get(&property_name) {
                                        if existing_value == value {
                                            return Err(Error::ConstraintViolation(format!(
                                                "UNIQUE constraint violated: property '{}' value already exists on another node with label '{}'",
                                                property_name, label_name
                                            )));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Convert expression to string representation
    pub(in crate::executor) fn expression_to_string(
        &self,
        expr: &parser::Expression,
    ) -> Result<String> {
        match expr {
            parser::Expression::Variable(name) => Ok(name.clone()),
            parser::Expression::PropertyAccess { variable, property } => {
                Ok(format!("{}.{}", variable, property))
            }
            parser::Expression::Literal(literal) => match literal {
                // Use single quotes for strings in filter predicates to match Cypher parser expectations
                parser::Literal::String(s) => Ok(format!("'{}'", s)),
                parser::Literal::Integer(i) => Ok(i.to_string()),
                parser::Literal::Float(f) => Ok(f.to_string()),
                parser::Literal::Boolean(b) => Ok(b.to_string()),
                parser::Literal::Null => Ok("NULL".to_string()),
                parser::Literal::Point(p) => Ok(p.to_string()),
            },
            parser::Expression::BinaryOp { left, op, right } => {
                let left_str = self.expression_to_string(left)?;
                let right_str = self.expression_to_string(right)?;
                let op_str = match op {
                    parser::BinaryOperator::Equal => "=",
                    parser::BinaryOperator::NotEqual => "!=",
                    parser::BinaryOperator::LessThan => "<",
                    parser::BinaryOperator::LessThanOrEqual => "<=",
                    parser::BinaryOperator::GreaterThan => ">",
                    parser::BinaryOperator::GreaterThanOrEqual => ">=",
                    parser::BinaryOperator::And => "AND",
                    parser::BinaryOperator::Or => "OR",
                    parser::BinaryOperator::Add => "+",
                    parser::BinaryOperator::Subtract => "-",
                    parser::BinaryOperator::Multiply => "*",
                    parser::BinaryOperator::Divide => "/",
                    parser::BinaryOperator::In => "IN",
                    _ => "?",
                };
                Ok(format!("{} {} {}", left_str, op_str, right_str))
            }
            parser::Expression::Parameter(name) => Ok(format!("${}", name)),
            _ => Ok("?".to_string()),
        }
    }
}
