use super::parser::{
    BinaryOperator, Clause, CypherQuery, Expression, Literal, Pattern, PatternElement,
    RelationshipDirection, ReturnItem,
};
use super::{Aggregation, Direction, Operator, ProjectionItem};
use crate::catalog::Catalog;
use crate::index::{KnnIndex, LabelIndex};
use crate::{Error, Result};
use std::collections::HashSet;

/// Query planner for optimizing Cypher execution
pub struct QueryPlanner<'a> {
    catalog: &'a Catalog,
    label_index: &'a LabelIndex,
    knn_index: &'a KnnIndex,
}

impl<'a> QueryPlanner<'a> {
    /// Create a new query planner
    pub fn new(catalog: &'a Catalog, label_index: &'a LabelIndex, knn_index: &'a KnnIndex) -> Self {
        Self {
            catalog,
            label_index,
            knn_index,
        }
    }

    /// Plan a Cypher query into optimized operators
    pub fn plan_query(&self, query: &CypherQuery) -> Result<Vec<Operator>> {
        // Validate that query has at least one clause
        if query.clauses.is_empty() {
            return Err(Error::CypherSyntax(
                "Query must contain at least one clause".to_string(),
            ));
        }

        // Check if query contains UNION - if so, split and plan separately
        if let Some(union_idx) = query
            .clauses
            .iter()
            .position(|c| matches!(c, Clause::Union(_)))
        {
            // Extract the UnionClause to get union_type
            let distinct = if let Some(Clause::Union(union_clause)) = query.clauses.get(union_idx) {
                union_clause.union_type == super::parser::UnionType::Distinct
            } else {
                true // Default to UNION (distinct)
            };

            // Split query into left and right parts
            let left_clauses: Vec<Clause> = query.clauses[..union_idx].to_vec();
            let right_clauses: Vec<Clause> = query.clauses[union_idx + 1..].to_vec();

            // Create separate queries for left and right
            let left_query = CypherQuery {
                clauses: left_clauses,
                params: query.params.clone(),
            };
            let right_query = CypherQuery {
                clauses: right_clauses,
                params: query.params.clone(),
            };

            // Plan both sides recursively
            let left_operators = self.plan_query(&left_query)?;
            let right_operators = self.plan_query(&right_query)?;

            // Create UNION operator with complete operator pipelines for each side
            let operators = vec![Operator::Union {
                left: left_operators,
                right: right_operators,
                distinct,
            }];

            return Ok(operators);
        }

        let mut operators = Vec::new();

        // Extract patterns and constraints
        let mut patterns = Vec::new();
        let mut where_clauses = Vec::new();
        let mut return_items = Vec::new();
        let mut limit_count = None;
        let mut return_distinct = false;

        for clause in &query.clauses {
            match clause {
                Clause::Match(match_clause) => {
                    // For OPTIONAL MATCH, we need to handle NULL values for unmatched patterns
                    // Store pattern with optional flag for later handling in executor
                    patterns.push(match_clause.pattern.clone());
                    if let Some(where_clause) = &match_clause.where_clause {
                        where_clauses.push(where_clause.expression.clone());
                    }
                    // OPTIONAL MATCH is handled by executor as LEFT OUTER JOIN semantics
                    // For now, we just collect the patterns - executor will handle NULL values
                }
                Clause::Create(create_clause) => {
                    // Add CREATE operator to create nodes/relationships in context
                    operators.push(Operator::Create {
                        pattern: create_clause.pattern.clone(),
                    });
                }
                Clause::Delete(delete_clause) => {
                    // Extract variables to delete from the delete clause
                    let variables = delete_clause.items.clone();

                    if delete_clause.detach {
                        operators.push(Operator::DetachDelete { variables });
                    } else {
                        operators.push(Operator::Delete { variables });
                    }
                }
                Clause::Merge(merge_clause) => {
                    patterns.push(merge_clause.pattern.clone());
                    // MERGE is handled as match-or-create
                    // Store pattern for executor to handle
                }
                Clause::Where(where_clause) => {
                    where_clauses.push(where_clause.expression.clone());
                }
                Clause::With(with_clause) => {
                    // WITH is similar to RETURN but for intermediate results
                    // Store the WITH items as new projection columns
                    return_items = with_clause.items.clone();
                    // Apply WHERE filtering if present in WITH clause
                    if let Some(where_clause) = &with_clause.where_clause {
                        where_clauses.push(where_clause.expression.clone());
                    }
                    return_distinct = with_clause.distinct;
                }
                Clause::Unwind(_unwind_clause) => {
                    // UNWIND expands a list into rows
                    // For now, we'll just track this - executor will handle list expansion
                    // UNWIND is typically used early in query pipelines
                }
                Clause::Return(return_clause) => {
                    return_items = return_clause.items.clone();
                    return_distinct = return_clause.distinct;
                }
                Clause::Limit(limit_clause) => {
                    if let Expression::Literal(Literal::Integer(count)) = &limit_clause.count {
                        limit_count = Some(*count as usize);
                    }
                }
                Clause::Union(_) => {
                    // Should have been handled above
                }
                _ => {
                    // Other clauses not implemented in MVP
                }
            }
        }

        // Plan execution strategy only if we have patterns to match
        // CREATE-only queries don't need pattern matching
        if !patterns.is_empty() {
            self.plan_execution_strategy(
                &patterns,
                &where_clauses,
                &return_items,
                limit_count,
                return_distinct,
                &mut operators,
            )?;
        }

        Ok(operators)
    }

