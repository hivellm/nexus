//! Expression/pattern serialisation and aggregation detection helpers.

use super::*;
use crate::executor::parser::WhenClause;

impl<'a> QueryPlanner<'a> {
    /// Convert expression to string representation
    // Visibility elevated to `planner` level: called from `planner/tests.rs`,
    // which sat next to this method before the split.
    pub(in crate::executor::planner) fn expression_to_string(
        &self,
        expr: &Expression,
    ) -> Result<String> {
        self.expr_to_string_impl(expr, false)
    }

    /// Render an aggregate function call as its verbatim-style column
    /// name for openCypher/Neo4j fidelity, e.g. `count(*)`, `count(n)`,
    /// `count(DISTINCT n.age)`, `sum(n.age)`, `percentileCont(n.x, 0.5)`.
    ///
    /// Unaliased aggregate columns previously rendered as the bare
    /// function name (`count`), which mismatched both the openCypher TCK
    /// (verbatim source) and Neo4j (which names the column after the whole
    /// call). The AST encodes `count(*)` as an empty argument list and
    /// `DISTINCT` as a leading synthetic `__DISTINCT__` variable argument,
    /// both of which are decoded here so they render as written. The
    /// original `name` casing is preserved (the dispatcher lowercases a
    /// separate copy for matching, never the AST node).
    pub(in crate::executor::planner) fn aggregate_display_name(
        &self,
        name: &str,
        args: &[Expression],
    ) -> String {
        let (distinct, real_args) = match args.first() {
            Some(Expression::Variable(v)) if v == "__DISTINCT__" => (true, &args[1..]),
            _ => (false, args),
        };
        let inner = if real_args.is_empty() {
            "*".to_string()
        } else {
            real_args
                .iter()
                .map(|a| {
                    self.expression_to_string(a)
                        .unwrap_or_else(|_| "?".to_string())
                })
                .collect::<Vec<_>>()
                .join(", ")
        };
        if distinct {
            format!("{}(DISTINCT {})", name, inner)
        } else {
            format!("{}({})", name, inner)
        }
    }

    /// Convert a WHERE predicate expression to its Cypher string
    /// representation, faithfully preserving grouping.
    ///
    /// WHERE clauses are lowered to a plain string (stored on
    /// `Operator::Filter` / `Operator::OptionalFilter`) and re-parsed later
    /// by the `Filter` operator. Re-parsing applies default operator
    /// precedence, so any grouping the author wrote with parentheses must
    /// survive the round trip or the re-parsed tree silently diverges from
    /// what was written (e.g. `(a OR b) AND c` must not round-trip to the
    /// unparenthesised `a OR b AND c`, which re-parses as `a OR (b AND c)`).
    /// This wraps every compound (`BinaryOp`/`UnaryOp`) operand in
    /// parentheses so the string always re-parses to the same tree,
    /// regardless of the concrete operators involved.
    pub(in crate::executor::planner) fn predicate_to_string(
        &self,
        expr: &Expression,
    ) -> Result<String> {
        self.expr_to_string_impl(expr, true)
    }

