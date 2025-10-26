//! Query optimization module
//!
//! This module provides cost-based query optimization for Cypher queries,
//! including join order optimization, index selection, and execution plan generation.

use crate::executor::{ExecutionPlan, Operator, Query};
use crate::index::IndexManager;
use crate::storage::RecordStore;
use crate::catalog::Catalog;
use crate::error::{Error, Result};
use std::collections::HashMap;
use std::sync::Arc;

/// Query optimizer that generates optimal execution plans
pub struct QueryOptimizer {
    /// Catalog for metadata access
    catalog: Arc<Catalog>,
    /// Storage for statistics
    storage: Arc<RecordStore>,
    /// Index manager for index selection
    indexes: Arc<IndexManager>,
    /// Statistics collector
    stats: StatisticsCollector,
}

/// Statistics about tables and indexes
#[derive(Debug, Clone)]
pub struct TableStats {
    /// Number of rows
    pub row_count: u64,
    /// Number of distinct values for each column
    pub distinct_values: HashMap<String, u64>,
    /// Average row size in bytes
    pub avg_row_size: f64,
    /// Most frequent values for each column
    pub most_frequent: HashMap<String, Vec<(String, u64)>>,
}

/// Statistics collector for query optimization
#[derive(Debug, Clone)]
pub struct StatisticsCollector {
    /// Table statistics
    table_stats: HashMap<String, TableStats>,
    /// Index statistics
    index_stats: HashMap<String, IndexStats>,
}

/// Index statistics
#[derive(Debug, Clone)]
pub struct IndexStats {
    /// Index cardinality
    pub cardinality: u64,
    /// Index size in bytes
    pub size_bytes: u64,
    /// Selectivity (0.0 to 1.0)
    pub selectivity: f64,
    /// Index height
    pub height: u32,
}

/// Cost model for query optimization
#[derive(Debug, Clone)]
pub struct CostModel {
    /// Cost per sequential scan
    pub seq_scan_cost: f64,
    /// Cost per index scan
    pub index_scan_cost: f64,
    /// Cost per random page access
    pub random_page_cost: f64,
    /// Cost per CPU operation
    pub cpu_tuple_cost: f64,
    /// Cost per join operation
    pub join_cost: f64,
}

impl Default for CostModel {
    fn default() -> Self {
        Self {
            seq_scan_cost: 1.0,
            index_scan_cost: 0.1,
            random_page_cost: 4.0,
            cpu_tuple_cost: 0.01,
            join_cost: 0.1,
        }
    }
}

/// Query optimization result
#[derive(Debug, Clone)]
pub struct OptimizationResult {
    /// Optimized execution plan
    pub plan: ExecutionPlan,
    /// Estimated cost
    pub estimated_cost: f64,
    /// Estimated rows returned
    pub estimated_rows: u64,
    /// Optimization statistics
    pub stats: OptimizationStats,
}

/// Optimization statistics
#[derive(Debug, Clone)]
pub struct OptimizationStats {
    /// Number of plans considered
    pub plans_considered: u32,
    /// Time spent optimizing (microseconds)
    pub optimization_time_us: u64,
    /// Indexes used
    pub indexes_used: Vec<String>,
    /// Join order
    pub join_order: Vec<String>,
}

impl QueryOptimizer {
    /// Create a new query optimizer
    pub fn new(
        catalog: Arc<Catalog>,
        storage: Arc<RecordStore>,
        indexes: Arc<IndexManager>,
    ) -> Self {
        Self {
            catalog,
            storage,
            indexes,
            stats: StatisticsCollector::new(),
        }
    }

    /// Optimize a query and return the best execution plan
    pub fn optimize(&mut self, query: &Query) -> Result<OptimizationResult> {
        let start_time = std::time::Instant::now();
        
        // Collect statistics
        self.stats.collect_table_stats(&self.storage)?;
        self.stats.collect_index_stats(&self.indexes)?;

        // Generate candidate plans
        let mut plans = self.generate_candidate_plans(query)?;
        let plans_count = plans.len();
        
        // Evaluate costs and select best plan
        let best_plan = self.select_best_plan(&mut plans)?;
        
        let optimization_time = start_time.elapsed();
        
        Ok(OptimizationResult {
            plan: best_plan.plan.clone(),
            estimated_cost: best_plan.cost,
            estimated_rows: best_plan.rows,
            stats: OptimizationStats {
                plans_considered: plans_count as u32,
                optimization_time_us: optimization_time.as_micros() as u64,
                indexes_used: best_plan.indexes_used.clone(),
                join_order: best_plan.join_order.clone(),
            },
        })
    }

