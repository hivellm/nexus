//! Query optimization module
//!
//! This module provides cost-based query optimization for Cypher queries,
//! including join order optimization, index selection, and execution plan generation.

use crate::catalog::Catalog;
use crate::error::{Error, Result};
use crate::executor::{ExecutionPlan, Operator, ProjectionItem, Query};
use crate::index::IndexManager;
use crate::storage::RecordStore;
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
    /// Query plan cache
    plan_cache: std::collections::HashMap<String, OptimizationResult>,
    /// Adaptive optimization settings
    adaptive_settings: AdaptiveSettings,
}

/// Adaptive optimization settings
#[derive(Debug, Clone)]
pub struct AdaptiveSettings {
    /// Enable plan caching
    pub enable_plan_cache: bool,
    /// Maximum cache size
    pub max_cache_size: usize,
    /// Enable adaptive statistics
    pub enable_adaptive_stats: bool,
    /// Statistics update frequency (queries)
    pub stats_update_frequency: u32,
    /// Query execution time threshold for re-optimization (ms)
    pub reoptimize_threshold_ms: u64,
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
#[derive(Debug, Clone, Default)]
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

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Current cache size
    pub cache_size: usize,
    /// Maximum cache size
    pub max_cache_size: usize,
    /// Cache hit rate (0.0 to 1.0)
    pub hit_rate: f64,
}