    /// Shared expression-to-string implementation. When `parenthesize` is
    /// `true`, every compound (`BinaryOp`/`UnaryOp`) child operand is
    /// wrapped in `(...)` so the resulting string re-parses to the exact
    /// same tree — required for WHERE-predicate lowering (see
    /// [`Self::predicate_to_string`]). The flag propagates through every
    /// nested position (function-call args, list elements, map values,
    /// array-index base/index, IS NULL operand, EXISTS inner WHERE, …)
    /// since the precedence hazard can nest inside any of them.
    fn expr_to_string_impl(&self, expr: &Expression, parenthesize: bool) -> Result<String> {
        // Renders `child`'s string form, wrapping it in parentheses when
        // `parenthesize` is set and `child` is itself a compound
        // (`BinaryOp`/`UnaryOp`) expression whose default precedence could
        // otherwise be reinterpreted differently once re-parsed.
        let render_operand = |child: &Expression| -> Result<String> {
            let child_str = self.expr_to_string_impl(child, parenthesize)?;
            if parenthesize
                && matches!(
                    child,
                    Expression::BinaryOp { .. } | Expression::UnaryOp { .. }
                )
            {
                Ok(format!("({})", child_str))
            } else {
                Ok(child_str)
            }
        };
        match expr {
            Expression::Variable(name) => Ok(name.clone()),
            Expression::PropertyAccess { variable, property } => {
                Ok(format!("{}.{}", variable, property))
            }
            Expression::ArrayIndex { base, index } => {
                let base_str = self.expr_to_string_impl(base, parenthesize)?;
                let index_str = self.expr_to_string_impl(index, parenthesize)?;
                Ok(format!("{}[{}]", base_str, index_str))
            }
            Expression::Literal(literal) => match literal {
                // Use single quotes for strings to match Cypher parser expectations
                // This is critical for filter predicates to work correctly
                Literal::String(s) => Ok(format!("'{}'", s)),
                Literal::Integer(i) => Ok(i.to_string()),
                Literal::Float(f) => {
                    // Rust renders an integral float without a fractional part
                    // (`1.0` → "1"), which loses column-name fidelity and, in a
                    // re-parsed WHERE predicate, would silently turn a float
                    // literal into an integer. Keep the `.0` so it stays a
                    // float and the column header matches the source.
                    let s = f.to_string();
                    if f.is_finite() && !s.contains(['.', 'e', 'E']) {
                        Ok(format!("{}.0", s))
                    } else {
                        Ok(s)
                    }
                }
                Literal::Boolean(b) => Ok(b.to_string()),
                // Lower-case, matching how `Literal::Boolean` above renders and
                // how openCypher writes the literal. The rendering doubles as an
                // unaliased column name, and the TCK compares that header
                // against the source text (`RETURN nodes(null)` →
                // `nodes(null)`), so an upper-cased `NULL` mismatched. Nothing
                // depends on the old casing: the string is only ever re-parsed
                // as a WHERE predicate, and the keyword match is
                // case-insensitive. (The AST carries no source span, so — just
                // as for booleans — this is canonical lower-case rather than a
                // true verbatim echo of the author's casing.)
                Literal::Null => Ok("null".to_string()),
                Literal::Point(p) => Ok(p.to_string()),
            },
            Expression::BinaryOp { left, op, right } => {
                let left_str = render_operand(left)?;
                let right_str = render_operand(right)?;
                let op_str = match op {
                    BinaryOperator::Equal => "=",
                    BinaryOperator::NotEqual => "!=",
                    BinaryOperator::LessThan => "<",
                    BinaryOperator::LessThanOrEqual => "<=",
                    BinaryOperator::GreaterThan => ">",
                    BinaryOperator::GreaterThanOrEqual => ">=",
                    BinaryOperator::And => "AND",
                    BinaryOperator::Or => "OR",
                    BinaryOperator::Xor => "XOR",
                    BinaryOperator::Add => "+",
                    BinaryOperator::Subtract => "-",
                    BinaryOperator::Multiply => "*",
                    BinaryOperator::Divide => "/",
                    BinaryOperator::In => "IN",
                    BinaryOperator::Contains => "CONTAINS",
                    BinaryOperator::StartsWith => "STARTS WITH",
                    BinaryOperator::EndsWith => "ENDS WITH",
                    BinaryOperator::RegexMatch => "=~",
                    BinaryOperator::Power => "^",
                    BinaryOperator::Modulo => "%",
                    _ => "?",
                };
                Ok(format!("{} {} {}", left_str, op_str, right_str))
            }
            Expression::Parameter(name) => Ok(format!("${}", name)),
            Expression::IsNull { expr, negated } => {
                let expr_str = self.expr_to_string_impl(expr, parenthesize)?;
                if *negated {
                    Ok(format!("{} IS NOT NULL", expr_str))
                } else {
                    Ok(format!("{} IS NULL", expr_str))
                }
            }
            Expression::List(elements) => {
                let elem_strs: Result<Vec<String>> = elements
                    .iter()
                    .map(|e| self.expr_to_string_impl(e, parenthesize))
                    .collect();
                Ok(format!("[{}]", elem_strs?.join(", ")))
            }
            Expression::Map(map) => {
                let mut pairs = Vec::new();
                for (key, value) in map {
                    let value_str = self.expr_to_string_impl(value, parenthesize)?;
                    pairs.push(format!("{}: {}", key, value_str));
                }
                Ok(format!("{{{}}}", pairs.join(", ")))
            }
            Expression::FunctionCall { name, args } => {
                // phase6_opencypher-quickwins §8 — render the synthetic
                // `__label_predicate__(var, 'Label')` back as the
                // text-mode `variable:Label` shape that the Filter
                // operator's fast path already understands (so static
                // and dynamic label predicates share that code path
                // instead of duplicating the has-label check).
                if name == "__label_predicate__" && args.len() == 2 {
                    if let (
                        Expression::Variable(var),
                        Expression::Literal(Literal::String(label)),
                    ) = (&args[0], &args[1])
                    {
                        return Ok(format!("{}:{}", var, label));
                    }
                }
                let arg_strs: Result<Vec<String>> = args
                    .iter()
                    .map(|a| self.expr_to_string_impl(a, parenthesize))
                    .collect();
                Ok(format!("{}({})", name, arg_strs?.join(", ")))
            }
            Expression::UnaryOp { op, operand } => {
                let operand_str = render_operand(operand)?;
                let op_str = match op {
                    UnaryOperator::Not => "NOT",
                    UnaryOperator::Minus => "-",
                    UnaryOperator::Plus => "+",
                };
                Ok(format!("{} {}", op_str, operand_str))
            }
            Expression::Exists { inner } => match inner {
                ExistsInner::Pattern {
                    pattern,
                    where_clause,
                } => {
                    let pattern_str = self.pattern_to_string(pattern)?;
                    if let Some(where_expr) = where_clause {
                        let where_str = self.expr_to_string_impl(where_expr, parenthesize)?;
                        Ok(format!("EXISTS {{ {} WHERE {} }}", pattern_str, where_str))
                    } else {
                        Ok(format!("EXISTS {{ {} }}", pattern_str))
                    }
                }
                // Same synthetic-shape rationale as `CollectSubquery`
                // below: this formatter serves diagnostics and the
                // no-AS column-name fallback, not a source
                // reconstruction, so the full clause list is summarised
                // rather than rendered verbatim.
                ExistsInner::Subquery { inner } => {
                    Ok(format!("EXISTS {{ {} clauses }}", inner.clauses.len()))
                }
            },
            Expression::CollectSubquery { inner } => {
                // The expression-to-string formatter is used for
                // diagnostic logging (and the projection-alias fallback
                // when no AS is given), so we render the synthetic
                // shape `COLLECT { … N clauses }` rather than try to
                // reconstruct the inner Cypher source.
                Ok(format!("COLLECT {{ {} clauses }}", inner.clauses.len()))
            }
            _ => Ok("?".to_string()),
        }
    }

