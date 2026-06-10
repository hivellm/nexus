//! ClusteringEngine core: struct definition, algorithm dispatch, feature
//! extraction, and distance calculation.

use crate::error::Result;
use crate::graph::simple::{Graph, Node};

use super::types::{
    ClusteringAlgorithm, ClusteringConfig, ClusteringMetrics, ClusteringResult, DistanceMetric,
    FeatureStrategy,
};

/// Main clustering engine
pub struct ClusteringEngine {
    pub(super) config: ClusteringConfig,
}

impl ClusteringEngine {
    /// Create a new clustering engine with the given configuration
    pub fn new(config: ClusteringConfig) -> Self {
        Self { config }
    }

    /// Perform clustering on the given graph
    pub fn cluster(&self, graph: &Graph) -> Result<ClusteringResult> {
        let nodes = graph.get_all_nodes()?;
        if nodes.is_empty() {
            return Ok(ClusteringResult {
                clusters: vec![],
                algorithm: self.config.algorithm.clone(),
                iterations: 0,
                converged: true,
                metrics: ClusteringMetrics::default(),
            });
        }

        // Convert Vec<&Node> to Vec<Node> for easier handling
        let nodes: Vec<Node> = nodes.into_iter().cloned().collect();

        match &self.config.algorithm {
            ClusteringAlgorithm::KMeans { k, max_iterations } => {
                self.kmeans_clustering(graph, *k, *max_iterations, &nodes)
            }
            ClusteringAlgorithm::Hierarchical { linkage } => {
                self.hierarchical_clustering(graph, linkage, &nodes)
            }
            ClusteringAlgorithm::LabelBased => self.label_based_grouping(graph, &nodes),
            ClusteringAlgorithm::PropertyBased { property_key } => {
                self.property_based_grouping(graph, property_key, &nodes)
            }
            ClusteringAlgorithm::CommunityDetection => self.community_detection(graph, &nodes),
            ClusteringAlgorithm::DBSCAN { eps, min_points } => {
                self.dbscan_clustering(graph, *eps, *min_points, &nodes)
            }
        }
    }

    /// Extract features from nodes based on the configured strategy
    pub(super) fn extract_features(&self, graph: &Graph, nodes: &[Node]) -> Result<Vec<Vec<f64>>> {
        let mut features = Vec::new();

        for node in nodes {
            let node_features = match &self.config.feature_strategy {
                FeatureStrategy::LabelBased => self.extract_label_features(node),
                FeatureStrategy::PropertyBased { property_keys } => {
                    self.extract_property_features(node, property_keys)
                }
                FeatureStrategy::Structural => self.extract_structural_features(graph, node),
                FeatureStrategy::Combined { strategies } => {
                    self.extract_combined_features(graph, node, strategies)
                }
            };
            features.push(node_features);
        }

        Ok(features)
    }

    /// Extract features from node labels
    fn extract_label_features(&self, node: &Node) -> Vec<f64> {
        // Create a binary vector for each possible label
        // This is a simplified implementation - in practice, you'd want to
        // maintain a global label vocabulary
        let mut features = vec![0.0; 10]; // Fixed size for simplicity

        for (i, _label) in node.labels.iter().enumerate() {
            if i < features.len() {
                features[i] = 1.0;
            }
        }

        features
    }

    /// Extract features from node properties
    fn extract_property_features(&self, node: &Node, property_keys: &[String]) -> Vec<f64> {
        use crate::graph::simple::PropertyValue;

        let mut features = Vec::new();

        for key in property_keys {
            if let Some(value) = node.get_property(key) {
                match value {
                    PropertyValue::Int64(i) => features.push(*i as f64),
                    PropertyValue::Float64(f) => features.push(*f),
                    PropertyValue::Bool(b) => features.push(if *b { 1.0 } else { 0.0 }),
                    PropertyValue::String(s) => {
                        // Simple string hashing for numeric representation
                        features.push(s.len() as f64);
                    }
                    _ => features.push(0.0),
                }
            } else {
                features.push(0.0);
            }
        }

        features
    }