    /// Generate candidate execution plans
    fn generate_candidate_plans(&self, query: &Query) -> Result<Vec<CandidatePlan>> {
        let mut plans = Vec::new();
        
        // Parse query to extract patterns
        let patterns = self.parse_patterns(query)?;
        
        // Generate different join orders
        for join_order in self.generate_join_orders(&patterns) {
            // Generate different index selections
            for index_selection in self.generate_index_selections(&patterns) {
                let plan = self.build_execution_plan(&patterns, &join_order, &index_selection)?;
                let cost = self.estimate_cost(&plan)?;
                let rows = self.estimate_rows(&plan)?;
                
                plans.push(CandidatePlan {
                    plan,
                    cost,
                    rows,
                    indexes_used: index_selection.iter().map(|(_, idx)| idx.clone()).collect(),
                    join_order: join_order.iter().map(|p| p.to_string()).collect(),
                });
            }
        }
        
        Ok(plans)
    }

    /// Parse query patterns
    fn parse_patterns(&self, query: &Query) -> Result<Vec<QueryPattern>> {
        // This is a simplified pattern parser
        // In a real implementation, this would parse the Cypher query
        let mut patterns = Vec::new();
        
        // Extract MATCH patterns
        if query.cypher.contains("MATCH") {
            // Simple pattern extraction - in reality this would be more sophisticated
            patterns.push(QueryPattern {
                node_labels: vec!["Person".to_string()],
                relationship_types: vec!["KNOWS".to_string()],
                filters: Vec::new(),
            });
        }
        
        Ok(patterns)
    }

    /// Generate different join orders
    fn generate_join_orders<'a>(&self, patterns: &'a [QueryPattern]) -> Vec<Vec<&'a QueryPattern>> {
        // Generate permutations of patterns for different join orders
        let mut orders = Vec::new();
        
        // Simple case: just return patterns in order
        // In a real implementation, this would generate all permutations
        orders.push(patterns.iter().collect());
        
        // Add some alternative orders
        if patterns.len() > 1 {
            let mut alt_order = patterns.iter().collect::<Vec<_>>();
            alt_order.reverse();
            orders.push(alt_order);
        }
        
        orders
    }

    /// Generate different index selections
    fn generate_index_selections(&self, patterns: &[QueryPattern]) -> Vec<Vec<(String, String)>> {
        let mut selections = Vec::new();
        
        // No indexes
        selections.push(Vec::new());
        
        // Use label indexes
        let mut with_labels = Vec::new();
        for pattern in patterns {
            for label in &pattern.node_labels {
                with_labels.push((label.clone(), format!("label_{}", label)));
            }
        }
        if !with_labels.is_empty() {
            selections.push(with_labels);
        }
        
        selections
    }

    /// Build execution plan from patterns, join order, and index selection
    fn build_execution_plan(
        &self,
        _patterns: &[QueryPattern],
        join_order: &[&QueryPattern],
        index_selection: &[(String, String)],
    ) -> Result<ExecutionPlan> {
        let mut operators = Vec::new();
        
        for (i, pattern) in join_order.iter().enumerate() {
            // Add scan operator
            if let Some((_, index_name)) = index_selection.iter().find(|(label, _)| {
                pattern.node_labels.contains(label)
            }) {
                operators.push(Operator::IndexScan {
                    index_name: index_name.clone(),
                    label: pattern.node_labels[0].clone(),
                });
            } else {
                operators.push(Operator::NodeByLabel {
                    label_id: 0, // Will be resolved later
                    variable: "n".to_string(),
                });
            }
            
            // Add filter operator if needed
            if !pattern.filters.is_empty() {
                operators.push(Operator::Filter {
                    predicate: pattern.filters[0].clone(),
                });
            }
            
            // Add join operator if not the first pattern
            if i > 0 {
                operators.push(Operator::HashJoin {
                    left_key: "id".to_string(),
                    right_key: "id".to_string(),
                });
            }
        }
        
        // Add final projection
        operators.push(Operator::Project {
            columns: vec!["n".to_string()],
        });
        
        Ok(ExecutionPlan { operators })
    }

    /// Estimate cost of execution plan
    fn estimate_cost(&self, plan: &ExecutionPlan) -> Result<f64> {
        let cost_model = CostModel::default();
        let mut total_cost = 0.0;
        
        for operator in &plan.operators {
            match operator {
                Operator::NodeByLabel { .. } => {
                    let stats = self.stats.get_table_stats("Person")?;
                    total_cost += cost_model.seq_scan_cost * stats.row_count as f64;
                }
                Operator::IndexScan { .. } => {
                    let stats = self.stats.get_index_stats("label_Person")?;
                    total_cost += cost_model.index_scan_cost * stats.cardinality as f64;
                }
                Operator::Filter { .. } => {
                    total_cost += cost_model.cpu_tuple_cost * 1000.0; // Estimate 1000 tuples
                }
                Operator::HashJoin { .. } => {
                    total_cost += cost_model.join_cost * 1000.0; // Estimate 1000 tuples
                }
                Operator::Project { .. } => {
                    total_cost += cost_model.cpu_tuple_cost * 100.0; // Estimate 100 tuples
                }
                _ => {
                    total_cost += cost_model.cpu_tuple_cost * 10.0; // Default cost
                }
            }
        }
        
        Ok(total_cost)
    }

    /// Estimate number of rows returned
    fn estimate_rows(&self, plan: &ExecutionPlan) -> Result<u64> {
        let mut estimated_rows = 1000; // Default estimate
        
        for operator in &plan.operators {
            match operator {
                Operator::NodeByLabel { .. } => {
                    let stats = self.stats.get_table_stats("Person")?;
                    estimated_rows = estimated_rows.min(stats.row_count);
                }
                Operator::IndexScan { .. } => {
                    estimated_rows = estimated_rows.min(100); // Index scans are more selective
                }
                Operator::Filter { .. } => {
                    estimated_rows = estimated_rows / 10; // Filters reduce rows
                }
                Operator::HashJoin { .. } => {
                    estimated_rows = estimated_rows * 2; // Joins can increase rows
                }
                _ => {}
            }
        }
        
        Ok(estimated_rows)
    }

    /// Select the best plan from candidates
    fn select_best_plan<'a>(&self, plans: &'a mut [CandidatePlan]) -> Result<&'a CandidatePlan> {
        plans.sort_by(|a, b| a.cost.partial_cmp(&b.cost).unwrap_or(std::cmp::Ordering::Equal));
        plans.first().ok_or_else(|| Error::internal("No plans generated"))
    }
}

