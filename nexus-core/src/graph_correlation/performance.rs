//! Performance Optimization Utilities for Graph Operations
//!
//! Provides caching, indexing, and optimization strategies

use crate::Result;
use crate::graph_correlation::CorrelationGraph;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Performance metrics for graph operations
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    /// Operation name
    pub operation: String,
    /// Duration of the operation
    pub duration: Duration,
    /// Number of nodes processed
    pub nodes_processed: usize,
    /// Number of edges processed
    pub edges_processed: usize,
    /// Memory usage estimate (bytes)
    pub memory_usage: usize,
}

/// Graph operation cache for frequently accessed data
pub struct GraphCache {
    /// Node adjacency lists (node_id -> list of connected node_ids)
    adjacency_cache: HashMap<String, Vec<String>>,
    /// Node degree cache (node_id -> degree)
    degree_cache: HashMap<String, usize>,
    /// Path cache (source_target key -> path exists)
    path_cache: HashMap<String, bool>,
    /// Cache hit/miss statistics
    cache_hits: usize,
    cache_misses: usize,
}

impl GraphCache {
    /// Create a new graph cache
    pub fn new() -> Self {
        Self {
            adjacency_cache: HashMap::new(),
            degree_cache: HashMap::new(),
            path_cache: HashMap::new(),
            cache_hits: 0,
            cache_misses: 0,
        }
    }

    /// Build cache from a graph
    pub fn build_from_graph(&mut self, graph: &CorrelationGraph) {
        self.clear();

        // Build adjacency lists
        for edge in &graph.edges {
            self.adjacency_cache
                .entry(edge.source.clone())
                .or_default()
                .push(edge.target.clone());
        }

        // Build degree cache
        for node in &graph.nodes {
            let degree = self
                .adjacency_cache
                .get(&node.id)
                .map(|v| v.len())
                .unwrap_or(0);
            self.degree_cache.insert(node.id.clone(), degree);
        }
    }

    /// Get adjacent nodes (cached)
    pub fn get_adjacent_nodes(&mut self, node_id: &str) -> Option<&Vec<String>> {
        if self.adjacency_cache.contains_key(node_id) {
            self.cache_hits += 1;
            self.adjacency_cache.get(node_id)
        } else {
            self.cache_misses += 1;
            None
        }
    }

    /// Get node degree (cached)
    pub fn get_node_degree(&mut self, node_id: &str) -> Option<usize> {
        if let Some(&degree) = self.degree_cache.get(node_id) {
            self.cache_hits += 1;
            Some(degree)
        } else {
            self.cache_misses += 1;
            None
        }
    }

    /// Check if path exists between two nodes (cached)
    pub fn has_path(&mut self, source: &str, target: &str) -> Option<bool> {
        let key = format!("{}:{}", source, target);
        if let Some(&exists) = self.path_cache.get(&key) {
            self.cache_hits += 1;
            Some(exists)
        } else {
            self.cache_misses += 1;
            None
        }
    }

    /// Cache path existence result
    pub fn cache_path(&mut self, source: &str, target: &str, exists: bool) {
        let key = format!("{}:{}", source, target);
        self.path_cache.insert(key, exists);
    }

    /// Get cache statistics
    pub fn get_stats(&self) -> (usize, usize, f64) {
        let total = self.cache_hits + self.cache_misses;
        let hit_rate = if total > 0 {
            self.cache_hits as f64 / total as f64
        } else {
            0.0
        };
        (self.cache_hits, self.cache_misses, hit_rate)
    }

    /// Clear all caches
    pub fn clear(&mut self) {
        self.adjacency_cache.clear();
        self.degree_cache.clear();
        self.path_cache.clear();
        self.cache_hits = 0;
        self.cache_misses = 0;
    }

    /// Get memory usage estimate
    pub fn memory_usage(&self) -> usize {
        let adjacency_size = self.adjacency_cache.len() * 64
            + self
                .adjacency_cache
                .values()
                .map(|v| v.len() * 32)
                .sum::<usize>();
        let degree_size = self.degree_cache.len() * 40;
        let path_size = self.path_cache.len() * 48;

        adjacency_size + degree_size + path_size
    }
}

impl Default for GraphCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Performance profiler for graph operations
pub struct PerformanceProfiler {
    /// Recorded metrics
    metrics: Vec<PerformanceMetrics>,
    /// Current operation start time
    current_operation: Option<(String, Instant)>,
}

impl PerformanceProfiler {
    /// Create a new profiler
    pub fn new() -> Self {
        Self {
            metrics: Vec::new(),
            current_operation: None,
        }
    }

    /// Start profiling an operation
    pub fn start_operation(&mut self, operation: impl Into<String>) {
        self.current_operation = Some((operation.into(), Instant::now()));
    }

    /// End profiling current operation
    pub fn end_operation(&mut self, nodes_processed: usize, edges_processed: usize) {
        if let Some((operation, start_time)) = self.current_operation.take() {
            let duration = start_time.elapsed();
            let memory_usage = (nodes_processed * 128) + (edges_processed * 96); // Estimate

            self.metrics.push(PerformanceMetrics {
                operation,
                duration,
                nodes_processed,
                edges_processed,
                memory_usage,
            });
        }
    }

    /// Get all recorded metrics
    pub fn get_metrics(&self) -> &[PerformanceMetrics] {
        &self.metrics
    }