    /// Convert a Pattern to its Cypher string representation
    pub(super) fn pattern_to_string(&self, pattern: &Pattern) -> Result<String> {
        let mut result = String::new();
        for element in pattern.elements.iter() {
            match element {
                PatternElement::Node(node) => {
                    result.push('(');
                    if let Some(ref var) = node.variable {
                        result.push_str(var);
                    }
                    for label in &node.labels {
                        result.push(':');
                        result.push_str(label);
                    }
                    if let Some(ref props) = node.properties {
                        if !props.properties.is_empty() {
                            result.push_str(" {");
                            let prop_strs: Vec<String> = props
                                .properties
                                .iter()
                                .map(|(k, v)| {
                                    format!(
                                        "{}: {}",
                                        k,
                                        self.expression_to_string(v)
                                            .unwrap_or_else(|_| "?".to_string())
                                    )
                                })
                                .collect();
                            result.push_str(&prop_strs.join(", "));
                            result.push('}');
                        }
                    }
                    result.push(')');
                }
                PatternElement::Relationship(rel) => {
                    // Build relationship pattern
                    match rel.direction {
                        RelationshipDirection::Outgoing => {
                            result.push_str("-[");
                        }
                        RelationshipDirection::Incoming => {
                            result.push_str("<-[");
                        }
                        RelationshipDirection::Both => {
                            result.push_str("-[");
                        }
                    }
                    if let Some(ref var) = rel.variable {
                        result.push_str(var);
                    }
                    for (j, rel_type) in rel.types.iter().enumerate() {
                        if j == 0 {
                            result.push(':');
                        } else {
                            result.push('|');
                        }
                        result.push_str(rel_type);
                    }
                    // Handle variable length patterns
                    if let Some(ref quant) = rel.quantifier {
                        match quant {
                            RelationshipQuantifier::Exact(n) => {
                                result.push_str(&format!("*{}", n));
                            }
                            RelationshipQuantifier::Range(min, max) => {
                                result.push_str(&format!("*{}..{}", min, max));
                            }
                            RelationshipQuantifier::ZeroOrMore => {
                                result.push_str("*");
                            }
                            RelationshipQuantifier::OneOrMore => {
                                result.push_str("*1..");
                            }
                            RelationshipQuantifier::ZeroOrOne => {
                                result.push_str("*0..1");
                            }
                        }
                    }
                    result.push(']');
                    match rel.direction {
                        RelationshipDirection::Outgoing => {
                            result.push_str("->");
                        }
                        RelationshipDirection::Incoming => {
                            result.push('-');
                        }
                        RelationshipDirection::Both => {
                            result.push('-');
                        }
                    }
                }
                PatternElement::QuantifiedGroup(group) => {
                    let inner = Pattern {
                        elements: group.inner.clone(),
                        path_variable: None,
                        extra_path_variables: Vec::new(),
                    };
                    let inner_str = self.pattern_to_string(&inner)?;
                    let quant = match &group.quantifier {
                        RelationshipQuantifier::Exact(n) => format!("{{{}}}", n),
                        RelationshipQuantifier::Range(min, max) => {
                            format!("{{{},{}}}", min, max)
                        }
                        RelationshipQuantifier::ZeroOrMore => "*".to_string(),
                        RelationshipQuantifier::OneOrMore => "+".to_string(),
                        RelationshipQuantifier::ZeroOrOne => "?".to_string(),
                    };
                    result.push('(');
                    result.push_str(&inner_str);
                    result.push(')');
                    result.push_str(&quant);
                }
            }
        }
        Ok(result)
    }