/// Query pattern extracted from Cypher
#[derive(Debug, Clone)]
pub struct QueryPattern {
    /// Node labels in this pattern
    pub node_labels: Vec<String>,
    /// Relationship types in this pattern
    pub relationship_types: Vec<String>,
    /// Filter predicates
    pub filters: Vec<String>,
}

impl std::fmt::Display for QueryPattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Pattern({:?})", self.node_labels)
    }
}

/// Candidate execution plan
#[derive(Debug, Clone)]
struct CandidatePlan {
    /// Execution plan
    plan: ExecutionPlan,
    /// Estimated cost
    cost: f64,
    /// Estimated rows
    rows: u64,
    /// Indexes used
    indexes_used: Vec<String>,
    /// Join order
    join_order: Vec<String>,
}

impl StatisticsCollector {
    /// Create new statistics collector
    pub fn new() -> Self {
        Self {
            table_stats: HashMap::new(),
            index_stats: HashMap::new(),
        }
    }

    /// Collect table statistics
    pub fn collect_table_stats(&mut self, _storage: &RecordStore) -> Result<()> {
        // In a real implementation, this would scan the storage and collect statistics
        // For now, we'll use dummy data
        self.table_stats.insert("Person".to_string(), TableStats {
            row_count: 10000,
            distinct_values: HashMap::new(),
            avg_row_size: 64.0,
            most_frequent: HashMap::new(),
        });
        
        self.table_stats.insert("Company".to_string(), TableStats {
            row_count: 1000,
            distinct_values: HashMap::new(),
            avg_row_size: 128.0,
            most_frequent: HashMap::new(),
        });
        
        Ok(())
    }

    /// Collect index statistics
    pub fn collect_index_stats(&mut self, _indexes: &IndexManager) -> Result<()> {
        // In a real implementation, this would collect actual index statistics
        self.index_stats.insert("label_Person".to_string(), IndexStats {
            cardinality: 10000,
            size_bytes: 1024,
            selectivity: 0.1,
            height: 3,
        });
        
        Ok(())
    }

