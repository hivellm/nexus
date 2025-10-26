//! Graph Statistics and Metrics
//!
//! Calculate various graph metrics and statistics

use crate::graph_correlation::CorrelationGraph;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Graph statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphStatistics {
    pub node_count: usize,
    pub edge_count: usize,
    pub avg_degree: f64,
    pub max_degree: usize,
    pub min_degree: usize,
    pub density: f64,
    pub connected_components: usize,
    pub avg_clustering_coefficient: f64,
    pub diameter: Option<usize>,
}

/// Calculate comprehensive graph statistics
pub fn calculate_statistics(graph: &CorrelationGraph) -> GraphStatistics {
    let node_count = graph.nodes.len();
    let edge_count = graph.edges.len();

    if node_count == 0 {
        return GraphStatistics {
            node_count: 0,
            edge_count: 0,
            avg_degree: 0.0,
            max_degree: 0,
            min_degree: 0,
            density: 0.0,
            connected_components: 0,
            avg_clustering_coefficient: 0.0,
            diameter: None,
        };
    }

    let degrees = calculate_degrees(graph);
    let avg_degree = degrees.values().sum::<usize>() as f64 / node_count as f64;
    let max_degree = *degrees.values().max().unwrap_or(&0);
    let min_degree = *degrees.values().min().unwrap_or(&0);

    let density = if node_count > 1 {
        (2 * edge_count) as f64 / (node_count * (node_count - 1)) as f64
    } else {
        0.0
    };

    let connected_components = count_connected_components(graph);
    let avg_clustering_coefficient = calculate_avg_clustering_coefficient(graph);
    let diameter = calculate_diameter(graph);

    GraphStatistics {
        node_count,
        edge_count,
        avg_degree,
        max_degree,
        min_degree,
        density,
        connected_components,
        avg_clustering_coefficient,
        diameter,
    }
}

fn calculate_degrees(graph: &CorrelationGraph) -> HashMap<String, usize> {
    let mut degrees = HashMap::new();

    for node in &graph.nodes {
        degrees.insert(node.id.clone(), 0);
    }

    for edge in &graph.edges {
        *degrees.entry(edge.source.clone()).or_insert(0) += 1;
        *degrees.entry(edge.target.clone()).or_insert(0) += 1;
    }

    degrees
}

fn count_connected_components(graph: &CorrelationGraph) -> usize {
    let mut visited = HashSet::new();
    let mut components = 0;

    for node in &graph.nodes {
        if !visited.contains(&node.id) {
            dfs_component(graph, &node.id, &mut visited);
            components += 1;
        }
    }

    components
}

fn dfs_component(graph: &CorrelationGraph, node_id: &str, visited: &mut HashSet<String>) {
    visited.insert(node_id.to_string());

    for edge in &graph.edges {
        if edge.source == node_id && !visited.contains(&edge.target) {
            dfs_component(graph, &edge.target, visited);
        } else if edge.target == node_id && !visited.contains(&edge.source) {
            dfs_component(graph, &edge.source, visited);
        }
    }
}

fn calculate_avg_clustering_coefficient(graph: &CorrelationGraph) -> f64 {
    if graph.nodes.len() < 3 {
        return 0.0;
    }

    let mut total_coefficient = 0.0;
    let mut count = 0;

    for node in &graph.nodes {
        let neighbors = get_neighbors(graph, &node.id);
        if neighbors.len() < 2 {
            continue;
        }

        let mut triangles = 0;
        let possible_triangles = neighbors.len() * (neighbors.len() - 1) / 2;

        for i in 0..neighbors.len() {
            for j in (i + 1)..neighbors.len() {
                if are_connected(graph, &neighbors[i], &neighbors[j]) {
                    triangles += 1;
                }
            }
        }

        if possible_triangles > 0 {
            total_coefficient += triangles as f64 / possible_triangles as f64;
            count += 1;
        }
    }

    if count > 0 {
        total_coefficient / count as f64
    } else {
        0.0
    }
}

