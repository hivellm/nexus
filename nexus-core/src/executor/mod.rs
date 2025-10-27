//! Cypher executor - Pattern matching, expand, filter, project
//!
//! Physical operators:
//! - NodeByLabel(label) → scan bitmap
//! - FilterProps(predicate) → apply in batch
//! - Expand(type, direction) → use linked lists (next_src_ptr/next_dst_ptr)
//! - Project, Aggregate, Order, Limit
//!
//! Heuristic cost-based planning:
//! - Statistics per label (|V|), per type (|E|), average degree
//! - Reorder patterns for selectivity

/// Query optimizer for cost-based optimization
pub mod optimizer;
pub mod parser;
/// Query planner for optimizing Cypher execution
pub mod planner;

use crate::catalog::Catalog;
use crate::index::{KnnIndex, LabelIndex};
use crate::storage::RecordStore;
use crate::{Error, Result};
use planner::QueryPlanner;
use serde_json::{Map, Value};
use std::collections::HashMap;

/// Cypher query
#[derive(Debug, Clone)]
pub struct Query {
    /// Query string
    pub cypher: String,
    /// Query parameters
    pub params: HashMap<String, Value>,
}

/// Query result row
#[derive(Debug, Clone)]
pub struct Row {
    /// Column values
    pub values: Vec<serde_json::Value>,
}

/// Query result set
#[derive(Debug, Clone)]
pub struct ResultSet {
    /// Column names
    pub columns: Vec<String>,
    /// Result rows
    pub rows: Vec<Row>,
}

/// Execution plan containing a sequence of operators
#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    /// Sequence of operators to execute
    pub operators: Vec<Operator>,
}

/// Physical operator
#[derive(Debug, Clone)]
pub enum Operator {
    /// Scan nodes by label
    NodeByLabel {
        /// Label ID
        label_id: u32,
        /// Variable name
        variable: String,
    },
    /// Filter by property predicate
    Filter {
        /// Predicate expression
        predicate: String,
    },
    /// Expand relationships
    Expand {
        /// Type ID (None = all types)
        type_id: Option<u32>,
        /// Direction (Outgoing, Incoming, Both)
        direction: Direction,
        /// Source variable
        source_var: String,
        /// Target variable
        target_var: String,
        /// Relationship variable
        rel_var: String,
    },
    /// Project columns
    Project {
        /// Column expressions
        columns: Vec<String>,
    },
    /// Limit results
    Limit {
        /// Maximum rows
        count: usize,
    },
    /// Sort results by columns
    Sort {
        /// Columns to sort by
        columns: Vec<String>,
        /// Sort order (true = ascending, false = descending)
        ascending: Vec<bool>,
    },
    /// Aggregate results
    Aggregate {
        /// Group by columns
        group_by: Vec<String>,
        /// Aggregation functions
        aggregations: Vec<Aggregation>,
    },
    /// Union two result sets
    Union {
        /// Left operand
        left: Box<Operator>,
        /// Right operand
        right: Box<Operator>,
    },
    /// Join two result sets
    Join {
        /// Left operand
        left: Box<Operator>,
        /// Right operand
        right: Box<Operator>,
        /// Join type
        join_type: JoinType,
        /// Join condition
        condition: Option<String>,
    },
    /// Scan using index
    IndexScan {
        /// Index name
        index_name: String,
        /// Label to scan
        label: String,
    },
    /// Distinct results
    Distinct {
        /// Columns to check for distinctness
        columns: Vec<String>,
    },
    /// Hash join operation
    HashJoin {
        /// Left join key
        left_key: String,
        /// Right join key
        right_key: String,
    },
}

/// Relationship direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Outgoing edges
    Outgoing,
    /// Incoming edges
    Incoming,
    /// Both directions
    Both,
}

/// Aggregation function
#[derive(Debug, Clone)]
pub enum Aggregation {
    /// Count rows
    Count {
        /// Column to count (None = count all)
        column: Option<String>,
        /// Alias for result
        alias: String,
    },
    /// Sum values
    Sum {
        /// Column to sum
        column: String,
        /// Alias for result
        alias: String,
    },
    /// Average values
    Avg {
        /// Column to average
        column: String,
        /// Alias for result
        alias: String,
    },
    /// Minimum value
    Min {
        /// Column to find minimum
        column: String,
        /// Alias for result
        alias: String,
    },
    /// Maximum value
    Max {
        /// Column to find maximum
        column: String,
        /// Alias for result
        alias: String,
    },
}

/// Join type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinType {
    /// Inner join
    Inner,
    /// Left outer join
    LeftOuter,
    /// Right outer join
    RightOuter,
    /// Full outer join
    FullOuter,
}

/// Index type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexType {
    /// Label index
    Label,
    /// Property index
    Property,
    /// KNN vector index
    Vector,
    /// Full-text index
    FullText,
}

/// Query executor
pub struct Executor {
    /// Catalog for label/type lookups
    catalog: Catalog,
    /// Record store for data access
    store: RecordStore,
    /// Label index for fast label scans
    label_index: LabelIndex,
    /// KNN index for vector operations
    knn_index: KnnIndex,
}

impl Executor {
    /// Create a new executor
    pub fn new(
        catalog: &Catalog,
        store: &RecordStore,
        label_index: &LabelIndex,
        knn_index: &KnnIndex,
    ) -> Result<Self> {
        Ok(Self {
            catalog: catalog.clone(),
            store: store.clone(),
            label_index: label_index.clone(),
            knn_index: knn_index.clone(),
        })
    }

    /// Execute a Cypher query
    pub fn execute(&mut self, query: &Query) -> Result<ResultSet> {
        // Parse the query into operators
        let operators = self.parse_and_plan(&query.cypher)?;

        // Execute the plan
        let mut context = ExecutionContext::new(query.params.clone());
        let mut results = Vec::new();

        for operator in operators {
            match operator {
                Operator::NodeByLabel { label_id, variable } => {
                    let nodes = self.execute_node_by_label(label_id)?;
                    context.set_variable(&variable, Value::Array(nodes));
                }
                Operator::Filter { predicate } => {
                    self.execute_filter(&mut context, &predicate)?;
                }
                Operator::Expand {
                    type_id,
                    direction,
                    source_var,
                    target_var,
                    rel_var,
                } => {
                    self.execute_expand(
                        &mut context,
                        type_id,
                        direction,
                        &source_var,
                        &target_var,
                        &rel_var,
                    )?;
                }
                Operator::Project { columns } => {
                    results = self.execute_project(&context, &columns)?;
                }
                Operator::Limit { count } => {
                    results.truncate(count);
                }
                Operator::Sort { columns, ascending } => {
                    self.execute_sort(&mut context, &columns, &ascending)?;
                }
                Operator::Aggregate {
                    group_by,
                    aggregations,
                } => {
                    self.execute_aggregate(&mut context, &group_by, &aggregations)?;
                }
                Operator::Union { left, right } => {
                    self.execute_union(&mut context, &left, &right)?;
                }
                Operator::Join {
                    left,
                    right,
                    join_type,
                    condition,
                } => {
                    self.execute_join(
                        &mut context,
                        &left,
                        &right,
                        join_type,
                        condition.as_deref(),
                    )?;
                }
                Operator::IndexScan { index_name, label } => {
                    self.execute_index_scan_new(&mut context, &index_name, &label)?;
                }
                Operator::Distinct { columns } => {
                    self.execute_distinct(&mut context, &columns)?;
                }
                Operator::HashJoin {
                    left_key,
                    right_key,
                } => {
                    self.execute_hash_join(&mut context, &left_key, &right_key)?;
                }
            }
        }

        Ok(ResultSet {
            columns: vec!["n".to_string()], // Simple MVP - just return nodes
            rows: results,
        })
    }

    /// Parse Cypher into physical plan
    pub fn parse_and_plan(&self, cypher: &str) -> Result<Vec<Operator>> {
        // Use the parser to parse the query
        let mut parser = parser::CypherParser::new(cypher.to_string());
        let ast = parser.parse()?;

        // Use the planner to create an optimized plan
        let planner = QueryPlanner::new(&self.catalog, &self.label_index, &self.knn_index);

        let mut operators = planner.plan_query(&ast)?;

        // Optimize the operator order
        operators = planner.optimize_operator_order(operators)?;

        Ok(operators)
    }

    /// Convert AST to physical operators
    fn ast_to_operators(&self, ast: &parser::CypherQuery) -> Result<Vec<Operator>> {
        let mut operators = Vec::new();

        for clause in &ast.clauses {
            match clause {
                parser::Clause::Match(match_clause) => {
                    // Add NodeByLabel operators for each node pattern
                    for element in &match_clause.pattern.elements {
                        if let parser::PatternElement::Node(node) = element {
                            if let Some(variable) = &node.variable {
                                if let Some(label) = node.labels.first() {
                                    let label_id = self.catalog.get_or_create_label(label)?;
                                    operators.push(Operator::NodeByLabel {
                                        label_id,
                                        variable: variable.clone(),
                                    });
                                }
                            }
                        }
                    }

                    // Add WHERE clause as Filter operator
                    if let Some(where_clause) = &match_clause.where_clause {
                        operators.push(Operator::Filter {
                            predicate: self.expression_to_string(&where_clause.expression)?,
                        });
                    }
                }
                parser::Clause::Create(create_clause) => {
                    // CREATE: create nodes and relationships from pattern
                    self.execute_create_pattern(&create_clause.pattern)?;
                }
                parser::Clause::Merge(merge_clause) => {
                    // MERGE: match-or-create pattern
                    // For now, treat as MATCH - executor will handle match-or-create logic
                    for element in &merge_clause.pattern.elements {
                        if let parser::PatternElement::Node(node) = element {
                            if let Some(variable) = &node.variable {
                                if let Some(label) = node.labels.first() {
                                    let label_id = self.catalog.get_or_create_label(label)?;
                                    operators.push(Operator::NodeByLabel {
                                        label_id,
                                        variable: variable.clone(),
                                    });
                                }
                            }
                        }
                    }
                }
                parser::Clause::Where(where_clause) => {
                    operators.push(Operator::Filter {
                        predicate: self.expression_to_string(&where_clause.expression)?,
                    });
                }
                parser::Clause::Return(return_clause) => {
                    let columns: Vec<String> = return_clause
                        .items
                        .iter()
                        .map(|item| {
                            if let Some(alias) = &item.alias {
                                alias.clone()
                            } else {
                                self.expression_to_string(&item.expression)
                                    .unwrap_or_default()
                            }
                        })
                        .collect();

                    operators.push(Operator::Project { columns });
                }
                parser::Clause::Limit(limit_clause) => {
                    if let parser::Expression::Literal(parser::Literal::Integer(count)) =
                        &limit_clause.count
                    {
                        operators.push(Operator::Limit {
                            count: *count as usize,
                        });
                    }
                }
                _ => {
                    // Other clauses not implemented in MVP
                }
            }
        }

        Ok(operators)
    }