    /// Get table statistics
    pub fn get_table_stats(&self, table_name: &str) -> Result<&TableStats> {
        self.table_stats.get(table_name)
            .ok_or_else(|| Error::internal(&format!("No statistics for table: {}", table_name)))
    }

    /// Get index statistics
    pub fn get_index_stats(&self, index_name: &str) -> Result<&IndexStats> {
        self.index_stats.get(index_name)
            .ok_or_else(|| Error::internal(&format!("No statistics for index: {}", index_name)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tempfile::tempdir;

    #[test]
    fn test_query_optimizer_creation() {
        let temp_dir = tempdir().unwrap();
        let catalog = Arc::new(Catalog::new(temp_dir.path()).unwrap());
        let storage = Arc::new(RecordStore::new(temp_dir.path()).unwrap());
        let indexes = Arc::new(IndexManager::new(temp_dir.path().join("indexes")).unwrap());
        
        let optimizer = QueryOptimizer::new(catalog, storage, indexes);
        assert!(optimizer.stats.table_stats.is_empty());
        assert!(optimizer.stats.index_stats.is_empty());
    }

    #[test]
    fn test_cost_model_default() {
        let cost_model = CostModel::default();
        assert_eq!(cost_model.seq_scan_cost, 1.0);
        assert_eq!(cost_model.index_scan_cost, 0.1);
        assert_eq!(cost_model.random_page_cost, 4.0);
        assert_eq!(cost_model.cpu_tuple_cost, 0.01);
        assert_eq!(cost_model.join_cost, 0.1);
    }

    #[test]
    fn test_table_stats() {
        let mut stats = TableStats {
            row_count: 1000,
            distinct_values: HashMap::new(),
            avg_row_size: 64.0,
            most_frequent: HashMap::new(),
        };
        
        stats.distinct_values.insert("name".to_string(), 100);
        stats.most_frequent.insert("name".to_string(), vec![("Alice".to_string(), 10)]);
        
        assert_eq!(stats.row_count, 1000);
        assert_eq!(stats.avg_row_size, 64.0);
        assert_eq!(stats.distinct_values.len(), 1);
        assert_eq!(stats.most_frequent.len(), 1);
    }

    #[test]
    fn test_index_stats() {
        let stats = IndexStats {
            cardinality: 1000,
            size_bytes: 1024,
            selectivity: 0.1,
            height: 3,
        };
        
        assert_eq!(stats.cardinality, 1000);
        assert_eq!(stats.size_bytes, 1024);
        assert_eq!(stats.selectivity, 0.1);
        assert_eq!(stats.height, 3);
    }

    #[test]
    fn test_statistics_collector() {
        let mut collector = StatisticsCollector::new();
        assert!(collector.table_stats.is_empty());
        assert!(collector.index_stats.is_empty());
        
        // Test adding table stats
        collector.table_stats.insert("test".to_string(), TableStats {
            row_count: 100,
            distinct_values: HashMap::new(),
            avg_row_size: 32.0,
            most_frequent: HashMap::new(),
        });
        
        assert_eq!(collector.table_stats.len(), 1);
        assert!(collector.table_stats.contains_key("test"));
    }

    #[test]
    fn test_query_pattern() {
        let pattern = QueryPattern {
            node_labels: vec!["Person".to_string(), "Employee".to_string()],
            relationship_types: vec!["WORKS_AT".to_string()],
            filters: vec!["age > 25".to_string()],
        };
        
        assert_eq!(pattern.node_labels.len(), 2);
        assert_eq!(pattern.relationship_types.len(), 1);
        assert_eq!(pattern.filters.len(), 1);
        assert!(pattern.node_labels.contains(&"Person".to_string()));
        assert!(pattern.relationship_types.contains(&"WORKS_AT".to_string()));
        assert!(pattern.filters.contains(&"age > 25".to_string()));
    }

    #[test]
    fn test_candidate_plan() {
        let plan = CandidatePlan {
            plan: ExecutionPlan { operators: vec![] },
            cost: 100.0,
            rows: 1000,
            indexes_used: vec!["idx1".to_string()],
            join_order: vec!["table1".to_string()],
        };
        
        assert_eq!(plan.cost, 100.0);
        assert_eq!(plan.rows, 1000);
        assert_eq!(plan.indexes_used.len(), 1);
        assert_eq!(plan.join_order.len(), 1);
    }
}