    /// Plan execution strategy based on patterns and constraints
    fn plan_execution_strategy(
        &self,
        patterns: &[Pattern],
        where_clauses: &[Expression],
        return_items: &[ReturnItem],
        limit_count: Option<usize>,
        distinct: bool,
        operators: &mut Vec<Operator>,
    ) -> Result<()> {
        // Process ALL patterns, not just the first one
        // Multiple patterns need Cartesian product (Join)
        let mut all_target_nodes = std::collections::HashSet::new();

        // Identify target nodes across all patterns
        for pattern in patterns {
            for (idx, element) in pattern.elements.iter().enumerate() {
                if let PatternElement::Relationship(_) = element {
                    if idx + 1 < pattern.elements.len() {
                        if let PatternElement::Node(node) = &pattern.elements[idx + 1] {
                            if let Some(var) = &node.variable {
                                if node.labels.is_empty() {
                                    all_target_nodes.insert(var.clone());
                                }
                            }
                        }
                    }
                }
            }
        }

        // Process the first pattern
        let start_pattern = self.select_start_pattern(patterns)?;

        // Add NodeByLabel operators for nodes in first pattern
        for element in &start_pattern.elements {
            if let PatternElement::Node(node) = element {
                if let Some(variable) = &node.variable {
                    // Skip if this node is a pure target without labels (will be populated by Expand)
                    if all_target_nodes.contains(variable) {
                        continue;
                    }
                    if !node.labels.is_empty() {
                        // Use first label for initial scan
                        let first_label = &node.labels[0];
                        let label_id = self.catalog.get_or_create_label(first_label)?;
                        operators.push(Operator::NodeByLabel {
                            label_id,
                            variable: variable.clone(),
                        });

                        // Add filters for additional labels (multiple label intersection)
                        if node.labels.len() > 1 {
                            for additional_label in &node.labels[1..] {
                                // Create a filter that checks if node has this label
                                let filter_expr = format!("{}:{}", variable, additional_label);
                                operators.push(Operator::Filter {
                                    predicate: filter_expr,
                                });
                            }
                        }
                    } else {
                        // No label specified - need to scan all nodes
                        // For now, use label_id 0 as a fallback (this might need lower-level storage access)
                        // TODO: Add a proper AllNodesScan operator for better performance
                        operators.push(Operator::NodeByLabel {
                            label_id: 0, // Temporary: scan all (will need proper implementation)
                            variable: variable.clone(),
                        });
                    }

                    // Add filters for inline properties: MATCH (n {property: value})
                    if let Some(property_map) = &node.properties {
                        for (prop_name, prop_value_expr) in &property_map.properties {
                            // Convert property value expression to string for filter
                            let value_str = match prop_value_expr {
                                Expression::Literal(lit) => match lit {
                                    Literal::String(s) => format!("\"{}\"", s),
                                    Literal::Integer(i) => i.to_string(),
                                    Literal::Float(f) => f.to_string(),
                                    Literal::Boolean(b) => b.to_string(),
                                    Literal::Null => "null".to_string(),
                                },
                                _ => self.expression_to_string(prop_value_expr)?,
                            };
                            let filter_expr = format!("{}.{} = {}", variable, prop_name, value_str);
                            operators.push(Operator::Filter {
                                predicate: filter_expr,
                            });
                        }
                    }
                }
            }
        }

        // Add relationship traversal operators for first pattern
        self.add_relationship_operators(std::slice::from_ref(start_pattern), operators)?;

        // Process additional patterns (for comma-separated MATCH patterns like (p1:...), (p2:...))
        // Each additional pattern needs its own NodeByLabel + Filter operators
        for (pattern_idx, pattern) in patterns.iter().enumerate() {
            if pattern_idx == 0 {
                continue; // Skip first pattern, already processed
            }

            // Add NodeByLabel operators for nodes in this additional pattern
            for element in &pattern.elements {
                if let PatternElement::Node(node) = element {
                    if let Some(variable) = &node.variable {
                        if all_target_nodes.contains(variable) {
                            continue;
                        }

                        if !node.labels.is_empty() {
                            let first_label = &node.labels[0];
                            let label_id = self.catalog.get_or_create_label(first_label)?;
                            operators.push(Operator::NodeByLabel {
                                label_id,
                                variable: variable.clone(),
                            });

                            // Add filters for additional labels
                            if node.labels.len() > 1 {
                                for additional_label in &node.labels[1..] {
                                    let filter_expr = format!("{}:{}", variable, additional_label);
                                    operators.push(Operator::Filter {
                                        predicate: filter_expr,
                                    });
                                }
                            }
                        }

                        // Add filters for inline properties
                        if let Some(property_map) = &node.properties {
                            for (prop_name, prop_value_expr) in &property_map.properties {
                                let value_str = match prop_value_expr {
                                    Expression::Literal(lit) => match lit {
                                        Literal::String(s) => format!("\"{}\"", s),
                                        Literal::Integer(i) => i.to_string(),
                                        Literal::Float(f) => f.to_string(),
                                        Literal::Boolean(b) => b.to_string(),
                                        Literal::Null => "null".to_string(),
                                    },
                                    _ => self.expression_to_string(prop_value_expr)?,
                                };
                                let filter_expr =
                                    format!("{}.{} = {}", variable, prop_name, value_str);
                                operators.push(Operator::Filter {
                                    predicate: filter_expr,
                                });
                            }
                        }
                    }
                }
            }

            // Add relationship operators for this pattern if any
            self.add_relationship_operators(std::slice::from_ref(pattern), operators)?;
        }

        // Add filter operators for WHERE clauses
        for where_clause in where_clauses {
            operators.push(Operator::Filter {
                predicate: self.expression_to_string(where_clause)?,
            });
        }

        // Add projection or aggregation operator for RETURN clause
        if !return_items.is_empty() {
            // Check if any return items contain aggregate functions
            let mut has_aggregation = false;
            let mut aggregations = Vec::new();
            let mut group_by_columns = Vec::new();

            let mut non_aggregate_aliases: Vec<String> = Vec::new();

            for item in return_items {
                match &item.expression {
                    Expression::FunctionCall { name, args } => {
                        let func_name = name.to_lowercase();
                        match func_name.as_str() {
                            "count" => {
                                has_aggregation = true;

                                // Check for DISTINCT marker
                                let mut distinct = false;
                                let mut real_args = args.clone();
                                if let Some(Expression::Variable(var)) = args.first() {
                                    if var == "__DISTINCT__" {
                                        distinct = true;
                                        real_args = args[1..].to_vec();
                                    }
                                }

                                let column = if real_args.is_empty() {
                                    None // COUNT(*) or COUNT(DISTINCT *)
                                } else if let Some(Expression::Variable(var)) = real_args.first() {
                                    Some(var.clone())
                                } else if let Some(Expression::PropertyAccess {
                                    variable,
                                    property,
                                }) = real_args.first()
                                {
                                    Some(format!("{}.{}", variable, property))
                                } else {
                                    None
                                };

                                aggregations.push(Aggregation::Count {
                                    column,
                                    alias: item
                                        .alias
                                        .clone()
                                        .unwrap_or_else(|| "count".to_string()),
                                    distinct,
                                });
                            }
                            "sum" => {
                                has_aggregation = true;
                                if let Some(arg) = args.first() {
                                    let column = match arg {
                                        Expression::Variable(var) => var.clone(),
                                        Expression::PropertyAccess { variable, property } => {
                                            format!("{}.{}", variable, property)
                                        }
                                        _ => continue,
                                    };
                                    aggregations.push(Aggregation::Sum {
                                        column,
                                        alias: item
                                            .alias
                                            .clone()
                                            .unwrap_or_else(|| "sum".to_string()),
                                    });
                                }
                            }
                            "avg" => {
                                has_aggregation = true;
                                if let Some(arg) = args.first() {
                                    let column = match arg {
                                        Expression::Variable(var) => var.clone(),
                                        Expression::PropertyAccess { variable, property } => {
                                            format!("{}.{}", variable, property)
                                        }
                                        _ => continue,
                                    };
                                    aggregations.push(Aggregation::Avg {
                                        column,
                                        alias: item
                                            .alias
                                            .clone()
                                            .unwrap_or_else(|| "avg".to_string()),
                                    });
                                }
                            }
                            "min" => {
                                has_aggregation = true;
                                if let Some(arg) = args.first() {
                                    let column = match arg {
                                        Expression::Variable(var) => var.clone(),
                                        Expression::PropertyAccess { variable, property } => {
                                            format!("{}.{}", variable, property)
                                        }
                                        _ => continue,
                                    };
                                    aggregations.push(Aggregation::Min {
                                        column,
                                        alias: item
                                            .alias
                                            .clone()
                                            .unwrap_or_else(|| "min".to_string()),
                                    });
                                }
                            }
                            "max" => {
                                has_aggregation = true;
                                if let Some(arg) = args.first() {
                                    let column = match arg {
                                        Expression::Variable(var) => var.clone(),
                                        Expression::PropertyAccess { variable, property } => {
                                            format!("{}.{}", variable, property)
                                        }
                                        _ => continue,
                                    };
                                    aggregations.push(Aggregation::Max {
                                        column,
                                        alias: item
                                            .alias
                                            .clone()
                                            .unwrap_or_else(|| "max".to_string()),
                                    });
                                }
                            }
                            _ => {
                                // Not an aggregate function, treat as regular column for GROUP BY
                                let alias = item.alias.clone().unwrap_or_else(|| {
                                    self.expression_to_string(&item.expression)
                                        .unwrap_or_default()
                                });
                                non_aggregate_aliases.push(alias);
                            }
                        }
                    }
                    _ => {
                        // Non-aggregate expression, add to GROUP BY if there are aggregations
                        let alias = item.alias.clone().unwrap_or_else(|| {
                            self.expression_to_string(&item.expression)
                                .unwrap_or_default()
                        });
                        non_aggregate_aliases.push(alias);
                    }
                }
            }

            if has_aggregation {
                if group_by_columns.is_empty() {
                    group_by_columns = non_aggregate_aliases.clone();
                } else {
                    for alias in &non_aggregate_aliases {
                        if !group_by_columns.contains(alias) {
                            group_by_columns.push(alias.clone());
                        }
                    }
                }

                let mut projection_items: Vec<ProjectionItem> = Vec::new();
                let mut required_columns: HashSet<String> = HashSet::new();

                for item in return_items {
                    match &item.expression {
                        Expression::FunctionCall { name, args } => {
                            let func_name = name.to_lowercase();
                            match func_name.as_str() {
                                "count" | "sum" | "avg" | "min" | "max" => {
                                    // Skip DISTINCT marker if present
                                    let real_args =
                                        if let Some(Expression::Variable(var)) = args.first() {
                                            if var == "__DISTINCT__" {
                                                &args[1..]
                                            } else {
                                                args.as_slice()
                                            }
                                        } else {
                                            args.as_slice()
                                        };

                                    if let Some(arg) = real_args.first() {
                                        match arg {
                                            Expression::Variable(var) => {
                                                required_columns.insert(var.clone());
                                            }
                                            Expression::PropertyAccess { variable, property } => {
                                                required_columns
                                                    .insert(format!("{}.{}", variable, property));
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                                _ => {
                                    let alias = item.alias.clone().unwrap_or_else(|| {
                                        self.expression_to_string(&item.expression)
                                            .unwrap_or_default()
                                    });
                                    projection_items.push(ProjectionItem {
                                        alias,
                                        expression: item.expression.clone(),
                                    });
                                }
                            }
                        }
                        _ => {
                            let alias = item.alias.clone().unwrap_or_else(|| {
                                self.expression_to_string(&item.expression)
                                    .unwrap_or_default()
                            });
                            projection_items.push(ProjectionItem {
                                alias,
                                expression: item.expression.clone(),
                            });
                        }
                    }
                }

                for column in required_columns {
                    if !projection_items.iter().any(|item| item.alias == column) {
                        let expression = if column.contains('.') {
                            let parts: Vec<&str> = column.split('.').collect();
                            if parts.len() == 2 {
                                Expression::PropertyAccess {
                                    variable: parts[0].to_string(),
                                    property: parts[1].to_string(),
                                }
                            } else {
                                Expression::Variable(column.clone())
                            }
                        } else {
                            Expression::Variable(column.clone())
                        };

                        projection_items.push(ProjectionItem {
                            alias: column.clone(),
                            expression,
                        });
                    }
                }

                if !projection_items.is_empty() {
                    operators.push(Operator::Project {
                        items: projection_items,
                    });
                }

                operators.push(Operator::Aggregate {
                    group_by: group_by_columns,
                    aggregations,
                });
            } else {
                // Regular projection
                let projection_items: Vec<ProjectionItem> = return_items
                    .iter()
                    .map(|item| ProjectionItem {
                        alias: item.alias.clone().unwrap_or_else(|| {
                            self.expression_to_string(&item.expression)
                                .unwrap_or_default()
                        }),
                        expression: item.expression.clone(),
                    })
                    .collect();

                operators.push(Operator::Project {
                    items: projection_items,
                });

                // Add DISTINCT operator if specified
                if distinct {
                    let distinct_columns: Vec<String> = return_items
                        .iter()
                        .map(|item| {
                            item.alias.clone().unwrap_or_else(|| {
                                self.expression_to_string(&item.expression)
                                    .unwrap_or_default()
                            })
                        })
                        .collect();
                    operators.push(Operator::Distinct {
                        columns: distinct_columns,
                    });
                }
            }
        }

        // Add limit operator if specified
        if let Some(count) = limit_count {
            operators.push(Operator::Limit { count });
        }

        Ok(())
    }

    /// Select the most selective pattern to start execution
    fn select_start_pattern<'b>(&self, patterns: &'b [Pattern]) -> Result<&'b Pattern> {
        if patterns.is_empty() {
            return Err(Error::CypherSyntax(
                "No patterns found in query".to_string(),
            ));
        }

        // For MVP, just return the first pattern
        // In a full implementation, we would analyze selectivity
        Ok(&patterns[0])
    }

    /// Add relationship traversal operators
    fn add_relationship_operators(
        &self,
        patterns: &[Pattern],
        operators: &mut Vec<Operator>,
    ) -> Result<()> {
        let mut tmp_var_counter = 0;

        for pattern in patterns {
            // Track previous node variable for relationship expansion
            let mut prev_node_var: Option<String> = None;

            for (idx, element) in pattern.elements.iter().enumerate() {
                match element {
                    PatternElement::Node(node_pattern) => {
                        // Update previous node variable
                        // If node has explicit variable, use it
                        // Otherwise, keep the previous value (from last Expand's target_var)
                        if let Some(var) = &node_pattern.variable {
                            prev_node_var = Some(var.clone());
                        }
                        // Don't update prev_node_var if no variable - it should already be set by previous Expand
                    }
                    PatternElement::Relationship(rel) => {
                        let direction = match rel.direction {
                            RelationshipDirection::Outgoing => Direction::Outgoing,
                            RelationshipDirection::Incoming => Direction::Incoming,
                            RelationshipDirection::Both => Direction::Both,
                        };

                        // Determine source and target variables
                        let source_var = prev_node_var.clone().unwrap_or_default();

                        // Target will be the next node in the pattern
                        let target_var = if idx + 1 < pattern.elements.len() {
                            if let PatternElement::Node(next_node) = &pattern.elements[idx + 1] {
                                // If target node has explicit variable, use it
                                // Otherwise, generate temporary variable for chaining
                                next_node.variable.clone().unwrap_or_else(|| {
                                    let tmp_var = format!("__tmp_{}", tmp_var_counter);
                                    tmp_var_counter += 1;
                                    tmp_var
                                })
                            } else {
                                "".to_string()
                            }
                        } else {
                            "".to_string()
                        };

                        // Update prev_node_var to the target for next relationship
                        // This ensures multi-hop patterns chain correctly
                        prev_node_var = Some(target_var.clone());

                        // Get type_id from relationship types
                        let type_id = if let Some(first_type) = rel.types.first() {
                            // Try to get type_id from catalog
                            self.catalog.get_type_id(first_type)?
                        } else {
                            // No type specified - match all types
                            None
                        };

                        operators.push(Operator::Expand {
                            type_id,
                            source_var,
                            target_var,
                            rel_var: rel.variable.clone().unwrap_or_default(),
                            direction,
                        });
                    }
                }
            }
        }

        Ok(())
    }

    /// Convert expression to string representation
    fn expression_to_string(&self, expr: &Expression) -> Result<String> {
        match expr {
            Expression::Variable(name) => Ok(name.clone()),
            Expression::PropertyAccess { variable, property } => {
                Ok(format!("{}.{}", variable, property))
            }
            Expression::Literal(literal) => match literal {
                Literal::String(s) => Ok(format!("\"{}\"", s)),
                Literal::Integer(i) => Ok(i.to_string()),
                Literal::Float(f) => Ok(f.to_string()),
                Literal::Boolean(b) => Ok(b.to_string()),
                Literal::Null => Ok("NULL".to_string()),
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
            _ => Ok("?".to_string()),
        }
    }

    /// Estimate query cost for optimization
    pub fn estimate_cost(&self, operators: &[Operator]) -> Result<f64> {
        let mut total_cost = 0.0;

        for operator in operators {
            match operator {
                Operator::NodeByLabel { label_id, .. } => {
                    // Estimate cost based on label selectivity
                    let selectivity = self.estimate_label_selectivity(*label_id)?;
                    total_cost += 1000.0 * selectivity;
                }
                Operator::Filter { .. } => {
                    // Filter operations are relatively cheap
                    total_cost += 10.0;
                }
                Operator::Expand { .. } => {
                    // Relationship traversal is expensive
                    total_cost += 100.0;
                }
                Operator::Project { .. } => {
                    // Projection is cheap
                    total_cost += 1.0;
                }
                Operator::Limit { count } => {
                    // Limit reduces cost
                    total_cost *= (*count as f64) / 1000.0;
                }
                Operator::Sort { .. } => {
                    // Sorting is moderately expensive
                    total_cost += 50.0;
                }
                Operator::Aggregate { .. } => {
                    // Aggregation is moderately expensive
                    total_cost += 30.0;
                }
                Operator::Union { .. } => {
                    // Union is relatively cheap
                    total_cost += 20.0;
                }
                Operator::Join { .. } => {
                    // Join is expensive
                    total_cost += 200.0;
                }
                Operator::IndexScan { .. } => {
                    // Index scan is very cheap
                    total_cost += 5.0;
                }
                Operator::Distinct { .. } => {
                    // Distinct is moderately expensive
                    total_cost += 40.0;
                }
                Operator::HashJoin { .. } => {
                    // Hash join operations are moderately expensive
                    total_cost += 200.0;
                }
                Operator::Create { .. } => {
                    // CREATE operations are moderately expensive
                    total_cost += 50.0;
                }
                Operator::Delete { .. } => {
                    // DELETE operations are moderately expensive
                    total_cost += 40.0;
                }
                Operator::DetachDelete { .. } => {
                    // DETACH DELETE is more expensive (deletes relationships first)
                    total_cost += 60.0;
                }
            }
        }

        Ok(total_cost)
    }

    /// Estimate selectivity of a label
    fn estimate_label_selectivity(&self, _label_id: u32) -> Result<f64> {
        // For MVP, return a simple estimate
        // In a full implementation, we would use statistics
        Ok(0.1) // 10% selectivity
    }

    /// Optimize operator order based on cost estimates
    pub fn optimize_operator_order(&self, operators: Vec<Operator>) -> Result<Vec<Operator>> {
        // For MVP, just return operators in original order
        // In a full implementation, we would reorder based on cost estimates
        Ok(operators)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Catalog;
    use crate::executor::JoinType;
    use crate::executor::parser::{
        BinaryOperator, Clause, CypherQuery, Expression, LimitClause, Literal, MatchClause,
        NodePattern, Pattern, PatternElement, RelationshipDirection, RelationshipPattern,
        ReturnClause, ReturnItem, WhereClause,
    };
    use crate::index::{KnnIndex, LabelIndex};

    #[test]
    fn test_plan_simple_query() {
        let catalog = Catalog::new(tempfile::tempdir().unwrap()).unwrap();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new(128).unwrap();
        let planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

        let query = CypherQuery {
            clauses: vec![
                Clause::Match(MatchClause {
                    pattern: Pattern {
                        elements: vec![PatternElement::Node(NodePattern {
                            variable: Some("n".to_string()),
                            labels: vec!["Person".to_string()],
                            properties: None,
                        })],
                    },
                    where_clause: None,
                    optional: false,
                }),
                Clause::Return(ReturnClause {
                    items: vec![ReturnItem {
                        expression: Expression::Variable("n".to_string()),
                        alias: None,
                    }],
                    distinct: false,
                }),
            ],
            params: std::collections::HashMap::new(),
        };

        let operators = planner.plan_query(&query).unwrap();
        assert_eq!(operators.len(), 2);

        match &operators[0] {
            Operator::NodeByLabel { variable, .. } => {
                assert_eq!(variable, "n");
            }
            _ => panic!("Expected NodeByLabel operator"),
        }

        match &operators[1] {
            Operator::Project { items } => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].alias, "n");
            }
            _ => panic!("Expected Project operator"),
        }
    }

    #[test]
    fn test_estimate_cost() {
        let catalog = Catalog::new(tempfile::tempdir().unwrap()).unwrap();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new(128).unwrap();
        let planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

        let operators = vec![
            Operator::NodeByLabel {
                label_id: 1,
                variable: "n".to_string(),
            },
            Operator::Filter {
                predicate: "n.age > 18".to_string(),
            },
            Operator::Project {
                items: vec![ProjectionItem {
                    alias: "n".to_string(),
                    expression: Expression::Variable("n".to_string()),
                }],
            },
        ];

        let cost = planner.estimate_cost(&operators).unwrap();
        assert!(cost > 0.0);
    }

    #[test]
    fn test_plan_query_with_where_clause() {
        let catalog = Catalog::new(tempfile::tempdir().unwrap()).unwrap();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new(128).unwrap();
        let planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

        let query = CypherQuery {
            clauses: vec![
                Clause::Match(MatchClause {
                    pattern: Pattern {
                        elements: vec![PatternElement::Node(NodePattern {
                            variable: Some("n".to_string()),
                            labels: vec!["Person".to_string()],
                            properties: None,
                        })],
                    },
                    where_clause: Some(WhereClause {
                        expression: Expression::BinaryOp {
                            left: Box::new(Expression::PropertyAccess {
                                variable: "n".to_string(),
                                property: "age".to_string(),
                            }),
                            op: BinaryOperator::GreaterThan,
                            right: Box::new(Expression::Literal(Literal::Integer(18))),
                        },
                    }),
                    optional: false,
                }),
                Clause::Return(ReturnClause {
                    items: vec![ReturnItem {
                        expression: Expression::Variable("n".to_string()),
                        alias: None,
                    }],
                    distinct: false,
                }),
            ],
            params: std::collections::HashMap::new(),
        };

        let operators = planner.plan_query(&query).unwrap();
        assert_eq!(operators.len(), 3); // NodeByLabel, Filter, Project

        match &operators[0] {
            Operator::NodeByLabel { variable, .. } => {
                assert_eq!(variable, "n");
            }
            _ => panic!("Expected NodeByLabel operator"),
        }

        match &operators[1] {
            Operator::Filter { predicate } => {
                assert!(predicate.contains("n.age"));
                assert!(predicate.contains(">"));
                assert!(predicate.contains("18"));
            }
            _ => panic!("Expected Filter operator"),
        }

        match &operators[2] {
            Operator::Project { items } => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].alias, "n");
            }
            _ => panic!("Expected Project operator"),
        }
    }

    #[test]
    fn test_plan_query_with_limit() {
        let catalog = Catalog::new(tempfile::tempdir().unwrap()).unwrap();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new(128).unwrap();
        let planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

        let query = CypherQuery {
            clauses: vec![
                Clause::Match(MatchClause {
                    pattern: Pattern {
                        elements: vec![PatternElement::Node(NodePattern {
                            variable: Some("n".to_string()),
                            labels: vec!["Person".to_string()],
                            properties: None,
                        })],
                    },
                    where_clause: None,
                    optional: false,
                }),
                Clause::Return(ReturnClause {
                    items: vec![ReturnItem {
                        expression: Expression::Variable("n".to_string()),
                        alias: None,
                    }],
                    distinct: false,
                }),
                Clause::Limit(LimitClause {
                    count: Expression::Literal(Literal::Integer(10)),
                }),
            ],
            params: std::collections::HashMap::new(),
        };

        let operators = planner.plan_query(&query).unwrap();
        assert_eq!(operators.len(), 3); // NodeByLabel, Project, Limit

        match &operators[2] {
            Operator::Limit { count } => {
                assert_eq!(*count, 10);
            }
            _ => panic!("Expected Limit operator"),
        }
    }

    #[test]
    fn test_plan_query_with_relationship() {
        let catalog = Catalog::new(tempfile::tempdir().unwrap()).unwrap();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new(128).unwrap();
        let planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

        let query = CypherQuery {
            clauses: vec![
                Clause::Match(MatchClause {
                    pattern: Pattern {
                        elements: vec![
                            PatternElement::Node(NodePattern {
                                variable: Some("a".to_string()),
                                labels: vec!["Person".to_string()],
                                properties: None,
                            }),
                            PatternElement::Relationship(RelationshipPattern {
                                variable: Some("r".to_string()),
                                types: vec!["KNOWS".to_string()],
                                direction: RelationshipDirection::Outgoing,
                                properties: None,
                                quantifier: None,
                            }),
                            PatternElement::Node(NodePattern {
                                variable: Some("b".to_string()),
                                labels: vec!["Person".to_string()],
                                properties: None,
                            }),
                        ],
                    },
                    where_clause: None,
                    optional: false,
                }),
                Clause::Return(ReturnClause {
                    items: vec![ReturnItem {
                        expression: Expression::Variable("a".to_string()),
                        alias: None,
                    }],
                    distinct: false,
                }),
            ],
            params: std::collections::HashMap::new(),
        };

        let operators = planner.plan_query(&query).unwrap();
        assert!(operators.len() >= 2); // At least NodeByLabel and Project

        // Check for Expand operator
        let has_expand = operators
            .iter()
            .any(|op| matches!(op, Operator::Expand { .. }));
        assert!(has_expand, "Expected Expand operator for relationship");
    }

    #[test]
    fn test_plan_query_empty_patterns() {
        let catalog = Catalog::new(tempfile::tempdir().unwrap()).unwrap();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new(128).unwrap();
        let planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

        let query = CypherQuery {
            clauses: vec![],
            params: std::collections::HashMap::new(),
        };

        let result = planner.plan_query(&query);
        assert!(result.is_err());
    }

    #[test]
    fn test_expression_to_string_variable() {
        let catalog = Catalog::new(tempfile::tempdir().unwrap()).unwrap();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new(128).unwrap();
        let planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

        let expr = Expression::Variable("test_var".to_string());
        let result = planner.expression_to_string(&expr).unwrap();
        assert_eq!(result, "test_var");
    }

    #[test]
    fn test_expression_to_string_property_access() {
        let catalog = Catalog::new(tempfile::tempdir().unwrap()).unwrap();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new(128).unwrap();
        let planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

        let expr = Expression::PropertyAccess {
            variable: "n".to_string(),
            property: "age".to_string(),
        };
        let result = planner.expression_to_string(&expr).unwrap();
        assert_eq!(result, "n.age");
    }

    #[test]
    fn test_expression_to_string_literals() {
        let catalog = Catalog::new(tempfile::tempdir().unwrap()).unwrap();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new(128).unwrap();
        let planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

        // Test string literal
        let expr = Expression::Literal(Literal::String("hello".to_string()));
        let result = planner.expression_to_string(&expr).unwrap();
        assert_eq!(result, "\"hello\"");

        // Test integer literal
        let expr = Expression::Literal(Literal::Integer(42));
        let result = planner.expression_to_string(&expr).unwrap();
        assert_eq!(result, "42");

        // Test float literal
        let expr = Expression::Literal(Literal::Float(std::f64::consts::PI));
        let result = planner.expression_to_string(&expr).unwrap();
        assert_eq!(result, "3.141592653589793");

        // Test boolean literal
        let expr = Expression::Literal(Literal::Boolean(true));
        let result = planner.expression_to_string(&expr).unwrap();
        assert_eq!(result, "true");

        // Test null literal
        let expr = Expression::Literal(Literal::Null);
        let result = planner.expression_to_string(&expr).unwrap();
        assert_eq!(result, "NULL");
    }

    #[test]
    fn test_expression_to_string_binary_operators() {
        let catalog = Catalog::new(tempfile::tempdir().unwrap()).unwrap();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new(128).unwrap();
        let planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

        let expr = Expression::BinaryOp {
            left: Box::new(Expression::Variable("a".to_string())),
            op: BinaryOperator::Equal,
            right: Box::new(Expression::Variable("b".to_string())),
        };
        let result = planner.expression_to_string(&expr).unwrap();
        assert_eq!(result, "a = b");

        let expr = Expression::BinaryOp {
            left: Box::new(Expression::Variable("x".to_string())),
            op: BinaryOperator::GreaterThan,
            right: Box::new(Expression::Literal(Literal::Integer(10))),
        };
        let result = planner.expression_to_string(&expr).unwrap();
        assert_eq!(result, "x > 10");
    }

    #[test]
    fn test_expression_to_string_parameter() {
        let catalog = Catalog::new(tempfile::tempdir().unwrap()).unwrap();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new(128).unwrap();
        let planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

        let expr = Expression::Parameter("param1".to_string());
        let result = planner.expression_to_string(&expr).unwrap();
        assert_eq!(result, "$param1");
    }

    #[test]
    fn test_estimate_cost_all_operators() {
        let catalog = Catalog::new(tempfile::tempdir().unwrap()).unwrap();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new(128).unwrap();
        let planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

        let operators = vec![
            Operator::NodeByLabel {
                label_id: 1,
                variable: "n".to_string(),
            },
            Operator::Filter {
                predicate: "n.age > 18".to_string(),
            },
            Operator::Expand {
                type_id: Some(1),
                source_var: "n".to_string(),
                target_var: "m".to_string(),
                rel_var: "r".to_string(),
                direction: Direction::Outgoing,
            },
            Operator::Project {
                items: vec![ProjectionItem {
                    alias: "n".to_string(),
                    expression: Expression::Variable("n".to_string()),
                }],
            },
            Operator::Limit { count: 10 },
            Operator::Sort {
                columns: vec!["n.name".to_string()],
                ascending: vec![true],
            },
            Operator::Aggregate {
                group_by: vec!["n".to_string()],
                aggregations: vec![],
            },
            Operator::Union {
                left: vec![Operator::NodeByLabel {
                    label_id: 1,
                    variable: "a".to_string(),
                }],
                right: vec![Operator::NodeByLabel {
                    label_id: 2,
                    variable: "b".to_string(),
                }],
                distinct: true,
            },
            Operator::Join {
                left: Box::new(Operator::NodeByLabel {
                    label_id: 1,
                    variable: "a".to_string(),
                }),
                right: Box::new(Operator::NodeByLabel {
                    label_id: 2,
                    variable: "b".to_string(),
                }),
                join_type: JoinType::Inner,
                condition: Some("a.id = b.id".to_string()),
            },
            Operator::IndexScan {
                index_name: "label_Person".to_string(),
                label: "Person".to_string(),
            },
            Operator::Distinct {
                columns: vec!["n".to_string()],
            },
        ];

        let cost = planner.estimate_cost(&operators).unwrap();
        assert!(cost > 0.0);
        // Should be substantial with all operators (adjusted threshold)
        assert!(cost > 100.0);
    }

    #[test]
    fn test_optimize_operator_order() {
        let catalog = Catalog::new(tempfile::tempdir().unwrap()).unwrap();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new(128).unwrap();
        let planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

        let operators = vec![
            Operator::NodeByLabel {
                label_id: 1,
                variable: "n".to_string(),
            },
            Operator::Filter {
                predicate: "n.age > 18".to_string(),
            },
        ];

        let optimized = planner.optimize_operator_order(operators.clone()).unwrap();
        assert_eq!(optimized.len(), operators.len());
        // For MVP, should return same order
        // For MVP, should return same order
        assert_eq!(optimized.len(), operators.len());
    }

    #[test]
    fn test_plan_query_with_return_alias() {
        let catalog = Catalog::new(tempfile::tempdir().unwrap()).unwrap();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new(128).unwrap();
        let planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

        let query = CypherQuery {
            clauses: vec![
                Clause::Match(MatchClause {
                    pattern: Pattern {
                        elements: vec![PatternElement::Node(NodePattern {
                            variable: Some("n".to_string()),
                            labels: vec!["Person".to_string()],
                            properties: None,
                        })],
                    },
                    where_clause: None,
                    optional: false,
                }),
                Clause::Return(ReturnClause {
                    items: vec![ReturnItem {
                        expression: Expression::Variable("n".to_string()),
                        alias: Some("person".to_string()),
                    }],
                    distinct: false,
                }),
            ],
            params: std::collections::HashMap::new(),
        };

        let operators = planner.plan_query(&query).unwrap();
        assert_eq!(operators.len(), 2);

        match &operators[1] {
            Operator::Project { items } => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].alias, "person");
            }
            _ => panic!("Expected Project operator with alias"),
        }
    }
}