impl Default for AdaptiveSettings {
    fn default() -> Self {
        Self {
            enable_plan_cache: true,
            max_cache_size: 1000,
            enable_adaptive_stats: true,
            stats_update_frequency: 100,
            reoptimize_threshold_ms: 1000,
        }
    }
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
            plan_cache: HashMap::new(),
            adaptive_settings: AdaptiveSettings::default(),
        }
    }

    /// Create a new query optimizer with custom settings
    pub fn new_with_settings(
        catalog: Arc<Catalog>,
        storage: Arc<RecordStore>,
        indexes: Arc<IndexManager>,
        settings: AdaptiveSettings,
    ) -> Self {
        Self {
            catalog,
            storage,
            indexes,
            stats: StatisticsCollector::new(),
            plan_cache: HashMap::new(),
            adaptive_settings: settings,
        }
    }

    /// Optimize a query and return the best execution plan
    pub fn optimize(&mut self, query: &Query) -> Result<OptimizationResult> {
        let start_time = std::time::Instant::now();

        // Check plan cache first
        let query_hash = self.hash_query(query);
        if self.adaptive_settings.enable_plan_cache {
            if let Some(cached_result) = self.plan_cache.get(&query_hash) {
                return Ok(cached_result.clone());
            }
        }

        // Collect statistics if needed
        if self.adaptive_settings.enable_adaptive_stats {
            self.stats.collect_table_stats(&self.storage)?;
            self.stats.collect_index_stats(&self.indexes)?;
        }

        // Generate candidate plans
        let mut plans = self.generate_candidate_plans(query)?;
        let plans_count = plans.len();

        // Evaluate costs and select best plan
        let best_plan = self.select_best_plan(&mut plans)?;

        let optimization_time = start_time.elapsed();

        let result = OptimizationResult {
            plan: best_plan.plan.clone(),
            estimated_cost: best_plan.cost,
            estimated_rows: best_plan.rows,
            stats: OptimizationStats {
                plans_considered: plans_count as u32,
                optimization_time_us: optimization_time.as_micros() as u64,
                indexes_used: best_plan.indexes_used.clone(),
                join_order: best_plan.join_order.clone(),
            },
        };

        // Cache the result if enabled
        if self.adaptive_settings.enable_plan_cache {
            if self.plan_cache.len() >= self.adaptive_settings.max_cache_size {
                // Remove oldest entry (simple LRU approximation)
                if let Some(key) = self.plan_cache.keys().next().cloned() {
                    self.plan_cache.remove(&key);
                }
            }
            self.plan_cache.insert(query_hash, result.clone());
        }

        Ok(result)
    }

    /// Hash a query for caching purposes
    fn hash_query(&self, query: &Query) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        query.cypher.hash(&mut hasher);
        hasher.finish().to_string()
    }

    /// Clear the plan cache
    pub fn clear_cache(&mut self) {
        self.plan_cache.clear();
    }

    /// Get cache statistics
    pub fn get_cache_stats(&self) -> CacheStats {
        CacheStats {
            cache_size: self.plan_cache.len(),
            max_cache_size: self.adaptive_settings.max_cache_size,
            hit_rate: 0.0, // Would need to track hits/misses
        }
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
        orders.push(patterns.iter().collect());

        // Generate all permutations for small numbers of patterns
        if patterns.len() <= 3 {
            let mut perm_indices: Vec<usize> = (0..patterns.len()).collect();
            self.generate_permutations(&mut perm_indices, 0, &mut orders, patterns);
        } else {
            // For larger numbers, use heuristics
            // Order by selectivity (most selective first)
            let mut sorted_patterns = patterns.iter().collect::<Vec<_>>();
            sorted_patterns.sort_by(|a, b| {
                let a_selectivity = self.estimate_pattern_selectivity(a);
                let b_selectivity = self.estimate_pattern_selectivity(b);
                a_selectivity
                    .partial_cmp(&b_selectivity)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            orders.push(sorted_patterns.clone());

            // Reverse order
            let mut reversed = sorted_patterns;
            reversed.reverse();
            orders.push(reversed);
        }

        orders
    }

    /// Generate all permutations of indices
    fn generate_permutations<'a>(
        &self,
        indices: &mut [usize],
        start: usize,
        orders: &mut Vec<Vec<&'a QueryPattern>>,
        patterns: &'a [QueryPattern],
    ) {
        if start == indices.len() {
            let perm: Vec<&QueryPattern> = indices.iter().map(|&i| &patterns[i]).collect();
            orders.push(perm);
            return;
        }

        for i in start..indices.len() {
            indices.swap(start, i);
            self.generate_permutations(indices, start + 1, orders, patterns);
            indices.swap(start, i);
        }
    }

    /// Estimate selectivity of a query pattern
    fn estimate_pattern_selectivity(&self, pattern: &QueryPattern) -> f64 {
        let mut selectivity = 1.0;

        // More labels = more selective
        selectivity *= 1.0 / (pattern.node_labels.len() as f64 + 1.0);

        // More relationship types = more selective
        selectivity *= 1.0 / (pattern.relationship_types.len() as f64 + 1.0);

        // More filters = more selective
        selectivity *= 1.0 / (pattern.filters.len() as f64 + 1.0);

        selectivity
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
            if let Some((_, index_name)) = index_selection
                .iter()
                .find(|(label, _)| pattern.node_labels.contains(label))
            {
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
            items: vec![ProjectionItem {
                alias: "n".to_string(),
                expression: crate::executor::parser::Expression::Variable("n".to_string()),
            }],
        });

        Ok(ExecutionPlan { operators })
    }

    /// Estimate cost of execution plan
    fn estimate_cost(&self, plan: &ExecutionPlan) -> Result<f64> {
        let cost_model = CostModel::default();
        let mut total_cost = 0.0;
        let mut estimated_rows = 1000.0; // Track estimated rows through the plan

        for operator in &plan.operators {
            match operator {
                Operator::NodeByLabel { label_id: _, .. } => {
                    let stats = self.stats.get_table_stats("Person")?;
                    let scan_cost = cost_model.seq_scan_cost * stats.row_count as f64;
                    total_cost += scan_cost;
                    estimated_rows = stats.row_count as f64;
                }
                Operator::AllNodesScan { .. } => {
                    // All nodes scan is typically more expensive than label scan
                    // Estimate based on total node count (assume larger than single label)
                    let scan_cost = cost_model.seq_scan_cost * estimated_rows * 2.0;
                    total_cost += scan_cost;
                }
                Operator::IndexScan { index_name, .. } => {
                    let stats = self.stats.get_index_stats(index_name)?;
                    let scan_cost = cost_model.index_scan_cost * stats.cardinality as f64;
                    total_cost += scan_cost;
                    estimated_rows = stats.cardinality as f64;
                }
                Operator::Filter { predicate } => {
                    // Estimate filter selectivity based on predicate complexity
                    let selectivity = self.estimate_filter_selectivity(predicate);
                    let filter_cost = cost_model.cpu_tuple_cost * estimated_rows;
                    total_cost += filter_cost;
                    estimated_rows *= selectivity;
                }
                Operator::HashJoin { .. } => {
                    // Hash join cost includes building hash table and probing
                    let build_cost = cost_model.cpu_tuple_cost * estimated_rows;
                    let probe_cost = cost_model.cpu_tuple_cost * estimated_rows * 0.1; // Assume 10% probe ratio
                    total_cost += build_cost + probe_cost;
                    // Join can increase or decrease rows depending on join type
                    estimated_rows *= 0.5; // Assume 50% reduction
                }
                Operator::Project { items } => {
                    let project_cost =
                        cost_model.cpu_tuple_cost * estimated_rows * items.len() as f64;
                    total_cost += project_cost;
                    // Projection doesn't change row count
                }
                Operator::Sort { columns: _, .. } => {
                    // Sort cost is O(n log n)
                    let sort_cost =
                        cost_model.cpu_tuple_cost * estimated_rows * (estimated_rows.log2() + 1.0);
                    total_cost += sort_cost;
                }
                Operator::Aggregate {
                    group_by,
                    aggregations,
                    projection_items: _,
                    source: _,
                    streaming_optimized: _,
                    push_down_optimized: _,
                } => {
                    // Aggregation cost depends on grouping
                    let group_cost = cost_model.cpu_tuple_cost
                        * estimated_rows
                        * (group_by.len() + aggregations.len()) as f64;
                    total_cost += group_cost;
                    // Aggregation reduces rows significantly
                    estimated_rows /= 10.0;
                }
                Operator::Limit { count } => {
                    // Limit is very cheap
                    let limit_cost = cost_model.cpu_tuple_cost * (*count as f64);
                    total_cost += limit_cost;
                    estimated_rows = estimated_rows.min(*count as f64);
                }
                Operator::Distinct { .. } => {
                    // Distinct cost is similar to sort
                    let distinct_cost =
                        cost_model.cpu_tuple_cost * estimated_rows * (estimated_rows.log2() + 1.0);
                    total_cost += distinct_cost;
                    // Distinct reduces rows
                    estimated_rows *= 0.7; // Assume 30% reduction
                }
                _ => {
                    total_cost += cost_model.cpu_tuple_cost * estimated_rows;
                }
            }
        }

        Ok(total_cost)
    }

    /// Estimate filter selectivity based on predicate
    fn estimate_filter_selectivity(&self, predicate: &str) -> f64 {
        // Simple heuristics for filter selectivity
        if predicate.contains("=") {
            0.1 // Equality filters are quite selective
        } else if predicate.contains(">") || predicate.contains("<") {
            0.3 // Range filters are moderately selective
        } else if predicate.contains("LIKE") {
            0.5 // Pattern matching is less selective
        } else {
            0.8 // Default selectivity
        }
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
                    estimated_rows /= 10; // Filters reduce rows
                }
                Operator::HashJoin { .. } => {
                    estimated_rows *= 2; // Joins can increase rows
                }
                _ => {}
            }
        }

        Ok(estimated_rows)
    }

    /// Select the best plan from candidates
    fn select_best_plan<'a>(&self, plans: &'a mut [CandidatePlan]) -> Result<&'a CandidatePlan> {
        plans.sort_by(|a, b| {
            a.cost
                .partial_cmp(&b.cost)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        plans
            .first()
            .ok_or_else(|| Error::internal("No plans generated"))
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
        Self::default()
    }

    /// Collect table statistics
    pub fn collect_table_stats(&mut self, _storage: &RecordStore) -> Result<()> {
        // In a real implementation, this would scan the storage and collect statistics
        // For now, we'll use dummy data
        self.table_stats.insert(
            "Person".to_string(),
            TableStats {
                row_count: 10000,
                distinct_values: HashMap::new(),
                avg_row_size: 64.0,
                most_frequent: HashMap::new(),
            },
        );

        self.table_stats.insert(
            "Company".to_string(),
            TableStats {
                row_count: 1000,
                distinct_values: HashMap::new(),
                avg_row_size: 128.0,
                most_frequent: HashMap::new(),
            },
        );

        Ok(())
    }

    /// Collect index statistics
    pub fn collect_index_stats(&mut self, _indexes: &IndexManager) -> Result<()> {
        // In a real implementation, this would collect actual index statistics
        self.index_stats.insert(
            "label_Person".to_string(),
            IndexStats {
                cardinality: 10000,
                size_bytes: 1024,
                selectivity: 0.1,
                height: 3,
            },
        );

        Ok(())
    }

    /// Get table statistics
    pub fn get_table_stats(&self, table_name: &str) -> Result<&TableStats> {
        self.table_stats
            .get(table_name)
            .ok_or_else(|| Error::internal(format!("No statistics for table: {}", table_name)))
    }

    /// Get index statistics
    pub fn get_index_stats(&self, index_name: &str) -> Result<&IndexStats> {
        self.index_stats
            .get(index_name)
            .ok_or_else(|| Error::internal(format!("No statistics for index: {}", index_name)))
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
        stats
            .most_frequent
            .insert("name".to_string(), vec![("Alice".to_string(), 10)]);

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
        collector.table_stats.insert(
            "test".to_string(),
            TableStats {
                row_count: 100,
                distinct_values: HashMap::new(),
                avg_row_size: 32.0,
                most_frequent: HashMap::new(),
            },
        );

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

    #[test]
    fn test_adaptive_settings_default() {
        let settings = AdaptiveSettings::default();
        assert!(settings.enable_plan_cache);
        assert_eq!(settings.max_cache_size, 1000);
        assert!(settings.enable_adaptive_stats);
        assert_eq!(settings.stats_update_frequency, 100);
        assert_eq!(settings.reoptimize_threshold_ms, 1000);
    }

    #[test]
    fn test_adaptive_settings_custom() {
        let settings = AdaptiveSettings {
            enable_plan_cache: false,
            max_cache_size: 500,
            enable_adaptive_stats: false,
            stats_update_frequency: 50,
            reoptimize_threshold_ms: 2000,
        };

        assert!(!settings.enable_plan_cache);
        assert_eq!(settings.max_cache_size, 500);
        assert!(!settings.enable_adaptive_stats);
        assert_eq!(settings.stats_update_frequency, 50);
        assert_eq!(settings.reoptimize_threshold_ms, 2000);
    }

    #[test]
    fn test_optimization_result() {
        let result = OptimizationResult {
            plan: ExecutionPlan { operators: vec![] },
            estimated_cost: 50.0,
            estimated_rows: 500,
            stats: OptimizationStats {
                optimization_time_us: 10,
                plans_considered: 0,
                indexes_used: vec![],
                join_order: vec![],
            },
        };

        assert_eq!(result.estimated_cost, 50.0);
        assert_eq!(result.estimated_rows, 500);
        assert_eq!(result.stats.plans_considered, 0);
        assert_eq!(result.stats.optimization_time_us, 10);
        assert!(result.stats.indexes_used.is_empty());
    }

    #[test]
    fn test_query_optimizer_with_settings() {
        let temp_dir = tempdir().unwrap();
        let catalog = Arc::new(Catalog::new(temp_dir.path()).unwrap());
        let storage = Arc::new(RecordStore::new(temp_dir.path()).unwrap());
        let indexes = Arc::new(IndexManager::new(temp_dir.path().join("indexes")).unwrap());

        let settings = AdaptiveSettings {
            enable_plan_cache: true,
            max_cache_size: 500,
            enable_adaptive_stats: true,
            stats_update_frequency: 50,
            reoptimize_threshold_ms: 2000,
        };

        let optimizer = QueryOptimizer::new_with_settings(catalog, storage, indexes, settings);
        assert!(optimizer.adaptive_settings.enable_plan_cache);
        assert_eq!(optimizer.adaptive_settings.max_cache_size, 500);
        assert!(optimizer.plan_cache.is_empty());
    }

    #[test]
    fn test_cost_model_custom() {
        let cost_model = CostModel {
            seq_scan_cost: 2.0,
            index_scan_cost: 0.2,
            random_page_cost: 8.0,
            cpu_tuple_cost: 0.02,
            join_cost: 0.2,
        };

        assert_eq!(cost_model.seq_scan_cost, 2.0);
        assert_eq!(cost_model.index_scan_cost, 0.2);
        assert_eq!(cost_model.random_page_cost, 8.0);
        assert_eq!(cost_model.cpu_tuple_cost, 0.02);
        assert_eq!(cost_model.join_cost, 0.2);
    }

    #[test]
    fn test_table_stats_operations() {
        let mut stats = TableStats {
            row_count: 2000,
            distinct_values: HashMap::new(),
            avg_row_size: 128.0,
            most_frequent: HashMap::new(),
        };

        // Test adding distinct values
        stats.distinct_values.insert("id".to_string(), 2000);
        stats.distinct_values.insert("name".to_string(), 100);
        stats.distinct_values.insert("age".to_string(), 50);

        // Test adding most frequent values
        stats.most_frequent.insert(
            "name".to_string(),
            vec![("John".to_string(), 50), ("Jane".to_string(), 30)],
        );

        assert_eq!(stats.row_count, 2000);
        assert_eq!(stats.avg_row_size, 128.0);
        assert_eq!(stats.distinct_values.len(), 3);
        assert_eq!(stats.most_frequent.len(), 1);
        assert_eq!(stats.distinct_values.get("id").unwrap(), &2000);
        assert_eq!(stats.most_frequent.get("name").unwrap().len(), 2);
    }

    #[test]
    fn test_index_stats_operations() {
        let mut stats = IndexStats {
            cardinality: 5000,
            size_bytes: 2048,
            selectivity: 0.05,
            height: 4,
        };

        // Test updating stats
        stats.cardinality = 6000;
        stats.size_bytes = 4096;
        stats.selectivity = 0.03;
        stats.height = 5;

        assert_eq!(stats.cardinality, 6000);
        assert_eq!(stats.size_bytes, 4096);
        assert_eq!(stats.selectivity, 0.03);
        assert_eq!(stats.height, 5);
    }

    #[test]
    fn test_statistics_collector_operations() {
        let mut collector = StatisticsCollector::new();

        // Test adding table stats
        let table_stats = TableStats {
            row_count: 1000,
            distinct_values: {
                let mut map = HashMap::new();
                map.insert("id".to_string(), 1000);
                map.insert("name".to_string(), 100);
                map
            },
            avg_row_size: 64.0,
            most_frequent: HashMap::new(),
        };

        collector
            .table_stats
            .insert("users".to_string(), table_stats);

        // Test adding index stats
        let index_stats = IndexStats {
            cardinality: 1000,
            size_bytes: 1024,
            selectivity: 0.1,
            height: 3,
        };

        collector
            .index_stats
            .insert("idx_users_id".to_string(), index_stats);

        assert_eq!(collector.table_stats.len(), 1);
        assert_eq!(collector.index_stats.len(), 1);
        assert!(collector.table_stats.contains_key("users"));
        assert!(collector.index_stats.contains_key("idx_users_id"));

        // Test getting table stats
        let retrieved_stats = collector.table_stats.get("users").unwrap();
        assert_eq!(retrieved_stats.row_count, 1000);
        assert_eq!(retrieved_stats.distinct_values.len(), 2);

        // Test getting index stats
        let retrieved_index = collector.index_stats.get("idx_users_id").unwrap();
        assert_eq!(retrieved_index.cardinality, 1000);
        assert_eq!(retrieved_index.selectivity, 0.1);
    }

    #[test]
    fn test_query_pattern_operations() {
        let mut pattern = QueryPattern {
            node_labels: vec!["Person".to_string()],
            relationship_types: vec![],
            filters: vec![],
        };

        // Test adding labels
        pattern.node_labels.push("Employee".to_string());
        pattern.node_labels.push("Manager".to_string());

        // Test adding relationship types
        pattern.relationship_types.push("WORKS_AT".to_string());
        pattern.relationship_types.push("MANAGES".to_string());

        // Test adding filters
        pattern.filters.push("age > 25".to_string());
        pattern.filters.push("salary > 50000".to_string());

        assert_eq!(pattern.node_labels.len(), 3);
        assert_eq!(pattern.relationship_types.len(), 2);
        assert_eq!(pattern.filters.len(), 2);

        assert!(pattern.node_labels.contains(&"Person".to_string()));
        assert!(pattern.node_labels.contains(&"Employee".to_string()));
        assert!(pattern.node_labels.contains(&"Manager".to_string()));

        assert!(pattern.relationship_types.contains(&"WORKS_AT".to_string()));
        assert!(pattern.relationship_types.contains(&"MANAGES".to_string()));

        assert!(pattern.filters.contains(&"age > 25".to_string()));
        assert!(pattern.filters.contains(&"salary > 50000".to_string()));
    }

    #[test]
    fn test_candidate_plan_operations() {
        let mut plan = CandidatePlan {
            plan: ExecutionPlan { operators: vec![] },
            cost: 100.0,
            rows: 1000,
            indexes_used: vec!["idx1".to_string()],
            join_order: vec!["table1".to_string()],
        };

        // Test updating plan properties
        plan.cost = 150.0;
        plan.rows = 2000;
        plan.indexes_used.push("idx2".to_string());
        plan.join_order.push("table2".to_string());

        assert_eq!(plan.cost, 150.0);
        assert_eq!(plan.rows, 2000);
        assert_eq!(plan.indexes_used.len(), 2);
        assert_eq!(plan.join_order.len(), 2);
        assert!(plan.indexes_used.contains(&"idx1".to_string()));
        assert!(plan.indexes_used.contains(&"idx2".to_string()));
        assert!(plan.join_order.contains(&"table1".to_string()));
        assert!(plan.join_order.contains(&"table2".to_string()));
    }

    #[test]
    fn test_optimization_result_operations() {
        let mut result = OptimizationResult {
            plan: ExecutionPlan { operators: vec![] },
            estimated_cost: 50.0,
            estimated_rows: 500,
            stats: OptimizationStats {
                optimization_time_us: 10,
                plans_considered: 0,
                indexes_used: vec![],
                join_order: vec![],
            },
        };

        // Test updating optimization result
        result.estimated_cost = 75.0;
        result.estimated_rows = 750;
        result.stats.optimization_time_us = 20;
        result.stats.plans_considered = 5;
        result.stats.indexes_used.push("idx1".to_string());
        result.stats.join_order.push("table1".to_string());

        assert_eq!(result.estimated_cost, 75.0);
        assert_eq!(result.estimated_rows, 750);
        assert_eq!(result.stats.optimization_time_us, 20);
        assert_eq!(result.stats.plans_considered, 5);
        assert_eq!(result.stats.indexes_used.len(), 1);
        assert!(result.stats.indexes_used.contains(&"idx1".to_string()));
    }
}