fn get_neighbors(graph: &CorrelationGraph, node_id: &str) -> Vec<String> {
    let mut neighbors = Vec::new();

    for edge in &graph.edges {
        if edge.source == node_id {
            neighbors.push(edge.target.clone());
        } else if edge.target == node_id {
            neighbors.push(edge.source.clone());
        }
    }

    neighbors
}

fn are_connected(graph: &CorrelationGraph, node_a: &str, node_b: &str) -> bool {
    graph.edges.iter().any(|edge| {
        (edge.source == node_a && edge.target == node_b)
            || (edge.source == node_b && edge.target == node_a)
    })
}

fn calculate_diameter(graph: &CorrelationGraph) -> Option<usize> {
    if graph.nodes.is_empty() {
        return None;
    }

    let mut max_distance = 0;

    for node in &graph.nodes {
        let distances = bfs_distances(graph, &node.id);
        if let Some(&max_dist) = distances.values().max() {
            max_distance = max_distance.max(max_dist);
        }
    }

    if max_distance == 0 {
        None
    } else {
        Some(max_distance)
    }
}

fn bfs_distances(graph: &CorrelationGraph, start: &str) -> HashMap<String, usize> {
    let mut distances = HashMap::new();
    let mut queue = std::collections::VecDeque::new();

    distances.insert(start.to_string(), 0);
    queue.push_back(start.to_string());

    while let Some(current) = queue.pop_front() {
        let current_dist = *distances.get(&current).unwrap();

        for edge in &graph.edges {
            let next = if edge.source == current {
                Some(&edge.target)
            } else if edge.target == current {
                Some(&edge.source)
            } else {
                None
            };

            if let Some(next_node) = next {
                if !distances.contains_key(next_node) {
                    distances.insert(next_node.clone(), current_dist + 1);
                    queue.push_back(next_node.clone());
                }
            }
        }
    }

    distances
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph_correlation::{EdgeType, GraphEdge, GraphNode, GraphType, NodeType};
    use std::collections::HashMap;

    fn create_test_graph() -> CorrelationGraph {
        let nodes = vec![
            GraphNode {
                id: "node1".to_string(),
                label: "Node 1".to_string(),
                node_type: NodeType::Function,
                properties: HashMap::new(),
            },
            GraphNode {
                id: "node2".to_string(),
                label: "Node 2".to_string(),
                node_type: NodeType::Function,
                properties: HashMap::new(),
            },
            GraphNode {
                id: "node3".to_string(),
                label: "Node 3".to_string(),
                node_type: NodeType::Function,
                properties: HashMap::new(),
            },
        ];

        let edges = vec![
            GraphEdge {
                source: "node1".to_string(),
                target: "node2".to_string(),
                edge_type: EdgeType::Calls,
                properties: HashMap::new(),
            },
            GraphEdge {
                source: "node2".to_string(),
                target: "node3".to_string(),
                edge_type: EdgeType::Calls,
                properties: HashMap::new(),
            },
        ];

        CorrelationGraph {
            graph_type: GraphType::Call,
            nodes,
            edges,
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn test_empty_graph_statistics() {
        let graph = CorrelationGraph::new(GraphType::Call, "Test".to_string());
        let stats = calculate_statistics(&graph);
        assert_eq!(stats.node_count, 0);
        assert_eq!(stats.edge_count, 0);
        assert_eq!(stats.density, 0.0);
        assert_eq!(stats.connected_components, 0);
    }

    #[test]
    fn test_graph_with_nodes_and_edges() {
        let graph = create_test_graph();
        let stats = calculate_statistics(&graph);
        assert_eq!(stats.node_count, 3);
        assert_eq!(stats.edge_count, 2);
        assert!(stats.density > 0.0);
        assert_eq!(stats.connected_components, 1);
    }

    #[test]
    fn test_calculate_degrees_empty() {
        let graph = CorrelationGraph::new(GraphType::Call, "Test".to_string());
        let degrees = calculate_degrees(&graph);
        assert_eq!(degrees.len(), 0);
    }

    #[test]
    fn test_calculate_degrees_with_edges() {
        let graph = create_test_graph();
        let degrees = calculate_degrees(&graph);
        assert_eq!(degrees.len(), 3);
        assert_eq!(degrees.get("node1"), Some(&1));
        assert_eq!(degrees.get("node2"), Some(&2));
        assert_eq!(degrees.get("node3"), Some(&1));
    }

    #[test]
    fn test_calculate_density_empty() {
        let graph = CorrelationGraph::new(GraphType::Call, "Test".to_string());
        let density = calculate_density(&graph);
        assert_eq!(density, 0.0);
    }

    #[test]
    fn test_calculate_density_single_node() {
        let mut graph = CorrelationGraph::new(GraphType::Call, "Test".to_string());
        graph.nodes.push(GraphNode {
            id: "node1".to_string(),
            label: "Node 1".to_string(),
            node_type: NodeType::Function,
            properties: HashMap::new(),
        });
        let density = calculate_density(&graph);
        assert_eq!(density, 0.0);
    }

    #[test]
    fn test_calculate_density_with_edges() {
        let graph = create_test_graph();
        let density = calculate_density(&graph);
        assert!(density > 0.0);
        assert!(density <= 1.0);
    }

    #[test]
    fn test_find_connected_components_empty() {
        let graph = CorrelationGraph::new(GraphType::Call, "Test".to_string());
        let components = find_connected_components(&graph);
        assert_eq!(components, 0);
    }

    #[test]
    fn test_find_connected_components_single() {
        let graph = create_test_graph();
        let components = find_connected_components(&graph);
        assert_eq!(components, 1);
    }

    #[test]
    fn test_calculate_clustering_coefficient_empty() {
        let graph = CorrelationGraph::new(GraphType::Call, "Test".to_string());
        let coefficient = calculate_clustering_coefficient(&graph);
        assert_eq!(coefficient, 0.0);
    }

    #[test]
    fn test_calculate_clustering_coefficient_with_nodes() {
        let graph = create_test_graph();
        let coefficient = calculate_clustering_coefficient(&graph);
        assert!(coefficient >= 0.0);
        assert!(coefficient <= 1.0);
    }

    #[test]
    fn test_calculate_graph_diameter_empty() {
        let graph = CorrelationGraph::new(GraphType::Call, "Test".to_string());
        let diameter = calculate_graph_diameter(&graph);
        assert_eq!(diameter, 0);
    }

    #[test]
    fn test_calculate_graph_diameter_with_path() {
        let graph = create_test_graph();
        let diameter = calculate_graph_diameter(&graph);
        assert_eq!(diameter, 2); // node1 -> node2 -> node3
    }

    #[test]
    fn test_dfs_traversal_empty() {
        let graph = CorrelationGraph::new(GraphType::Call, "Test".to_string());
        let visited = dfs_traversal(&graph, "nonexistent");
        assert_eq!(visited.len(), 0);
    }

    #[test]
    fn test_dfs_traversal_from_start() {
        let graph = create_test_graph();
        let visited = dfs_traversal(&graph, "node1");
        assert_eq!(visited.len(), 3);
        assert!(visited.contains("node1"));
        assert!(visited.contains("node2"));
        assert!(visited.contains("node3"));
    }

    #[test]
    fn test_bfs_distances_empty() {
        let graph = CorrelationGraph::new(GraphType::Call, "Test".to_string());
        let distances = bfs_distances(&graph, "nonexistent");
        assert_eq!(distances.len(), 0);
    }

    #[test]
    fn test_bfs_distances_from_start() {
        let graph = create_test_graph();
        let distances = bfs_distances(&graph, "node1");
        assert_eq!(distances.get("node1"), Some(&0));
        assert_eq!(distances.get("node2"), Some(&1));
        assert_eq!(distances.get("node3"), Some(&2));
    }

    #[test]
    fn test_graph_statistics_structure() {
        let stats = GraphStatistics {
            node_count: 10,
            edge_count: 20,
            avg_degree: 4.0,
            density: 0.5,
            connected_components: 2,
            clustering_coefficient: 0.3,
            diameter: 5,
        };

        assert_eq!(stats.node_count, 10);
        assert_eq!(stats.edge_count, 20);
        assert_eq!(stats.avg_degree, 4.0);
        assert_eq!(stats.density, 0.5);
    }
}
