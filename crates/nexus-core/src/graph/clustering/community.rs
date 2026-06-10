//! Community detection and density-based clustering (DBSCAN).

use crate::error::Result;
use crate::graph::simple::{Graph, Node, NodeId, PropertyValue};
use std::collections::HashSet;

use super::engine::ClusteringEngine;
use super::types::{Cluster, ClusteringMetrics, ClusteringResult};

impl ClusteringEngine {
    /// Community detection using a simplified approach
    pub(super) fn community_detection(
        &self,
        graph: &Graph,
        nodes: &[Node],
    ) -> Result<ClusteringResult> {
        // This is a simplified community detection implementation
        // A full implementation would use algorithms like Louvain or Leiden
        let mut visited = HashSet::new();
        let mut clusters = Vec::new();
        let mut cluster_id = 0;

        for node in nodes {
            if !visited.contains(&node.id) {
                let mut cluster_nodes = Vec::new();
                self.dfs_community_detection(graph, node.id, &mut visited, &mut cluster_nodes)?;

                if !cluster_nodes.is_empty() {
                    let mut cluster = Cluster::new(cluster_id, cluster_nodes);
                    cluster.set_metadata(
                        "community_id".to_string(),
                        PropertyValue::Int64(cluster_id as i64),
                    );
                    clusters.push(cluster);
                    cluster_id += 1;
                }
            }
        }

        let features = self.extract_features(graph, nodes)?;
        let metrics = self.calculate_metrics(graph, &clusters, &features, nodes)?;

        Ok(ClusteringResult {
            clusters,
            algorithm: self.config.algorithm.clone(),
            iterations: 0,
            converged: true,
            metrics,
        })
    }

    /// DFS for community detection
    fn dfs_community_detection(
        &self,
        graph: &Graph,
        node_id: NodeId,
        visited: &mut HashSet<NodeId>,
        cluster_nodes: &mut Vec<NodeId>,
    ) -> Result<()> {
        visited.insert(node_id);
        cluster_nodes.push(node_id);

        let edges = graph.get_edges_for_node(node_id)?;
        for edge in edges {
            let neighbor = edge.other_end(node_id).unwrap();
            if !visited.contains(&neighbor) {
                self.dfs_community_detection(graph, neighbor, visited, cluster_nodes)?;
            }
        }

        Ok(())
    }

    /// DBSCAN clustering implementation
    pub(super) fn dbscan_clustering(
        &self,
        graph: &Graph,
        eps: f64,
        min_points: usize,
        nodes: &[Node],
    ) -> Result<ClusteringResult> {
        if nodes.is_empty() {
            return Ok(ClusteringResult {
                clusters: vec![],
                algorithm: self.config.algorithm.clone(),
                iterations: 0,
                converged: true,
                metrics: ClusteringMetrics::default(),
            });
        }

        let features = self.extract_features(graph, nodes)?;
        let mut visited = vec![false; nodes.len()];
        let mut clusters = Vec::new();
        let mut cluster_id = 0;

        for i in 0..nodes.len() {
            if visited[i] {
                continue;
            }

            let neighbors = self.get_neighbors(&features, i, eps);
            if neighbors.len() < min_points {
                continue; // Noise point
            }

            let mut cluster_nodes = Vec::new();
            let mut stack = vec![i];
            visited[i] = true;

            while let Some(point_idx) = stack.pop() {
                cluster_nodes.push(nodes[point_idx].id);
                let point_neighbors = self.get_neighbors(&features, point_idx, eps);

                if point_neighbors.len() >= min_points {
                    for neighbor_idx in point_neighbors {
                        if !visited[neighbor_idx] {
                            visited[neighbor_idx] = true;
                            stack.push(neighbor_idx);
                        }
                    }
                }
            }

            if !cluster_nodes.is_empty() {
                let mut cluster = Cluster::new(cluster_id, cluster_nodes);
                cluster.set_metadata(
                    "density".to_string(),
                    PropertyValue::Int64(min_points as i64),
                );
                clusters.push(cluster);
                cluster_id += 1;
            }
        }

        let metrics = self.calculate_metrics(graph, &clusters, &features, nodes)?;

        Ok(ClusteringResult {
            clusters,
            algorithm: self.config.algorithm.clone(),
            iterations: 0,
            converged: true,
            metrics,
        })
    }

    /// Get neighbors within eps distance
    pub(super) fn get_neighbors(
        &self,
        features: &[Vec<f64>],
        point_idx: usize,
        eps: f64,
    ) -> Vec<usize> {
        let mut neighbors = Vec::new();
        let point_features = &features[point_idx];

        for (i, other_features) in features.iter().enumerate() {
            if i != point_idx {
                let distance = self.calculate_distance(point_features, other_features);
                if distance <= eps {
                    neighbors.push(i);
                }
            }
        }

        neighbors
    }
}