    /// Extract structural features from the graph
    fn extract_structural_features(&self, graph: &Graph, node: &Node) -> Vec<f64> {
        let mut features = Vec::new();

        // Degree centrality (number of connections)
        let edges = graph.get_edges_for_node(node.id).unwrap_or_default();
        features.push(edges.len() as f64);

        // In-degree and out-degree
        let in_degree = edges.iter().filter(|e| e.target == node.id).count();
        let out_degree = edges.iter().filter(|e| e.source == node.id).count();
        features.push(in_degree as f64);
        features.push(out_degree as f64);

        // Label count
        features.push(node.labels.len() as f64);

        // Property count
        features.push(node.properties.len() as f64);

        features
    }

    /// Extract features using multiple strategies
    fn extract_combined_features(
        &self,
        graph: &Graph,
        node: &Node,
        strategies: &[FeatureStrategy],
    ) -> Vec<f64> {
        let mut all_features = Vec::new();

        for strategy in strategies {
            let features = match strategy {
                FeatureStrategy::LabelBased => self.extract_label_features(node),
                FeatureStrategy::PropertyBased { property_keys } => {
                    self.extract_property_features(node, property_keys)
                }
                FeatureStrategy::Structural => self.extract_structural_features(graph, node),
                FeatureStrategy::Combined { .. } => {
                    // Avoid infinite recursion
                    vec![]
                }
            };
            all_features.extend(features);
        }

        all_features
    }

    /// Calculate distance between two feature vectors
    pub(super) fn calculate_distance(&self, features1: &[f64], features2: &[f64]) -> f64 {
        if features1.len() != features2.len() {
            return f64::INFINITY;
        }

        match self.config.distance_metric {
            DistanceMetric::Euclidean => features1
                .iter()
                .zip(features2.iter())
                .map(|(a, b)| (a - b).powi(2))
                .sum::<f64>()
                .sqrt(),
            DistanceMetric::Manhattan => features1
                .iter()
                .zip(features2.iter())
                .map(|(a, b)| (a - b).abs())
                .sum(),
            DistanceMetric::Cosine => {
                let dot_product: f64 = features1
                    .iter()
                    .zip(features2.iter())
                    .map(|(a, b)| a * b)
                    .sum();
                let norm1: f64 = features1.iter().map(|x| x.powi(2)).sum::<f64>().sqrt();
                let norm2: f64 = features2.iter().map(|x| x.powi(2)).sum::<f64>().sqrt();

                if norm1 == 0.0 || norm2 == 0.0 {
                    1.0
                } else {
                    1.0 - (dot_product / (norm1 * norm2))
                }
            }
            DistanceMetric::Jaccard => {
                // For binary features
                let intersection: f64 = features1
                    .iter()
                    .zip(features2.iter())
                    .map(|(a, b)| if *a > 0.0 && *b > 0.0 { 1.0 } else { 0.0 })
                    .sum();
                let union: f64 = features1
                    .iter()
                    .zip(features2.iter())
                    .map(|(a, b)| if *a > 0.0 || *b > 0.0 { 1.0 } else { 0.0 })
                    .sum();

                if union == 0.0 {
                    1.0
                } else {
                    1.0 - (intersection / union)
                }
            }
            DistanceMetric::Hamming => features1
                .iter()
                .zip(features2.iter())
                .map(|(a, b)| {
                    if (a - b).abs() < f64::EPSILON {
                        0.0
                    } else {
                        1.0
                    }
                })
                .sum(),
        }
    }

    /// Check if k-means has converged
    pub(super) fn check_convergence(
        &self,
        old_centroids: &[Vec<f64>],
        new_centroids: &[Vec<f64>],
    ) -> bool {
        if old_centroids.len() != new_centroids.len() {
            return false;
        }

        let threshold = 1e-6;
        for (old, new) in old_centroids.iter().zip(new_centroids.iter()) {
            if old.len() != new.len() {
                return false;
            }
            for (o, n) in old.iter().zip(new.iter()) {
                if (o - n).abs() > threshold {
                    return false;
                }
            }
        }
        true
    }

    /// Extract features for a specific node
    pub(super) fn extract_features_for_node(&self, graph: &Graph, node: &Node) -> Result<Vec<f64>> {
        let nodes = vec![node.clone()];
        let features = self.extract_features(graph, &nodes)?;
        Ok(features.into_iter().next().unwrap_or_default())
    }
}
