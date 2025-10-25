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

pub mod parser;

use crate::{Result};
use crate::catalog::Catalog;
use crate::storage::RecordStore;
use crate::index::{LabelIndex, KnnIndex};
use serde_json::{Value, Map};
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
    pub fn new(catalog: Catalog, store: RecordStore, label_index: LabelIndex, knn_index: KnnIndex) -> Result<Self> {
        Ok(Self {
            catalog,
            store,
            label_index,
            knn_index,
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
                Operator::Expand { type_id, direction, source_var, target_var, rel_var } => {
                    self.execute_expand(&mut context, type_id, direction, &source_var, &target_var, &rel_var)?;
                }
                Operator::Project { columns } => {
                    results = self.execute_project(&context, &columns)?;
                }
                Operator::Limit { count } => {
                    results.truncate(count);
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
        // Use the new parser
        let mut parser = parser::CypherParser::new(cypher.to_string());
        let ast = parser.parse()?;
        
        // Convert AST to physical operators
        self.ast_to_operators(&ast)
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
                parser::Clause::Where(where_clause) => {
                    operators.push(Operator::Filter {
                        predicate: self.expression_to_string(&where_clause.expression)?,
                    });
                }
                parser::Clause::Return(return_clause) => {
                    let columns: Vec<String> = return_clause.items.iter()
                        .map(|item| {
                            if let Some(alias) = &item.alias {
                                alias.clone()
                            } else {
                                self.expression_to_string(&item.expression).unwrap_or_default()
                            }
                        })
                        .collect();
                    
                    operators.push(Operator::Project { columns });
                }
                parser::Clause::Limit(limit_clause) => {
                    if let parser::Expression::Literal(parser::Literal::Integer(count)) = &limit_clause.count {
                        operators.push(Operator::Limit { count: *count as usize });
                    }
                }
                _ => {
                    // Other clauses not implemented in MVP
                }
            }
        }
        
        Ok(operators)
    }

    /// Convert expression to string representation
    fn expression_to_string(&self, expr: &parser::Expression) -> Result<String> {
        match expr {
            parser::Expression::Variable(name) => Ok(name.clone()),
            parser::Expression::PropertyAccess { variable, property } => {
                Ok(format!("{}.{}", variable, property))
            }
            parser::Expression::Literal(literal) => {
                match literal {
                    parser::Literal::String(s) => Ok(format!("\"{}\"", s)),
                    parser::Literal::Integer(i) => Ok(i.to_string()),
                    parser::Literal::Float(f) => Ok(f.to_string()),
                    parser::Literal::Boolean(b) => Ok(b.to_string()),
                    parser::Literal::Null => Ok("NULL".to_string()),
                }
            }
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
            node.insert("labels".to_string(), Value::Array(vec![Value::String(format!("label_{}", label_id))]));
            node.insert("properties".to_string(), Value::Object(Map::new()));
            
            results.push(Value::Object(node));
        }
        
        Ok(results)
    }

    /// Execute Filter operator
    fn execute_filter(&self, _context: &mut ExecutionContext, _predicate: &str) -> Result<()> {
        // MVP: No filtering implemented yet
        Ok(())
    }

    /// Execute Expand operator
    fn execute_expand(&self, _context: &mut ExecutionContext, _type_id: Option<u32>, _direction: Direction, _source_var: &str, _target_var: &str, _rel_var: &str) -> Result<()> {
        // MVP: No relationship expansion implemented yet
        Ok(())
    }

    /// Execute Project operator
    fn execute_project(&self, context: &ExecutionContext, _columns: &[String]) -> Result<Vec<Row>> {
        // MVP: Simple projection - return all variables
        let mut rows = Vec::new();
        
        for (_var_name, value) in &context.variables {
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
}

/// Execution context for query processing
#[derive(Debug)]
struct ExecutionContext {
    /// Query parameters
    params: HashMap<String, Value>,
    /// Variable bindings
    variables: HashMap<String, Value>,
}

impl ExecutionContext {
    fn new(params: HashMap<String, Value>) -> Self {
        Self {
            params,
            variables: HashMap::new(),
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
        let store = RecordStore::default();
        let label_index = LabelIndex::default();
        let knn_index = KnnIndex::new_default(128).expect("Failed to create default KNN index");
        
        Self::new(catalog, store, label_index, knn_index).expect("Failed to create default executor")
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
        
        let executor = Executor::new(catalog, store, label_index, knn_index).unwrap();
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
        assert_eq!(query.params.get("name").unwrap(), &Value::String("test".to_string()));
    }

    #[test]
    fn test_parse_match_query() {
        let (executor, _dir) = create_test_executor();
        
        // Create a label first
        let catalog = Catalog::new("./test_data").unwrap();
        let label_id = catalog.get_or_create_label("Person").unwrap();
        
        // Test parsing
        let operators = executor.parse_and_plan("MATCH (n:Person) RETURN n").unwrap();
        assert_eq!(operators.len(), 2);
        
        match &operators[0] {
            Operator::NodeByLabel { label_id: parsed_label_id, variable } => {
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
        
        let result = executor.parse_and_plan("CREATE (n:Person)");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Only MATCH queries supported"));
    }

    #[test]
    fn test_execution_context() {
        let mut params = HashMap::new();
        params.insert("param1".to_string(), Value::String("value1".to_string()));
        
        let mut context = ExecutionContext::new(params);
        
        // Test setting and getting variables
        context.set_variable("n", Value::Array(vec![Value::String("node1".to_string())]));
        
        assert_eq!(context.get_variable("n"), Some(&Value::Array(vec![Value::String("node1".to_string())])));
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
}
