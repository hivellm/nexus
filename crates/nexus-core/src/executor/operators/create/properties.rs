//! CREATE property-expression resolution shared by the standalone and
//! row-aware CREATE paths, plus the NOT NULL/uniqueness constraint check
//! run before insert.
//!
//! - `resolve_property_expr_for_create` — row-aware path (an upstream
//!   MATCH/UNWIND row exists): literal fast path, else evaluate +
//!   canonicalize against that row.
//! - `resolve_standalone_create_property` — no row exists yet (bare
//!   `CREATE (...)`, no preceding MATCH/UNWIND): same fast path, else
//!   evaluate + canonicalize against an empty row.
//! - `expression_to_json_value` — static literal/parameter-only
//!   conversion; the fast path both resolvers above try first.
//! - `expression_to_string` — string rendering used by filter-predicate
//!   construction.
//! - `check_constraints` — EXISTS/UNIQUE constraint enforcement.
//!
//! # Undefined-variable references
//!
//! `semantic_validation::validate` is the *primary* rejection path for a
//! CREATE property value referencing a name bound nowhere in the query
//! (`CREATE (b {name: missing})` — TCK `Create1[20]`/`Create2[24]`): it
//! runs before either resolver below and classifies the failure as the
//! correct `SyntaxError`/`UndefinedVariable`. Both resolvers here ALSO run
//! the same comprehension-aware reference walk
//! (`semantic_validation::collect_expr_binders` +
//! `check_expr_references`) as defense in depth, so an entry point that
//! bypasses validation (see `phase21_tck-semantic-validation-entry-point-
//! coverage`) still cannot silently evaluate an unbound name to `Null`
//! instead of erroring.
//!
//! # Unknown function names
//!
//! Both resolvers also run `check_function_names` — a tree-wide walk (not
//! just a top-level `FunctionCall`) that rejects any function name the
//! dispatch surface (registered UDFs + the six builtin groups) does not
//! recognise, e.g. `{d: 1 + bogusFn(1)}`, before evaluating anything.