    /// Execute CREATE pattern to create nodes and relationships
    fn execute_create_pattern(&self, _pattern: &parser::Pattern) -> Result<()> {
        // For now, CREATE is not fully implemented in this MVP executor
        // It requires transaction support which is not yet integrated here
        // The parser works correctly, but execution is deferred to future implementation
        Ok(())
    }

    /// Convert expression to JSON value
    fn expression_to_json_value(&self, expr: &parser::Expression) -> Result<Value> {
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
            },
            parser::Expression::Variable(_) => Err(Error::CypherExecution(
                "Variables not supported in CREATE properties".to_string(),
            )),
            _ => Err(Error::CypherExecution(
                "Complex expressions not supported in CREATE properties".to_string(),
            )),
        }
    }

    /// Convert expression to string representation
    fn expression_to_string(&self, expr: &parser::Expression) -> Result<String> {
        match expr {
            parser::Expression::Variable(name) => Ok(name.clone()),
            parser::Expression::PropertyAccess { variable, property } => {
                Ok(format!("{}.{}", variable, property))
            }
            parser::Expression::Literal(literal) => match literal {
                parser::Literal::String(s) => Ok(format!("\"{}\"", s)),
                parser::Literal::Integer(i) => Ok(i.to_string()),
                parser::Literal::Float(f) => Ok(f.to_string()),
                parser::Literal::Boolean(b) => Ok(b.to_string()),
                parser::Literal::Null => Ok("NULL".to_string()),
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
                    _ => "?",
                };
                Ok(format!("{} {} {}", left_str, op_str, right_str))
            }
            parser::Expression::Parameter(name) => Ok(format!("${}", name)),
            _ => Ok("?".to_string()),
        }
    }

    /// Execute NodeByLabel operator
    fn execute_node_by_label(&self, label_id: u32) -> Result<Vec<Value>> {
        let bitmap = self.label_index.get_nodes(label_id)?;
        let mut results = Vec::new();

        for node_id in bitmap.iter() {
            let _node_record = self.store.read_node(node_id as u64)?;

            // Create node representation
            let mut node = Map::new();
            node.insert("id".to_string(), Value::Number((node_id as u64).into()));
            node.insert(
                "labels".to_string(),
                Value::Array(vec![Value::String(format!("label_{}", label_id))]),
            );
            node.insert("properties".to_string(), Value::Object(Map::new()));

            results.push(Value::Object(node));
        }

        Ok(results)
    }

    /// Execute Filter operator
    fn execute_filter(&self, context: &mut ExecutionContext, predicate: &str) -> Result<()> {
        // Parse the predicate expression
        let mut parser = parser::CypherParser::new(predicate.to_string());
        let expr = parser.parse_expression()?;

        // Apply filter to all variables
        let mut filtered_variables = HashMap::new();

        for (var_name, value) in &context.variables {
            if let Value::Array(nodes) = value {
                let mut filtered_nodes = Vec::new();

                for node in nodes {
                    if self.evaluate_predicate(node, &expr, context)? {
                        filtered_nodes.push(node.clone());
                    }
                }

                filtered_variables.insert(var_name.clone(), Value::Array(filtered_nodes));
            } else {
                filtered_variables.insert(var_name.clone(), value.clone());
            }
        }

        context.variables = filtered_variables;
        Ok(())
    }

    /// Execute Expand operator
    fn execute_expand(
        &self,
        context: &mut ExecutionContext,
        type_id: Option<u32>,
        direction: Direction,
        source_var: &str,
        target_var: &str,
        rel_var: &str,
    ) -> Result<()> {
        // Get source nodes from context
        let source_nodes = if let Some(Value::Array(nodes)) = context.get_variable(source_var) {
            nodes.clone()
        } else {
            return Ok(());
        };

        let mut expanded_results = Vec::new();
        let mut relationships = Vec::new();

        for source_node in source_nodes {
            if let Value::Object(node_obj) = &source_node {
                if let Some(Value::Number(node_id)) = node_obj.get("id") {
                    let node_id = node_id.as_u64().unwrap_or(0);

                    // Find relationships for this node
                    let rels = self.find_relationships(node_id, type_id, direction)?;

                    for rel in rels {
                        // Get target node
                        let target_id = match direction {
                            Direction::Outgoing => rel.target_id,
                            Direction::Incoming => rel.source_id,
                            Direction::Both => {
                                // For both directions, we need to determine which is the target
                                if rel.source_id == node_id {
                                    rel.target_id
                                } else {
                                    rel.source_id
                                }
                            }
                        };

                        if let Ok(target_node) = self.read_node_as_value(target_id) {
                            // Create relationship object
                            let mut rel_obj = Map::new();
                            rel_obj.insert("id".to_string(), Value::Number(rel.id.into()));
                            rel_obj.insert(
                                "type".to_string(),
                                Value::String(format!("type_{}", rel.type_id)),
                            );
                            rel_obj.insert(
                                "source_id".to_string(),
                                Value::Number(rel.source_id.into()),
                            );
                            rel_obj.insert(
                                "target_id".to_string(),
                                Value::Number(rel.target_id.into()),
                            );
                            rel_obj.insert("properties".to_string(), Value::Object(Map::new()));

                            relationships.push(Value::Object(rel_obj));
                            expanded_results.push(target_node);
                        }
                    }
                }
            }
        }

        // Update context with expanded results
        context.set_variable(target_var, Value::Array(expanded_results));
        if !rel_var.is_empty() {
            context.set_variable(rel_var, Value::Array(relationships));
        }

        Ok(())
    }

    /// Execute Project operator
    fn execute_project(&self, context: &ExecutionContext, _columns: &[String]) -> Result<Vec<Row>> {
        // MVP: Simple projection - return all variables
        let mut rows = Vec::new();

        for value in context.variables.values() {
            if let Value::Array(nodes) = value {
                for node in nodes {
                    rows.push(Row {
                        values: vec![node.clone()],
                    });
                }
            }
        }

        Ok(rows)
    }

    /// Execute Limit operator
    fn execute_limit(&self, context: &mut ExecutionContext, count: usize) -> Result<()> {
        // Limit the number of rows in the result set
        if context.result_set.rows.len() > count {
            context.result_set.rows.truncate(count);
        }

        // Also limit any array variables that might be used for further processing
        for value in context.variables.values_mut() {
            if let Value::Array(nodes) = value {
                if nodes.len() > count {
                    nodes.truncate(count);
                }
            }
        }

        Ok(())
    }

    /// Execute Sort operator
    fn execute_sort(
        &self,
        context: &mut ExecutionContext,
        columns: &[String],
        ascending: &[bool],
    ) -> Result<()> {
        // Get all variables that need to be sorted
        let mut sort_data = Vec::new();

        for (var_name, value) in &context.variables {
            if let Value::Array(nodes) = value {
                for (idx, node) in nodes.iter().enumerate() {
                    sort_data.push((idx, var_name.clone(), node.clone()));
                }
            }
        }

        // Sort based on the specified columns
        sort_data.sort_by(|a, b| {
            for (i, column) in columns.iter().enumerate() {
                let ascending = i < ascending.len() && ascending[i];

                let a_val = self.get_column_value(&a.2, column);
                let b_val = self.get_column_value(&b.2, column);

                let comparison = self.compare_values_for_sort(&a_val, &b_val);
                if comparison != std::cmp::Ordering::Equal {
                    return if ascending {
                        comparison
                    } else {
                        comparison.reverse()
                    };
                }
            }
            std::cmp::Ordering::Equal
        });

        // Reorganize variables based on sorted order
        let mut sorted_variables: HashMap<String, Vec<Value>> = HashMap::new();

        for (_, var_name, node) in sort_data {
            sorted_variables.entry(var_name).or_default().push(node);
        }

        // Update context with sorted variables
        for (var_name, nodes) in sorted_variables {
            context.set_variable(&var_name, Value::Array(nodes));
        }

        Ok(())
    }

    /// Execute Aggregate operator
    fn execute_aggregate(
        &self,
        context: &mut ExecutionContext,
        group_by: &[String],
        aggregations: &[Aggregation],
    ) -> Result<()> {
        use std::collections::HashMap;

        // Group rows by group_by columns
        // Collect rows first to avoid borrow checker issues
        let rows = context.result_set.rows.clone();
        let mut groups: HashMap<Vec<Value>, Vec<Row>> = HashMap::new();

        // Process each row
        for row in rows {
            // Extract group key
            let mut group_key = Vec::new();
            for col in group_by {
                if let Some(value) = context.result_set.columns.iter().position(|c| c == col) {
                    if value < row.values.len() {
                        group_key.push(row.values[value].clone());
                    } else {
                        group_key.push(Value::Null);
                    }
                } else {
                    group_key.push(Value::Null);
                }
            }

            // Add row to group
            groups.entry(group_key).or_default().push(row);
        }

        // Clear current results
        context.result_set.rows.clear();

        // Process each group
        for (group_key, group_rows) in groups {
            let mut result_row = group_key;

            // Apply aggregations
            for agg in aggregations {
                let agg_value = match agg {
                    Aggregation::Count { column, alias: _ } => {
                        if column.is_none() {
                            // COUNT(*)
                            Value::Number(serde_json::Number::from(group_rows.len()))
                        } else {
                            // COUNT(column) - count non-null values
                            let col_name = column.as_ref().unwrap();
                            let col_idx = context
                                .result_set
                                .columns
                                .iter()
                                .position(|c| c == col_name.as_str());
                            let count = if let Some(idx) = col_idx {
                                group_rows
                                    .iter()
                                    .filter(|row| {
                                        idx < row.values.len() && !row.values[idx].is_null()
                                    })
                                    .count()
                            } else {
                                0
                            };
                            Value::Number(serde_json::Number::from(count))
                        }
                    }
                    Aggregation::Sum { column, alias: _ } => {
                        let col_name = &column;
                        let col_idx = context
                            .result_set
                            .columns
                            .iter()
                            .position(|c| c == col_name.as_str());
                        if let Some(idx) = col_idx {
                            let sum: f64 = group_rows
                                .iter()
                                .filter_map(|row| {
                                    if idx < row.values.len() {
                                        self.value_to_number(&row.values[idx]).ok()
                                    } else {
                                        None
                                    }
                                })
                                .sum();
                            Value::Number(
                                serde_json::Number::from_f64(sum)
                                    .unwrap_or(serde_json::Number::from(0)),
                            )
                        } else {
                            Value::Number(serde_json::Number::from(0))
                        }
                    }
                    Aggregation::Avg { column, alias: _ } => {
                        let col_name = &column;
                        let col_idx = context
                            .result_set
                            .columns
                            .iter()
                            .position(|c| c == col_name.as_str());
                        if let Some(idx) = col_idx {
                            let values: Vec<f64> = group_rows
                                .iter()
                                .filter_map(|row| {
                                    if idx < row.values.len() {
                                        self.value_to_number(&row.values[idx]).ok()
                                    } else {
                                        None
                                    }
                                })
                                .collect();
                            if values.is_empty() {
                                Value::Null
                            } else {
                                let avg = values.iter().sum::<f64>() / values.len() as f64;
                                Value::Number(
                                    serde_json::Number::from_f64(avg)
                                        .unwrap_or(serde_json::Number::from(0)),
                                )
                            }
                        } else {
                            Value::Null
                        }
                    }
                    Aggregation::Min { column, alias: _ } => {
                        let col_name = &column;
                        let col_idx = context
                            .result_set
                            .columns
                            .iter()
                            .position(|c| c == col_name.as_str());
                        if let Some(idx) = col_idx {
                            let min_value = group_rows
                                .iter()
                                .filter_map(|row| {
                                    if idx < row.values.len() {
                                        self.value_to_number(&row.values[idx]).ok()
                                    } else {
                                        None
                                    }
                                })
                                .fold(f64::INFINITY, |a, b| a.min(b));
                            if min_value == f64::INFINITY {
                                Value::Null
                            } else {
                                Value::Number(
                                    serde_json::Number::from_f64(min_value)
                                        .unwrap_or(serde_json::Number::from(0)),
                                )
                            }
                        } else {
                            Value::Null
                        }
                    }
                    Aggregation::Max { column, alias: _ } => {
                        let col_name = &column;
                        let col_idx = context
                            .result_set
                            .columns
                            .iter()
                            .position(|c| c == col_name.as_str());
                        if let Some(idx) = col_idx {
                            let max_value = group_rows
                                .iter()
                                .filter_map(|row| {
                                    if idx < row.values.len() {
                                        self.value_to_number(&row.values[idx]).ok()
                                    } else {
                                        None
                                    }
                                })
                                .fold(f64::NEG_INFINITY, |a, b| a.max(b));
                            if max_value == f64::NEG_INFINITY {
                                Value::Null
                            } else {
                                Value::Number(
                                    serde_json::Number::from_f64(max_value)
                                        .unwrap_or(serde_json::Number::from(0)),
                                )
                            }
                        } else {
                            Value::Null
                        }
                    }
                };
                result_row.push(agg_value);
            }

            context.result_set.rows.push(Row { values: result_row });
        }

        Ok(())
    }

    /// Execute Union operator
    fn execute_union(
        &self,
        context: &mut ExecutionContext,
        left: &Operator,
        right: &Operator,
    ) -> Result<()> {
        // Execute left operator and collect its results
        let mut left_context = ExecutionContext::new(context.params.clone());
        self.execute_operator(&mut left_context, left)?;

        // Execute right operator and collect its results
        let mut right_context = ExecutionContext::new(context.params.clone());
        self.execute_operator(&mut right_context, right)?;

        // Combine results from both sides
        let mut combined_rows = Vec::new();
        combined_rows.extend(left_context.result_set.rows);
        combined_rows.extend(right_context.result_set.rows);

        // Update the main context with combined results
        context.result_set.rows = combined_rows;

        // Ensure columns are consistent (use left side columns as base)
        if !left_context.result_set.columns.is_empty() {
            context.result_set.columns = left_context.result_set.columns.clone();
        } else if !right_context.result_set.columns.is_empty() {
            context.result_set.columns = right_context.result_set.columns.clone();
        }

        Ok(())
    }

    /// Execute a single operator and return results
    fn execute_operator(&self, context: &mut ExecutionContext, operator: &Operator) -> Result<()> {
        match operator {
            Operator::NodeByLabel { label_id, variable } => {
                let nodes = self.execute_node_by_label(*label_id)?;
                context.set_variable(variable, Value::Array(nodes));
            }
            Operator::Filter { predicate } => {
                self.execute_filter(context, predicate)?;
            }
            Operator::Expand {
                type_id,
                direction,
                source_var,
                target_var,
                rel_var,
            } => {
                self.execute_expand(
                    context, *type_id, *direction, source_var, target_var, rel_var,
                )?;
            }
            Operator::Project { columns } => {
                let rows = self.execute_project(context, columns)?;
                context.result_set.rows = rows;
                context.result_set.columns = columns.clone();
            }
            Operator::Limit { count } => {
                self.execute_limit(context, *count)?;
            }
            Operator::Sort { columns, ascending } => {
                self.execute_sort(context, columns, ascending)?;
            }
            Operator::Aggregate {
                group_by,
                aggregations,
            } => {
                self.execute_aggregate(context, group_by, aggregations)?;
            }
            Operator::Union { left, right } => {
                self.execute_union(context, left, right)?;
            }
            Operator::Join {
                left,
                right,
                join_type,
                condition,
            } => {
                self.execute_join(context, left, right, *join_type, condition.as_deref())?;
            }
            Operator::IndexScan { index_name, label } => {
                self.execute_index_scan_new(context, index_name, label)?;
            }
            Operator::Distinct { columns } => {
                self.execute_distinct(context, columns)?;
            }
            Operator::HashJoin {
                left_key,
                right_key,
            } => {
                self.execute_hash_join(context, left_key, right_key)?;
            }
        }
        Ok(())
    }

    /// Execute Join operator
    fn execute_join(
        &self,
        context: &mut ExecutionContext,
        left: &Operator,
        right: &Operator,
        join_type: JoinType,
        condition: Option<&str>,
    ) -> Result<()> {
        // Execute left operator and collect its results
        let mut left_context = ExecutionContext::new(context.params.clone());
        self.execute_operator(&mut left_context, left)?;

        // Execute right operator and collect its results
        let mut right_context = ExecutionContext::new(context.params.clone());
        self.execute_operator(&mut right_context, right)?;

        let mut result_rows = Vec::new();

        // Perform the join based on type
        match join_type {
            JoinType::Inner => {
                // Inner join: only rows that match in both sides
                for left_row in &left_context.result_set.rows {
                    for right_row in &right_context.result_set.rows {
                        if self.rows_match(left_row, right_row, condition)? {
                            let mut combined_row = left_row.values.clone();
                            combined_row.extend(right_row.values.clone());
                            result_rows.push(Row {
                                values: combined_row,
                            });
                        }
                    }
                }
            }
            JoinType::LeftOuter => {
                // Left outer join: all left rows, matched right rows where possible
                for left_row in &left_context.result_set.rows {
                    let mut matched = false;
                    for right_row in &right_context.result_set.rows {
                        if self.rows_match(left_row, right_row, condition)? {
                            let mut combined_row = left_row.values.clone();
                            combined_row.extend(right_row.values.clone());
                            result_rows.push(Row {
                                values: combined_row,
                            });
                            matched = true;
                        }
                    }
                    if !matched {
                        // Add left row with null values for right side
                        let mut combined_row = left_row.values.clone();
                        combined_row.extend(vec![
                            serde_json::Value::Null;
                            right_context.result_set.columns.len()
                        ]);
                        result_rows.push(Row {
                            values: combined_row,
                        });
                    }
                }
            }
            JoinType::RightOuter => {
                // Right outer join: all right rows, matched left rows where possible
                for right_row in &right_context.result_set.rows {
                    let mut matched = false;
                    for left_row in &left_context.result_set.rows {
                        if self.rows_match(left_row, right_row, condition)? {
                            let mut combined_row = left_row.values.clone();
                            combined_row.extend(right_row.values.clone());
                            result_rows.push(Row {
                                values: combined_row,
                            });
                            matched = true;
                        }
                    }
                    if !matched {
                        // Add right row with null values for left side
                        let mut combined_row =
                            vec![serde_json::Value::Null; left_context.result_set.columns.len()];
                        combined_row.extend(right_row.values.clone());
                        result_rows.push(Row {
                            values: combined_row,
                        });
                    }
                }
            }
            JoinType::FullOuter => {
                // Full outer join: all rows from both sides
                let mut left_matched = vec![false; left_context.result_set.rows.len()];
                let mut right_matched = vec![false; right_context.result_set.rows.len()];

                for (i, left_row) in left_context.result_set.rows.iter().enumerate() {
                    for (j, right_row) in right_context.result_set.rows.iter().enumerate() {
                        if self.rows_match(left_row, right_row, condition)? {
                            let mut combined_row = left_row.values.clone();
                            combined_row.extend(right_row.values.clone());
                            result_rows.push(Row {
                                values: combined_row,
                            });
                            left_matched[i] = true;
                            right_matched[j] = true;
                        }
                    }
                }

                // Add unmatched left rows
                for (i, left_row) in left_context.result_set.rows.iter().enumerate() {
                    if !left_matched[i] {
                        let mut combined_row = left_row.values.clone();
                        combined_row.extend(vec![
                            serde_json::Value::Null;
                            right_context.result_set.columns.len()
                        ]);
                        result_rows.push(Row {
                            values: combined_row,
                        });
                    }
                }

                // Add unmatched right rows
                for (j, right_row) in right_context.result_set.rows.iter().enumerate() {
                    if !right_matched[j] {
                        let mut combined_row =
                            vec![serde_json::Value::Null; left_context.result_set.columns.len()];
                        combined_row.extend(right_row.values.clone());
                        result_rows.push(Row {
                            values: combined_row,
                        });
                    }
                }
            }
        }

        // Update context with joined results
        context.result_set.rows = result_rows;

        // Combine column names
        let mut combined_columns = left_context.result_set.columns.clone();
        combined_columns.extend(right_context.result_set.columns.clone());
        context.result_set.columns = combined_columns;

        Ok(())
    }

    /// Check if two rows match based on join condition
    fn rows_match(&self, left_row: &Row, right_row: &Row, condition: Option<&str>) -> Result<bool> {
        match condition {
            Some(_cond) => {
                // For now, implement simple equality matching
                // In a full implementation, this would parse and evaluate the condition
                if left_row.values.len() != right_row.values.len() {
                    return Ok(false);
                }

                for (left_val, right_val) in left_row.values.iter().zip(right_row.values.iter()) {
                    if left_val != right_val {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            None => {
                // No condition means all rows match (Cartesian product)
                Ok(true)
            }
        }
    }

    /// Execute IndexScan operator
    fn execute_index_scan(
        &self,
        context: &mut ExecutionContext,
        index_type: IndexType,
        key: &str,
        variable: &str,
    ) -> Result<()> {
        let mut results = Vec::new();

        match index_type {
            IndexType::Label => {
                // Scan label index for nodes with the given label
                if let Ok(label_id) = self.catalog.get_or_create_label(key) {
                    let nodes = self.execute_node_by_label(label_id)?;
                    results.extend(nodes);
                }
            }
            IndexType::Property => {
                // Scan property index for nodes with the given property value
                // For now, implement a simple property lookup
                // In a full implementation, this would use the property index
                let nodes = self.execute_node_by_label(0)?; // Get all nodes
                for node in nodes {
                    if let Some(properties) = node.get("properties") {
                        if properties.is_object() {
                            let mut found = false;
                            for (prop_key, prop_value) in properties.as_object().unwrap() {
                                if prop_key == key || (prop_value.as_str() == Some(key)) {
                                    found = true;
                                    break;
                                }
                            }
                            if found {
                                results.push(node);
                            }
                        }
                    }
                }
            }
            IndexType::Vector => {
                // Scan vector index for similar vectors
                // For now, return empty results as vector search requires specific implementation
                // In a full implementation, this would use the KNN index
                results = Vec::new();
            }
            IndexType::FullText => {
                // Scan full-text index for text matches
                // For now, implement a simple text search in properties
                let nodes = self.execute_node_by_label(0)?; // Get all nodes
                for node in nodes {
                    if let Some(properties) = node.get("properties") {
                        if properties.is_object() {
                            let mut found = false;
                            for (_, prop_value) in properties.as_object().unwrap() {
                                if prop_value.is_string() {
                                    let text = prop_value.as_str().unwrap().to_lowercase();
                                    if text.contains(&key.to_lowercase()) {
                                        found = true;
                                        break;
                                    }
                                }
                            }
                            if found {
                                results.push(node);
                            }
                        }
                    }
                }
            }
        }

        // Set the results in the context
        context.set_variable(variable, Value::Array(results));

        Ok(())
    }

    /// Execute Distinct operator
    fn execute_distinct(&self, context: &mut ExecutionContext, columns: &[String]) -> Result<()> {
        if context.result_set.rows.is_empty() {
            return Ok(());
        }

        // Create a set to track unique combinations of values
        let mut seen = std::collections::HashSet::new();
        let mut distinct_rows = Vec::new();

        for row in &context.result_set.rows {
            // Extract values for the specified columns
            let mut key_values = Vec::new();

            if columns.is_empty() {
                // If no columns specified, use all values
                key_values = row.values.clone();
            } else {
                // Extract values for specified columns
                for column in columns {
                    if let Some(index) = self.get_column_index(column, &context.result_set.columns)
                    {
                        if index < row.values.len() {
                            key_values.push(row.values[index].clone());
                        } else {
                            key_values.push(serde_json::Value::Null);
                        }
                    } else {
                        key_values.push(serde_json::Value::Null);
                    }
                }
            }

            // Create a key for uniqueness checking
            let key = serde_json::to_string(&key_values).unwrap_or_default();

            if seen.insert(key) {
                distinct_rows.push(row.clone());
            }
        }

        // Update context with distinct results
        context.result_set.rows = distinct_rows;

        Ok(())
    }

    /// Get the index of a column by name
    fn get_column_index(&self, column_name: &str, columns: &[String]) -> Option<usize> {
        columns.iter().position(|col| col == column_name)
    }

    /// Evaluate a predicate expression against a node
    fn evaluate_predicate(
        &self,
        node: &Value,
        expr: &parser::Expression,
        context: &ExecutionContext,
    ) -> Result<bool> {
        match expr {
            parser::Expression::BinaryOp { left, op, right } => {
                let left_val = self.evaluate_expression(node, left, context)?;
                let right_val = self.evaluate_expression(node, right, context)?;

                match op {
                    parser::BinaryOperator::Equal => Ok(left_val == right_val),
                    parser::BinaryOperator::NotEqual => Ok(left_val != right_val),
                    parser::BinaryOperator::LessThan => {
                        self.compare_values(&left_val, &right_val, |a, b| a < b)
                    }
                    parser::BinaryOperator::LessThanOrEqual => {
                        self.compare_values(&left_val, &right_val, |a, b| a <= b)
                    }
                    parser::BinaryOperator::GreaterThan => {
                        self.compare_values(&left_val, &right_val, |a, b| a > b)
                    }
                    parser::BinaryOperator::GreaterThanOrEqual => {
                        self.compare_values(&left_val, &right_val, |a, b| a >= b)
                    }
                    parser::BinaryOperator::And => {
                        let left_bool = self.value_to_bool(&left_val)?;
                        let right_bool = self.value_to_bool(&right_val)?;
                        Ok(left_bool && right_bool)
                    }
                    parser::BinaryOperator::Or => {
                        let left_bool = self.value_to_bool(&left_val)?;
                        let right_bool = self.value_to_bool(&right_val)?;
                        Ok(left_bool || right_bool)
                    }
                    _ => Ok(false), // Other operators not implemented in MVP
                }
            }
            parser::Expression::UnaryOp { op, operand } => {
                let operand_val = self.evaluate_expression(node, operand, context)?;
                match op {
                    parser::UnaryOperator::Not => {
                        let bool_val = self.value_to_bool(&operand_val)?;
                        Ok(!bool_val)
                    }
                    _ => Ok(false),
                }
            }
            _ => {
                let result = self.evaluate_expression(node, expr, context)?;
                self.value_to_bool(&result)
            }
        }
    }

    /// Evaluate an expression against a node
    fn evaluate_expression(
        &self,
        node: &Value,
        expr: &parser::Expression,
        context: &ExecutionContext,
    ) -> Result<Value> {
        match expr {
            parser::Expression::Variable(name) => {
                if let Some(value) = context.get_variable(name) {
                    Ok(value.clone())
                } else {
                    Ok(Value::Null)
                }
            }
            parser::Expression::PropertyAccess { variable, property } => {
                if variable == "n" || variable == "node" {
                    // Access property of the current node
                    if let Value::Object(props) = node {
                        Ok(props.get(property).cloned().unwrap_or(Value::Null))
                    } else {
                        Ok(Value::Null)
                    }
                } else {
                    // Access property of a variable
                    if let Some(Value::Object(props)) = context.get_variable(variable) {
                        Ok(props.get(property).cloned().unwrap_or(Value::Null))
                    } else {
                        Ok(Value::Null)
                    }
                }
            }
            parser::Expression::Literal(literal) => match literal {
                parser::Literal::String(s) => Ok(Value::String(s.clone())),
                parser::Literal::Integer(i) => Ok(Value::Number((*i).into())),
                parser::Literal::Float(f) => Ok(Value::Number(
                    serde_json::Number::from_f64(*f).unwrap_or(serde_json::Number::from(0)),
                )),
                parser::Literal::Boolean(b) => Ok(Value::Bool(*b)),
                parser::Literal::Null => Ok(Value::Null),
            },
            parser::Expression::Parameter(name) => {
                if let Some(value) = context.params.get(name) {
                    Ok(value.clone())
                } else {
                    Ok(Value::Null)
                }
            }
            _ => Ok(Value::Null), // Other expressions not implemented in MVP
        }
    }

    /// Compare two values using a comparison function
    fn compare_values<F>(&self, left: &Value, right: &Value, compare_fn: F) -> Result<bool>
    where
        F: FnOnce(f64, f64) -> bool,
    {
        let left_num = self.value_to_number(left)?;
        let right_num = self.value_to_number(right)?;
        Ok(compare_fn(left_num, right_num))
    }

    /// Convert a value to a number
    fn value_to_number(&self, value: &Value) -> Result<f64> {
        match value {
            Value::Number(n) => n.as_f64().ok_or_else(|| Error::TypeMismatch {
                expected: "number".to_string(),
                actual: "invalid number".to_string(),
            }),
            Value::String(s) => s.parse::<f64>().map_err(|_| Error::TypeMismatch {
                expected: "number".to_string(),
                actual: "string".to_string(),
            }),
            Value::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
            Value::Null => Ok(0.0),
            _ => Err(Error::TypeMismatch {
                expected: "number".to_string(),
                actual: "unknown type".to_string(),
            }),
        }
    }

    /// Convert a value to a boolean
    fn value_to_bool(&self, value: &Value) -> Result<bool> {
        match value {
            Value::Bool(b) => Ok(*b),
            Value::Number(n) => Ok(n.as_f64().unwrap_or(0.0) != 0.0),
            Value::String(s) => Ok(!s.is_empty()),
            Value::Null => Ok(false),
            Value::Array(arr) => Ok(!arr.is_empty()),
            Value::Object(obj) => Ok(!obj.is_empty()),
        }
    }

    /// Find relationships for a node
    fn find_relationships(
        &self,
        node_id: u64,
        type_id: Option<u32>,
        direction: Direction,
    ) -> Result<Vec<RelationshipInfo>> {
        let mut relationships = Vec::new();

        // For MVP, we'll simulate finding relationships
        // In a real implementation, this would traverse the linked lists in the storage layer

        // Read the node record to get the first relationship pointer
        if let Ok(node_record) = self.store.read_node(node_id) {
            let mut rel_ptr = node_record.first_rel_ptr;

            // Traverse the relationship chain
            while rel_ptr != 0 {
                if let Ok(rel_record) = self.store.read_rel(rel_ptr) {
                    // Check if this relationship matches our criteria
                    let matches_type = type_id.is_none() || Some(rel_record.type_id) == type_id;
                    let matches_direction = match direction {
                        Direction::Outgoing => rel_record.src_id == node_id,
                        Direction::Incoming => rel_record.dst_id == node_id,
                        Direction::Both => true,
                    };

                    if matches_type && matches_direction {
                        relationships.push(RelationshipInfo {
                            id: rel_ptr,
                            source_id: rel_record.src_id,
                            target_id: rel_record.dst_id,
                            type_id: rel_record.type_id,
                        });
                    }

                    // Move to next relationship
                    rel_ptr = if rel_record.src_id == node_id {
                        rel_record.next_src_ptr
                    } else {
                        rel_record.next_dst_ptr
                    };
                } else {
                    break;
                }
            }
        }

        Ok(relationships)
    }

    /// Read a node as a JSON value
    fn read_node_as_value(&self, node_id: u64) -> Result<Value> {
        let node_record = self.store.read_node(node_id)?;

        // Create node representation
        let mut node = Map::new();
        node.insert("id".to_string(), Value::Number(node_id.into()));

        // Get labels from the label bits
        let mut labels = Vec::new();
        for i in 0..32 {
            if node_record.label_bits & (1 << i) != 0 {
                labels.push(Value::String(format!("label_{}", i)));
            }
        }
        node.insert("labels".to_string(), Value::Array(labels));

        // For MVP, we'll return empty properties
        node.insert("properties".to_string(), Value::Object(Map::new()));

        Ok(Value::Object(node))
    }

    /// Get a column value from a node for sorting
    fn get_column_value(&self, node: &Value, column: &str) -> Value {
        if let Value::Object(props) = node {
            if let Some(value) = props.get(column) {
                value.clone()
            } else {
                // Try to access as property access (e.g., "n.name")
                if let Some(dot_pos) = column.find('.') {
                    let var_name = &column[..dot_pos];
                    let prop_name = &column[dot_pos + 1..];

                    if let Some(Value::Object(var_props)) = props.get(var_name) {
                        if let Some(prop_value) = var_props.get(prop_name) {
                            return prop_value.clone();
                        }
                    }
                }
                Value::Null
            }
        } else {
            Value::Null
        }
    }

    /// Compare values for sorting
    fn compare_values_for_sort(&self, a: &Value, b: &Value) -> std::cmp::Ordering {
        match (a, b) {
            (Value::Null, Value::Null) => std::cmp::Ordering::Equal,
            (Value::Null, _) => std::cmp::Ordering::Less,
            (_, Value::Null) => std::cmp::Ordering::Greater,
            (Value::Number(a_num), Value::Number(b_num)) => {
                let a_f64 = a_num.as_f64().unwrap_or(0.0);
                let b_f64 = b_num.as_f64().unwrap_or(0.0);
                a_f64
                    .partial_cmp(&b_f64)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }
            (Value::String(a_str), Value::String(b_str)) => a_str.cmp(b_str),
            (Value::Bool(a_bool), Value::Bool(b_bool)) => a_bool.cmp(b_bool),
            (Value::Array(a_arr), Value::Array(b_arr)) => match a_arr.len().cmp(&b_arr.len()) {
                std::cmp::Ordering::Equal => {
                    for (a_item, b_item) in a_arr.iter().zip(b_arr.iter()) {
                        let comparison = self.compare_values_for_sort(a_item, b_item);
                        if comparison != std::cmp::Ordering::Equal {
                            return comparison;
                        }
                    }
                    std::cmp::Ordering::Equal
                }
                other => other,
            },
            _ => {
                // Convert to strings for comparison
                let a_str = self.value_to_string(a);
                let b_str = self.value_to_string(b);
                a_str.cmp(&b_str)
            }
        }
    }

    /// Convert a value to string for comparison
    fn value_to_string(&self, value: &Value) -> String {
        match value {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Null => "null".to_string(),
            Value::Array(arr) => format!("[{}]", arr.len()),
            Value::Object(obj) => format!("{{{}}}", obj.len()),
        }
    }

    /// Execute hash join operation
    fn execute_hash_join(
        &self,
        _context: &mut ExecutionContext,
        _left_key: &str,
        _right_key: &str,
    ) -> Result<()> {
        // MVP implementation - just pass through for now
        // In a real implementation, this would perform hash join
        Ok(())
    }

    /// Execute new index scan operation
    fn execute_index_scan_new(
        &self,
        context: &mut ExecutionContext,
        _index_name: &str,
        label: &str,
    ) -> Result<()> {
        // Get label ID from catalog
        let label_id = self.catalog.get_or_create_label(label)?;

        // Execute node by label scan
        let nodes = self.execute_node_by_label(label_id)?;
        context.set_variable("n", Value::Array(nodes));

        Ok(())
    }
}

/// Relationship information for expansion
#[derive(Debug, Clone)]
struct RelationshipInfo {
    id: u64,
    source_id: u64,
    target_id: u64,
    type_id: u32,
}

/// Execution context for query processing
#[derive(Debug)]
struct ExecutionContext {
    /// Query parameters
    params: HashMap<String, Value>,
    /// Variable bindings
    variables: HashMap<String, Value>,
    /// Query result set
    result_set: ResultSet,
}

impl ExecutionContext {
    fn new(params: HashMap<String, Value>) -> Self {
        Self {
            params,
            variables: HashMap::new(),
            result_set: ResultSet {
                columns: Vec::new(),
                rows: Vec::new(),
            },
        }
    }

    fn set_variable(&mut self, name: &str, value: Value) {
        self.variables.insert(name.to_string(), value);
    }

    fn get_variable(&self, name: &str) -> Option<&Value> {
        self.variables.get(name)
    }
}

impl Default for Executor {
    fn default() -> Self {
        // Create default components for testing
        let catalog = Catalog::default();
        let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
        let store = RecordStore::new(temp_dir.path()).expect("Failed to create record store");
        // Keep temp_dir alive by leaking it (acceptable for testing)
        std::mem::forget(temp_dir);
        let label_index = LabelIndex::default();
        let knn_index = KnnIndex::new_default(128).expect("Failed to create default KNN index");

        Self::new(&catalog, &store, &label_index, &knn_index)
            .expect("Failed to create default executor")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_executor() -> (Executor, TempDir) {
        let dir = TempDir::new().unwrap();
        let catalog = Catalog::new(dir.path()).unwrap();
        let store = RecordStore::new(dir.path()).unwrap();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new_default(128).unwrap();

        let executor = Executor::new(&catalog, &store, &label_index, &knn_index).unwrap();
        (executor, dir)
    }

    #[test]
    fn test_executor_creation() {
        let (_executor, _dir) = create_test_executor();
        // Test passes if creation succeeds
    }

    #[test]
    fn test_query_creation() {
        let mut params = HashMap::new();
        params.insert("name".to_string(), Value::String("test".to_string()));

        let query = Query {
            cypher: "MATCH (n:Person) RETURN n".to_string(),
            params,
        };

        assert_eq!(query.cypher, "MATCH (n:Person) RETURN n");
        assert_eq!(
            query.params.get("name").unwrap(),
            &Value::String("test".to_string())
        );
    }

    #[test]
    fn test_parse_match_query() {
        let (executor, _dir) = create_test_executor();

        // Create a label first
        let catalog = Catalog::new("./test_data").unwrap();
        let label_id = catalog.get_or_create_label("Person").unwrap();

        // Test parsing
        let operators = executor
            .parse_and_plan("MATCH (n:Person) RETURN n")
            .unwrap();
        assert_eq!(operators.len(), 2);

        match &operators[0] {
            Operator::NodeByLabel {
                label_id: parsed_label_id,
                variable,
            } => {
                assert_eq!(*parsed_label_id, label_id);
                assert_eq!(variable, "n");
            }
            _ => panic!("Expected NodeByLabel operator"),
        }

        match &operators[1] {
            Operator::Project { columns } => {
                assert_eq!(columns, &vec!["n".to_string()]);
            }
            _ => panic!("Expected Project operator"),
        }
    }

    #[test]
    fn test_parse_invalid_query() {
        let (executor, _dir) = create_test_executor();

        // Test with actually invalid query syntax
        let result = executor.parse_and_plan("INVALID SYNTAX!!!");
        // Invalid syntax should return an error
        assert!(result.is_err());
    }

    #[test]
    fn test_execution_context() {
        let mut params = HashMap::new();
        params.insert("param1".to_string(), Value::String("value1".to_string()));

        let mut context = ExecutionContext::new(params);

        // Test setting and getting variables
        context.set_variable("n", Value::Array(vec![Value::String("node1".to_string())]));

        assert_eq!(
            context.get_variable("n"),
            Some(&Value::Array(vec![Value::String("node1".to_string())]))
        );
        assert_eq!(context.get_variable("nonexistent"), None);
    }

    #[test]
    fn test_direction_enum() {
        assert_eq!(Direction::Outgoing, Direction::Outgoing);
        assert_ne!(Direction::Outgoing, Direction::Incoming);
        assert_ne!(Direction::Outgoing, Direction::Both);
    }

    #[test]
    fn test_operator_cloning() {
        let op = Operator::NodeByLabel {
            label_id: 1,
            variable: "n".to_string(),
        };

        let cloned = op.clone();
        match cloned {
            Operator::NodeByLabel { label_id, variable } => {
                assert_eq!(label_id, 1);
                assert_eq!(variable, "n");
            }
            _ => panic!("Expected NodeByLabel operator"),
        }
    }

    #[test]
    fn test_result_set() {
        let mut result_set = ResultSet {
            columns: vec!["n".to_string()],
            rows: vec![],
        };

        result_set.rows.push(Row {
            values: vec![Value::String("test".to_string())],
        });

        assert_eq!(result_set.columns.len(), 1);
        assert_eq!(result_set.rows.len(), 1);
        assert_eq!(result_set.rows[0].values.len(), 1);
    }

    #[test]
    fn test_executor_default() {
        let executor = Executor::default();
        // Test passes if default creation succeeds
        drop(executor);
    }

    #[test]
    fn test_aggregate_count_star() {
        let (executor, _dir) = create_test_executor();

        // Create test data
        let mut context = ExecutionContext::new(HashMap::new());
        context.result_set.columns = vec!["name".to_string(), "age".to_string()];
        context.result_set.rows = vec![
            Row {
                values: vec![
                    Value::String("Alice".to_string()),
                    Value::Number(serde_json::Number::from(25)),
                ],
            },
            Row {
                values: vec![
                    Value::String("Bob".to_string()),
                    Value::Number(serde_json::Number::from(30)),
                ],
            },
            Row {
                values: vec![
                    Value::String("Charlie".to_string()),
                    Value::Number(serde_json::Number::from(35)),
                ],
            },
        ];

        // Test COUNT(*)
        let aggregations = vec![Aggregation::Count {
            column: None,
            alias: "count".to_string(),
        }];
        executor
            .execute_aggregate(&mut context, &[], &aggregations)
            .unwrap();

        assert_eq!(context.result_set.rows.len(), 1);
        assert_eq!(context.result_set.rows[0].values.len(), 1);
        assert_eq!(
            context.result_set.rows[0].values[0],
            Value::Number(serde_json::Number::from(3))
        );
    }

    #[test]
    fn test_aggregate_count_column() {
        let (executor, _dir) = create_test_executor();

        // Create test data with null values
        let mut context = ExecutionContext::new(HashMap::new());
        context.result_set.columns = vec!["name".to_string(), "age".to_string()];
        context.result_set.rows = vec![
            Row {
                values: vec![
                    Value::String("Alice".to_string()),
                    Value::Number(serde_json::Number::from(25)),
                ],
            },
            Row {
                values: vec![Value::String("Bob".to_string()), Value::Null],
            },
            Row {
                values: vec![
                    Value::String("Charlie".to_string()),
                    Value::Number(serde_json::Number::from(35)),
                ],
            },
        ];

        // Test COUNT(age) - should count non-null values
        let aggregations = vec![Aggregation::Count {
            column: Some("age".to_string()),
            alias: "count".to_string(),
        }];
        executor
            .execute_aggregate(&mut context, &[], &aggregations)
            .unwrap();

        assert_eq!(context.result_set.rows.len(), 1);
        assert_eq!(context.result_set.rows[0].values.len(), 1);
        assert_eq!(
            context.result_set.rows[0].values[0],
            Value::Number(serde_json::Number::from(2))
        );
    }

    #[test]
    fn test_aggregate_sum() {
        let (executor, _dir) = create_test_executor();

        // Create test data
        let mut context = ExecutionContext::new(HashMap::new());
        context.result_set.columns = vec!["name".to_string(), "score".to_string()];
        context.result_set.rows = vec![
            Row {
                values: vec![
                    Value::String("Alice".to_string()),
                    Value::Number(serde_json::Number::from(10)),
                ],
            },
            Row {
                values: vec![
                    Value::String("Bob".to_string()),
                    Value::Number(serde_json::Number::from(20)),
                ],
            },
            Row {
                values: vec![
                    Value::String("Charlie".to_string()),
                    Value::Number(serde_json::Number::from(30)),
                ],
            },
        ];

        // Test SUM(score)
        let aggregations = vec![Aggregation::Sum {
            column: "score".to_string(),
            alias: "total".to_string(),
        }];
        executor
            .execute_aggregate(&mut context, &[], &aggregations)
            .unwrap();

        assert_eq!(context.result_set.rows.len(), 1);
        assert_eq!(context.result_set.rows[0].values.len(), 1);
        assert_eq!(
            context.result_set.rows[0].values[0],
            Value::Number(serde_json::Number::from_f64(60.0).unwrap())
        );
    }

    #[test]
    fn test_aggregate_avg() {
        let (executor, _dir) = create_test_executor();

        // Create test data
        let mut context = ExecutionContext::new(HashMap::new());
        context.result_set.columns = vec!["name".to_string(), "score".to_string()];
        context.result_set.rows = vec![
            Row {
                values: vec![
                    Value::String("Alice".to_string()),
                    Value::Number(serde_json::Number::from(10)),
                ],
            },
            Row {
                values: vec![
                    Value::String("Bob".to_string()),
                    Value::Number(serde_json::Number::from(20)),
                ],
            },
            Row {
                values: vec![
                    Value::String("Charlie".to_string()),
                    Value::Number(serde_json::Number::from(30)),
                ],
            },
        ];

        // Test AVG(score)
        let aggregations = vec![Aggregation::Avg {
            column: "score".to_string(),
            alias: "average".to_string(),
        }];
        executor
            .execute_aggregate(&mut context, &[], &aggregations)
            .unwrap();

        assert_eq!(context.result_set.rows.len(), 1);
        assert_eq!(context.result_set.rows[0].values.len(), 1);
        // Average should be 20.0
        if let Value::Number(n) = &context.result_set.rows[0].values[0] {
            assert_eq!(n.as_f64().unwrap(), 20.0);
        } else {
            panic!("Expected number");
        }
    }

    #[test]
    fn test_aggregate_min_max() {
        let (executor, _dir) = create_test_executor();

        // Create test data
        let mut context = ExecutionContext::new(HashMap::new());
        context.result_set.columns = vec!["name".to_string(), "score".to_string()];
        context.result_set.rows = vec![
            Row {
                values: vec![
                    Value::String("Alice".to_string()),
                    Value::Number(serde_json::Number::from(10)),
                ],
            },
            Row {
                values: vec![
                    Value::String("Bob".to_string()),
                    Value::Number(serde_json::Number::from(20)),
                ],
            },
            Row {
                values: vec![
                    Value::String("Charlie".to_string()),
                    Value::Number(serde_json::Number::from(30)),
                ],
            },
        ];

        // Test MIN and MAX together
        let aggregations = vec![
            Aggregation::Min {
                column: "score".to_string(),
                alias: "min_score".to_string(),
            },
            Aggregation::Max {
                column: "score".to_string(),
                alias: "max_score".to_string(),
            },
        ];
        executor
            .execute_aggregate(&mut context, &[], &aggregations)
            .unwrap();

        assert_eq!(context.result_set.rows.len(), 1);
        assert_eq!(context.result_set.rows[0].values.len(), 2);

        // Check MIN
        if let Value::Number(n) = &context.result_set.rows[0].values[0] {
            assert_eq!(n.as_f64().unwrap(), 10.0);
        } else {
            panic!("Expected number for MIN");
        }

        // Check MAX
        if let Value::Number(n) = &context.result_set.rows[0].values[1] {
            assert_eq!(n.as_f64().unwrap(), 30.0);
        } else {
            panic!("Expected number for MAX");
        }
    }

    #[test]
    fn test_aggregate_group_by() {
        let (executor, _dir) = create_test_executor();

        // Create test data with groups
        let mut context = ExecutionContext::new(HashMap::new());
        context.result_set.columns = vec!["department".to_string(), "salary".to_string()];
        context.result_set.rows = vec![
            Row {
                values: vec![
                    Value::String("IT".to_string()),
                    Value::Number(serde_json::Number::from(1000)),
                ],
            },
            Row {
                values: vec![
                    Value::String("IT".to_string()),
                    Value::Number(serde_json::Number::from(2000)),
                ],
            },
            Row {
                values: vec![
                    Value::String("HR".to_string()),
                    Value::Number(serde_json::Number::from(1500)),
                ],
            },
            Row {
                values: vec![
                    Value::String("HR".to_string()),
                    Value::Number(serde_json::Number::from(2500)),
                ],
            },
        ];

        // Test GROUP BY department with SUM(salary)
        let aggregations = vec![Aggregation::Sum {
            column: "salary".to_string(),
            alias: "total_salary".to_string(),
        }];
        executor
            .execute_aggregate(&mut context, &["department".to_string()], &aggregations)
            .unwrap();

        assert_eq!(context.result_set.rows.len(), 2); // Two groups

        // Sort results for consistent testing
        context.result_set.rows.sort_by(|a, b| {
            let dept_a = a.values[0].as_str().unwrap();
            let dept_b = b.values[0].as_str().unwrap();
            dept_a.cmp(dept_b)
        });

        // Check IT department total
        assert_eq!(
            context.result_set.rows[0].values[0],
            Value::String("HR".to_string())
        );
        if let Value::Number(n) = &context.result_set.rows[0].values[1] {
            assert_eq!(n.as_f64().unwrap(), 4000.0); // 1500 + 2500
        } else {
            panic!("Expected number for HR total");
        }

        // Check HR department total
        assert_eq!(
            context.result_set.rows[1].values[0],
            Value::String("IT".to_string())
        );
        if let Value::Number(n) = &context.result_set.rows[1].values[1] {
            assert_eq!(n.as_f64().unwrap(), 3000.0); // 1000 + 2000
        } else {
            panic!("Expected number for IT total");
        }
    }

    #[test]
    fn test_execute_node_by_label() {
        let (_executor, _dir) = create_test_executor();

        // Create a label and add some nodes to the index
        let catalog = Catalog::new("./test_data").unwrap();
        let label_id = catalog.get_or_create_label("Person").unwrap();

        // Add some test nodes to the label index
        let label_index = LabelIndex::new();
        label_index.add_node(1, &[label_id]).unwrap();
        label_index.add_node(2, &[label_id]).unwrap();
        label_index.add_node(3, &[label_id]).unwrap();

        // Create executor with the populated index
        let temp_dir = TempDir::new().unwrap();
        let store = RecordStore::new(temp_dir.path()).unwrap();
        let knn_index = KnnIndex::new_default(128).unwrap();
        let executor = Executor::new(&catalog, &store, &label_index, &knn_index).unwrap();

        // Test executing node by label
        let results = executor.execute_node_by_label(label_id).unwrap();
        assert_eq!(results.len(), 3);

        // Check that all results are valid node objects
        for result in results {
            if let Value::Object(node_obj) = result {
                assert!(node_obj.contains_key("id"));
                assert!(node_obj.contains_key("labels"));
                assert!(node_obj.contains_key("properties"));
            } else {
                panic!("Expected object");
            }
        }
    }

    #[test]
    fn test_execute_filter() {
        let (executor, _dir) = create_test_executor();

        let mut context = ExecutionContext::new(HashMap::new());

        // Create test nodes
        let mut node1 = Map::new();
        node1.insert("id".to_string(), Value::Number(1.into()));
        node1.insert("name".to_string(), Value::String("Alice".to_string()));
        node1.insert("age".to_string(), Value::Number(25.into()));

        let mut node2 = Map::new();
        node2.insert("id".to_string(), Value::Number(2.into()));
        node2.insert("name".to_string(), Value::String("Bob".to_string()));
        node2.insert("age".to_string(), Value::Number(30.into()));

        context.set_variable(
            "n",
            Value::Array(vec![Value::Object(node1), Value::Object(node2)]),
        );

        // Test filter with age > 25
        executor.execute_filter(&mut context, "n.age > 25").unwrap();

        // Should only have Bob's node
        if let Some(Value::Array(nodes)) = context.get_variable("n") {
            assert_eq!(nodes.len(), 1);
            if let Value::Object(node_obj) = &nodes[0] {
                assert_eq!(
                    node_obj.get("name"),
                    Some(&Value::String("Bob".to_string()))
                );
            }
        } else {
            panic!("Expected array of nodes");
        }
    }

    #[test]
    fn test_execute_expand() {
        let (executor, _dir) = create_test_executor();

        let mut context = ExecutionContext::new(HashMap::new());

        // Create test source nodes
        let mut source_node = Map::new();
        source_node.insert("id".to_string(), Value::Number(1.into()));
        context.set_variable("source", Value::Array(vec![Value::Object(source_node)]));

        // Test expand operation
        executor
            .execute_expand(
                &mut context,
                None, // any type
                Direction::Outgoing,
                "source",
                "target",
                "rel",
            )
            .unwrap();

        // Should have target nodes and relationships
        assert!(context.get_variable("target").is_some());
        assert!(context.get_variable("rel").is_some());
    }

    #[test]
    fn test_execute_project() {
        let (executor, _dir) = create_test_executor();

        let mut context = ExecutionContext::new(HashMap::new());

        // Create test nodes
        let mut node = Map::new();
        node.insert("id".to_string(), Value::Number(1.into()));
        node.insert("name".to_string(), Value::String("Alice".to_string()));

        context.set_variable("n", Value::Array(vec![Value::Object(node)]));

        // Test project operation
        let results = executor
            .execute_project(&context, &["n".to_string()])
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].values.len(), 1);
    }

    #[test]
    fn test_execute_sort() {
        let (executor, _dir) = create_test_executor();

        let mut context = ExecutionContext::new(HashMap::new());

        // Create test nodes with different ages
        let mut node1 = Map::new();
        node1.insert("id".to_string(), Value::Number(1.into()));
        node1.insert("age".to_string(), Value::Number(30.into()));

        let mut node2 = Map::new();
        node2.insert("id".to_string(), Value::Number(2.into()));
        node2.insert("age".to_string(), Value::Number(20.into()));

        let mut node3 = Map::new();
        node3.insert("id".to_string(), Value::Number(3.into()));
        node3.insert("age".to_string(), Value::Number(25.into()));

        context.set_variable(
            "n",
            Value::Array(vec![
                Value::Object(node1),
                Value::Object(node2),
                Value::Object(node3),
            ]),
        );

        // Test sort by age ascending
        executor
            .execute_sort(&mut context, &["age".to_string()], &[true])
            .unwrap();

        // Check that nodes are sorted by age
        if let Some(Value::Array(nodes)) = context.get_variable("n") {
            assert_eq!(nodes.len(), 3);
            // First node should be youngest (age 20)
            if let Value::Object(first_node) = &nodes[0] {
                assert_eq!(first_node.get("age"), Some(&Value::Number(20.into())));
            }
            // Middle node should be age 25
            if let Value::Object(middle_node) = &nodes[1] {
                assert_eq!(middle_node.get("age"), Some(&Value::Number(25.into())));
            }
            // Last node should be oldest (age 30)
            if let Value::Object(last_node) = &nodes[2] {
                assert_eq!(last_node.get("age"), Some(&Value::Number(30.into())));
            }
        } else {
            panic!("Expected array of nodes");
        }
    }

    #[test]
    fn test_evaluate_predicate() {
        let (executor, _dir) = create_test_executor();

        let mut context = ExecutionContext::new(HashMap::new());
        context
            .params
            .insert("min_age".to_string(), Value::Number(25.into()));

        // Create test node
        let mut node = Map::new();
        node.insert("id".to_string(), Value::Number(1.into()));
        node.insert("age".to_string(), Value::Number(30.into()));
        let node_value = Value::Object(node);

        // Test various predicates
        let mut parser = parser::CypherParser::new("n.age > $min_age".to_string());
        let expr = parser.parse_expression().unwrap();
        let result = executor
            .evaluate_predicate(&node_value, &expr, &context)
            .unwrap();
        assert!(result); // 30 > 25

        let mut parser = parser::CypherParser::new("n.age < 20".to_string());
        let expr = parser.parse_expression().unwrap();
        let result = executor
            .evaluate_predicate(&node_value, &expr, &context)
            .unwrap();
        assert!(!result); // 30 < 20 is false

        let mut parser = parser::CypherParser::new("n.age = 30".to_string());
        let expr = parser.parse_expression().unwrap();
        let result = executor
            .evaluate_predicate(&node_value, &expr, &context)
            .unwrap();
        assert!(result); // 30 = 30
    }

    #[test]
    fn test_evaluate_expression() {
        let (executor, _dir) = create_test_executor();

        let mut context = ExecutionContext::new(HashMap::new());
        context
            .params
            .insert("param1".to_string(), Value::String("test".to_string()));

        // Create test node
        let mut node = Map::new();
        node.insert("id".to_string(), Value::Number(1.into()));
        node.insert("name".to_string(), Value::String("Alice".to_string()));
        let node_value = Value::Object(node);

        // Test variable access
        let mut parser = parser::CypherParser::new("n.name".to_string());
        let expr = parser.parse_expression().unwrap();
        let result = executor
            .evaluate_expression(&node_value, &expr, &context)
            .unwrap();
        assert_eq!(result, Value::String("Alice".to_string()));

        // Test parameter access
        let mut parser = parser::CypherParser::new("$param1".to_string());
        let expr = parser.parse_expression().unwrap();
        let result = executor
            .evaluate_expression(&node_value, &expr, &context)
            .unwrap();
        assert_eq!(result, Value::String("test".to_string()));

        // Test literal
        let mut parser = parser::CypherParser::new("42".to_string());
        let expr = parser.parse_expression().unwrap();
        let result = executor
            .evaluate_expression(&node_value, &expr, &context)
            .unwrap();
        assert_eq!(result, Value::Number(42.into()));
    }

    #[test]
    fn test_value_conversion() {
        let (executor, _dir) = create_test_executor();

        // Test value_to_number
        assert_eq!(
            executor.value_to_number(&Value::Number(42.into())).unwrap(),
            42.0
        );
        assert_eq!(
            executor
                .value_to_number(&Value::String("123".to_string()))
                .unwrap(),
            123.0
        );
        assert_eq!(executor.value_to_number(&Value::Bool(true)).unwrap(), 1.0);
        assert_eq!(executor.value_to_number(&Value::Bool(false)).unwrap(), 0.0);
        assert_eq!(executor.value_to_number(&Value::Null).unwrap(), 0.0);

        // Test value_to_bool
        assert!(executor.value_to_bool(&Value::Bool(true)).unwrap());
        assert!(!executor.value_to_bool(&Value::Bool(false)).unwrap());
        assert!(executor.value_to_bool(&Value::Number(1.into())).unwrap());
        assert!(!executor.value_to_bool(&Value::Number(0.into())).unwrap());
        assert!(
            executor
                .value_to_bool(&Value::String("hello".to_string()))
                .unwrap()
        );
        assert!(
            !executor
                .value_to_bool(&Value::String("".to_string()))
                .unwrap()
        );
        assert!(!executor.value_to_bool(&Value::Null).unwrap());
    }

    #[test]
    fn test_compare_values_for_sort() {
        let (executor, _dir) = create_test_executor();

        use std::cmp::Ordering;

        // Test null comparisons
        assert_eq!(
            executor.compare_values_for_sort(&Value::Null, &Value::Null),
            Ordering::Equal
        );
        assert_eq!(
            executor.compare_values_for_sort(&Value::Null, &Value::Number(1.into())),
            Ordering::Less
        );
        assert_eq!(
            executor.compare_values_for_sort(&Value::Number(1.into()), &Value::Null),
            Ordering::Greater
        );

        // Test number comparisons
        assert_eq!(
            executor.compare_values_for_sort(&Value::Number(1.into()), &Value::Number(2.into())),
            Ordering::Less
        );
        assert_eq!(
            executor.compare_values_for_sort(&Value::Number(2.into()), &Value::Number(1.into())),
            Ordering::Greater
        );
        assert_eq!(
            executor.compare_values_for_sort(&Value::Number(1.into()), &Value::Number(1.into())),
            Ordering::Equal
        );

        // Test string comparisons
        assert_eq!(
            executor.compare_values_for_sort(
                &Value::String("a".to_string()),
                &Value::String("b".to_string())
            ),
            Ordering::Less
        );
        assert_eq!(
            executor.compare_values_for_sort(
                &Value::String("b".to_string()),
                &Value::String("a".to_string())
            ),
            Ordering::Greater
        );
    }

    #[test]
    fn test_operator_variants() {
        // Test all operator variants for coverage
        let node_op = Operator::NodeByLabel {
            label_id: 1,
            variable: "n".to_string(),
        };
        assert!(matches!(node_op, Operator::NodeByLabel { .. }));

        let filter_op = Operator::Filter {
            predicate: "n.age > 25".to_string(),
        };
        assert!(matches!(filter_op, Operator::Filter { .. }));

        let expand_op = Operator::Expand {
            type_id: Some(1),
            direction: Direction::Outgoing,
            source_var: "n".to_string(),
            target_var: "m".to_string(),
            rel_var: "r".to_string(),
        };
        assert!(matches!(expand_op, Operator::Expand { .. }));

        let project_op = Operator::Project {
            columns: vec!["n".to_string()],
        };
        assert!(matches!(project_op, Operator::Project { .. }));

        let limit_op = Operator::Limit { count: 10 };
        assert!(matches!(limit_op, Operator::Limit { .. }));

        let sort_op = Operator::Sort {
            columns: vec!["n.age".to_string()],
            ascending: vec![true],
        };
        assert!(matches!(sort_op, Operator::Sort { .. }));

        let aggregate_op = Operator::Aggregate {
            group_by: vec!["dept".to_string()],
            aggregations: vec![Aggregation::Count {
                column: None,
                alias: "count".to_string(),
            }],
        };
        assert!(matches!(aggregate_op, Operator::Aggregate { .. }));

        let union_op = Operator::Union {
            left: Box::new(node_op.clone()),
            right: Box::new(node_op.clone()),
        };
        assert!(matches!(union_op, Operator::Union { .. }));

        let join_op = Operator::Join {
            left: Box::new(node_op.clone()),
            right: Box::new(node_op.clone()),
            join_type: JoinType::Inner,
            condition: Some("n.id = m.id".to_string()),
        };
        assert!(matches!(join_op, Operator::Join { .. }));

        let index_scan_op = Operator::IndexScan {
            index_name: "label_Person".to_string(),
            label: "Person".to_string(),
        };
        assert!(matches!(index_scan_op, Operator::IndexScan { .. }));

        let distinct_op = Operator::Distinct {
            columns: vec!["n.id".to_string()],
        };
        assert!(matches!(distinct_op, Operator::Distinct { .. }));
    }

    #[test]
    fn test_enum_variants() {
        // Test Direction enum
        assert_eq!(Direction::Outgoing, Direction::Outgoing);
        assert_ne!(Direction::Outgoing, Direction::Incoming);
        assert_ne!(Direction::Outgoing, Direction::Both);

        // Test JoinType enum
        assert_eq!(JoinType::Inner, JoinType::Inner);
        assert_ne!(JoinType::Inner, JoinType::LeftOuter);
        assert_ne!(JoinType::Inner, JoinType::RightOuter);
        assert_ne!(JoinType::Inner, JoinType::FullOuter);

        // Test IndexType enum
        assert_eq!(IndexType::Label, IndexType::Label);
        assert_ne!(IndexType::Label, IndexType::Property);
        assert_ne!(IndexType::Label, IndexType::Vector);
        assert_ne!(IndexType::Label, IndexType::FullText);

        // Test Aggregation enum variants
        let count_agg = Aggregation::Count {
            column: None,
            alias: "count".to_string(),
        };
        assert!(matches!(count_agg, Aggregation::Count { .. }));

        let sum_agg = Aggregation::Sum {
            column: "age".to_string(),
            alias: "total_age".to_string(),
        };
        assert!(matches!(sum_agg, Aggregation::Sum { .. }));

        let avg_agg = Aggregation::Avg {
            column: "score".to_string(),
            alias: "avg_score".to_string(),
        };
        assert!(matches!(avg_agg, Aggregation::Avg { .. }));

        let min_agg = Aggregation::Min {
            column: "price".to_string(),
            alias: "min_price".to_string(),
        };
        assert!(matches!(min_agg, Aggregation::Min { .. }));

        let max_agg = Aggregation::Max {
            column: "price".to_string(),
            alias: "max_price".to_string(),
        };
        assert!(matches!(max_agg, Aggregation::Max { .. }));
    }

    #[test]
    fn test_expression_to_string() {
        let (executor, _dir) = create_test_executor();

        // Test variable
        let mut parser = parser::CypherParser::new("n".to_string());
        let expr = parser.parse_expression().unwrap();
        assert_eq!(executor.expression_to_string(&expr).unwrap(), "n");

        // Test property access
        let mut parser = parser::CypherParser::new("n.name".to_string());
        let expr = parser.parse_expression().unwrap();
        assert_eq!(executor.expression_to_string(&expr).unwrap(), "n.name");

        // Test literals
        let mut parser = parser::CypherParser::new("\"hello\"".to_string());
        let expr = parser.parse_expression().unwrap();
        assert_eq!(executor.expression_to_string(&expr).unwrap(), "\"hello\"");

        let mut parser = parser::CypherParser::new("42".to_string());
        let expr = parser.parse_expression().unwrap();
        assert_eq!(executor.expression_to_string(&expr).unwrap(), "42");

        let mut parser = parser::CypherParser::new("true".to_string());
        let expr = parser.parse_expression().unwrap();
        assert_eq!(executor.expression_to_string(&expr).unwrap(), "true");

        // Test binary operations
        let mut parser = parser::CypherParser::new("n.age = 25".to_string());
        let expr = parser.parse_expression().unwrap();
        assert_eq!(executor.expression_to_string(&expr).unwrap(), "n.age = 25");

        // Test parameters
        let mut parser = parser::CypherParser::new("$param1".to_string());
        let expr = parser.parse_expression().unwrap();
        assert_eq!(executor.expression_to_string(&expr).unwrap(), "$param1");
    }

    #[test]
    fn test_ast_to_operators() {
        let (executor, _dir) = create_test_executor();

        // Create a catalog with a label
        let catalog = Catalog::new("./test_data").unwrap();
        let label_id = catalog.get_or_create_label("Person").unwrap();

        // Test AST to operators conversion
        let mut parser = parser::CypherParser::new(
            "MATCH (n:Person) WHERE n.age > 25 RETURN n LIMIT 10".to_string(),
        );
        let ast = parser.parse().unwrap();
        let operators = executor.ast_to_operators(&ast).unwrap();

        assert_eq!(operators.len(), 4); // NodeByLabel, Filter, Project, Limit

        match &operators[0] {
            Operator::NodeByLabel {
                label_id: parsed_label_id,
                variable,
            } => {
                assert_eq!(*parsed_label_id, label_id);
                assert_eq!(variable, "n");
            }
            _ => panic!("Expected NodeByLabel operator"),
        }

        match &operators[1] {
            Operator::Filter { predicate } => {
                assert!(predicate.contains("n.age > 25"));
            }
            _ => panic!("Expected Filter operator"),
        }

        match &operators[2] {
            Operator::Project { columns } => {
                assert_eq!(columns, &vec!["n".to_string()]);
            }
            _ => panic!("Expected Project operator"),
        }

        match &operators[3] {
            Operator::Limit { count } => {
                assert_eq!(*count, 10);
            }
            _ => panic!("Expected Limit operator"),
        }
    }

    #[test]
    fn test_mvp_operators() {
        let (executor, _dir) = create_test_executor();

        let mut context = ExecutionContext::new(HashMap::new());

        // Test Union operator (MVP implementation)
        let left_op = Operator::NodeByLabel {
            label_id: 1,
            variable: "n".to_string(),
        };
        let right_op = Operator::NodeByLabel {
            label_id: 2,
            variable: "m".to_string(),
        };
        executor
            .execute_union(&mut context, &left_op, &right_op)
            .unwrap();
        // Should not panic (MVP implementation does nothing)

        // Test Join operator (MVP implementation)
        executor
            .execute_join(
                &mut context,
                &left_op,
                &right_op,
                JoinType::Inner,
                Some("n.id = m.id"),
            )
            .unwrap();
        // Should not panic (MVP implementation does nothing)

        // Test IndexScan operator (MVP implementation)
        executor
            .execute_index_scan(&mut context, IndexType::Label, "Person", "n")
            .unwrap();
        // Should not panic (MVP implementation does nothing)

        // Test Distinct operator (MVP implementation)
        executor
            .execute_distinct(&mut context, &["n.id".to_string()])
            .unwrap();
        // Should not panic (MVP implementation does nothing)
    }

    #[test]
    fn test_row_creation() {
        let row = Row {
            values: vec![Value::String("test".to_string()), Value::Number(42.into())],
        };
        assert_eq!(row.values.len(), 2);
    }

    #[test]
    fn test_result_set_creation() {
        let result_set = ResultSet {
            columns: vec!["name".to_string(), "age".to_string()],
            rows: vec![
                Row {
                    values: vec![Value::String("Alice".to_string()), Value::Number(25.into())],
                },
                Row {
                    values: vec![Value::String("Bob".to_string()), Value::Number(30.into())],
                },
            ],
        };
        assert_eq!(result_set.columns.len(), 2);
        assert_eq!(result_set.rows.len(), 2);
    }

    #[test]
    fn test_join_type_enum() {
        assert_eq!(JoinType::Inner, JoinType::Inner);
        assert_ne!(JoinType::Inner, JoinType::LeftOuter);
        assert_ne!(JoinType::Inner, JoinType::RightOuter);
        assert_ne!(JoinType::Inner, JoinType::FullOuter);
    }

    #[test]
    fn test_index_type_enum() {
        assert_eq!(IndexType::Label, IndexType::Label);
        assert_ne!(IndexType::Label, IndexType::Property);
        assert_ne!(IndexType::Label, IndexType::Vector);
        assert_ne!(IndexType::Label, IndexType::FullText);
    }

    #[test]
    fn test_execution_context_creation() {
        let mut params = HashMap::new();
        params.insert("name".to_string(), Value::String("Alice".to_string()));
        let context = ExecutionContext::new(params);
        assert!(context.params.contains_key("name"));
    }

    #[test]
    fn test_execution_context_variable_operations() {
        let mut context = ExecutionContext::new(HashMap::new());

        // Set and get variable
        context.set_variable("test", Value::String("value".to_string()));
        assert_eq!(
            context.get_variable("test"),
            Some(&Value::String("value".to_string()))
        );

        // Get non-existent variable
        assert!(context.get_variable("nonexistent").is_none());
    }

    #[test]
    fn test_execution_context_parameter_operations() {
        let mut params = HashMap::new();
        params.insert("param1".to_string(), Value::String("value1".to_string()));
        let context = ExecutionContext::new(params);

        // Get parameter
        assert_eq!(
            context.params.get("param1"),
            Some(&Value::String("value1".to_string()))
        );

        // Get non-existent parameter
        assert!(!context.params.contains_key("nonexistent"));
    }

    #[test]
    fn test_execution_context_clear() {
        let mut context = ExecutionContext::new(HashMap::new());
        context.set_variable("test", Value::String("value".to_string()));
        assert!(context.get_variable("test").is_some());

        context.variables.clear();
        assert!(context.get_variable("test").is_none());
    }
}
