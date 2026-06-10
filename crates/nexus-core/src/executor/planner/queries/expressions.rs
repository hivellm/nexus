//! Expression/pattern serialisation and aggregation detection helpers.

use super::*;

impl<'a> QueryPlanner<'a> {
    /// Convert expression to string representation
    // Visibility elevated to `planner` level: called from `planner/tests.rs`,
    // which sat next to this method before the split.
    pub(in crate::executor::planner) fn expression_to_string(
        &self,
        expr: &Expression,
    ) -> Result<String> {
        match expr {
            Expression::Variable(name) => Ok(name.clone()),
            Expression::PropertyAccess { variable, property } => {
                Ok(format!("{}.{}", variable, property))
            }
            Expression::ArrayIndex { base, index } => {
                let base_str = self.expression_to_string(base)?;
                let index_str = self.expression_to_string(index)?;
                Ok(format!("{}[{}]", base_str, index_str))
            }
            Expression::Literal(literal) => match literal {
                // Use single quotes for strings to match Cypher parser expectations
                // This is critical for filter predicates to work correctly
                Literal::String(s) => Ok(format!("'{}'", s)),
                Literal::Integer(i) => Ok(i.to_string()),
                Literal::Float(f) => Ok(f.to_string()),
                Literal::Boolean(b) => Ok(b.to_string()),
                Literal::Null => Ok("NULL".to_string()),
                Literal::Point(p) => Ok(p.to_string()),
            },
            Expression::BinaryOp { left, op, right } => {
                let left_str = self.expression_to_string(left)?;
                let right_str = self.expression_to_string(right)?;
                let op_str = match op {
                    BinaryOperator::Equal => "=",
                    BinaryOperator::NotEqual => "!=",
                    BinaryOperator::LessThan => "<",
                    BinaryOperator::LessThanOrEqual => "<=",
                    BinaryOperator::GreaterThan => ">",
                    BinaryOperator::GreaterThanOrEqual => ">=",
                    BinaryOperator::And => "AND",
                    BinaryOperator::Or => "OR",
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
                let expr_str = self.expression_to_string(expr)?;
                if *negated {
                    Ok(format!("{} IS NOT NULL", expr_str))
                } else {
                    Ok(format!("{} IS NULL", expr_str))
                }
            }
            Expression::List(elements) => {
                let elem_strs: Result<Vec<String>> = elements
                    .iter()
                    .map(|e| self.expression_to_string(e))
                    .collect();
                Ok(format!("[{}]", elem_strs?.join(", ")))
            }
            Expression::Map(map) => {
                let mut pairs = Vec::new();
                for (key, value) in map {
                    let value_str = self.expression_to_string(value)?;
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
                let arg_strs: Result<Vec<String>> =
                    args.iter().map(|a| self.expression_to_string(a)).collect();
                Ok(format!("{}({})", name, arg_strs?.join(", ")))
            }
            Expression::UnaryOp { op, operand } => {
                let operand_str = self.expression_to_string(operand)?;
                let op_str = match op {
                    UnaryOperator::Not => "NOT",
                    UnaryOperator::Minus => "-",
                    UnaryOperator::Plus => "+",
                };
                Ok(format!("{} {}", op_str, operand_str))
            }
            Expression::Exists {
                pattern,
                where_clause,
            } => {
                let pattern_str = self.pattern_to_string(pattern)?;
                if let Some(where_expr) = where_clause {
                    let where_str = self.expression_to_string(where_expr)?;
                    Ok(format!("EXISTS {{ {} WHERE {} }}", pattern_str, where_str))
                } else {
                    Ok(format!("EXISTS {{ {} }}", pattern_str))
                }
            }
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

    /// Check if an expression contains an aggregation function (recursively)
    pub(super) fn contains_aggregation(&self, expr: &Expression) -> bool {
        match expr {
            Expression::FunctionCall { name, args } => {
                let func_name = name.to_lowercase();
                // Check if this is an aggregation function
                // phase6 §9 — statistical aggregations must trigger the
                // same row-collapse path as count/sum/avg. Before adding
                // these, `MATCH (n:A) RETURN stdev(n.score)` returned
                // 20 rows (one per matched :A node) instead of one
                // aggregated row because the planner didn't treat
                // stdev/variance/percentile* as aggregations and so
                // never introduced the Aggregate operator.
                if matches!(
                    func_name.as_str(),
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
                ) {
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

    /// Replace nested aggregations in an expression with variable references
    pub(super) fn replace_nested_aggregations(
        &self,
        expr: &Expression,
        aggregations: &[Aggregation],
    ) -> Expression {
        match expr {
            Expression::FunctionCall { name, args } => {
                let func_name = name.to_lowercase();
                // Check if this is a nested aggregation function
                if func_name == "collect" {
                    // Find matching aggregation by checking if the arguments match
                    for (idx, agg) in aggregations.iter().enumerate() {
                        if let Aggregation::Collect { column, .. } = agg {
                            // Check if this collect() matches the aggregation
                            if let Some(arg) = args.first() {
                                let matches = match arg {
                                    Expression::Variable(var) => var == column,
                                    Expression::PropertyAccess { variable, property } => {
                                        format!("{}.{}", variable, property) == *column
                                    }
                                    _ => false,
                                };
                                if matches {
                                    // Replace with variable reference to aggregation result
                                    let temp_alias = format!("__agg_{}", idx);
                                    return Expression::Variable(temp_alias);
                                }
                            }
                        }
                    }
                }

                // Recursively replace nested aggregations in arguments
                let new_args: Vec<Expression> = args
                    .iter()
                    .map(|arg| self.replace_nested_aggregations(arg, aggregations))
                    .collect();

                Expression::FunctionCall {
                    name: name.clone(),
                    args: new_args,
                }
            }
            Expression::BinaryOp { left, right, op } => Expression::BinaryOp {
                left: Box::new(self.replace_nested_aggregations(left, aggregations)),
                right: Box::new(self.replace_nested_aggregations(right, aggregations)),
                op: *op,
            },
            Expression::UnaryOp { op, operand } => Expression::UnaryOp {
                op: *op,
                operand: Box::new(self.replace_nested_aggregations(operand, aggregations)),
            },
            _ => expr.clone(),
        }
    }
}