    /// True when `name` (assumed already lower-cased) names one of the
    /// aggregate functions the planner recognises. This is the single
    /// source of truth for that list — factored out so
    /// [`Self::contains_aggregation`] and [`Self::lift_aggregations`] can
    /// never drift apart on which function names collapse row
    /// cardinality.
    pub(super) fn is_aggregate_function_name(name: &str) -> bool {
        // phase6 §9 — statistical aggregations must trigger the same
        // row-collapse path as count/sum/avg. Before adding these,
        // `MATCH (n:A) RETURN stdev(n.score)` returned 20 rows (one per
        // matched :A node) instead of one aggregated row because the
        // planner didn't treat stdev/variance/percentile* as
        // aggregations and so never introduced the Aggregate operator.
        matches!(
            name,
            "count"
                | "sum"
                | "avg"
                | "min"
                | "max"
                | "collect"
                | "stdev"
                | "stdevp"
                | "variance"
                | "variancep"
                | "percentilecont"
                | "percentiledisc"
        )
    }

    /// Check if an expression contains an aggregation function (recursively)
    pub(super) fn contains_aggregation(&self, expr: &Expression) -> bool {
        match expr {
            Expression::FunctionCall { name, args } => {
                let func_name = name.to_lowercase();
                // Check if this is an aggregation function
                if Self::is_aggregate_function_name(&func_name) {
                    return true;
                }
                // Recursively check arguments
                for arg in args {
                    if self.contains_aggregation(arg) {
                        return true;
                    }
                }
                false
            }
            Expression::BinaryOp { left, right, .. } => {
                self.contains_aggregation(left) || self.contains_aggregation(right)
            }
            Expression::UnaryOp { operand, .. } => self.contains_aggregation(operand),
            Expression::List(elements) => elements.iter().any(|e| self.contains_aggregation(e)),
            Expression::Map(map) => map.values().any(|e| self.contains_aggregation(e)),
            Expression::Case {
                input,
                when_clauses,
                else_clause,
            } => {
                if let Some(input_expr) = input {
                    if self.contains_aggregation(input_expr) {
                        return true;
                    }
                }
                for when in when_clauses {
                    if self.contains_aggregation(&when.condition)
                        || self.contains_aggregation(&when.result)
                    {
                        return true;
                    }
                }
                if let Some(else_expr) = else_clause {
                    if self.contains_aggregation(else_expr) {
                        return true;
                    }
                }
                false
            }
            Expression::IsNull { expr, .. } => self.contains_aggregation(expr),
            Expression::ArrayIndex { base, index } => {
                self.contains_aggregation(base) || self.contains_aggregation(index)
            }
            _ => false,
        }
    }

