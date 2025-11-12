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
use chrono::{Datelike, TimeZone};
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
        /// Projection expressions with aliases
        items: Vec<ProjectionItem>,
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
        /// Left operator pipeline
        left: Vec<Operator>,
        /// Right operator pipeline
        right: Vec<Operator>,
        /// Distinct flag (true = UNION, false = UNION ALL)
        distinct: bool,
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
    /// Create nodes and relationships from pattern
    Create {
        /// Pattern to create
        pattern: parser::Pattern,
    },
    /// Delete nodes (without detaching relationships)
    Delete {
        /// Variables to delete
        variables: Vec<String>,
    },
    /// Delete nodes and their relationships
    DetachDelete {
        /// Variables to delete
        variables: Vec<String>,
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
    /// Unwind a list into rows
    Unwind {
        /// Expression that evaluates to a list
        expression: String,
        /// Variable name to bind each list item
        variable: String,
    },
}

/// Projection entry describing an expression and its alias
#[derive(Debug, Clone)]
pub struct ProjectionItem {
    /// Expression to evaluate
    pub expression: parser::Expression,
    /// Alias to use in the result set
    pub alias: String,
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
        /// Distinct flag for COUNT(DISTINCT ...)
        distinct: bool,
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
    /// Collect values into array
    Collect {
        /// Column to collect
        column: String,
        /// Alias for result
        alias: String,
        /// Distinct flag for COLLECT(DISTINCT ...)
        distinct: bool,
    },
    /// Discrete percentile (nearest value)
    PercentileDisc {
        /// Column to calculate percentile
        column: String,
        /// Alias for result
        alias: String,
        /// Percentile value (0.0 to 1.0)
        percentile: f64,
    },
    /// Continuous percentile (interpolated)
    PercentileCont {
        /// Column to calculate percentile
        column: String,
        /// Alias for result
        alias: String,
        /// Percentile value (0.0 to 1.0)
        percentile: f64,
    },
    /// Sample standard deviation
    StDev {
        /// Column to calculate standard deviation
        column: String,
        /// Alias for result
        alias: String,
    },
    /// Population standard deviation
    StDevP {
        /// Column to calculate population standard deviation
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
        let mut projection_columns: Vec<String> = Vec::new();

        for operator in operators {
            match operator {
                Operator::NodeByLabel { label_id, variable } => {
                    let nodes = self.execute_node_by_label(label_id)?;
                    context.set_variable(&variable, Value::Array(nodes));
                    let rows = self.materialize_rows_from_variables(&context);
                    self.update_result_set_from_rows(&mut context, &rows);
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
                Operator::Project { items } => {
                    projection_columns = items.iter().map(|item| item.alias.clone()).collect();
                    results = self.execute_project(&mut context, &items)?;
                }
                Operator::Limit { count } => {
                    self.execute_limit(&mut context, count)?;
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
                Operator::Union {
                    left,
                    right,
                    distinct,
                } => {
                    self.execute_union(&mut context, &left, &right, distinct)?;
                }
                Operator::Create { pattern } => {
                    self.execute_create_with_context(&mut context, &pattern)?;
                }
                Operator::Delete { variables } => {
                    self.execute_delete(&mut context, &variables, false)?;
                }
                Operator::DetachDelete { variables } => {
                    self.execute_delete(&mut context, &variables, true)?;
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
                Operator::Unwind {
                    expression,
                    variable,
                } => {
                    self.execute_unwind(&mut context, &expression, &variable)?;
                }
            }
        }

        let final_columns = if !context.result_set.columns.is_empty() {
            context.result_set.columns.clone()
        } else {
            projection_columns
        };

        let final_rows = if !context.result_set.rows.is_empty() {
            context.result_set.rows.clone()
        } else {
            results
        };

        Ok(ResultSet {
            columns: final_columns,
            rows: final_rows,
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
    fn ast_to_operators(&mut self, ast: &parser::CypherQuery) -> Result<Vec<Operator>> {
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
                    let projection_items: Vec<ProjectionItem> = return_clause
                        .items
                        .iter()
                        .map(|item| ProjectionItem {
                            expression: item.expression.clone(),
                            alias: item.alias.clone().unwrap_or_else(|| {
                                self.expression_to_string(&item.expression)
                                    .unwrap_or_default()
                            }),
                        })
                        .collect();

                    operators.push(Operator::Project {
                        items: projection_items,
                    });
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
    fn execute_create_pattern(&mut self, pattern: &parser::Pattern) -> Result<()> {
        use crate::transaction::TransactionManager;
        use std::collections::HashMap;

        // Create a transaction manager for this operation
        let mut tx_mgr = TransactionManager::new()?;
        let mut tx = tx_mgr.begin_write()?;

        // Map of variable names to created node IDs
        let mut created_nodes: HashMap<String, u64> = HashMap::new();
        let mut last_node_id: Option<u64> = None;

        // Process pattern elements in sequence
        // Pattern alternates: Node -> Relationship -> Node -> Relationship ...
        for (i, element) in pattern.elements.iter().enumerate() {
            match element {
                parser::PatternElement::Node(node) => {
                    // Build label bitmap
                    let mut label_bits = 0u64;
                    for label in &node.labels {
                        let label_id = self.catalog.get_or_create_label(label)?;
                        if label_id < 64 {
                            label_bits |= 1u64 << label_id;
                        }
                    }

                    // Extract properties
                    let properties = if let Some(props_map) = &node.properties {
                        let mut json_props = serde_json::Map::new();
                        for (key, value_expr) in &props_map.properties {
                            let json_value = self.expression_to_json_value(value_expr)?;
                            json_props.insert(key.clone(), json_value);
                        }
                        serde_json::Value::Object(json_props)
                    } else {
                        serde_json::Value::Null
                    };

                    // Create the node
                    let node_id = self
                        .store
                        .create_node_with_label_bits(&mut tx, label_bits, properties)?;

                    // Store node ID if variable exists
                    if let Some(var) = &node.variable {
                        created_nodes.insert(var.clone(), node_id);
                    }

                    // Track last node for relationship creation
                    last_node_id = Some(node_id);
                }
                parser::PatternElement::Relationship(rel) => {
                    // Get source node (previous element should be a node)
                    let source_id = if i > 0 {
                        last_node_id.ok_or_else(|| {
                            Error::CypherExecution("Relationship must follow a node".to_string())
                        })?
                    } else {
                        return Err(Error::CypherExecution(
                            "Pattern must start with a node".to_string(),
                        ));
                    };

                    // Get target node (next element should be a node)
                    let target_id = if i + 1 < pattern.elements.len() {
                        if let parser::PatternElement::Node(target_node) = &pattern.elements[i + 1]
                        {
                            // Build label bitmap for target
                            let mut target_label_bits = 0u64;
                            for label in &target_node.labels {
                                let label_id = self.catalog.get_or_create_label(label)?;
                                if label_id < 64 {
                                    target_label_bits |= 1u64 << label_id;
                                }
                            }

                            // Extract target properties
                            let target_properties = if let Some(props_map) = &target_node.properties
                            {
                                let mut json_props = serde_json::Map::new();
                                for (key, value_expr) in &props_map.properties {
                                    let json_value = self.expression_to_json_value(value_expr)?;
                                    json_props.insert(key.clone(), json_value);
                                }
                                serde_json::Value::Object(json_props)
                            } else {
                                serde_json::Value::Null
                            };

                            // Create target node (we'll skip it in the next iteration)
                            let tid = self.store.create_node_with_label_bits(
                                &mut tx,
                                target_label_bits,
                                target_properties,
                            )?;

                            // Store target node ID if variable exists
                            if let Some(var) = &target_node.variable {
                                created_nodes.insert(var.clone(), tid);
                            }

                            last_node_id = Some(tid);
                            tid
                        } else {
                            return Err(Error::CypherExecution(
                                "Relationship must be followed by a node".to_string(),
                            ));
                        }
                    } else {
                        return Err(Error::CypherExecution(
                            "Pattern must end with a node".to_string(),
                        ));
                    };

                    // Get relationship type
                    let rel_type = rel.types.first().ok_or_else(|| {
                        Error::CypherExecution("Relationship must have a type".to_string())
                    })?;

                    let type_id = self.catalog.get_or_create_type(rel_type)?;

                    // Extract relationship properties
                    let rel_properties = if let Some(props_map) = &rel.properties {
                        let mut json_props = serde_json::Map::new();
                        for (key, value_expr) in &props_map.properties {
                            let json_value = self.expression_to_json_value(value_expr)?;
                            json_props.insert(key.clone(), json_value);
                        }
                        serde_json::Value::Object(json_props)
                    } else {
                        serde_json::Value::Null
                    };

                    // Create the relationship
                    self.store.create_relationship(
                        &mut tx,
                        source_id,
                        target_id,
                        type_id,
                        rel_properties,
                    )?;
                }
            }
        }

        // Commit transaction
        tx_mgr.commit(&mut tx)?;

        // Flush to ensure persistence
        self.store.flush()?;

        // Update label index with created nodes
        for node_id in created_nodes.values() {
            // Read the node to get its labels
            if let Ok(node_record) = self.store.read_node(*node_id) {
                let mut label_ids = Vec::new();
                for bit in 0..64 {
                    if (node_record.label_bits & (1u64 << bit)) != 0 {
                        label_ids.push(bit as u32);
                    }
                }
                if !label_ids.is_empty() {
                    self.label_index.add_node(*node_id, &label_ids)?;
                }
            }
        }

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
        let bitmap = if label_id == 0 {
            // Special case: scan all nodes
            // Get all nodes from storage
            let total_nodes = self.store.node_count();
            let mut all_nodes = roaring::RoaringBitmap::new();
            for node_id in 0..total_nodes.min(u32::MAX as u64) {
                all_nodes.insert(node_id as u32);
            }
            all_nodes
        } else {
            self.label_index.get_nodes(label_id)?
        };

        let mut results = Vec::new();

        for node_id in bitmap.iter() {
            // Skip deleted nodes
            if let Ok(node_record) = self.store.read_node(node_id as u64) {
                if node_record.is_deleted() {
                    continue;
                }
            }

            match self.read_node_as_value(node_id as u64)? {
                Value::Null => continue,
                value => results.push(value),
            }
        }

        Ok(results)
    }

    /// Execute Filter operator
    fn execute_filter(&self, context: &mut ExecutionContext, predicate: &str) -> Result<()> {
        // Check for label check pattern: variable:Label
        if predicate.contains(':') && !predicate.contains("::") {
            let parts: Vec<&str> = predicate.split(':').collect();
            if parts.len() == 2 && !parts[0].contains(' ') && !parts[1].contains(' ') {
                // This is a label check: variable:Label
                let variable = parts[0].trim();
                let label_name = parts[1].trim();

                // Get label ID
                if let Ok(label_id) = self.catalog.get_label_id(label_name) {
                    // Filter rows where variable has this label
                    let rows = self.materialize_rows_from_variables(context);
                    let mut filtered_rows = Vec::new();

                    for row in rows {
                        if let Some(Value::Object(obj)) = row.get(variable) {
                            if let Some(Value::Number(id)) = obj.get("_nexus_id") {
                                if let Some(node_id) = id.as_u64() {
                                    // Read node and check if it has the label
                                    if let Ok(node_record) = self.store.read_node(node_id) {
                                        let has_label =
                                            (node_record.label_bits & (1u64 << label_id)) != 0;
                                        if has_label {
                                            filtered_rows.push(row);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    self.update_variables_from_rows(context, &filtered_rows);
                    self.update_result_set_from_rows(context, &filtered_rows);
                    return Ok(());
                }
            }
        }

        // Regular predicate expression
        let mut parser = parser::CypherParser::new(predicate.to_string());
        let expr = parser.parse_expression()?;

        let rows = self.materialize_rows_from_variables(context);
        let mut filtered_rows = Vec::new();

        for row in rows {
            if self.evaluate_predicate_on_row(&row, context, &expr)? {
                filtered_rows.push(row);
            }
        }

        self.update_variables_from_rows(context, &filtered_rows);
        self.update_result_set_from_rows(context, &filtered_rows);
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
        // Use result_set rows instead of variables to maintain row context from previous operators
        let rows = if !context.result_set.rows.is_empty() {
            self.result_set_as_rows(context)
        } else {
            self.materialize_rows_from_variables(context)
        };
        let mut expanded_rows = Vec::new();

        // Special case: if source_var is empty or rows is empty, scan all relationships directly
        // This handles queries like MATCH ()-[r:MENTIONS]->() RETURN count(r)
        if source_var.is_empty() || rows.is_empty() {
            // Scan all relationships from storage
            let total_rels = self.store.relationship_count();
            for rel_id in 0..total_rels {
                if let Ok(rel_record) = self.store.read_rel(rel_id) {
                    if rel_record.is_deleted() {
                        continue;
                    }

                    let matches_type = type_id.is_none() || Some(rel_record.type_id) == type_id;
                    if !matches_type {
                        continue;
                    }

                    let rel_info = RelationshipInfo {
                        id: rel_id,
                        source_id: rel_record.src_id,
                        target_id: rel_record.dst_id,
                        type_id: rel_record.type_id,
                    };

                    // For bidirectional patterns, return each relationship twice (once for each direction)
                    let directions_to_emit = match direction {
                        Direction::Outgoing | Direction::Incoming => vec![direction],
                        Direction::Both => vec![Direction::Outgoing, Direction::Incoming],
                    };

                    for emit_direction in directions_to_emit {
                        let mut new_row = HashMap::new();

                        // Determine target based on direction
                        let target_id = match emit_direction {
                            Direction::Outgoing => rel_record.dst_id,
                            Direction::Incoming => rel_record.src_id,
                            Direction::Both => unreachable!(),
                        };

                        // Only add target node if target_var is specified
                        if !target_var.is_empty() {
                            let target_node = self.read_node_as_value(target_id)?;
                            new_row.insert(target_var.to_string(), target_node);
                        }

                        if !rel_var.is_empty() {
                            let relationship_value = self.read_relationship_as_value(&rel_info)?;
                            new_row.insert(rel_var.to_string(), relationship_value);
                        }

                        expanded_rows.push(new_row);
                    }
                }
            }
        } else {
            // Normal case: expand from source nodes
            // Only apply target filtering if the target variable is already populated
            // (this happens when we're doing a join-like operation, not a pure expansion)
            let allowed_target_ids: Option<std::collections::HashSet<u64>> =
                if target_var.is_empty() {
                    None
                } else {
                    context
                        .get_variable(target_var)
                        .and_then(|value| match value {
                            Value::Array(values) => {
                                let ids: std::collections::HashSet<u64> =
                                    values.iter().filter_map(Self::extract_entity_id).collect();
                                // Only use the set if it's not empty (empty set means "filter everything out")
                                if ids.is_empty() { None } else { Some(ids) }
                            }
                            _ => None,
                        })
                };

            for row in rows {
                let source_value = row
                    .get(source_var)
                    .cloned()
                    .or_else(|| context.get_variable(source_var).cloned())
                    .unwrap_or(Value::Null);

                let source_id = match Self::extract_entity_id(&source_value) {
                    Some(id) => id,
                    None => continue,
                };

                let relationships = self.find_relationships(source_id, type_id, direction)?;
                if relationships.is_empty() {
                    continue;
                }

                for rel_info in relationships {
                    let target_id = match direction {
                        Direction::Outgoing => rel_info.target_id,
                        Direction::Incoming => rel_info.source_id,
                        Direction::Both => {
                            if rel_info.source_id == source_id {
                                rel_info.target_id
                            } else {
                                rel_info.source_id
                            }
                        }
                    };

                    let target_node = self.read_node_as_value(target_id)?;

                    if let Some(ref allowed) = allowed_target_ids {
                        // Only filter if allowed set is non-empty and doesn't contain target
                        if !allowed.is_empty() && !allowed.contains(&target_id) {
                            continue;
                        }
                    }

                    let mut new_row = row.clone();
                    new_row.insert(source_var.to_string(), source_value.clone());
                    new_row.insert(target_var.to_string(), target_node);

                    if !rel_var.is_empty() {
                        let relationship_value = self.read_relationship_as_value(&rel_info)?;
                        new_row.insert(rel_var.to_string(), relationship_value);
                    }

                    expanded_rows.push(new_row);
                }
            }
        }

        self.update_variables_from_rows(context, &expanded_rows);
        self.update_result_set_from_rows(context, &expanded_rows);

        Ok(())
    }

    /// Execute DELETE or DETACH DELETE operator
    /// Note: This collects node IDs but doesn't actually delete them.
    /// Actual deletion must be handled at Engine level (lib.rs) before executor runs.
    fn execute_delete(
        &self,
        context: &mut ExecutionContext,
        _variables: &[String],
        _detach: bool,
    ) -> Result<()> {
        // DELETE is handled at Engine level (lib.rs) like CREATE
        // This function is called AFTER deletion has already occurred
        // We just need to clear the result set

        // Clear the result set since deleted nodes shouldn't be returned
        context.result_set.rows.clear();
        context.variables.clear();

        Ok(())
    }

    /// Execute Project operator
    fn execute_project(
        &self,
        context: &mut ExecutionContext,
        items: &[ProjectionItem],
    ) -> Result<Vec<Row>> {
        // Use existing result_set.rows if available (from UNWIND, etc), otherwise materialize from variables
        let rows = if !context.result_set.rows.is_empty() {
            // Convert existing rows to row maps for projection
            let existing_columns = context.result_set.columns.clone();
            context
                .result_set
                .rows
                .iter()
                .map(|row| self.row_to_map(row, &existing_columns))
                .collect()
        } else {
            let materialized = self.materialize_rows_from_variables(context);
            if materialized.is_empty()
                && context.variables.is_empty()
                && context.result_set.columns.is_empty()
            {
                // No rows from variables AND no variables AND no previous operations - create single empty row for literal/function evaluation
                // This handles: RETURN 1+1 AS result, toLower('HELLO') AS lower
                // But NOT: MATCH (n:NonExistent) RETURN n (which should return 0 rows)
                // And NOT: UNWIND [] AS x RETURN x (which should return 0 rows)
                vec![std::collections::HashMap::new()]
            } else {
                materialized
            }
        };

        let mut projected_rows = Vec::new();

        for row_map in &rows {
            let mut values = Vec::with_capacity(items.len());
            for item in items {
                let value =
                    self.evaluate_projection_expression(row_map, context, &item.expression)?;
                values.push(value);
            }
            projected_rows.push(Row { values });
        }

        context.result_set.columns = items.iter().map(|item| item.alias.clone()).collect();
        context.result_set.rows = projected_rows.clone();
        let row_maps = self.result_set_as_rows(context);
        self.update_variables_from_rows(context, &row_maps);

        Ok(projected_rows)
    }

    /// Execute Limit operator
    fn execute_limit(&self, context: &mut ExecutionContext, count: usize) -> Result<()> {
        if context.result_set.rows.is_empty() {
            let rows = self.materialize_rows_from_variables(context);
            self.update_result_set_from_rows(context, &rows);
        }

        if context.result_set.rows.len() > count {
            context.result_set.rows.truncate(count);
        }

        let row_maps = self.result_set_as_rows(context);
        self.update_variables_from_rows(context, &row_maps);
        Ok(())
    }

    /// Execute Sort operator
    fn execute_sort(
        &self,
        context: &mut ExecutionContext,
        columns: &[String],
        ascending: &[bool],
    ) -> Result<()> {
        if context.result_set.rows.is_empty() && !context.variables.is_empty() {
            let rows = self.materialize_rows_from_variables(context);
            self.update_result_set_from_rows(context, &rows);
        }

        if context.result_set.rows.is_empty() {
            return Ok(());
        }

        context.result_set.rows.sort_by(|a, b| {
            for (idx, column) in columns.iter().enumerate() {
                let col_idx = self
                    .get_column_index(column, &context.result_set.columns)
                    .unwrap_or(usize::MAX);
                if col_idx == usize::MAX {
                    continue;
                }
                let asc = ascending.get(idx).copied().unwrap_or(true);
                let left = a.values.get(col_idx).cloned().unwrap_or(Value::Null);
                let right = b.values.get(col_idx).cloned().unwrap_or(Value::Null);
                let ordering = self.compare_values_for_sort(&left, &right);
                if ordering != std::cmp::Ordering::Equal {
                    return if asc { ordering } else { ordering.reverse() };
                }
            }
            std::cmp::Ordering::Equal
        });

        let row_maps = self.result_set_as_rows(context);
        self.update_variables_from_rows(context, &row_maps);
        self.update_result_set_from_rows(context, &row_maps);
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

        if context.result_set.rows.is_empty() && !context.variables.is_empty() {
            let rows = self.materialize_rows_from_variables(context);
            self.update_result_set_from_rows(context, &rows);
        }

        let rows = context.result_set.rows.clone();
        let mut groups: HashMap<Vec<Value>, Vec<Row>> = HashMap::new();

        for row in rows {
            let mut group_key = Vec::new();
            for col in group_by {
                if let Some(index) = self.get_column_index(col, &context.result_set.columns) {
                    if index < row.values.len() {
                        group_key.push(row.values[index].clone());
                    } else {
                        group_key.push(Value::Null);
                    }
                } else {
                    group_key.push(Value::Null);
                }
            }

            groups.entry(group_key).or_default().push(row);
        }

        context.result_set.rows.clear();

        // Check if we have an empty result set with aggregations but no GROUP BY
        let is_empty_aggregation = groups.is_empty() && group_by.is_empty();

        for (group_key, group_rows) in groups {
            let mut result_row = group_key;
            for agg in aggregations {
                let agg_value = match agg {
                    Aggregation::Count {
                        column, distinct, ..
                    } => {
                        if column.is_none() {
                            // COUNT(*) - just count rows
                            Value::Number(serde_json::Number::from(group_rows.len()))
                        } else {
                            let col_name = column.as_ref().unwrap();
                            let col_idx =
                                self.get_column_index(col_name, &context.result_set.columns);
                            let count = if let Some(idx) = col_idx {
                                if *distinct {
                                    // COUNT(DISTINCT col) - collect unique values
                                    let unique_values: std::collections::HashSet<_> = group_rows
                                        .iter()
                                        .filter(|row| {
                                            idx < row.values.len() && !row.values[idx].is_null()
                                        })
                                        .map(|row| row.values[idx].to_string())
                                        .collect();
                                    unique_values.len()
                                } else {
                                    // COUNT(col) - count non-null values
                                    group_rows
                                        .iter()
                                        .filter(|row| {
                                            idx < row.values.len() && !row.values[idx].is_null()
                                        })
                                        .count()
                                }
                            } else {
                                0
                            };
                            Value::Number(serde_json::Number::from(count))
                        }
                    }
                    Aggregation::Sum { column, .. } => {
                        let col_idx = self.get_column_index(column, &context.result_set.columns);
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
                    Aggregation::Avg { column, .. } => {
                        let col_idx = self.get_column_index(column, &context.result_set.columns);
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
                    Aggregation::Min { column, .. } => {
                        let col_idx = self.get_column_index(column, &context.result_set.columns);
                        if let Some(idx) = col_idx {
                            // Find minimum value while preserving original type
                            let min_val = group_rows
                                .iter()
                                .filter_map(|row| {
                                    if idx < row.values.len() && !row.values[idx].is_null() {
                                        Some(&row.values[idx])
                                    } else {
                                        None
                                    }
                                })
                                .min_by(|a, b| {
                                    // Compare as numbers
                                    let a_num = self.value_to_number(a).ok();
                                    let b_num = self.value_to_number(b).ok();
                                    match (a_num, b_num) {
                                        (Some(an), Some(bn)) => {
                                            an.partial_cmp(&bn).unwrap_or(std::cmp::Ordering::Equal)
                                        }
                                        _ => std::cmp::Ordering::Equal,
                                    }
                                });
                            min_val.cloned().unwrap_or(Value::Null)
                        } else {
                            Value::Null
                        }
                    }
                    Aggregation::Max { column, .. } => {
                        let col_idx = self.get_column_index(column, &context.result_set.columns);
                        if let Some(idx) = col_idx {
                            // Find maximum value while preserving original type
                            let max_val = group_rows
                                .iter()
                                .filter_map(|row| {
                                    if idx < row.values.len() && !row.values[idx].is_null() {
                                        Some(&row.values[idx])
                                    } else {
                                        None
                                    }
                                })
                                .max_by(|a, b| {
                                    // Compare as numbers
                                    let a_num = self.value_to_number(a).ok();
                                    let b_num = self.value_to_number(b).ok();
                                    match (a_num, b_num) {
                                        (Some(an), Some(bn)) => {
                                            an.partial_cmp(&bn).unwrap_or(std::cmp::Ordering::Equal)
                                        }
                                        _ => std::cmp::Ordering::Equal,
                                    }
                                });
                            max_val.cloned().unwrap_or(Value::Null)
                        } else {
                            Value::Null
                        }
                    }
                    Aggregation::Collect {
                        column, distinct, ..
                    } => {
                        let col_idx = self.get_column_index(column, &context.result_set.columns);
                        if let Some(idx) = col_idx {
                            let values: Vec<Value> = if *distinct {
                                // COLLECT(DISTINCT col) - collect unique values
                                let unique_values: std::collections::HashSet<String> = group_rows
                                    .iter()
                                    .filter_map(|row| {
                                        if idx < row.values.len() && !row.values[idx].is_null() {
                                            Some(row.values[idx].to_string())
                                        } else {
                                            None
                                        }
                                    })
                                    .collect();
                                // Convert back to original values (sorted for determinism)
                                let mut sorted: Vec<_> = unique_values.into_iter().collect();
                                sorted.sort();
                                group_rows
                                    .iter()
                                    .filter_map(|row| {
                                        if idx < row.values.len() && !row.values[idx].is_null() {
                                            let val_str = row.values[idx].to_string();
                                            if sorted.contains(&val_str) {
                                                sorted.retain(|s| s != &val_str);
                                                Some(row.values[idx].clone())
                                            } else {
                                                None
                                            }
                                        } else {
                                            None
                                        }
                                    })
                                    .collect()
                            } else {
                                // COLLECT(col) - collect all non-null values
                                group_rows
                                    .iter()
                                    .filter_map(|row| {
                                        if idx < row.values.len() && !row.values[idx].is_null() {
                                            Some(row.values[idx].clone())
                                        } else {
                                            None
                                        }
                                    })
                                    .collect()
                            };
                            Value::Array(values)
                        } else {
                            Value::Array(Vec::new())
                        }
                    }
                    Aggregation::PercentileDisc {
                        column, percentile, ..
                    } => {
                        let col_idx = self.get_column_index(column, &context.result_set.columns);
                        if let Some(idx) = col_idx {
                            let mut values: Vec<f64> = group_rows
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
                                values.sort_by(|a, b| {
                                    a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
                                });
                                // Discrete percentile: nearest value
                                let index = ((*percentile * (values.len() - 1) as f64).round()
                                    as usize)
                                    .min(values.len() - 1);
                                Value::Number(
                                    serde_json::Number::from_f64(values[index])
                                        .unwrap_or(serde_json::Number::from(0)),
                                )
                            }
                        } else {
                            Value::Null
                        }
                    }
                    Aggregation::PercentileCont {
                        column, percentile, ..
                    } => {
                        let col_idx = self.get_column_index(column, &context.result_set.columns);
                        if let Some(idx) = col_idx {
                            let mut values: Vec<f64> = group_rows
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
                                values.sort_by(|a, b| {
                                    a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
                                });
                                // Continuous percentile: linear interpolation
                                let position = *percentile * (values.len() - 1) as f64;
                                let lower_idx = position.floor() as usize;
                                let upper_idx = position.ceil() as usize;

                                let result = if lower_idx == upper_idx {
                                    values[lower_idx]
                                } else {
                                    let lower = values[lower_idx];
                                    let upper = values[upper_idx];
                                    let fraction = position - lower_idx as f64;
                                    lower + (upper - lower) * fraction
                                };

                                Value::Number(
                                    serde_json::Number::from_f64(result)
                                        .unwrap_or(serde_json::Number::from(0)),
                                )
                            }
                        } else {
                            Value::Null
                        }
                    }
                    Aggregation::StDev { column, .. } => {
                        let col_idx = self.get_column_index(column, &context.result_set.columns);
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

                            if values.len() < 2 {
                                Value::Null
                            } else {
                                // Sample standard deviation (Bessel's correction: n-1)
                                let mean = values.iter().sum::<f64>() / values.len() as f64;
                                let variance = values
                                    .iter()
                                    .map(|v| {
                                        let diff = v - mean;
                                        diff * diff
                                    })
                                    .sum::<f64>()
                                    / (values.len() - 1) as f64;
                                let std_dev = variance.sqrt();
                                Value::Number(
                                    serde_json::Number::from_f64(std_dev)
                                        .unwrap_or(serde_json::Number::from(0)),
                                )
                            }
                        } else {
                            Value::Null
                        }
                    }
                    Aggregation::StDevP { column, .. } => {
                        let col_idx = self.get_column_index(column, &context.result_set.columns);
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
                                // Population standard deviation (divide by n)
                                let mean = values.iter().sum::<f64>() / values.len() as f64;
                                let variance = values
                                    .iter()
                                    .map(|v| {
                                        let diff = v - mean;
                                        diff * diff
                                    })
                                    .sum::<f64>()
                                    / values.len() as f64;
                                let std_dev = variance.sqrt();
                                Value::Number(
                                    serde_json::Number::from_f64(std_dev)
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

        // If no groups and no GROUP BY, still return one row with aggregation values
        // This handles cases like: MATCH (n:NonExistent) RETURN count(*)
        if is_empty_aggregation {
            let mut result_row = Vec::new();
            for agg in aggregations {
                let agg_value = match agg {
                    Aggregation::Count { .. } => {
                        // COUNT on empty set returns 0
                        Value::Number(serde_json::Number::from(0))
                    }
                    Aggregation::Collect { .. } => {
                        // COLLECT on empty set returns empty array
                        Value::Array(Vec::new())
                    }
                    _ => {
                        // AVG/MIN/MAX/SUM on empty set return NULL
                        Value::Null
                    }
                };
                result_row.push(agg_value);
            }
            context.result_set.rows.push(Row { values: result_row });
        }

        let mut columns = group_by.to_vec();
        columns.extend(aggregations.iter().map(|agg| self.aggregation_alias(agg)));
        context.result_set.columns = columns;

        let row_maps = self.result_set_as_rows(context);
        self.update_variables_from_rows(context, &row_maps);

        Ok(())
    }

    /// Execute Union operator
    fn execute_union(
        &self,
        context: &mut ExecutionContext,
        left: &[Operator],
        right: &[Operator],
        distinct: bool,
    ) -> Result<()> {
        // Execute left operator pipeline and collect its results
        let mut left_context = ExecutionContext::new(context.params.clone());
        for operator in left {
            self.execute_operator(&mut left_context, operator)?;
        }

        // Execute right operator pipeline and collect its results
        let mut right_context = ExecutionContext::new(context.params.clone());
        for operator in right {
            self.execute_operator(&mut right_context, operator)?;
        }

        // Combine results from both sides
        let mut combined_rows = Vec::new();
        combined_rows.extend(left_context.result_set.rows);
        combined_rows.extend(right_context.result_set.rows);

        // If UNION (not UNION ALL), deduplicate results
        if distinct {
            let mut seen = std::collections::HashSet::new();
            let mut deduped_rows = Vec::new();

            for row in combined_rows {
                // Serialize row values to a string for comparison
                let row_key = serde_json::to_string(&row.values).unwrap_or_default();
                if seen.insert(row_key) {
                    deduped_rows.push(row);
                }
            }
            combined_rows = deduped_rows;
        }

        // Use columns from left context (both sides should have same columns)
        let columns = if !left_context.result_set.columns.is_empty() {
            left_context.result_set.columns.clone()
        } else {
            right_context.result_set.columns.clone()
        };

        // Update the main context with combined results
        context.set_columns_and_rows(columns, combined_rows);
        let row_maps = self.result_set_as_rows(context);
        self.update_variables_from_rows(context, &row_maps);
        Ok(())
    }

    /// Execute CREATE operator with context from MATCH
    fn execute_create_with_context(
        &self,
        context: &mut ExecutionContext,
        pattern: &parser::Pattern,
    ) -> Result<()> {
        use serde_json::Value as JsonValue;

        // Get current rows from context (from MATCH)
        let current_rows = self.materialize_rows_from_variables(context);

        // If no rows from MATCH, nothing to create
        if current_rows.is_empty() {
            return Ok(());
        }

        // For each row in the MATCH result, create the pattern
        for row in &current_rows {
            let mut node_ids: std::collections::HashMap<String, u64> =
                std::collections::HashMap::new();

            // First, resolve existing node variables from the row
            for (var_name, var_value) in row {
                if let JsonValue::Object(obj) = var_value {
                    if let Some(JsonValue::Number(id)) = obj.get("_nexus_id") {
                        if let Some(node_id) = id.as_u64() {
                            node_ids.insert(var_name.clone(), node_id);
                        }
                    }
                }
            }

            // Now process the pattern elements to create new nodes and relationships
            let mut last_node_var: Option<String> = None;

            for element in &pattern.elements {
                match element {
                    parser::PatternElement::Node(node) => {
                        if let Some(var) = &node.variable {
                            if !node_ids.contains_key(var) {
                                // Create new node (not from MATCH)
                                let _labels: Vec<u64> = node
                                    .labels
                                    .iter()
                                    .filter_map(|l| self.catalog.get_or_create_label(l).ok())
                                    .map(|id| id as u64)
                                    .collect();

                                let mut _label_bits = 0u64;
                                for label_id in _labels {
                                    _label_bits |= 1u64 << label_id;
                                }

                                // Extract properties
                                let _properties = if let Some(props_map) = &node.properties {
                                    JsonValue::Object(
                                        props_map
                                            .properties
                                            .iter()
                                            .filter_map(|(k, v)| {
                                                self.expression_to_json_value(v)
                                                    .ok()
                                                    .map(|val| (k.clone(), val))
                                            })
                                            .collect(),
                                    )
                                } else {
                                    JsonValue::Object(serde_json::Map::new())
                                };

                                // Executor can't create nodes - no mutable Transaction
                                // Skip creating new nodes in executor context
                            }

                            // Track this node as the last one for relationship creation
                            last_node_var = Some(var.clone());
                        }
                    }
                    parser::PatternElement::Relationship(rel) => {
                        // Create relationship between last_node and next_node
                        if let Some(rel_type) = rel.types.first() {
                            let _type_id = self.catalog.get_or_create_type(rel_type)?;

                            // Extract relationship properties
                            let _properties = if let Some(props_map) = &rel.properties {
                                JsonValue::Object(
                                    props_map
                                        .properties
                                        .iter()
                                        .filter_map(|(k, v)| {
                                            self.expression_to_json_value(v)
                                                .ok()
                                                .map(|val| (k.clone(), val))
                                        })
                                        .collect(),
                                )
                            } else {
                                JsonValue::Object(serde_json::Map::new())
                            };

                            // Source is the last_node_var, target will be the next node in pattern
                            // We need to peek ahead to find the target node variable
                            if let Some(source_var) = &last_node_var {
                                if let Some(_source_id) = node_ids.get(source_var) {
                                    // Find target node (next element after this relationship)
                                    let current_idx = pattern
                                        .elements
                                        .iter()
                                        .position(|e| {
                                            matches!(e, parser::PatternElement::Relationship(_))
                                        })
                                        .unwrap_or(0);

                                    if current_idx + 1 < pattern.elements.len() {
                                        if let parser::PatternElement::Node(target_node) =
                                            &pattern.elements[current_idx + 1]
                                        {
                                            if let Some(target_var) = &target_node.variable {
                                                if let Some(_target_id) = node_ids.get(target_var) {
                                                    // Executor can't create relationships - no mutable Transaction
                                                    // Skip creating relationships in executor context
                                                }
                                            }
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
            Operator::Project { items } => {
                self.execute_project(context, items)?;
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
            Operator::Union {
                left,
                right,
                distinct,
            } => {
                self.execute_union(context, left, right, *distinct)?;
            }
            Operator::Create { pattern } => {
                self.execute_create_with_context(context, pattern)?;
            }
            Operator::Delete { variables } => {
                self.execute_delete(context, variables, false)?;
            }
            Operator::DetachDelete { variables } => {
                self.execute_delete(context, variables, true)?;
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
            Operator::Unwind {
                expression,
                variable,
            } => {
                self.execute_unwind(context, expression, variable)?;
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
        let row_maps = self.result_set_as_rows(context);
        self.update_variables_from_rows(context, &row_maps);

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
        let rows = self.materialize_rows_from_variables(context);
        self.update_result_set_from_rows(context, &rows);

        Ok(())
    }

    /// Execute Distinct operator
    fn execute_distinct(&self, context: &mut ExecutionContext, columns: &[String]) -> Result<()> {
        if context.result_set.rows.is_empty() && !context.variables.is_empty() {
            let rows = self.materialize_rows_from_variables(context);
            self.update_result_set_from_rows(context, &rows);
        }

        if context.result_set.rows.is_empty() {
            return Ok(());
        }

        let mut seen = std::collections::HashSet::new();
        let mut distinct_rows = Vec::new();

        for row in &context.result_set.rows {
            let mut key_values = Vec::new();
            if columns.is_empty() {
                key_values = row.values.clone();
            } else {
                for column in columns {
                    if let Some(index) = self.get_column_index(column, &context.result_set.columns)
                    {
                        if index < row.values.len() {
                            key_values.push(row.values[index].clone());
                        } else {
                            key_values.push(Value::Null);
                        }
                    } else {
                        key_values.push(Value::Null);
                    }
                }
            }

            let key = serde_json::to_string(&key_values).unwrap_or_default();
            if seen.insert(key) {
                distinct_rows.push(row.clone());
            }
        }

        context.result_set.rows = distinct_rows.clone();
        let row_maps = self.result_set_as_rows(context);
        self.update_variables_from_rows(context, &row_maps);
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
                    parser::BinaryOperator::StartsWith => {
                        let left_str = self.value_to_string(&left_val);
                        let right_str = self.value_to_string(&right_val);
                        Ok(left_str.starts_with(&right_str))
                    }
                    parser::BinaryOperator::EndsWith => {
                        let left_str = self.value_to_string(&left_val);
                        let right_str = self.value_to_string(&right_val);
                        Ok(left_str.ends_with(&right_str))
                    }
                    parser::BinaryOperator::Contains => {
                        let left_str = self.value_to_string(&left_val);
                        let right_str = self.value_to_string(&right_val);
                        Ok(left_str.contains(&right_str))
                    }
                    parser::BinaryOperator::RegexMatch => {
                        let left_str = self.value_to_string(&left_val);
                        let right_str = self.value_to_string(&right_val);
                        // Use regex crate for pattern matching
                        match regex::Regex::new(&right_str) {
                            Ok(re) => Ok(re.is_match(&left_str)),
                            Err(_) => Ok(false), // Invalid regex pattern returns false
                        }
                    }
                    _ => Ok(false), // Other operators not implemented
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
            parser::Expression::IsNull { expr, negated } => {
                let value = self.evaluate_expression(node, expr, context)?;
                let is_null = value.is_null();
                Ok(if *negated { !is_null } else { is_null })
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
            parser::Expression::Case {
                input,
                when_clauses,
                else_clause,
            } => {
                // Evaluate input expression if present (generic CASE)
                let input_value = if let Some(input_expr) = input {
                    Some(self.evaluate_expression(node, input_expr, context)?)
                } else {
                    None
                };

                // Evaluate WHEN clauses
                for when_clause in when_clauses {
                    let condition_value =
                        self.evaluate_expression(node, &when_clause.condition, context)?;

                    // For generic CASE: compare input with condition
                    // For simple CASE: evaluate condition as boolean
                    let matches = if let Some(ref input_val) = input_value {
                        // Generic CASE: input == condition
                        input_val == &condition_value
                    } else {
                        // Simple CASE: condition is boolean expression
                        self.value_to_bool(&condition_value)?
                    };

                    if matches {
                        return self.evaluate_expression(node, &when_clause.result, context);
                    }
                }

                // No WHEN clause matched, return ELSE or NULL
                if let Some(else_expr) = else_clause {
                    self.evaluate_expression(node, else_expr, context)
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
            Value::Null => Err(Error::TypeMismatch {
                expected: "number".to_string(),
                actual: "null".to_string(),
            }),
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

            while rel_ptr != 0 {
                let current_rel_id = rel_ptr.saturating_sub(1);
                if let Ok(rel_record) = self.store.read_rel(current_rel_id) {
                    if rel_record.is_deleted() {
                        rel_ptr = if rel_record.src_id == node_id {
                            rel_record.next_src_ptr
                        } else {
                            rel_record.next_dst_ptr
                        };
                        continue;
                    }

                    let matches_type = type_id.is_none() || Some(rel_record.type_id) == type_id;
                    let matches_direction = match direction {
                        Direction::Outgoing => rel_record.src_id == node_id,
                        Direction::Incoming => rel_record.dst_id == node_id,
                        Direction::Both => true,
                    };

                    if matches_type && matches_direction {
                        relationships.push(RelationshipInfo {
                            id: current_rel_id,
                            source_id: rel_record.src_id,
                            target_id: rel_record.dst_id,
                            type_id: rel_record.type_id,
                        });
                    }

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

        if node_record.is_deleted() {
            return Ok(Value::Null);
        }

        let label_names = self
            .catalog
            .get_labels_from_bitmap(node_record.label_bits)?;
        let _labels: Vec<Value> = label_names.into_iter().map(Value::String).collect();

        let properties_value = self.store.load_node_properties(node_id)?;

        let properties_value = properties_value.unwrap_or_else(|| Value::Object(Map::new()));

        let properties_map = match properties_value {
            Value::Object(map) => map,
            other => {
                let mut map = Map::new();
                map.insert("value".to_string(), other);
                map
            }
        };

        // Return only the properties as a flat object, matching Neo4j's format
        // But include _nexus_id for internal ID extraction during relationship traversal
        let mut node = properties_map;
        node.insert("_nexus_id".to_string(), Value::Number(node_id.into()));

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

    /// Execute UNWIND operator - expands a list into rows
    fn execute_unwind(
        &self,
        context: &mut ExecutionContext,
        expression: &str,
        variable: &str,
    ) -> Result<()> {
        // Materialize rows from variables if needed (like execute_distinct does)
        if context.result_set.rows.is_empty() && !context.variables.is_empty() {
            let rows = self.materialize_rows_from_variables(context);
            self.update_result_set_from_rows(context, &rows);
        }

        // Parse the expression string
        let mut parser_instance = parser::CypherParser::new(expression.to_string());
        let parsed_expr = parser_instance.parse_expression().map_err(|e| {
            Error::CypherSyntax(format!("Failed to parse UNWIND expression: {}", e))
        })?;

        // If no existing rows, evaluate expression once and create new rows
        if context.result_set.rows.is_empty() {
            // Evaluate expression with empty row context
            let empty_row = HashMap::new();
            let list_value =
                self.evaluate_projection_expression(&empty_row, context, &parsed_expr)?;

            // Convert to array if needed
            let list_items = match list_value {
                Value::Array(items) => items,
                Value::Null => Vec::new(), // NULL list produces no rows
                other => vec![other],      // Single value wraps into single-item list
            };

            // Add variable as column
            context.result_set.columns.push(variable.to_string());

            // Create one row per list item
            for item in list_items {
                let row = Row { values: vec![item] };
                context.result_set.rows.push(row);
            }
        } else {
            // Expand existing rows: for each existing row, evaluate expression and create N new rows
            let existing_rows = std::mem::take(&mut context.result_set.rows);
            let existing_columns = context.result_set.columns.clone();

            // Find or add variable column index
            let var_col_idx = if let Some(idx) = self.get_column_index(variable, &existing_columns)
            {
                idx
            } else {
                // Add new column
                context.result_set.columns.push(variable.to_string());
                existing_columns.len()
            };

            // For each existing row, evaluate expression and create new rows with each list item
            for existing_row in existing_rows.iter() {
                // Convert Row to HashMap for evaluation
                let row_map = self.row_to_map(existing_row, &existing_columns);

                // Evaluate expression in context of this row
                let list_value =
                    self.evaluate_projection_expression(&row_map, context, &parsed_expr)?;

                // Convert to array if needed
                let list_items = match list_value {
                    Value::Array(items) => items,
                    Value::Null => Vec::new(), // NULL list produces no rows
                    other => vec![other],      // Single value wraps into single-item list
                };

                if list_items.is_empty() {
                    // Empty list produces no rows (Cartesian product with empty set)
                    continue;
                }

                for item in &list_items {
                    let mut new_values = existing_row.values.clone();

                    // If var_col_idx equals existing length, append; otherwise replace
                    if var_col_idx >= new_values.len() {
                        new_values.resize(var_col_idx + 1, Value::Null);
                    }
                    new_values[var_col_idx] = item.clone();

                    let new_row = Row { values: new_values };
                    context.result_set.rows.push(new_row);
                }
            }
        }

        Ok(())
    }

    /// Convert Row to HashMap for expression evaluation
    fn row_to_map(&self, row: &Row, columns: &[String]) -> HashMap<String, Value> {
        let mut map = HashMap::new();
        for (idx, col_name) in columns.iter().enumerate() {
            if let Some(value) = row.values.get(idx) {
                map.insert(col_name.clone(), value.clone());
            }
        }
        map
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

    fn materialize_rows_from_variables(
        &self,
        context: &ExecutionContext,
    ) -> Vec<HashMap<String, Value>> {
        let mut arrays: HashMap<String, Vec<Value>> = HashMap::new();

        for (var, value) in &context.variables {
            match value {
                Value::Array(values) => {
                    arrays.insert(var.clone(), values.clone());
                }
                other => {
                    arrays.insert(var.clone(), vec![other.clone()]);
                }
            }
        }

        if arrays.is_empty() {
            return Vec::new();
        }

        let max_len = arrays
            .values()
            .map(|values| values.len())
            .max()
            .unwrap_or(0);

        let mut rows = Vec::new();
        for idx in 0..max_len {
            let mut row = HashMap::new();
            for (var, values) in &arrays {
                let value = if values.len() == max_len {
                    values.get(idx).cloned().unwrap_or(Value::Null)
                } else if values.len() == 1 {
                    values[0].clone()
                } else {
                    values.get(idx).cloned().unwrap_or(Value::Null)
                };
                row.insert(var.clone(), value);
            }
            rows.push(row);
        }

        rows
    }

    fn update_result_set_from_rows(
        &self,
        context: &mut ExecutionContext,
        rows: &[HashMap<String, Value>],
    ) {
        let mut columns: Vec<String> = context.variables.keys().cloned().collect();
        columns.sort();

        context.result_set.columns = columns.clone();
        context.result_set.rows = rows
            .iter()
            .map(|row_map| Row {
                values: columns
                    .iter()
                    .map(|column| row_map.get(column).cloned().unwrap_or(Value::Null))
                    .collect(),
            })
            .collect();
    }

    fn evaluate_projection_expression(
        &self,
        row: &HashMap<String, Value>,
        context: &ExecutionContext,
        expr: &parser::Expression,
    ) -> Result<Value> {
        match expr {
            parser::Expression::Variable(name) => Ok(row.get(name).cloned().unwrap_or(Value::Null)),
            parser::Expression::PropertyAccess { variable, property } => Ok(row
                .get(variable)
                .map(|entity| Self::extract_property(entity, property))
                .unwrap_or(Value::Null)),
            parser::Expression::Literal(literal) => match literal {
                parser::Literal::String(s) => Ok(Value::String(s.clone())),
                parser::Literal::Integer(i) => Ok(Value::Number((*i).into())),
                parser::Literal::Float(f) => Ok(serde_json::Number::from_f64(*f)
                    .map(Value::Number)
                    .unwrap_or(Value::Null)),
                parser::Literal::Boolean(b) => Ok(Value::Bool(*b)),
                parser::Literal::Null => Ok(Value::Null),
            },
            parser::Expression::Parameter(name) => {
                Ok(context.params.get(name).cloned().unwrap_or(Value::Null))
            }
            parser::Expression::FunctionCall { name, args } => {
                let lowered = name.to_lowercase();
                match lowered.as_str() {
                    "labels" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            // Extract node ID from the value
                            let node_id = if let Value::Object(obj) = &value {
                                // Try to get _nexus_id from the object
                                if let Some(Value::Number(id)) = obj.get("_nexus_id") {
                                    id.as_u64()
                                } else {
                                    None
                                }
                            } else if let Value::String(id_str) = &value {
                                // Try to parse as string ID
                                id_str.parse::<u64>().ok()
                            } else {
                                None
                            };

                            if let Some(nid) = node_id {
                                // Read the node record to get labels
                                if let Ok(node_record) = self.store.read_node(nid) {
                                    if let Ok(label_names) =
                                        self.catalog.get_labels_from_bitmap(node_record.label_bits)
                                    {
                                        let labels: Vec<Value> =
                                            label_names.into_iter().map(Value::String).collect();
                                        return Ok(Value::Array(labels));
                                    }
                                }
                            }
                        }
                        Ok(Value::Null)
                    }
                    "type" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            // Extract relationship ID from the value
                            let rel_id = if let Value::Object(obj) = &value {
                                // Try to get _nexus_id from the object
                                if let Some(Value::Number(id)) = obj.get("_nexus_id") {
                                    id.as_u64()
                                } else {
                                    None
                                }
                            } else if let Value::String(id_str) = &value {
                                // Try to parse as string ID
                                id_str.parse::<u64>().ok()
                            } else {
                                None
                            };

                            if let Some(rid) = rel_id {
                                // Read the relationship record to get type_id
                                if let Ok(rel_record) = self.store.read_rel(rid) {
                                    if let Ok(Some(type_name)) =
                                        self.catalog.get_type_name(rel_record.type_id)
                                    {
                                        return Ok(Value::String(type_name));
                                    }
                                }
                            }
                        }
                        Ok(Value::Null)
                    }
                    "keys" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            // Extract keys from the value (node or relationship)
                            if let Value::Object(obj) = &value {
                                let mut keys: Vec<String> = obj
                                    .keys()
                                    .filter(|k| !k.starts_with('_')) // Exclude internal fields like _nexus_id
                                    .map(|k| k.to_string())
                                    .collect();
                                keys.sort();
                                let key_values: Vec<Value> =
                                    keys.into_iter().map(Value::String).collect();
                                return Ok(Value::Array(key_values));
                            }
                        }
                        Ok(Value::Array(Vec::new()))
                    }
                    "id" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            // Extract node or relationship ID from _nexus_id
                            if let Value::Object(obj) = &value {
                                if let Some(Value::Number(id)) = obj.get("_nexus_id") {
                                    return Ok(Value::Number(id.clone()));
                                }
                            }
                        }
                        Ok(Value::Null)
                    }
                    // String functions
                    "tolower" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if let Value::String(s) = value {
                                return Ok(Value::String(s.to_lowercase()));
                            }
                        }
                        Ok(Value::Null)
                    }
                    "toupper" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if let Value::String(s) = value {
                                return Ok(Value::String(s.to_uppercase()));
                            }
                        }
                        Ok(Value::Null)
                    }
                    "substring" => {
                        // substring(string, start, [length])
                        if args.len() >= 2 {
                            let string_val =
                                self.evaluate_projection_expression(row, context, &args[0])?;
                            let start_val =
                                self.evaluate_projection_expression(row, context, &args[1])?;

                            if let (Value::String(s), Value::Number(start_num)) =
                                (string_val, start_val)
                            {
                                let start = start_num.as_i64().unwrap_or(0).max(0) as usize;

                                if args.len() >= 3 {
                                    let length_val = self
                                        .evaluate_projection_expression(row, context, &args[2])?;
                                    if let Value::Number(len_num) = length_val {
                                        let length = len_num.as_i64().unwrap_or(0).max(0) as usize;
                                        let end = (start + length).min(s.len());
                                        return Ok(Value::String(
                                            s.chars().skip(start).take(end - start).collect(),
                                        ));
                                    }
                                } else {
                                    // No length specified - take from start to end
                                    return Ok(Value::String(s.chars().skip(start).collect()));
                                }
                            }
                        }
                        Ok(Value::Null)
                    }
                    "trim" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if let Value::String(s) = value {
                                return Ok(Value::String(s.trim().to_string()));
                            }
                        }
                        Ok(Value::Null)
                    }
                    "ltrim" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if let Value::String(s) = value {
                                return Ok(Value::String(s.trim_start().to_string()));
                            }
                        }
                        Ok(Value::Null)
                    }
                    "rtrim" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if let Value::String(s) = value {
                                return Ok(Value::String(s.trim_end().to_string()));
                            }
                        }
                        Ok(Value::Null)
                    }
                    "replace" => {
                        // replace(string, search, replace)
                        if args.len() >= 3 {
                            let string_val =
                                self.evaluate_projection_expression(row, context, &args[0])?;
                            let search_val =
                                self.evaluate_projection_expression(row, context, &args[1])?;
                            let replace_val =
                                self.evaluate_projection_expression(row, context, &args[2])?;

                            if let (
                                Value::String(s),
                                Value::String(search),
                                Value::String(replace),
                            ) = (string_val, search_val, replace_val)
                            {
                                return Ok(Value::String(s.replace(&search, &replace)));
                            }
                        }
                        Ok(Value::Null)
                    }
                    "split" => {
                        // split(string, delimiter)
                        if args.len() >= 2 {
                            let string_val =
                                self.evaluate_projection_expression(row, context, &args[0])?;
                            let delim_val =
                                self.evaluate_projection_expression(row, context, &args[1])?;

                            if let (Value::String(s), Value::String(delim)) =
                                (string_val, delim_val)
                            {
                                let parts: Vec<Value> = s
                                    .split(&delim)
                                    .map(|part| Value::String(part.to_string()))
                                    .collect();
                                return Ok(Value::Array(parts));
                            }
                        }
                        Ok(Value::Null)
                    }
                    // Math functions
                    "abs" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if value.is_null() {
                                return Ok(Value::Null);
                            }
                            let num = self.value_to_number(&value)?;
                            return serde_json::Number::from_f64(num.abs())
                                .map(Value::Number)
                                .ok_or_else(|| Error::TypeMismatch {
                                    expected: "number".to_string(),
                                    actual: "non-finite".to_string(),
                                });
                        }
                        Ok(Value::Null)
                    }
                    "ceil" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if value.is_null() {
                                return Ok(Value::Null);
                            }
                            let num = self.value_to_number(&value)?;
                            return serde_json::Number::from_f64(num.ceil())
                                .map(Value::Number)
                                .ok_or_else(|| Error::TypeMismatch {
                                    expected: "number".to_string(),
                                    actual: "non-finite".to_string(),
                                });
                        }
                        Ok(Value::Null)
                    }
                    "floor" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if value.is_null() {
                                return Ok(Value::Null);
                            }
                            let num = self.value_to_number(&value)?;
                            return serde_json::Number::from_f64(num.floor())
                                .map(Value::Number)
                                .ok_or_else(|| Error::TypeMismatch {
                                    expected: "number".to_string(),
                                    actual: "non-finite".to_string(),
                                });
                        }
                        Ok(Value::Null)
                    }
                    "round" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if value.is_null() {
                                return Ok(Value::Null);
                            }
                            let num = self.value_to_number(&value)?;
                            return serde_json::Number::from_f64(num.round())
                                .map(Value::Number)
                                .ok_or_else(|| Error::TypeMismatch {
                                    expected: "number".to_string(),
                                    actual: "non-finite".to_string(),
                                });
                        }
                        Ok(Value::Null)
                    }
                    "sqrt" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if value.is_null() {
                                return Ok(Value::Null);
                            }
                            let num = self.value_to_number(&value)?;
                            return serde_json::Number::from_f64(num.sqrt())
                                .map(Value::Number)
                                .ok_or_else(|| Error::TypeMismatch {
                                    expected: "number".to_string(),
                                    actual: "non-finite".to_string(),
                                });
                        }
                        Ok(Value::Null)
                    }
                    "pow" => {
                        // pow(base, exponent)
                        if args.len() >= 2 {
                            let base_val =
                                self.evaluate_projection_expression(row, context, &args[0])?;
                            let exp_val =
                                self.evaluate_projection_expression(row, context, &args[1])?;
                            if base_val.is_null() || exp_val.is_null() {
                                return Ok(Value::Null);
                            }
                            let base = self.value_to_number(&base_val)?;
                            let exp = self.value_to_number(&exp_val)?;
                            return serde_json::Number::from_f64(base.powf(exp))
                                .map(Value::Number)
                                .ok_or_else(|| Error::TypeMismatch {
                                    expected: "number".to_string(),
                                    actual: "non-finite".to_string(),
                                });
                        }
                        Ok(Value::Null)
                    }
                    // Type conversion functions
                    "tointeger" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            match value {
                                Value::Number(n) => {
                                    if let Some(i) = n.as_i64() {
                                        return Ok(Value::Number(i.into()));
                                    }
                                    if let Some(f) = n.as_f64() {
                                        return Ok(Value::Number((f as i64).into()));
                                    }
                                }
                                Value::String(s) => {
                                    if let Ok(i) = s.parse::<i64>() {
                                        return Ok(Value::Number(i.into()));
                                    }
                                }
                                _ => {}
                            }
                        }
                        Ok(Value::Null)
                    }
                    "tofloat" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            match value {
                                Value::Number(n) => {
                                    if let Some(f) = n.as_f64() {
                                        return serde_json::Number::from_f64(f)
                                            .map(Value::Number)
                                            .ok_or_else(|| Error::TypeMismatch {
                                                expected: "float".to_string(),
                                                actual: "non-finite".to_string(),
                                            });
                                    }
                                }
                                Value::String(s) => {
                                    if let Ok(f) = s.parse::<f64>() {
                                        return serde_json::Number::from_f64(f)
                                            .map(Value::Number)
                                            .ok_or_else(|| Error::TypeMismatch {
                                                expected: "float".to_string(),
                                                actual: "non-finite".to_string(),
                                            });
                                    }
                                }
                                _ => {}
                            }
                        }
                        Ok(Value::Null)
                    }
                    "tostring" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            match value {
                                Value::String(s) => Ok(Value::String(s)),
                                Value::Number(n) => Ok(Value::String(n.to_string())),
                                Value::Bool(b) => Ok(Value::String(b.to_string())),
                                Value::Null => Ok(Value::Null),
                                Value::Array(_) | Value::Object(_) => {
                                    Ok(Value::String(value.to_string()))
                                }
                            }
                        } else {
                            Ok(Value::Null)
                        }
                    }
                    "toboolean" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            match value {
                                Value::Bool(b) => Ok(Value::Bool(b)),
                                Value::String(s) => {
                                    let lower = s.to_lowercase();
                                    if lower == "true" {
                                        Ok(Value::Bool(true))
                                    } else if lower == "false" {
                                        Ok(Value::Bool(false))
                                    } else {
                                        Ok(Value::Null)
                                    }
                                }
                                Value::Number(n) => {
                                    // 0 = false, non-zero = true
                                    Ok(Value::Bool(n.as_f64().unwrap_or(0.0) != 0.0))
                                }
                                _ => Ok(Value::Null),
                            }
                        } else {
                            Ok(Value::Null)
                        }
                    }
                    // Temporal functions
                    "date" => {
                        if args.is_empty() {
                            // Return current date in ISO format (YYYY-MM-DD)
                            let now = chrono::Local::now();
                            return Ok(Value::String(now.format("%Y-%m-%d").to_string()));
                        } else if let Some(arg) = args.first() {
                            // Parse date from string or map
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            match value {
                                Value::String(s) => {
                                    // Try to parse ISO date format
                                    if let Ok(date) =
                                        chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d")
                                    {
                                        return Ok(Value::String(
                                            date.format("%Y-%m-%d").to_string(),
                                        ));
                                    }
                                }
                                Value::Object(map) => {
                                    // Support {year, month, day} format
                                    let year = map
                                        .get("year")
                                        .and_then(|v| v.as_i64())
                                        .unwrap_or_else(|| chrono::Local::now().year() as i64)
                                        as i32;
                                    let month =
                                        map.get("month").and_then(|v| v.as_u64()).unwrap_or(1)
                                            as u32;
                                    let day =
                                        map.get("day").and_then(|v| v.as_u64()).unwrap_or(1) as u32;

                                    if let Some(date) =
                                        chrono::NaiveDate::from_ymd_opt(year, month, day)
                                    {
                                        return Ok(Value::String(
                                            date.format("%Y-%m-%d").to_string(),
                                        ));
                                    }
                                }
                                _ => {}
                            }
                        }
                        Ok(Value::Null)
                    }
                    "datetime" => {
                        if args.is_empty() {
                            // Return current datetime in ISO format
                            let now = chrono::Local::now();
                            return Ok(Value::String(now.to_rfc3339()));
                        } else if let Some(arg) = args.first() {
                            // Parse datetime from string or map
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            match value {
                                Value::String(s) => {
                                    // Try to parse RFC3339/ISO8601 datetime
                                    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                                        return Ok(Value::String(dt.to_rfc3339()));
                                    }
                                    // Try to parse without timezone
                                    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(
                                        &s,
                                        "%Y-%m-%dT%H:%M:%S",
                                    ) {
                                        let local = chrono::Local::now().timezone();
                                        let dt_local = local
                                            .from_local_datetime(&dt)
                                            .earliest()
                                            .unwrap_or_else(|| local.from_utc_datetime(&dt));
                                        return Ok(Value::String(dt_local.to_rfc3339()));
                                    }
                                }
                                Value::Object(map) => {
                                    // Support {year, month, day, hour, minute, second} format
                                    let year = map
                                        .get("year")
                                        .and_then(|v| v.as_i64())
                                        .unwrap_or_else(|| chrono::Local::now().year() as i64)
                                        as i32;
                                    let month =
                                        map.get("month").and_then(|v| v.as_u64()).unwrap_or(1)
                                            as u32;
                                    let day =
                                        map.get("day").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
                                    let hour = map.get("hour").and_then(|v| v.as_u64()).unwrap_or(0)
                                        as u32;
                                    let minute =
                                        map.get("minute").and_then(|v| v.as_u64()).unwrap_or(0)
                                            as u32;
                                    let second =
                                        map.get("second").and_then(|v| v.as_u64()).unwrap_or(0)
                                            as u32;

                                    if let Some(date) =
                                        chrono::NaiveDate::from_ymd_opt(year, month, day)
                                    {
                                        if let Some(time) =
                                            chrono::NaiveTime::from_hms_opt(hour, minute, second)
                                        {
                                            let dt = chrono::NaiveDateTime::new(date, time);
                                            let local = chrono::Local::now().timezone();
                                            let dt_local = local
                                                .from_local_datetime(&dt)
                                                .earliest()
                                                .unwrap_or_else(|| local.from_utc_datetime(&dt));
                                            return Ok(Value::String(dt_local.to_rfc3339()));
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        Ok(Value::Null)
                    }
                    "time" => {
                        if args.is_empty() {
                            // Return current time in HH:MM:SS format
                            let now = chrono::Local::now();
                            return Ok(Value::String(now.format("%H:%M:%S").to_string()));
                        } else if let Some(arg) = args.first() {
                            // Parse time from string or map
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            match value {
                                Value::String(s) => {
                                    // Try to parse time format HH:MM:SS
                                    if let Ok(time) =
                                        chrono::NaiveTime::parse_from_str(&s, "%H:%M:%S")
                                    {
                                        return Ok(Value::String(
                                            time.format("%H:%M:%S").to_string(),
                                        ));
                                    }
                                    // Try HH:MM format
                                    if let Ok(time) = chrono::NaiveTime::parse_from_str(&s, "%H:%M")
                                    {
                                        return Ok(Value::String(
                                            time.format("%H:%M:%S").to_string(),
                                        ));
                                    }
                                }
                                Value::Object(map) => {
                                    // Support {hour, minute, second} format
                                    let hour = map.get("hour").and_then(|v| v.as_u64()).unwrap_or(0)
                                        as u32;
                                    let minute =
                                        map.get("minute").and_then(|v| v.as_u64()).unwrap_or(0)
                                            as u32;
                                    let second =
                                        map.get("second").and_then(|v| v.as_u64()).unwrap_or(0)
                                            as u32;

                                    if let Some(time) =
                                        chrono::NaiveTime::from_hms_opt(hour, minute, second)
                                    {
                                        return Ok(Value::String(
                                            time.format("%H:%M:%S").to_string(),
                                        ));
                                    }
                                }
                                _ => {}
                            }
                        }
                        Ok(Value::Null)
                    }
                    "timestamp" => {
                        if args.is_empty() {
                            // Return current Unix timestamp in milliseconds
                            let now = chrono::Local::now();
                            let millis = now.timestamp_millis();
                            return Ok(Value::Number(millis.into()));
                        } else if let Some(arg) = args.first() {
                            // Parse timestamp from string or return existing number
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            match value {
                                Value::Number(n) => {
                                    // Return as-is if already a number
                                    return Ok(Value::Number(n));
                                }
                                Value::String(s) => {
                                    // Try to parse datetime and convert to timestamp
                                    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                                        let millis = dt.timestamp_millis();
                                        return Ok(Value::Number(millis.into()));
                                    }
                                }
                                _ => {}
                            }
                        }
                        Ok(Value::Null)
                    }
                    "duration" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if let Value::Object(map) = value {
                                // Support duration components: years, months, days, hours, minutes, seconds
                                let mut duration_map = Map::new();

                                if let Some(years) = map.get("years") {
                                    duration_map.insert("years".to_string(), years.clone());
                                }
                                if let Some(months) = map.get("months") {
                                    duration_map.insert("months".to_string(), months.clone());
                                }
                                if let Some(days) = map.get("days") {
                                    duration_map.insert("days".to_string(), days.clone());
                                }
                                if let Some(hours) = map.get("hours") {
                                    duration_map.insert("hours".to_string(), hours.clone());
                                }
                                if let Some(minutes) = map.get("minutes") {
                                    duration_map.insert("minutes".to_string(), minutes.clone());
                                }
                                if let Some(seconds) = map.get("seconds") {
                                    duration_map.insert("seconds".to_string(), seconds.clone());
                                }

                                return Ok(Value::Object(duration_map));
                            }
                        }
                        Ok(Value::Null)
                    }
                    // Path functions
                    "nodes" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            // If value is already an array, treat it as a path of nodes
                            if let Value::Array(arr) = value {
                                // Filter only node objects (objects with _nexus_id)
                                let nodes: Vec<Value> = arr
                                    .into_iter()
                                    .filter(|v| {
                                        if let Value::Object(obj) = v {
                                            obj.contains_key("_nexus_id")
                                        } else {
                                            false
                                        }
                                    })
                                    .collect();
                                return Ok(Value::Array(nodes));
                            }
                            // If it's a single node, return it as array
                            if let Value::Object(obj) = &value {
                                if obj.contains_key("_nexus_id") {
                                    return Ok(Value::Array(vec![value]));
                                }
                            }
                        }
                        Ok(Value::Array(Vec::new()))
                    }
                    "relationships" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            // If value is already an array, extract relationships
                            if let Value::Array(arr) = value {
                                // Filter only relationship objects (objects with _nexus_type and source/target)
                                let rels: Vec<Value> = arr
                                    .into_iter()
                                    .filter(|v| {
                                        if let Value::Object(obj) = v {
                                            obj.contains_key("_nexus_type")
                                                && (obj.contains_key("_source")
                                                    || obj.contains_key("_target"))
                                        } else {
                                            false
                                        }
                                    })
                                    .collect();
                                return Ok(Value::Array(rels));
                            }
                            // If it's a single relationship, return it as array
                            if let Value::Object(obj) = &value {
                                if obj.contains_key("_nexus_type") {
                                    return Ok(Value::Array(vec![value]));
                                }
                            }
                        }
                        Ok(Value::Array(Vec::new()))
                    }
                    "length" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            // For arrays representing paths, length is the number of relationships
                            // which is (number of nodes - 1) or number of relationship objects
                            if let Value::Array(arr) = value {
                                // Count relationship objects in the path
                                let rel_count = arr
                                    .iter()
                                    .filter(|v| {
                                        if let Value::Object(obj) = v {
                                            obj.contains_key("_nexus_type")
                                        } else {
                                            false
                                        }
                                    })
                                    .count();
                                return Ok(Value::Number((rel_count as i64).into()));
                            }
                            // For a single relationship, length is 1
                            if let Value::Object(obj) = &value {
                                if obj.contains_key("_nexus_type") {
                                    return Ok(Value::Number(1.into()));
                                }
                            }
                        }
                        Ok(Value::Number(0.into()))
                    }
                    // List functions
                    "size" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            match value {
                                Value::Array(arr) => Ok(Value::Number((arr.len() as i64).into())),
                                Value::String(s) => Ok(Value::Number((s.len() as i64).into())),
                                _ => Ok(Value::Null),
                            }
                        } else {
                            Ok(Value::Null)
                        }
                    }
                    "head" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if let Value::Array(arr) = value {
                                return Ok(arr.first().cloned().unwrap_or(Value::Null));
                            }
                        }
                        Ok(Value::Null)
                    }
                    "tail" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if let Value::Array(arr) = value {
                                if arr.len() > 1 {
                                    return Ok(Value::Array(arr[1..].to_vec()));
                                }
                                return Ok(Value::Array(Vec::new()));
                            }
                        }
                        Ok(Value::Null)
                    }
                    "last" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if let Value::Array(arr) = value {
                                return Ok(arr.last().cloned().unwrap_or(Value::Null));
                            }
                        }
                        Ok(Value::Null)
                    }
                    "range" => {
                        // range(start, end, [step])
                        if args.len() >= 2 {
                            let start_val =
                                self.evaluate_projection_expression(row, context, &args[0])?;
                            let end_val =
                                self.evaluate_projection_expression(row, context, &args[1])?;

                            if let (Value::Number(start_num), Value::Number(end_num)) =
                                (start_val, end_val)
                            {
                                // Convert to i64, handling both integer and float cases
                                let start = start_num
                                    .as_i64()
                                    .or_else(|| start_num.as_f64().map(|f| f as i64))
                                    .unwrap_or(0);
                                let end = end_num
                                    .as_i64()
                                    .or_else(|| end_num.as_f64().map(|f| f as i64))
                                    .unwrap_or(0);
                                let step = if args.len() >= 3 {
                                    let step_val = self
                                        .evaluate_projection_expression(row, context, &args[2])?;
                                    if let Value::Number(s) = step_val {
                                        s.as_i64()
                                            .or_else(|| s.as_f64().map(|f| f as i64))
                                            .unwrap_or(1)
                                    } else {
                                        1
                                    }
                                } else {
                                    1
                                };

                                if step == 0 {
                                    return Ok(Value::Array(Vec::new()));
                                }

                                let mut result = Vec::new();
                                if step > 0 {
                                    let mut i = start;
                                    while i <= end {
                                        result.push(Value::Number(i.into()));
                                        i += step;
                                    }
                                } else {
                                    let mut i = start;
                                    while i >= end {
                                        result.push(Value::Number(i.into()));
                                        i += step;
                                    }
                                }
                                return Ok(Value::Array(result));
                            }
                        }
                        Ok(Value::Null)
                    }
                    "reverse" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if let Value::Array(mut arr) = value {
                                arr.reverse();
                                return Ok(Value::Array(arr));
                            }
                        }
                        Ok(Value::Null)
                    }
                    _ => Ok(Value::Null),
                }
            }
            parser::Expression::BinaryOp { left, op, right } => {
                let left_val = self.evaluate_projection_expression(row, context, left)?;
                let right_val = self.evaluate_projection_expression(row, context, right)?;
                match op {
                    parser::BinaryOperator::Add => self.add_values(&left_val, &right_val),
                    parser::BinaryOperator::Subtract => self.subtract_values(&left_val, &right_val),
                    parser::BinaryOperator::Multiply => self.multiply_values(&left_val, &right_val),
                    parser::BinaryOperator::Divide => self.divide_values(&left_val, &right_val),
                    parser::BinaryOperator::Equal => Ok(Value::Bool(left_val == right_val)),
                    parser::BinaryOperator::NotEqual => Ok(Value::Bool(left_val != right_val)),
                    parser::BinaryOperator::LessThan => Ok(Value::Bool(
                        self.compare_values_for_sort(&left_val, &right_val)
                            == std::cmp::Ordering::Less,
                    )),
                    parser::BinaryOperator::LessThanOrEqual => Ok(Value::Bool(matches!(
                        self.compare_values_for_sort(&left_val, &right_val),
                        std::cmp::Ordering::Less | std::cmp::Ordering::Equal
                    ))),
                    parser::BinaryOperator::GreaterThan => Ok(Value::Bool(
                        self.compare_values_for_sort(&left_val, &right_val)
                            == std::cmp::Ordering::Greater,
                    )),
                    parser::BinaryOperator::GreaterThanOrEqual => Ok(Value::Bool(matches!(
                        self.compare_values_for_sort(&left_val, &right_val),
                        std::cmp::Ordering::Greater | std::cmp::Ordering::Equal
                    ))),
                    parser::BinaryOperator::And => {
                        let result =
                            self.value_to_bool(&left_val)? && self.value_to_bool(&right_val)?;
                        Ok(Value::Bool(result))
                    }
                    parser::BinaryOperator::Or => {
                        let result =
                            self.value_to_bool(&left_val)? || self.value_to_bool(&right_val)?;
                        Ok(Value::Bool(result))
                    }
                    parser::BinaryOperator::StartsWith => {
                        let left_str = self.value_to_string(&left_val);
                        let right_str = self.value_to_string(&right_val);
                        Ok(Value::Bool(left_str.starts_with(&right_str)))
                    }
                    parser::BinaryOperator::EndsWith => {
                        let left_str = self.value_to_string(&left_val);
                        let right_str = self.value_to_string(&right_val);
                        Ok(Value::Bool(left_str.ends_with(&right_str)))
                    }
                    parser::BinaryOperator::Contains => {
                        let left_str = self.value_to_string(&left_val);
                        let right_str = self.value_to_string(&right_val);
                        Ok(Value::Bool(left_str.contains(&right_str)))
                    }
                    parser::BinaryOperator::RegexMatch => {
                        let left_str = self.value_to_string(&left_val);
                        let right_str = self.value_to_string(&right_val);
                        // Use regex crate for pattern matching
                        match regex::Regex::new(&right_str) {
                            Ok(re) => Ok(Value::Bool(re.is_match(&left_str))),
                            Err(_) => Ok(Value::Bool(false)), // Invalid regex pattern returns false
                        }
                    }
                    _ => Ok(Value::Null),
                }
            }
            parser::Expression::UnaryOp { op, operand } => {
                let value = self.evaluate_projection_expression(row, context, operand)?;
                match op {
                    parser::UnaryOperator::Not => Ok(Value::Bool(!self.value_to_bool(&value)?)),
                    parser::UnaryOperator::Minus => {
                        let number = self.value_to_number(&value)?;
                        serde_json::Number::from_f64(-number)
                            .map(Value::Number)
                            .ok_or_else(|| Error::TypeMismatch {
                                expected: "number".to_string(),
                                actual: "non-finite".to_string(),
                            })
                    }
                    parser::UnaryOperator::Plus => Ok(value),
                }
            }
            parser::Expression::IsNull { expr, negated } => {
                let value = self.evaluate_projection_expression(row, context, expr)?;
                let is_null = value.is_null();
                Ok(Value::Bool(if *negated { !is_null } else { is_null }))
            }
            parser::Expression::Exists {
                pattern,
                where_clause,
            } => {
                // Check if the pattern exists in the current context
                let pattern_exists = self.check_pattern_exists(row, context, pattern)?;

                // If pattern doesn't exist, return false
                if !pattern_exists {
                    return Ok(Value::Bool(false));
                }

                // If WHERE clause is present, evaluate it
                if let Some(where_expr) = where_clause {
                    // Create a context with pattern variables for WHERE evaluation
                    let mut exists_row = row.clone();

                    // Extract variables from pattern and add to row context
                    for element in &pattern.elements {
                        match element {
                            parser::PatternElement::Node(node) => {
                                if let Some(var) = &node.variable {
                                    // Try to get variable from current row or context
                                    if let Some(value) = row.get(var) {
                                        exists_row.insert(var.clone(), value.clone());
                                    } else if let Some(value) = context.get_variable(var) {
                                        exists_row.insert(var.clone(), value.clone());
                                    }
                                }
                            }
                            parser::PatternElement::Relationship(rel) => {
                                if let Some(var) = &rel.variable {
                                    if let Some(value) = row.get(var) {
                                        exists_row.insert(var.clone(), value.clone());
                                    } else if let Some(value) = context.get_variable(var) {
                                        exists_row.insert(var.clone(), value.clone());
                                    }
                                }
                            }
                        }
                    }

                    // Evaluate WHERE condition
                    let condition_value =
                        self.evaluate_projection_expression(&exists_row, context, where_expr)?;
                    let condition_true = self.value_to_bool(&condition_value)?;

                    Ok(Value::Bool(condition_true))
                } else {
                    Ok(Value::Bool(pattern_exists))
                }
            }
            parser::Expression::MapProjection { source, items } => {
                // Evaluate the source expression (should be a node/map)
                let source_value = self.evaluate_projection_expression(row, context, source)?;

                // Build the projected map
                let mut projected_map = serde_json::Map::new();

                for item in items {
                    match item {
                        parser::MapProjectionItem::Property { property, alias } => {
                            // Extract property from source
                            let prop_value = if let Value::Object(obj) = &source_value {
                                // If source is a node object, get property from properties
                                if let Some(Value::Object(props)) = obj.get("properties") {
                                    props.get(property.as_str()).cloned().unwrap_or(Value::Null)
                                } else {
                                    obj.get(property.as_str()).cloned().unwrap_or(Value::Null)
                                }
                            } else {
                                Value::Null
                            };

                            // Use alias if provided, otherwise use property name
                            let key = alias
                                .as_ref()
                                .map(|s| s.as_str())
                                .unwrap_or(property.as_str())
                                .to_string();
                            projected_map.insert(key, prop_value);
                        }
                        parser::MapProjectionItem::VirtualKey { key, expression } => {
                            // Evaluate the expression and use as value
                            let expr_value =
                                self.evaluate_projection_expression(row, context, expression)?;
                            projected_map.insert(key.clone(), expr_value);
                        }
                    }
                }

                Ok(Value::Object(projected_map))
            }
            parser::Expression::ListComprehension {
                variable,
                list_expression,
                where_clause,
                transform_expression,
            } => {
                // Evaluate the list expression
                let list_value =
                    self.evaluate_projection_expression(row, context, list_expression)?;

                // Convert to array if needed
                let list_items = match list_value {
                    Value::Array(items) => items,
                    Value::Null => Vec::new(),
                    other => vec![other],
                };

                // Filter and transform items
                let mut result_items = Vec::new();

                for item in list_items {
                    // Create a new row context with the variable bound to this item
                    let mut comprehension_row = row.clone();
                    let item_clone = item.clone();
                    comprehension_row.insert(variable.clone(), item_clone);

                    // Apply WHERE clause if present
                    if let Some(where_expr) = where_clause {
                        let condition_value = self.evaluate_projection_expression(
                            &comprehension_row,
                            context,
                            where_expr,
                        )?;

                        // Only include item if condition is true
                        if !self.value_to_bool(&condition_value)? {
                            continue;
                        }
                    }

                    // Apply transformation if present, otherwise use item as-is
                    if let Some(transform_expr) = transform_expression {
                        let transformed_value = self.evaluate_projection_expression(
                            &comprehension_row,
                            context,
                            transform_expr,
                        )?;
                        result_items.push(transformed_value);
                    } else {
                        result_items.push(item);
                    }
                }

                Ok(Value::Array(result_items))
            }
            parser::Expression::PatternComprehension {
                pattern,
                where_clause,
                transform_expression,
            } => {
                // Pattern comprehensions collect matching patterns and transform them
                // This is a simplified implementation that works within the current context

                // For a full implementation, we would need to:
                // 1. Execute the pattern as a subquery within the current context
                // 2. Collect all matching results
                // 3. Apply WHERE clause filtering
                // 4. Apply transformation expression
                // 5. Return as array

                // For now, we'll implement a basic version that:
                // - Extracts variables from the pattern
                // - Checks if they exist in the current row context
                // - Applies WHERE and transform if present

                // Extract variables from pattern
                let mut pattern_vars = Vec::new();
                for element in &pattern.elements {
                    match element {
                        parser::PatternElement::Node(node) => {
                            if let Some(var) = &node.variable {
                                pattern_vars.push(var.clone());
                            }
                        }
                        parser::PatternElement::Relationship(rel) => {
                            if let Some(var) = &rel.variable {
                                pattern_vars.push(var.clone());
                            }
                        }
                    }
                }

                // Check if all pattern variables exist in current row
                let mut all_vars_exist = true;
                let mut pattern_row = HashMap::new();
                for var in &pattern_vars {
                    if let Some(value) = row.get(var) {
                        pattern_row.insert(var.clone(), value.clone());
                    } else {
                        all_vars_exist = false;
                        break;
                    }
                }

                // If pattern variables don't exist in current row, return empty array
                if !all_vars_exist || pattern_row.is_empty() {
                    return Ok(Value::Array(Vec::new()));
                }

                // Apply WHERE clause if present
                if let Some(where_expr) = where_clause {
                    let condition_value =
                        self.evaluate_projection_expression(&pattern_row, context, where_expr)?;

                    // If WHERE condition is false, return empty array
                    if !self.value_to_bool(&condition_value)? {
                        return Ok(Value::Array(Vec::new()));
                    }
                }

                // Apply transformation if present, otherwise return the pattern variables
                if let Some(transform_expr) = transform_expression {
                    // Evaluate transformation expression (can be MapProjection, property access, etc.)
                    let transformed_value =
                        self.evaluate_projection_expression(&pattern_row, context, transform_expr)?;

                    // Always return as array (even if single value)
                    Ok(Value::Array(vec![transformed_value]))
                } else {
                    // No transformation - return array of pattern variable values
                    let values: Vec<Value> = pattern_vars
                        .iter()
                        .filter_map(|var| pattern_row.get(var).cloned())
                        .collect();
                    Ok(Value::Array(values))
                }
            }
            parser::Expression::List(elements) => {
                // Evaluate each element and return as JSON array
                let mut items = Vec::new();
                for element in elements {
                    let value = self.evaluate_projection_expression(row, context, element)?;
                    items.push(value);
                }
                Ok(Value::Array(items))
            }
            parser::Expression::Map(map) => {
                // Evaluate each value and return as JSON object
                let mut obj = serde_json::Map::new();
                for (key, expr) in map {
                    let value = self.evaluate_projection_expression(row, context, expr)?;
                    obj.insert(key.clone(), value);
                }
                Ok(Value::Object(obj))
            }
            parser::Expression::Case {
                input,
                when_clauses,
                else_clause,
            } => {
                // Evaluate input expression if present (generic CASE)
                let input_value = if let Some(input_expr) = input {
                    Some(self.evaluate_projection_expression(row, context, input_expr)?)
                } else {
                    None
                };

                // Evaluate WHEN clauses
                for when_clause in when_clauses {
                    let condition_value =
                        self.evaluate_projection_expression(row, context, &when_clause.condition)?;

                    // For generic CASE: compare input with condition
                    // For simple CASE: evaluate condition as boolean
                    let matches = if let Some(ref input_val) = input_value {
                        // Generic CASE: input == condition
                        input_val == &condition_value
                    } else {
                        // Simple CASE: condition is boolean expression
                        self.value_to_bool(&condition_value)?
                    };

                    if matches {
                        return self.evaluate_projection_expression(
                            row,
                            context,
                            &when_clause.result,
                        );
                    }
                }

                // No WHEN clause matched, return ELSE or NULL
                if let Some(else_expr) = else_clause {
                    self.evaluate_projection_expression(row, context, else_expr)
                } else {
                    Ok(Value::Null)
                }
            }
        }
    }

    /// Check if a pattern exists in the current context
    fn check_pattern_exists(
        &self,
        row: &HashMap<String, Value>,
        context: &ExecutionContext,
        pattern: &parser::Pattern,
    ) -> Result<bool> {
        // For EXISTS, we need to check if the pattern matches in the current context
        // This is a simplified implementation that checks if nodes and relationships exist

        // If pattern is empty, return false
        if pattern.elements.is_empty() {
            return Ok(false);
        }

        // For now, implement a basic check:
        // - If pattern has a single node, check if it exists in context
        // - If pattern has relationships, check if they exist

        // Get the first node from the pattern
        if let Some(parser::PatternElement::Node(first_node)) = pattern.elements.first() {
            // If the node has a variable, check if it exists in the current row/context
            if let Some(var_name) = &first_node.variable {
                // Check if variable exists in current row
                if let Some(Value::Object(obj)) = row.get(var_name) {
                    // If it's a valid node object, the pattern exists
                    if obj.contains_key("_nexus_id") {
                        // Node exists, check relationships if any
                        if pattern.elements.len() > 1 {
                            // Pattern has relationships - for now, return true if node exists
                            // Full relationship checking would require more complex logic
                            return Ok(true);
                        }
                        return Ok(true);
                    }
                }

                // Check if variable exists in context variables
                if let Some(Value::Array(nodes)) = context.variables.get(var_name) {
                    if !nodes.is_empty() {
                        return Ok(true);
                    }
                }
            } else {
                // No variable - pattern exists if we can find matching nodes
                // For simplicity, if no variable is specified, assume pattern might exist
                // This is a basic implementation
                return Ok(true);
            }
        }

        // Pattern doesn't match
        Ok(false)
    }

    fn extract_property(entity: &Value, property: &str) -> Value {
        if let Value::Object(obj) = entity {
            if let Some(Value::Object(props)) = obj.get("properties") {
                return props.get(property).cloned().unwrap_or(Value::Null);
            }
            return obj.get(property).cloned().unwrap_or(Value::Null);
        }
        Value::Null
    }

    fn add_values(&self, left: &Value, right: &Value) -> Result<Value> {
        let l = self.value_to_number(left)?;
        let r = self.value_to_number(right)?;
        serde_json::Number::from_f64(l + r)
            .map(Value::Number)
            .ok_or_else(|| Error::TypeMismatch {
                expected: "number".to_string(),
                actual: "non-finite sum".to_string(),
            })
    }

    fn subtract_values(&self, left: &Value, right: &Value) -> Result<Value> {
        let l = self.value_to_number(left)?;
        let r = self.value_to_number(right)?;
        serde_json::Number::from_f64(l - r)
            .map(Value::Number)
            .ok_or_else(|| Error::TypeMismatch {
                expected: "number".to_string(),
                actual: "non-finite difference".to_string(),
            })
    }

    fn multiply_values(&self, left: &Value, right: &Value) -> Result<Value> {
        let l = self.value_to_number(left)?;
        let r = self.value_to_number(right)?;
        serde_json::Number::from_f64(l * r)
            .map(Value::Number)
            .ok_or_else(|| Error::TypeMismatch {
                expected: "number".to_string(),
                actual: "non-finite product".to_string(),
            })
    }

    fn divide_values(&self, left: &Value, right: &Value) -> Result<Value> {
        let l = self.value_to_number(left)?;
        let r = self.value_to_number(right)?;
        if r == 0.0 {
            return Err(Error::TypeMismatch {
                expected: "non-zero".to_string(),
                actual: "division by zero".to_string(),
            });
        }
        serde_json::Number::from_f64(l / r)
            .map(Value::Number)
            .ok_or_else(|| Error::TypeMismatch {
                expected: "number".to_string(),
                actual: "non-finite quotient".to_string(),
            })
    }

    fn update_variables_from_rows(
        &self,
        context: &mut ExecutionContext,
        rows: &[HashMap<String, Value>],
    ) {
        let mut arrays: HashMap<String, Vec<Value>> = HashMap::new();
        for row in rows {
            for (var, value) in row {
                arrays.entry(var.clone()).or_default().push(value.clone());
            }
        }
        context.variables.clear();
        for (var, values) in arrays {
            context.variables.insert(var, Value::Array(values));
        }
    }

    fn evaluate_predicate_on_row(
        &self,
        row: &HashMap<String, Value>,
        context: &ExecutionContext,
        expr: &parser::Expression,
    ) -> Result<bool> {
        let value = self.evaluate_projection_expression(row, context, expr)?;
        self.value_to_bool(&value)
    }

    fn extract_entity_id(value: &Value) -> Option<u64> {
        match value {
            Value::Object(obj) => {
                if let Some(id) = obj.get("_nexus_id").and_then(|id| id.as_u64()) {
                    Some(id)
                } else if let Some(id) = obj
                    .get("_element_id")
                    .and_then(|id| id.as_str())
                    .and_then(|s| s.parse::<u64>().ok())
                {
                    Some(id)
                } else if let Some(id_value) = obj.get("id") {
                    match id_value {
                        Value::Number(num) => num.as_u64(),
                        Value::String(s) => s.parse::<u64>().ok(),
                        _ => None,
                    }
                } else {
                    None
                }
            }
            Value::Number(num) => num.as_u64(),
            _ => None,
        }
    }

    fn read_relationship_as_value(&self, rel: &RelationshipInfo) -> Result<Value> {
        let _type_name = self
            .catalog
            .get_type_name(rel.type_id)?
            .unwrap_or_else(|| format!("type_{}", rel.type_id));

        let properties_value = self
            .store
            .load_relationship_properties(rel.id)?
            .unwrap_or_else(|| Value::Object(Map::new()));

        let properties_map = match properties_value {
            Value::Object(map) => map,
            other => {
                let mut map = Map::new();
                map.insert("value".to_string(), other);
                map
            }
        };

        // Add _nexus_id for internal ID extraction (e.g., for type() function)
        let mut rel_obj = properties_map;
        rel_obj.insert("_nexus_id".to_string(), Value::Number(rel.id.into()));

        // Return only the properties as a flat object, matching Neo4j's format
        Ok(Value::Object(rel_obj))
    }

    fn result_set_as_rows(&self, context: &ExecutionContext) -> Vec<HashMap<String, Value>> {
        context
            .result_set
            .rows
            .iter()
            .map(|row| {
                let mut map = HashMap::new();
                for (idx, column) in context.result_set.columns.iter().enumerate() {
                    if idx < row.values.len() {
                        map.insert(column.clone(), row.values[idx].clone());
                    } else {
                        map.insert(column.clone(), Value::Null);
                    }
                }
                map
            })
            .collect()
    }

    fn aggregation_alias(&self, aggregation: &Aggregation) -> String {
        match aggregation {
            Aggregation::Count { alias, .. }
            | Aggregation::Sum { alias, .. }
            | Aggregation::Avg { alias, .. }
            | Aggregation::Min { alias, .. }
            | Aggregation::Max { alias, .. }
            | Aggregation::Collect { alias, .. }
            | Aggregation::PercentileDisc { alias, .. }
            | Aggregation::PercentileCont { alias, .. }
            | Aggregation::StDev { alias, .. }
            | Aggregation::StDevP { alias, .. } => alias.clone(),
        }
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

    fn set_columns_and_rows(&mut self, columns: Vec<String>, rows: Vec<Row>) {
        self.result_set.columns = columns;
        self.result_set.rows = rows;
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
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn create_executor() -> (Executor, TempDir) {
        let dir = TempDir::new().unwrap();
        let catalog = Catalog::new(dir.path()).unwrap();
        let store = RecordStore::new(dir.path()).unwrap();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new_default(128).unwrap();

        let executor = Executor::new(&catalog, &store, &label_index, &knn_index).unwrap();
        (executor, dir)
    }

    fn build_node(id: u64, name: &str, age: i64) -> Value {
        let mut props = Map::new();
        props.insert("name".to_string(), Value::String(name.to_string()));
        props.insert("age".to_string(), Value::Number(age.into()));

        let mut node = Map::new();
        node.insert("id".to_string(), Value::Number(id.into()));
        node.insert(
            "labels".to_string(),
            Value::Array(vec![Value::String("Person".to_string())]),
        );
        node.insert("properties".to_string(), Value::Object(props));
        Value::Object(node)
    }

    #[test]
    fn project_node_property_returns_alias() {
        let (executor, _dir) = create_executor();
        let mut context = ExecutionContext::new(HashMap::new());
        context.set_variable("n", Value::Array(vec![build_node(1, "Alice", 30)]));

        let item = ProjectionItem {
            expression: parser::Expression::PropertyAccess {
                variable: "n".to_string(),
                property: "name".to_string(),
            },
            alias: "name".to_string(),
        };

        let rows = executor.execute_project(&mut context, &[item]).unwrap();
        assert_eq!(context.result_set.columns, vec!["name".to_string()]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].values[0], Value::String("Alice".to_string()))
    }

    #[test]
    fn filter_removes_non_matching_rows() {
        let (executor, _dir) = create_executor();
        let mut context = ExecutionContext::new(HashMap::new());
        context.set_variable(
            "n",
            Value::Array(vec![build_node(1, "Alice", 30), build_node(2, "Bob", 20)]),
        );

        executor
            .execute_filter(&mut context, "n.age > 25")
            .expect("filter should succeed");

        assert_eq!(context.result_set.rows.len(), 1);
        let row = &context.result_set.rows[0];
        assert_eq!(row.values.len(), context.result_set.columns.len());
    }
}
