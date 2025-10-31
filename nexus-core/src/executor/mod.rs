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
                Operator::Union { left, right, distinct } => {
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

                    let mut new_row = HashMap::new();

                    // Only add target node if target_var is specified
                    if !target_var.is_empty() {
                        let target_node = self.read_node_as_value(rel_record.dst_id)?;
                        new_row.insert(target_var.to_string(), target_node);
                    }

                    if !rel_var.is_empty() {
                        let relationship_value = self.read_relationship_as_value(&rel_info)?;
                        new_row.insert(rel_var.to_string(), relationship_value);
                    }

                    expanded_rows.push(new_row);
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
        let rows = self.materialize_rows_from_variables(context);
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

        for (group_key, group_rows) in groups {
            let mut result_row = group_key;
            for agg in aggregations {
                let agg_value = match agg {
                    Aggregation::Count { column, distinct, .. } => {
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
                    Aggregation::Max { column, .. } => {
                        let col_idx = self.get_column_index(column, &context.result_set.columns);
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
            let mut node_ids: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
            
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
                                                self.expression_to_json_value(v).ok().map(|val| (k.clone(), val))
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
                                            self.expression_to_json_value(v).ok().map(|val| (k.clone(), val))
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
                                    let current_idx = pattern.elements
                                        .iter()
                                        .position(|e| matches!(e, parser::PatternElement::Relationship(_)))
                                        .unwrap_or(0);
                                    
                                    if current_idx + 1 < pattern.elements.len() {
                                        if let parser::PatternElement::Node(target_node) = &pattern.elements[current_idx + 1] {
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
            Operator::Union { left, right, distinct } => {
                self.execute_union(context, left, right, *distinct)?;
            }
            Operator::Create { pattern } => {
                self.execute_create_with_context(context, &pattern)?;
            }
            Operator::Delete { variables } => {
                self.execute_delete(context, &variables, false)?;
            }
            Operator::DetachDelete { variables } => {
                self.execute_delete(context, &variables, true)?;
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
            _ => Ok(Value::Null),
        }
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
            | Aggregation::Max { alias, .. } => alias.clone(),
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