use crate::executor::context::ExecutionContext;
use crate::executor::engine::Executor;
use crate::executor::eval::temporal_value;
use crate::executor::parser;
use crate::executor::semantic_validation;
use crate::{Error, Result};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

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
        row: &HashMap<String, Value>,
        params: &HashMap<String, Value>,
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

        // Reference-walk defense in depth (module doc). `row`'s keys are
        // legitimately in scope here (bound by an upstream MATCH/UNWIND);
        // `collect_expr_binders` adds anything the expression itself binds
        // inline (a comprehension's own loop variable, an any/all/none/
        // single predicate name, …) on top, so this does not flag those —
        // only a name that is neither one nor the other, e.g.
        // `MATCH (a) CREATE (a)-[:KNOWS]->(b {name: missing})`, where
        // `missing` is in neither set.
        let mut local_binders: HashSet<String> = row.keys().cloned().collect();
        semantic_validation::collect_expr_binders(expr, &mut local_binders);
        semantic_validation::check_expr_references(expr, &local_binders)?;

        // Unknown-function-name pre-pass (module doc / MAJOR1): walks the
        // WHOLE expression tree, not just a top-level `FunctionCall`, so
        // `{d: 1 + bogusFn(1)}` and `{d: [bogusFn(1)]}` error instead of
        // silently persisting `null` for the nested call.
        self.check_function_names(expr)?;

        // Row-aware path — resolves Variable / PropertyAccess / etc.
        // against the row scope. Build a temporary inner ctx carrying the
        // request's parameters so `$param` references inside complex
        // expressions resolve too (the ctx used to be built EMPTY, which
        // made every parameterized property unresolvable here — G1).
        let inner_ctx = ExecutionContext::new(params.clone(), None);
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
        temporal_value::canonicalize_value_in_place(&mut value);
        reject_non_primitive_property_value(value)
    }

    /// Resolve a **standalone** `CREATE` property expression — no upstream
    /// `MATCH`/`UNWIND` row exists yet, so unlike
    /// [`Self::resolve_property_expr_for_create`] there is no `row` to
    /// evaluate a variable reference against.
    ///
    /// Tries the static literal/parameter fast path first
    /// ([`Self::expression_to_json_value`]); every other expression shape
    /// (function calls such as `date(...)` / `duration(...)` /
    /// `toUpper(...)`, arithmetic, list/map literals, CASE, …) is
    /// evaluated through the same row-aware evaluator + temporal
    /// canonicalization `resolve_property_expr_for_create` uses, just
    /// with an empty row — reusing the real evaluator instead of
    /// hand-rolling a parallel one inside `expression_to_json_value`. A
    /// reference to a name that cannot be bound *anywhere* in a
    /// standalone CREATE (module doc — TCK `Create1[20]`) is caught by
    /// the same defense-in-depth reference walk `resolve_property_expr_
    /// for_create` runs, seeded with an empty outer scope.
    pub(in crate::executor) fn resolve_standalone_create_property(
        &self,
        expr: &parser::Expression,
        params: &HashMap<String, Value>,
    ) -> Result<Value> {
        if let Ok(v) = self.expression_to_json_value(expr, params) {
            return Ok(v);
        }

        // No row at all — only names an inline construct binds within
        // `expr` itself (comprehension loop variable, predicate name, …)
        // are in scope; everything else is unconditionally unresolvable.
        let mut local_binders: HashSet<String> = HashSet::new();
        semantic_validation::collect_expr_binders(expr, &mut local_binders);
        semantic_validation::check_expr_references(expr, &local_binders)?;

        // Unknown-function-name pre-pass — see `resolve_property_expr_
        // for_create`'s identical call for the full rationale.
        self.check_function_names(expr)?;

        let empty_row: HashMap<String, Value> = HashMap::new();
        let inner_ctx = ExecutionContext::new(params.clone(), None);
        let mut value = self.evaluate_projection_expression(&empty_row, &inner_ctx, expr)?;
        temporal_value::canonicalize_value_in_place(&mut value);
        reject_non_primitive_property_value(value)
    }

    /// Recursively validate every `FunctionCall` name reachable from
    /// `expr` — a registered UDF or a name one of the six builtin groups
    /// recognises (`Executor::is_known_function_name`) — before either
    /// resolver above evaluates anything. Checking only a top-level
    /// `FunctionCall` (the round-1 shape) let a nested one slip through:
    /// `{d: 1 + bogusFn(1)}` and `{d: [bogusFn(1)]}` both silently
    /// persisted `null` for the unrecognised call instead of erroring.
    ///
    /// Skips an aggregate name (`count`, `sum`, …): already rejected, with
    /// the more precise `InvalidAggregation`, by `semantic_validation::
    /// check_aggregation_placement`'s `Clause::Create` arm. Does not
    /// descend into `EXISTS { … }` / `COLLECT { … }` subquery bodies —
    /// those are full inner queries validated on their own, not CREATE
    /// property-map surface — by reusing `semantic_validation::
    /// child_exprs`'s identical "stop at EXISTS/COLLECT" traversal
    /// instead of re-deriving a second copy of it.
    ///
    /// Only the function *name* is checked here, never arity or argument
    /// validity — the accepted boundary from the round-1 review (a
    /// recognised name with a degenerate/invalid argument, e.g.
    /// `toUpper()` or `date('not-a-date')`, legitimately evaluates to
    /// `Null` per the same convention every builtin-function group already
    /// uses in read positions).
    fn check_function_names(&self, expr: &parser::Expression) -> Result<()> {
        if let parser::Expression::FunctionCall { name, args } = expr {
            if !semantic_validation::is_aggregate_name(name) && !self.is_known_function_name(name) {
                return Err(Error::CypherSyntax(format!(
                    "UnknownFunction: function `{name}` does not exist"
                )));
            }
            for a in args {
                self.check_function_names(a)?;
            }
            return Ok(());
        }
        if matches!(
            expr,
            parser::Expression::Exists { .. } | parser::Expression::CollectSubquery { .. }
        ) {
            return Ok(());
        }
        for child in semantic_validation::child_exprs(expr) {
            self.check_function_names(child)?;
        }
        Ok(())
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
        params: &HashMap<String, Value>,
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

/// Coordinate Reference System values `geospatial::Point::to_json_value`
/// actually emits — the full discriminator (with numeric `x`/`y`/`z`)
/// [`is_point_shaped`] checks, not just `contains_key("crs")`, which lets
/// an unrelated map that happens to carry its own `crs` key
/// (`{crs: 'x', secret: 'leak'}`) slip through and persist verbatim.
const KNOWN_CRS_VALUES: &[&str] = &["cartesian", "cartesian-3d", "wgs-84", "wgs-84-3d"];

/// True when `map` has the real shape `geospatial::Point::to_json_value`
/// produces: numeric `x`/`y` (an optional numeric `z` for the 3D CRS
/// variants), and `crs` one of the values Nexus actually emits.
fn is_point_shaped(map: &serde_json::Map<String, Value>) -> bool {
    let is_number = |key: &str| matches!(map.get(key), Some(Value::Number(_)));
    let crs_known = matches!(
        map.get("crs"),
        Some(Value::String(crs)) if KNOWN_CRS_VALUES.contains(&crs.as_str())
    );
    is_number("x")
        && is_number("y")
        && crs_known
        && map.get("z").map_or(true, |z| matches!(z, Value::Number(_)))
}

/// True when `value` is a legal openCypher property value on its own: a
/// primitive scalar (string, number, boolean, null) or the Point object
/// shape [`is_point_shaped`] recognises.
fn is_valid_property_scalar(value: &Value) -> bool {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => true,
        Value::Object(map) => is_point_shaped(map),
        Value::Array(_) => false,
    }
}

/// Reject a resolved property value that is not a primitive scalar or a
/// flat array of primitive scalars. Property storage has no map/object
/// type and no nested arrays, so a whole node/relationship (`{peer: b}`
/// where `b` is a bound node — MAJOR3 in the function-call-properties
/// review), a literal map (`{p: {a: 1}}`), or an array containing either
/// would otherwise be persisted as-is: a marker-collision hazard for the
/// former (a raw `_nexus_id`/`_nexus_rel_type`-bearing object landing in
/// property storage) and simply wrong for the latter two. `Null` and
/// arrays are checked element-wise via [`is_valid_property_scalar`].
fn reject_non_primitive_property_value(value: Value) -> Result<Value> {
    let ok = match &value {
        Value::Array(items) => items.iter().all(is_valid_property_scalar),
        other => is_valid_property_scalar(other),
    };
    if ok {
        return Ok(value);
    }
    let actual = if crate::executor::is_node_value(&value) {
        "a node".to_string()
    } else if crate::executor::is_relationship_value(&value) {
        "a relationship".to_string()
    } else {
        match &value {
            Value::Object(_) => "a map".to_string(),
            Value::Array(_) => "an array containing a non-primitive element".to_string(),
            _ => "a non-primitive value".to_string(),
        }
    };
    Err(Error::TypeMismatch {
        expected: "a primitive property value (or array of primitives)".to_string(),
        actual,
    })
}