    /// Lift every aggregate call out of `expr`, replacing each in place with a
    /// reference to a synthetic column, so the enclosing expression can be
    /// computed *after* the aggregation.
    ///
    /// Returns the rewritten expression and the lifted calls paired with the
    /// synthetic alias each was replaced by. `next_index` is threaded across
    /// calls so aliases stay unique across a whole projection list.
    pub(super) fn lift_aggregations(
        &self,
        expr: &Expression,
        next_index: &mut usize,
    ) -> (Expression, Vec<(String, Expression)>) {
        // A bare aggregate call — anywhere, including at the top level —
        // lifts whole: an aggregate nested inside an aggregate is invalid
        // Cypher and rejected elsewhere, so its arguments are never
        // recursed into.
        if let Expression::FunctionCall { name, .. } = expr {
            if Self::is_aggregate_function_name(&name.to_lowercase()) {
                let alias = format!("__agg_lift_{}", *next_index);
                *next_index += 1;
                return (
                    Expression::Variable(alias.clone()),
                    vec![(alias, expr.clone())],
                );
            }
        }

        match expr {
            Expression::FunctionCall { name, args } => {
                let mut lifted = Vec::new();
                let new_args = args
                    .iter()
                    .map(|arg| {
                        let (new_arg, arg_lifted) = self.lift_aggregations(arg, next_index);
                        lifted.extend(arg_lifted);
                        new_arg
                    })
                    .collect();
                (
                    Expression::FunctionCall {
                        name: name.clone(),
                        args: new_args,
                    },
                    lifted,
                )
            }
            Expression::BinaryOp { left, op, right } => {
                let (new_left, mut lifted) = self.lift_aggregations(left, next_index);
                let (new_right, right_lifted) = self.lift_aggregations(right, next_index);
                lifted.extend(right_lifted);
                (
                    Expression::BinaryOp {
                        left: Box::new(new_left),
                        right: Box::new(new_right),
                        op: *op,
                    },
                    lifted,
                )
            }
            Expression::UnaryOp { op, operand } => {
                let (new_operand, lifted) = self.lift_aggregations(operand, next_index);
                (
                    Expression::UnaryOp {
                        op: *op,
                        operand: Box::new(new_operand),
                    },
                    lifted,
                )
            }
            Expression::List(elements) => {
                let mut lifted = Vec::new();
                let new_elements = elements
                    .iter()
                    .map(|element| {
                        let (new_element, element_lifted) =
                            self.lift_aggregations(element, next_index);
                        lifted.extend(element_lifted);
                        new_element
                    })
                    .collect();
                (Expression::List(new_elements), lifted)
            }
            Expression::Map(map) => {
                let mut lifted = Vec::new();
                let new_map = map
                    .iter()
                    .map(|(key, value)| {
                        let (new_value, value_lifted) = self.lift_aggregations(value, next_index);
                        lifted.extend(value_lifted);
                        (key.clone(), new_value)
                    })
                    .collect();
                (Expression::Map(new_map), lifted)
            }
            Expression::Case {
                input,
                when_clauses,
                else_clause,
            } => {
                let mut lifted = Vec::new();
                let new_input = input.as_ref().map(|input_expr| {
                    let (new_input_expr, input_lifted) =
                        self.lift_aggregations(input_expr, next_index);
                    lifted.extend(input_lifted);
                    Box::new(new_input_expr)
                });
                let new_when_clauses = when_clauses
                    .iter()
                    .map(|when| {
                        let (new_condition, condition_lifted) =
                            self.lift_aggregations(&when.condition, next_index);
                        lifted.extend(condition_lifted);
                        let (new_result, result_lifted) =
                            self.lift_aggregations(&when.result, next_index);
                        lifted.extend(result_lifted);
                        WhenClause {
                            condition: new_condition,
                            result: new_result,
                        }
                    })
                    .collect();
                let new_else_clause = else_clause.as_ref().map(|else_expr| {
                    let (new_else_expr, else_lifted) =
                        self.lift_aggregations(else_expr, next_index);
                    lifted.extend(else_lifted);
                    Box::new(new_else_expr)
                });
                (
                    Expression::Case {
                        input: new_input,
                        when_clauses: new_when_clauses,
                        else_clause: new_else_clause,
                    },
                    lifted,
                )
            }
            Expression::IsNull { expr, negated } => {
                let (new_expr, lifted) = self.lift_aggregations(expr, next_index);
                (
                    Expression::IsNull {
                        expr: Box::new(new_expr),
                        negated: *negated,
                    },
                    lifted,
                )
            }
            Expression::ArrayIndex { base, index } => {
                let (new_base, mut lifted) = self.lift_aggregations(base, next_index);
                let (new_index, index_lifted) = self.lift_aggregations(index, next_index);
                lifted.extend(index_lifted);
                (
                    Expression::ArrayIndex {
                        base: Box::new(new_base),
                        index: Box::new(new_index),
                    },
                    lifted,
                )
            }
            _ => (expr.clone(), vec![]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::CATALOG_MMAP_INITIAL_SIZE;
    use crate::testing::TestContext;

    /// Builds an isolated `Catalog` + `TestContext` pair, mirroring
    /// `planner::tests::create_test_catalog` (that helper is `pub(super)`
    /// to the sibling `planner::tests` module and unreachable from here).
    fn create_test_catalog() -> (Catalog, TestContext) {
        let ctx = TestContext::new();
        let catalog =
            Catalog::with_isolated_path(ctx.path().join("catalog.mdb"), CATALOG_MMAP_INITIAL_SIZE)
                .expect("failed to create isolated test catalog");
        (catalog, ctx)
    }

    fn function_call(name: &str, args: Vec<Expression>) -> Expression {
        Expression::FunctionCall {
            name: name.to_string(),
            args,
        }
    }

    fn property(variable: &str, property: &str) -> Expression {
        Expression::PropertyAccess {
            variable: variable.to_string(),
            property: property.to_string(),
        }
    }

    /// `Expression` does not implement `PartialEq` — nothing in production
    /// code compares two expression trees — so these tests compare the
    /// derived `Debug` rendering instead, which distinguishes every variant
    /// and field used here.
    fn assert_expr_eq(actual: &Expression, expected: &Expression) {
        assert_eq!(format!("{:?}", actual), format!("{:?}", expected));
    }

    /// Same, for the `(synthetic alias, lifted call)` pairs.
    fn assert_lifted_eq(actual: &[(String, Expression)], expected: &[(String, Expression)]) {
        assert_eq!(format!("{:?}", actual), format!("{:?}", expected));
    }

    #[test]
    fn bare_aggregate_call_lifts_to_variable_and_one_pair() {
        let (catalog, _ctx) = create_test_catalog();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new(crate::index::DEFAULT_VECTORIZER_DIMENSION).unwrap();
        let planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

        let count_star = function_call("count", vec![]);
        let mut next_index = 0usize;
        let (rewritten, lifted) = planner.lift_aggregations(&count_star, &mut next_index);

        assert_expr_eq(
            &rewritten,
            &Expression::Variable("__agg_lift_0".to_string()),
        );
        assert_eq!(lifted.len(), 1);
        assert_eq!(lifted[0].0, "__agg_lift_0");
        assert_expr_eq(&lifted[0].1, &count_star);
        assert_eq!(next_index, 1);
    }

    #[test]
    fn count_star_greater_than_zero_lifts_the_count_call() {
        let (catalog, _ctx) = create_test_catalog();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new(crate::index::DEFAULT_VECTORIZER_DIMENSION).unwrap();
        let planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

        let count_star = function_call("count", vec![]);
        let expr = Expression::BinaryOp {
            left: Box::new(count_star.clone()),
            op: BinaryOperator::GreaterThan,
            right: Box::new(Expression::Literal(Literal::Integer(0))),
        };
        let mut next_index = 0usize;
        let (rewritten, lifted) = planner.lift_aggregations(&expr, &mut next_index);

        assert_expr_eq(
            &rewritten,
            &Expression::BinaryOp {
                left: Box::new(Expression::Variable("__agg_lift_0".to_string())),
                op: BinaryOperator::GreaterThan,
                right: Box::new(Expression::Literal(Literal::Integer(0))),
            },
        );
        assert_lifted_eq(&lifted, &[("__agg_lift_0".to_string(), count_star)]);
        assert_eq!(next_index, 1);
    }

    #[test]
    fn count_star_plus_one_threads_a_non_zero_starting_index() {
        let (catalog, _ctx) = create_test_catalog();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new(crate::index::DEFAULT_VECTORIZER_DIMENSION).unwrap();
        let planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

        let count_star = function_call("count", vec![]);
        let expr = Expression::BinaryOp {
            left: Box::new(count_star.clone()),
            op: BinaryOperator::Add,
            right: Box::new(Expression::Literal(Literal::Integer(1))),
        };
        let mut next_index = 5usize;
        let (rewritten, lifted) = planner.lift_aggregations(&expr, &mut next_index);

        assert_expr_eq(
            &rewritten,
            &Expression::BinaryOp {
                left: Box::new(Expression::Variable("__agg_lift_5".to_string())),
                op: BinaryOperator::Add,
                right: Box::new(Expression::Literal(Literal::Integer(1))),
            },
        );
        assert_lifted_eq(&lifted, &[("__agg_lift_5".to_string(), count_star)]);
        // `next_index` must advance past the index it just handed out so a
        // caller threading it across a whole projection list never reuses
        // an alias.
        assert_eq!(next_index, 6);
    }

    #[test]
    fn list_of_two_bare_aggregates_yields_two_distinct_aliases() {
        let (catalog, _ctx) = create_test_catalog();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new(crate::index::DEFAULT_VECTORIZER_DIMENSION).unwrap();
        let planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

        let count_star = function_call("count", vec![]);
        let expr = Expression::List(vec![count_star.clone(), count_star.clone()]);
        let mut next_index = 0usize;
        let (rewritten, lifted) = planner.lift_aggregations(&expr, &mut next_index);

        assert_expr_eq(
            &rewritten,
            &Expression::List(vec![
                Expression::Variable("__agg_lift_0".to_string()),
                Expression::Variable("__agg_lift_1".to_string()),
            ]),
        );
        assert_lifted_eq(
            &lifted,
            &[
                ("__agg_lift_0".to_string(), count_star.clone()),
                ("__agg_lift_1".to_string(), count_star),
            ],
        );
        assert_eq!(next_index, 2);
    }

    #[test]
    fn case_when_result_aggregate_is_lifted() {
        let (catalog, _ctx) = create_test_catalog();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new(crate::index::DEFAULT_VECTORIZER_DIMENSION).unwrap();
        let planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

        let sum_call = function_call("sum", vec![property("n", "age")]);
        let expr = Expression::Case {
            input: None,
            when_clauses: vec![WhenClause {
                condition: Expression::Literal(Literal::Boolean(true)),
                result: sum_call.clone(),
            }],
            else_clause: Some(Box::new(Expression::Literal(Literal::Integer(0)))),
        };
        let mut next_index = 0usize;
        let (rewritten, lifted) = planner.lift_aggregations(&expr, &mut next_index);

        assert_expr_eq(
            &rewritten,
            &Expression::Case {
                input: None,
                when_clauses: vec![WhenClause {
                    condition: Expression::Literal(Literal::Boolean(true)),
                    result: Expression::Variable("__agg_lift_0".to_string()),
                }],
                else_clause: Some(Box::new(Expression::Literal(Literal::Integer(0)))),
            },
        );
        assert_lifted_eq(&lifted, &[("__agg_lift_0".to_string(), sum_call)]);
        assert_eq!(next_index, 1);
    }

    #[test]
    fn map_value_aggregate_is_lifted() {
        let (catalog, _ctx) = create_test_catalog();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new(crate::index::DEFAULT_VECTORIZER_DIMENSION).unwrap();
        let planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

        let count_star = function_call("count", vec![]);
        let mut map = HashMap::new();
        map.insert("total".to_string(), count_star.clone());
        let expr = Expression::Map(map);
        let mut next_index = 0usize;
        let (rewritten, lifted) = planner.lift_aggregations(&expr, &mut next_index);

        let mut expected_map = HashMap::new();
        expected_map.insert(
            "total".to_string(),
            Expression::Variable("__agg_lift_0".to_string()),
        );
        assert_expr_eq(&rewritten, &Expression::Map(expected_map));
        assert_lifted_eq(&lifted, &[("__agg_lift_0".to_string(), count_star)]);
        assert_eq!(next_index, 1);
    }

    #[test]
    fn expression_without_aggregate_is_returned_unchanged_with_empty_vec() {
        let (catalog, _ctx) = create_test_catalog();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new(crate::index::DEFAULT_VECTORIZER_DIMENSION).unwrap();
        let planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

        let expr = Expression::BinaryOp {
            left: Box::new(property("n", "age")),
            op: BinaryOperator::GreaterThan,
            right: Box::new(Expression::Literal(Literal::Integer(18))),
        };
        let mut next_index = 0usize;
        let (rewritten, lifted) = planner.lift_aggregations(&expr, &mut next_index);

        assert_expr_eq(&rewritten, &expr);
        assert!(lifted.is_empty());
        assert_eq!(next_index, 0);
    }

    #[test]
    fn head_of_collect_lifts_only_the_inner_collect_call() {
        let (catalog, _ctx) = create_test_catalog();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new(crate::index::DEFAULT_VECTORIZER_DIMENSION).unwrap();
        let planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

        let collect_call = function_call("collect", vec![property("n", "x")]);
        let expr = function_call("head", vec![collect_call.clone()]);
        let mut next_index = 0usize;
        let (rewritten, lifted) = planner.lift_aggregations(&expr, &mut next_index);

        assert_expr_eq(
            &rewritten,
            &function_call(
                "head",
                vec![Expression::Variable("__agg_lift_0".to_string())],
            ),
        );
        assert_lifted_eq(&lifted, &[("__agg_lift_0".to_string(), collect_call)]);
        assert_eq!(next_index, 1);
    }
}