    /// Get summary statistics
    pub fn get_summary(&self) -> PerformanceSummary {
        let total_duration: Duration = self.metrics.iter().map(|m| m.duration).sum();
        let total_nodes: usize = self.metrics.iter().map(|m| m.nodes_processed).sum();
        let total_edges: usize = self.metrics.iter().map(|m| m.edges_processed).sum();
        let total_memory: usize = self.metrics.iter().map(|m| m.memory_usage).sum();

        let avg_duration = if !self.metrics.is_empty() {
            total_duration / self.metrics.len() as u32
        } else {
            Duration::ZERO
        };

        PerformanceSummary {
            total_operations: self.metrics.len(),
            total_duration,
            avg_duration,
            total_nodes_processed: total_nodes,
            total_edges_processed: total_edges,
            total_memory_usage: total_memory,
        }
    }

    /// Clear all metrics
    pub fn clear(&mut self) {
        self.metrics.clear();
        self.current_operation = None;
    }
}

impl Default for PerformanceProfiler {
    fn default() -> Self {
        Self::new()
    }
}

/// Performance summary statistics
#[derive(Debug, Clone)]
pub struct PerformanceSummary {
    /// Total number of operations
    pub total_operations: usize,
    /// Total duration of all operations
    pub total_duration: Duration,
    /// Average duration per operation
    pub avg_duration: Duration,
    /// Total nodes processed
    pub total_nodes_processed: usize,
    /// Total edges processed
    pub total_edges_processed: usize,
    /// Total memory usage
    pub total_memory_usage: usize,
}

/// Optimize graph structure for faster queries
pub fn optimize_graph(graph: &mut CorrelationGraph) -> Result<()> {
    // Remove duplicate edges
    let mut seen_edges = std::collections::HashSet::new();
    graph.edges.retain(|edge| {
        let key = (
            edge.source.clone(),
            edge.target.clone(),
            format!("{:?}", edge.edge_type),
        );
        seen_edges.insert(key)
    });

    // Remove orphaned edges (edges pointing to non-existent nodes)
    let node_ids: std::collections::HashSet<_> = graph.nodes.iter().map(|n| n.id.clone()).collect();
    graph
        .edges
        .retain(|e| node_ids.contains(&e.source) && node_ids.contains(&e.target));

    Ok(())
}

/// Calculate graph complexity score
pub fn calculate_complexity(graph: &CorrelationGraph) -> f64 {
    let node_count = graph.nodes.len() as f64;
    let edge_count = graph.edges.len() as f64;

    if node_count == 0.0 {
        return 0.0;
    }

    // Complexity based on density and structure
    let density = if node_count > 1.0 {
        edge_count / (node_count * (node_count - 1.0))
    } else {
        0.0
    };

    let avg_degree = if node_count > 0.0 {
        (2.0 * edge_count) / node_count
    } else {
        0.0
    };

    // Normalize complexity score (0.0 to 1.0)
    (density * 0.5 + (avg_degree / 10.0).min(1.0) * 0.5).min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph_correlation::{EdgeType, GraphEdge, GraphNode, GraphType, NodeType};

    fn create_test_graph() -> CorrelationGraph {
        CorrelationGraph {
            name: "Test Graph".to_string(),
            graph_type: GraphType::Call,
            nodes: vec![
                GraphNode {
                    id: "n1".to_string(),
                    node_type: NodeType::Function,
                    label: "func1".to_string(),
                    metadata: serde_json::Map::new(),
                    position: None,
                    size: None,
                },
                GraphNode {
                    id: "n2".to_string(),
                    node_type: NodeType::Function,
                    label: "func2".to_string(),
                    metadata: serde_json::Map::new(),
                    position: None,
                    size: None,
                },
            ],
            edges: vec![GraphEdge {
                source: "n1".to_string(),
                target: "n2".to_string(),
                edge_type: EdgeType::Calls,
                label: None,
                metadata: serde_json::Map::new(),
            }],
            metadata: serde_json::Map::new(),
        }
    }

    #[test]
    fn test_graph_cache_creation() {
        let cache = GraphCache::new();
        assert_eq!(cache.cache_hits, 0);
        assert_eq!(cache.cache_misses, 0);
    }

    #[test]
    fn test_graph_cache_build() {
        let graph = create_test_graph();
        let mut cache = GraphCache::new();
        cache.build_from_graph(&graph);

        assert!(cache.adjacency_cache.contains_key("n1"));
        assert_eq!(cache.degree_cache.get("n1"), Some(&1));
    }

    #[test]
    fn test_cache_statistics() {
        let mut cache = GraphCache::new();
        cache
            .adjacency_cache
            .insert("n1".to_string(), vec!["n2".to_string()]);

        let _ = cache.get_adjacent_nodes("n1");
        let _ = cache.get_adjacent_nodes("n3");

        let (hits, misses, hit_rate) = cache.get_stats();
        assert_eq!(hits, 1);
        assert_eq!(misses, 1);
        assert!((hit_rate - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_performance_profiler() {
        let mut profiler = PerformanceProfiler::new();
        profiler.start_operation("test_operation");
        std::thread::sleep(Duration::from_millis(10));
        profiler.end_operation(100, 50);

        let metrics = profiler.get_metrics();
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].operation, "test_operation");
        assert!(metrics[0].duration.as_millis() >= 10);
    }

    #[test]
    fn test_optimize_graph() {
        let mut graph = create_test_graph();

        // Add duplicate edge
        graph.edges.push(GraphEdge {
            source: "n1".to_string(),
            target: "n2".to_string(),
            edge_type: EdgeType::Calls,
            label: None,
            metadata: serde_json::Map::new(),
        });

        let result = optimize_graph(&mut graph);
        assert!(result.is_ok());
        assert_eq!(graph.edges.len(), 1);
    }

    #[test]
    fn test_calculate_complexity() {
        let graph = create_test_graph();
        let complexity = calculate_complexity(&graph);
        assert!(complexity >= 0.0 && complexity <= 1.0);
    }
}
