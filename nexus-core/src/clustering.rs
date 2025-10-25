//! Node clustering and grouping algorithms
//!
//! This module provides various clustering algorithms and grouping strategies
//! for organizing nodes in the graph based on their properties, labels, and
//! structural relationships.

use crate::error::Result;
use crate::graph_simple::{Graph, Node, NodeId, PropertyValue};
use std::collections::{HashMap, HashSet};
// use std::cmp::{max, min}; // Not used in current implementation
use std::f64;

/// Represents a cluster of nodes
#[derive(Debug, Clone)]
pub struct Cluster {
    /// Unique identifier for this cluster
    pub id: u64,
    /// Nodes belonging to this cluster
    pub nodes: Vec<NodeId>,
    /// Cluster centroid (for geometric algorithms)
    pub centroid: Option<Vec<f64>>,
    /// Cluster metadata
    pub metadata: HashMap<String, PropertyValue>,
}

impl Cluster {
    /// Create a new cluster with the given ID and nodes
    pub fn new(id: u64, nodes: Vec<NodeId>) -> Self {
        Self {
            id,
            nodes,
            centroid: None,
            metadata: HashMap::new(),
        }
    }

    /// Create a new cluster with centroid
    pub fn with_centroid(id: u64, nodes: Vec<NodeId>, centroid: Vec<f64>) -> Self {
        Self {
            id,
            nodes,
            centroid: Some(centroid),
            metadata: HashMap::new(),
        }
    }

    /// Get the number of nodes in this cluster
    pub fn size(&self) -> usize {
        self.nodes.len()
    }

    /// Check if this cluster is empty
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Add a node to this cluster
    pub fn add_node(&mut self, node_id: NodeId) {
        if !self.nodes.contains(&node_id) {
            self.nodes.push(node_id);
        }
    }

    /// Remove a node from this cluster
    pub fn remove_node(&mut self, node_id: NodeId) -> bool {
        if let Some(pos) = self.nodes.iter().position(|&id| id == node_id) {
            self.nodes.remove(pos);
            true
        } else {
            false
        }
    }

    /// Set cluster metadata
    pub fn set_metadata(&mut self, key: String, value: PropertyValue) {
        self.metadata.insert(key, value);
    }

    /// Get cluster metadata
    pub fn get_metadata(&self, key: &str) -> Option<&PropertyValue> {
        self.metadata.get(key)
    }
}

/// Clustering algorithm types
#[derive(Debug, Clone, PartialEq)]
pub enum ClusteringAlgorithm {
    /// K-means clustering
    KMeans { k: usize, max_iterations: usize },
    /// Hierarchical clustering
    Hierarchical { linkage: LinkageType },
    /// Label-based grouping
    LabelBased,
    /// Property-based grouping
    PropertyBased { property_key: String },
    /// Community detection (Louvain algorithm)
    CommunityDetection,
    /// Density-based clustering (DBSCAN)
    DBSCAN { eps: f64, min_points: usize },
}

/// Linkage types for hierarchical clustering
#[derive(Debug, Clone, PartialEq)]
pub enum LinkageType {
    /// Single linkage (minimum distance)
    Single,
    /// Complete linkage (maximum distance)
    Complete,
    /// Average linkage (average distance)
    Average,
    /// Ward linkage (minimizes within-cluster variance)
    Ward,
}

/// Clustering configuration
#[derive(Debug, Clone)]
pub struct ClusteringConfig {
    /// Algorithm to use
    pub algorithm: ClusteringAlgorithm,
    /// Feature extraction strategy
    pub feature_strategy: FeatureStrategy,
    /// Distance metric for clustering
    pub distance_metric: DistanceMetric,
    /// Random seed for reproducible results
    pub random_seed: Option<u64>,
}

impl Default for ClusteringConfig {
    fn default() -> Self {
        Self {
            algorithm: ClusteringAlgorithm::KMeans {
                k: 3,
                max_iterations: 100,
            },
            feature_strategy: FeatureStrategy::LabelBased,
            distance_metric: DistanceMetric::Euclidean,
            random_seed: None,
        }
    }
}

/// Feature extraction strategies
#[derive(Debug, Clone, PartialEq)]
pub enum FeatureStrategy {
    /// Extract features from node labels
    LabelBased,
    /// Extract features from node properties
    PropertyBased { property_keys: Vec<String> },
    /// Extract features from structural properties (degree, centrality, etc.)
    Structural,
    /// Extract features from a combination of sources
    Combined { strategies: Vec<FeatureStrategy> },
}

/// Distance metrics for clustering
#[derive(Debug, Clone, PartialEq)]
pub enum DistanceMetric {
    /// Euclidean distance
    Euclidean,
    /// Manhattan distance
    Manhattan,
    /// Cosine similarity
    Cosine,
    /// Jaccard similarity
    Jaccard,
    /// Hamming distance
    Hamming,
}

/// Clustering result containing clusters and metadata
#[derive(Debug, Clone)]
pub struct ClusteringResult {
    /// Generated clusters
    pub clusters: Vec<Cluster>,
    /// Algorithm used
    pub algorithm: ClusteringAlgorithm,
    /// Number of iterations performed
    pub iterations: usize,
    /// Convergence status
    pub converged: bool,
    /// Quality metrics
    pub metrics: ClusteringMetrics,
}

/// Quality metrics for clustering results
#[derive(Debug, Clone)]
pub struct ClusteringMetrics {
    /// Silhouette score (-1 to 1, higher is better)
    pub silhouette_score: f64,
    /// Within-cluster sum of squares
    pub wcss: f64,
    /// Between-cluster sum of squares
    pub bcss: f64,
    /// Calinski-Harabasz index (higher is better)
    pub calinski_harabasz: f64,
    /// Davies-Bouldin index (lower is better)
    pub davies_bouldin: f64,
}

impl Default for ClusteringMetrics {
    fn default() -> Self {
        Self {
            silhouette_score: 0.0,
            wcss: 0.0,
            bcss: 0.0,
            calinski_harabasz: 0.0,
            davies_bouldin: f64::INFINITY,
        }
    }
}

/// Main clustering engine
pub struct ClusteringEngine {
    config: ClusteringConfig,
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
    fn extract_features(&self, graph: &Graph, nodes: &[Node]) -> Result<Vec<Vec<f64>>> {
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
    fn calculate_distance(&self, features1: &[f64], features2: &[f64]) -> f64 {
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

    /// K-means clustering implementation
    fn kmeans_clustering(
        &self,
        graph: &Graph,
        k: usize,
        max_iterations: usize,
        nodes: &[Node],
    ) -> Result<ClusteringResult> {
        if nodes.is_empty() || k == 0 {
            return Ok(ClusteringResult {
                clusters: vec![],
                algorithm: self.config.algorithm.clone(),
                iterations: 0,
                converged: true,
                metrics: ClusteringMetrics::default(),
            });
        }

        let features = self.extract_features(graph, nodes)?;
        let n = features.len();
        let actual_k = k.min(n);

        // Initialize centroids randomly
        let mut centroids = self.initialize_centroids(&features, actual_k)?;
        let mut clusters = vec![Cluster::new(0, vec![]); actual_k];
        let mut converged = false;
        let mut iterations = 0;

        for iteration in 0..max_iterations {
            iterations = iteration + 1;

            // Assign nodes to clusters
            for cluster in clusters.iter_mut().take(actual_k) {
                cluster.nodes.clear();
            }

            for (node_idx, feature_vector) in features.iter().enumerate() {
                let mut best_cluster = 0;
                let mut best_distance = f64::INFINITY;

                for (cluster_idx, centroid) in centroids.iter().enumerate() {
                    let distance = self.calculate_distance(feature_vector, centroid);
                    if distance < best_distance {
                        best_distance = distance;
                        best_cluster = cluster_idx;
                    }
                }

                clusters[best_cluster].add_node(nodes[node_idx].id);
            }

            // Update centroids
            let mut new_centroids = Vec::new();
            for cluster in &clusters {
                if cluster.is_empty() {
                    new_centroids.push(centroids[cluster.id as usize].clone());
                    continue;
                }

                let mut new_centroid = vec![0.0; features[0].len()];
                for &node_id in &cluster.nodes {
                    if let Some(node) = nodes.iter().find(|n| n.id == node_id) {
                        let node_features = self.extract_features_for_node(graph, node)?;
                        for (i, feature) in node_features.iter().enumerate() {
                            if i < new_centroid.len() {
                                new_centroid[i] += feature;
                            }
                        }
                    }
                }

                let cluster_size = cluster.size() as f64;
                if cluster_size > 0.0 {
                    for feature in &mut new_centroid {
                        *feature /= cluster_size;
                    }
                }

                new_centroids.push(new_centroid);
            }

            // Check for convergence
            converged = self.check_convergence(&centroids, &new_centroids);
            centroids = new_centroids;

            if converged {
                break;
            }
        }

        // Update cluster centroids
        for (i, cluster) in clusters.iter_mut().enumerate() {
            cluster.id = i as u64;
            cluster.centroid = Some(centroids[i].clone());
        }

        // Calculate metrics
        let metrics = self.calculate_metrics(graph, &clusters, &features, nodes)?;

        Ok(ClusteringResult {
            clusters,
            algorithm: self.config.algorithm.clone(),
            iterations,
            converged,
            metrics,
        })
    }

    /// Initialize centroids for k-means
    fn initialize_centroids(&self, features: &[Vec<f64>], k: usize) -> Result<Vec<Vec<f64>>> {
        if features.is_empty() {
            return Ok(vec![]);
        }

        let mut centroids = Vec::new();
        let _feature_dim = features[0].len();

        // Use k-means++ initialization
        let mut rng = if let Some(seed) = self.config.random_seed {
            SimpleRng::new(seed)
        } else {
            SimpleRng::new(42)
        };

        // Choose first centroid randomly
        let first_idx = rng.gen_range(0..features.len());
        centroids.push(features[first_idx].clone());

        // Choose remaining centroids using k-means++ strategy
        for _ in 1..k {
            let mut distances = Vec::new();
            for feature in features {
                let min_distance = centroids
                    .iter()
                    .map(|centroid| self.calculate_distance(feature, centroid))
                    .fold(f64::INFINITY, f64::min);
                distances.push(min_distance);
            }

            // Convert distances to probabilities
            let total_distance: f64 = distances.iter().sum();
            if total_distance > 0.0 {
                let probabilities: Vec<f64> =
                    distances.iter().map(|d| d / total_distance).collect();

                // Choose next centroid based on probabilities
                let random_val: f64 = rng.gen_f64();
                let mut cumulative = 0.0;
                let mut chosen_idx = 0;

                for (i, prob) in probabilities.iter().enumerate() {
                    cumulative += prob;
                    if random_val <= cumulative {
                        chosen_idx = i;
                        break;
                    }
                }

                centroids.push(features[chosen_idx].clone());
            } else {
                // Fallback to random selection
                let random_idx = rng.gen_range(0..features.len());
                centroids.push(features[random_idx].clone());
            }
        }

        Ok(centroids)
    }

    /// Check if k-means has converged
    fn check_convergence(&self, old_centroids: &[Vec<f64>], new_centroids: &[Vec<f64>]) -> bool {
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
    fn extract_features_for_node(&self, graph: &Graph, node: &Node) -> Result<Vec<f64>> {
        let nodes = vec![node.clone()];
        let features = self.extract_features(graph, &nodes)?;
        Ok(features.into_iter().next().unwrap_or_default())
    }

    /// Hierarchical clustering implementation
    fn hierarchical_clustering(
        &self,
        graph: &Graph,
        _linkage: &LinkageType,
        nodes: &[Node],
    ) -> Result<ClusteringResult> {
        // Simplified hierarchical clustering implementation
        let features = self.extract_features(graph, nodes)?;
        let n = features.len();

        // Start with each node as its own cluster
        let clusters: Vec<Cluster> = (0..n)
            .map(|i| Cluster::new(i as u64, vec![nodes[i].id]))
            .collect();

        // For now, just return the initial clusters
        // A full implementation would perform the hierarchical merging
        let metrics = self.calculate_metrics(graph, &clusters, &features, nodes)?;

        Ok(ClusteringResult {
            clusters,
            algorithm: self.config.algorithm.clone(),
            iterations: 0,
            converged: true,
            metrics,
        })
    }

    /// Label-based grouping
    fn label_based_grouping(&self, graph: &Graph, nodes: &[Node]) -> Result<ClusteringResult> {
        let mut label_groups: HashMap<String, Vec<NodeId>> = HashMap::new();

        for node in nodes {
            for label in &node.labels {
                label_groups.entry(label.clone()).or_default().push(node.id);
            }
        }

        let clusters: Vec<Cluster> = label_groups
            .into_iter()
            .enumerate()
            .map(|(i, (label, node_ids))| {
                let mut cluster = Cluster::new(i as u64, node_ids);
                cluster.set_metadata("label".to_string(), PropertyValue::String(label));
                cluster
            })
            .collect();

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

    /// Property-based grouping
    fn property_based_grouping(
        &self,
        graph: &Graph,
        property_key: &str,
        nodes: &[Node],
    ) -> Result<ClusteringResult> {
        let mut property_groups: HashMap<String, Vec<NodeId>> = HashMap::new();

        for node in nodes {
            let group_key = if let Some(value) = node.get_property(property_key) {
                match value {
                    PropertyValue::String(s) => s.clone(),
                    PropertyValue::Int64(i) => i.to_string(),
                    PropertyValue::Float64(f) => f.to_string(),
                    PropertyValue::Bool(b) => b.to_string(),
                    _ => "unknown".to_string(),
                }
            } else {
                "null".to_string()
            };

            property_groups.entry(group_key).or_default().push(node.id);
        }

        let clusters: Vec<Cluster> = property_groups
            .into_iter()
            .enumerate()
            .map(|(i, (value, node_ids))| {
                let mut cluster = Cluster::new(i as u64, node_ids);
                cluster.set_metadata(property_key.to_string(), PropertyValue::String(value));
                cluster
            })
            .collect();

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

    /// Community detection using a simplified approach
    fn community_detection(&self, graph: &Graph, nodes: &[Node]) -> Result<ClusteringResult> {
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
    fn dbscan_clustering(
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
    fn get_neighbors(&self, features: &[Vec<f64>], point_idx: usize, eps: f64) -> Vec<usize> {
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

    /// Calculate clustering quality metrics
    fn calculate_metrics(
        &self,
        _graph: &Graph,
        clusters: &[Cluster],
        features: &[Vec<f64>],
        nodes: &[Node],
    ) -> Result<ClusteringMetrics> {
        if clusters.is_empty() {
            return Ok(ClusteringMetrics::default());
        }

        // Calculate silhouette score
        let silhouette_score = self.calculate_silhouette_score(clusters, features, nodes)?;

        // Calculate WCSS and BCSS
        let (wcss, bcss) = self.calculate_wcss_bcss(clusters, features, nodes)?;

        // Calculate Calinski-Harabasz index
        let calinski_harabasz = if wcss > 0.0 {
            (bcss / (clusters.len() - 1) as f64) / (wcss / (nodes.len() - clusters.len()) as f64)
        } else {
            0.0
        };

        // Calculate Davies-Bouldin index
        let davies_bouldin = self.calculate_davies_bouldin_index(clusters, features, nodes)?;

        Ok(ClusteringMetrics {
            silhouette_score,
            wcss,
            bcss,
            calinski_harabasz,
            davies_bouldin,
        })
    }

    /// Calculate silhouette score
    fn calculate_silhouette_score(
        &self,
        clusters: &[Cluster],
        features: &[Vec<f64>],
        nodes: &[Node],
    ) -> Result<f64> {
        let mut total_score = 0.0;
        let mut total_points = 0;

        for cluster in clusters {
            for &node_id in &cluster.nodes {
                if let Some(node_idx) = nodes.iter().position(|n| n.id == node_id) {
                    let node_features = &features[node_idx];

                    // Calculate average distance to other points in same cluster
                    let mut intra_cluster_dist = 0.0;
                    let mut intra_count = 0;

                    for &other_node_id in &cluster.nodes {
                        if other_node_id != node_id {
                            if let Some(other_idx) =
                                nodes.iter().position(|n| n.id == other_node_id)
                            {
                                intra_cluster_dist +=
                                    self.calculate_distance(node_features, &features[other_idx]);
                                intra_count += 1;
                            }
                        }
                    }

                    let a = if intra_count > 0 {
                        intra_cluster_dist / intra_count as f64
                    } else {
                        0.0
                    };

                    // Calculate average distance to nearest other cluster
                    let mut min_inter_cluster_dist = f64::INFINITY;

                    for other_cluster in clusters {
                        if other_cluster.id != cluster.id {
                            let mut inter_cluster_dist = 0.0;
                            let mut inter_count = 0;

                            for &other_node_id in &other_cluster.nodes {
                                if let Some(other_idx) =
                                    nodes.iter().position(|n| n.id == other_node_id)
                                {
                                    inter_cluster_dist += self
                                        .calculate_distance(node_features, &features[other_idx]);
                                    inter_count += 1;
                                }
                            }

                            let b = if inter_count > 0 {
                                inter_cluster_dist / inter_count as f64
                            } else {
                                0.0
                            };
                            min_inter_cluster_dist = min_inter_cluster_dist.min(b);
                        }
                    }

                    let b = if min_inter_cluster_dist == f64::INFINITY {
                        0.0
                    } else {
                        min_inter_cluster_dist
                    };

                    // Calculate silhouette score for this point
                    let max_ab = a.max(b);
                    let silhouette = if max_ab > 0.0 { (b - a) / max_ab } else { 0.0 };

                    total_score += silhouette;
                    total_points += 1;
                }
            }
        }

        Ok(if total_points > 0 {
            total_score / total_points as f64
        } else {
            0.0
        })
    }

    /// Calculate within-cluster and between-cluster sum of squares
    fn calculate_wcss_bcss(
        &self,
        clusters: &[Cluster],
        features: &[Vec<f64>],
        nodes: &[Node],
    ) -> Result<(f64, f64)> {
        if features.is_empty() {
            return Ok((0.0, 0.0));
        }

        let feature_dim = features[0].len();
        let mut global_centroid = vec![0.0; feature_dim];
        let mut total_points = 0;

        // Calculate global centroid
        for feature in features {
            for (i, value) in feature.iter().enumerate() {
                if i < global_centroid.len() {
                    global_centroid[i] += value;
                }
            }
            total_points += 1;
        }

        if total_points > 0 {
            for value in &mut global_centroid {
                *value /= total_points as f64;
            }
        }

        // Calculate WCSS
        let mut wcss = 0.0;
        for cluster in clusters {
            if let Some(centroid) = &cluster.centroid {
                for &node_id in &cluster.nodes {
                    if let Some(node_idx) = nodes.iter().position(|n| n.id == node_id) {
                        let distance = self.calculate_distance(&features[node_idx], centroid);
                        wcss += distance.powi(2);
                    }
                }
            }
        }

        // Calculate BCSS
        let mut bcss = 0.0;
        for cluster in clusters {
            if let Some(centroid) = &cluster.centroid {
                let distance = self.calculate_distance(centroid, &global_centroid);
                bcss += distance.powi(2) * cluster.size() as f64;
            }
        }

        Ok((wcss, bcss))
    }

    /// Calculate Davies-Bouldin index
    fn calculate_davies_bouldin_index(
        &self,
        clusters: &[Cluster],
        features: &[Vec<f64>],
        nodes: &[Node],
    ) -> Result<f64> {
        if clusters.len() <= 1 {
            return Ok(0.0);
        }

        let mut total_db = 0.0;

        for cluster_i in clusters {
            let mut max_ratio: f64 = 0.0;

            for cluster_j in clusters {
                if cluster_i.id != cluster_j.id {
                    let s_i = self.calculate_cluster_dispersion(cluster_i, features, nodes)?;
                    let s_j = self.calculate_cluster_dispersion(cluster_j, features, nodes)?;
                    let m_ij = self.calculate_cluster_distance_simple(cluster_i, cluster_j)?;

                    let ratio = if m_ij > 0.0 { (s_i + s_j) / m_ij } else { 0.0 };
                    max_ratio = max_ratio.max(ratio);
                }
            }

            total_db += max_ratio;
        }

        Ok(total_db / clusters.len() as f64)
    }

    /// Calculate cluster dispersion
    fn calculate_cluster_dispersion(
        &self,
        cluster: &Cluster,
        features: &[Vec<f64>],
        nodes: &[Node],
    ) -> Result<f64> {
        if cluster.is_empty() {
            return Ok(0.0);
        }

        let mut total_distance = 0.0;
        let mut count = 0;

        if let Some(centroid) = &cluster.centroid {
            for &node_id in &cluster.nodes {
                if let Some(node_idx) = nodes.iter().position(|n| n.id == node_id) {
                    let distance = self.calculate_distance(&features[node_idx], centroid);
                    total_distance += distance.powi(2);
                    count += 1;
                }
            }
        }

        Ok(if count > 0 {
            (total_distance / count as f64).sqrt()
        } else {
            0.0
        })
    }

    /// Calculate simple distance between cluster centroids
    fn calculate_cluster_distance_simple(
        &self,
        cluster1: &Cluster,
        cluster2: &Cluster,
    ) -> Result<f64> {
        if let (Some(centroid1), Some(centroid2)) = (&cluster1.centroid, &cluster2.centroid) {
            Ok(self.calculate_distance(centroid1, centroid2))
        } else {
            Ok(0.0)
        }
    }
}

/// Simple random number generator for reproducible results
struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn gen_range(&mut self, range: std::ops::Range<usize>) -> usize {
        self.state = self.state.wrapping_mul(1103515245).wrapping_add(12345);
        let normalized = (self.state as f64) / (u64::MAX as f64);
        let range_size = range.end - range.start;
        range.start + (normalized * range_size as f64) as usize
    }

    fn gen_f64(&mut self) -> f64 {
        self.state = self.state.wrapping_mul(1103515245).wrapping_add(12345);
        (self.state as f64) / (u64::MAX as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_graph() -> Graph {
        let mut graph = Graph::new();

        // Create test nodes with different labels and properties
        let person1 = graph
            .create_node(vec!["Person".to_string(), "Employee".to_string()])
            .unwrap();
        let person2 = graph
            .create_node(vec!["Person".to_string(), "Manager".to_string()])
            .unwrap();
        let person3 = graph
            .create_node(vec!["Person".to_string(), "Employee".to_string()])
            .unwrap();
        let company1 = graph.create_node(vec!["Company".to_string()]).unwrap();
        let company2 = graph.create_node(vec!["Company".to_string()]).unwrap();

        // Add properties
        let mut node1 = graph.get_node_mut(person1).unwrap().unwrap().clone();
        node1.set_property("age".to_string(), PropertyValue::Int64(25));
        node1.set_property(
            "department".to_string(),
            PropertyValue::String("Engineering".to_string()),
        );
        graph.update_node(node1).unwrap();

        let mut node2 = graph.get_node_mut(person2).unwrap().unwrap().clone();
        node2.set_property("age".to_string(), PropertyValue::Int64(35));
        node2.set_property(
            "department".to_string(),
            PropertyValue::String("Management".to_string()),
        );
        graph.update_node(node2).unwrap();

        let mut node3 = graph.get_node_mut(person3).unwrap().unwrap().clone();
        node3.set_property("age".to_string(), PropertyValue::Int64(28));
        node3.set_property(
            "department".to_string(),
            PropertyValue::String("Engineering".to_string()),
        );
        graph.update_node(node3).unwrap();

        let mut comp1 = graph.get_node_mut(company1).unwrap().unwrap().clone();
        comp1.set_property(
            "industry".to_string(),
            PropertyValue::String("Technology".to_string()),
        );
        graph.update_node(comp1).unwrap();

        let mut comp2 = graph.get_node_mut(company2).unwrap().unwrap().clone();
        comp2.set_property(
            "industry".to_string(),
            PropertyValue::String("Finance".to_string()),
        );
        graph.update_node(comp2).unwrap();

        graph
    }

    #[test]
    fn test_label_based_grouping() {
        let graph = create_test_graph();
        let config = ClusteringConfig {
            algorithm: ClusteringAlgorithm::LabelBased,
            feature_strategy: FeatureStrategy::LabelBased,
            distance_metric: DistanceMetric::Euclidean,
            random_seed: Some(42),
        };

        let engine = ClusteringEngine::new(config);
        let result = engine.cluster(&graph).unwrap();

        assert!(!result.clusters.is_empty());
        assert!(result.converged);
        assert_eq!(result.algorithm, ClusteringAlgorithm::LabelBased);
    }

    #[test]
    fn test_property_based_grouping() {
        let graph = create_test_graph();
        let config = ClusteringConfig {
            algorithm: ClusteringAlgorithm::PropertyBased {
                property_key: "department".to_string(),
            },
            feature_strategy: FeatureStrategy::PropertyBased {
                property_keys: vec!["department".to_string()],
            },
            distance_metric: DistanceMetric::Euclidean,
            random_seed: Some(42),
        };

        let engine = ClusteringEngine::new(config);
        let result = engine.cluster(&graph).unwrap();

        assert!(!result.clusters.is_empty());
        assert!(result.converged);
    }

    #[test]
    fn test_kmeans_clustering() {
        let graph = create_test_graph();
        let config = ClusteringConfig {
            algorithm: ClusteringAlgorithm::KMeans {
                k: 2,
                max_iterations: 10,
            },
            feature_strategy: FeatureStrategy::Structural,
            distance_metric: DistanceMetric::Euclidean,
            random_seed: Some(42),
        };

        let engine = ClusteringEngine::new(config);
        let result = engine.cluster(&graph).unwrap();

        assert!(!result.clusters.is_empty());
        assert!(result.clusters.len() <= 2);
    }

    #[test]
    fn test_cluster_creation() {
        let cluster = Cluster::new(0, vec![NodeId::new(1), NodeId::new(2)]);
        assert_eq!(cluster.id, 0);
        assert_eq!(cluster.size(), 2);
        assert!(!cluster.is_empty());

        let empty_cluster = Cluster::new(1, vec![]);
        assert!(empty_cluster.is_empty());
    }

    #[test]
    fn test_cluster_operations() {
        let mut cluster = Cluster::new(0, vec![NodeId::new(1)]);

        cluster.add_node(NodeId::new(2));
        assert_eq!(cluster.size(), 2);

        cluster.remove_node(NodeId::new(1));
        assert_eq!(cluster.size(), 1);

        cluster.set_metadata(
            "test".to_string(),
            PropertyValue::String("value".to_string()),
        );
        assert!(cluster.get_metadata("test").is_some());
    }

    #[test]
    fn test_distance_calculations() {
        let config = ClusteringConfig::default();
        let engine = ClusteringEngine::new(config);

        let features1 = vec![1.0, 2.0, 3.0];
        let features2 = vec![4.0, 5.0, 6.0];

        let euclidean = engine.calculate_distance(&features1, &features2);
        assert!((euclidean - 5.196).abs() < 0.01); // sqrt(3^2 + 3^2 + 3^2)
    }

    #[test]
    fn test_cluster_with_centroid() {
        let nodes = vec![NodeId::new(1), NodeId::new(2)];
        let centroid = vec![1.5, 2.5, 3.5];
        let cluster = Cluster::with_centroid(0, nodes, centroid.clone());

        assert_eq!(cluster.id, 0);
        assert_eq!(cluster.size(), 2);
        assert_eq!(cluster.centroid, Some(centroid));
    }

    #[test]
    fn test_cluster_metadata_operations() {
        let mut cluster = Cluster::new(0, vec![NodeId::new(1)]);
        
        // Test setting metadata
        cluster.set_metadata("key1".to_string(), PropertyValue::String("value1".to_string()));
        cluster.set_metadata("key2".to_string(), PropertyValue::Int64(42));
        
        // Test getting metadata
        assert_eq!(
            cluster.get_metadata("key1"),
            Some(&PropertyValue::String("value1".to_string()))
        );
        assert_eq!(
            cluster.get_metadata("key2"),
            Some(&PropertyValue::Int64(42))
        );
        assert_eq!(cluster.get_metadata("nonexistent"), None);
        
        // Test removing metadata by setting to None
        cluster.set_metadata("key1".to_string(), PropertyValue::Null);
        assert_eq!(cluster.get_metadata("key1"), Some(&PropertyValue::Null));
        assert_eq!(cluster.get_metadata("key2"), Some(&PropertyValue::Int64(42)));
    }

    #[test]
    fn test_cluster_contains_node() {
        let cluster = Cluster::new(0, vec![NodeId::new(1), NodeId::new(2), NodeId::new(3)]);
        
        assert!(cluster.nodes.contains(&NodeId::new(1)));
        assert!(cluster.nodes.contains(&NodeId::new(2)));
        assert!(cluster.nodes.contains(&NodeId::new(3)));
        assert!(!cluster.nodes.contains(&NodeId::new(4)));
    }

    #[test]
    fn test_cluster_clear() {
        let mut cluster = Cluster::new(0, vec![NodeId::new(1), NodeId::new(2)]);
        cluster.set_metadata("test".to_string(), PropertyValue::String("value".to_string()));
        
        assert_eq!(cluster.size(), 2);
        assert!(!cluster.is_empty());
        
        cluster.nodes.clear();
        cluster.metadata.clear();
        
        assert_eq!(cluster.size(), 0);
        assert!(cluster.is_empty());
        assert!(cluster.get_metadata("test").is_none());
    }

    #[test]
    fn test_clustering_config_default() {
        let config = ClusteringConfig::default();
        assert!(matches!(config.algorithm, ClusteringAlgorithm::KMeans { k: 3, max_iterations: 100 }));
        assert!(matches!(config.feature_strategy, FeatureStrategy::LabelBased));
        assert!(matches!(config.distance_metric, DistanceMetric::Euclidean));
        assert_eq!(config.random_seed, None);
    }

    #[test]
    fn test_clustering_config_creation() {
        let config = ClusteringConfig {
            algorithm: ClusteringAlgorithm::LabelBased,
            feature_strategy: FeatureStrategy::Structural,
            distance_metric: DistanceMetric::Manhattan,
            random_seed: Some(42),
        };
        
        assert!(matches!(config.algorithm, ClusteringAlgorithm::LabelBased));
        assert!(matches!(config.feature_strategy, FeatureStrategy::Structural));
        assert!(matches!(config.distance_metric, DistanceMetric::Manhattan));
        assert_eq!(config.random_seed, Some(42));
    }

    #[test]
    fn test_clustering_engine_new() {
        let config = ClusteringConfig::default();
        let engine = ClusteringEngine::new(config);
        assert!(engine.config.random_seed.is_none());
    }

    #[test]
    fn test_distance_metrics() {
        let config_euclidean = ClusteringConfig {
            algorithm: ClusteringAlgorithm::LabelBased,
            feature_strategy: FeatureStrategy::LabelBased,
            distance_metric: DistanceMetric::Euclidean,
            random_seed: None,
        };
        let engine_euclidean = ClusteringEngine::new(config_euclidean);

        let config_manhattan = ClusteringConfig {
            algorithm: ClusteringAlgorithm::LabelBased,
            feature_strategy: FeatureStrategy::LabelBased,
            distance_metric: DistanceMetric::Manhattan,
            random_seed: None,
        };
        let engine_manhattan = ClusteringEngine::new(config_manhattan);

        let config_cosine = ClusteringConfig {
            algorithm: ClusteringAlgorithm::LabelBased,
            feature_strategy: FeatureStrategy::LabelBased,
            distance_metric: DistanceMetric::Cosine,
            random_seed: None,
        };
        let engine_cosine = ClusteringEngine::new(config_cosine);

        let features1 = vec![1.0, 0.0];
        let features2 = vec![0.0, 1.0];

        let euclidean = engine_euclidean.calculate_distance(&features1, &features2);
        let manhattan = engine_manhattan.calculate_distance(&features1, &features2);
        let cosine = engine_cosine.calculate_distance(&features1, &features2);

        assert!((euclidean - 1.414).abs() < 0.01); // sqrt(2)
        assert!((manhattan - 2.0).abs() < 0.01); // 1 + 1
        assert!((cosine - 1.0).abs() < 0.01); // 1 - 0 = 1
    }

    #[test]
    fn test_clustering_result_creation() {
        let clusters = vec![
            Cluster::new(0, vec![NodeId::new(1)]),
            Cluster::new(1, vec![NodeId::new(2)]),
        ];
        let mut metrics = ClusteringMetrics::default();
        metrics.silhouette_score = 0.8;
        let result = ClusteringResult {
            clusters,
            algorithm: ClusteringAlgorithm::LabelBased,
            converged: true,
            iterations: 5,
            metrics,
        };

        assert_eq!(result.clusters.len(), 2);
        assert!(result.converged);
        assert_eq!(result.iterations, 5);
        assert_eq!(result.metrics.silhouette_score, 0.8);
    }

    #[test]
    fn test_empty_graph_clustering() {
        let graph = Graph::new();
        let config = ClusteringConfig::default();
        let engine = ClusteringEngine::new(config);
        
        let result = engine.cluster(&graph).unwrap();
        assert!(result.clusters.is_empty());
        assert!(result.converged);
    }

    #[test]
    fn test_single_node_clustering() {
        let mut graph = Graph::new();
        let _node = graph.create_node(vec!["Person".to_string()]).unwrap();
        
        let config = ClusteringConfig::default();
        let engine = ClusteringEngine::new(config);
        
        let result = engine.cluster(&graph).unwrap();
        assert!(!result.clusters.is_empty());
        assert!(result.converged);
    }

    #[test]
    fn test_dbscan_clustering() {
        let graph = create_test_graph();
        let config = ClusteringConfig {
            algorithm: ClusteringAlgorithm::DBSCAN { eps: 0.5, min_points: 2 },
            feature_strategy: FeatureStrategy::Structural,
            distance_metric: DistanceMetric::Euclidean,
            random_seed: Some(42),
        };

        let engine = ClusteringEngine::new(config);
        let result = engine.cluster(&graph).unwrap();

        assert!(!result.clusters.is_empty());
        assert!(result.converged);
    }

    #[test]
    fn test_hierarchical_clustering() {
        let graph = create_test_graph();
        let config = ClusteringConfig {
            algorithm: ClusteringAlgorithm::Hierarchical { linkage: LinkageType::Single },
            feature_strategy: FeatureStrategy::Structural,
            distance_metric: DistanceMetric::Euclidean,
            random_seed: Some(42),
        };

        let engine = ClusteringEngine::new(config);
        let result = engine.cluster(&graph).unwrap();

        assert!(!result.clusters.is_empty());
        assert!(result.converged);
    }

    #[test]
    fn test_community_detection_clustering() {
        let graph = create_test_graph();
        let config = ClusteringConfig {
            algorithm: ClusteringAlgorithm::CommunityDetection,
            feature_strategy: FeatureStrategy::Structural,
            distance_metric: DistanceMetric::Euclidean,
            random_seed: Some(42),
        };

        let engine = ClusteringEngine::new(config);
        let result = engine.cluster(&graph).unwrap();

        assert!(!result.clusters.is_empty());
        assert!(result.converged);
    }

    #[test]
    fn test_structural_feature_strategy() {
        let graph = create_test_graph();
        let config = ClusteringConfig {
            algorithm: ClusteringAlgorithm::KMeans { k: 2, max_iterations: 10 },
            feature_strategy: FeatureStrategy::Structural,
            distance_metric: DistanceMetric::Euclidean,
            random_seed: Some(42),
        };

        let engine = ClusteringEngine::new(config);
        let result = engine.cluster(&graph).unwrap();

        assert!(!result.clusters.is_empty());
        assert!(result.converged);
    }

    #[test]
    fn test_combined_feature_strategy() {
        let graph = create_test_graph();
        let config = ClusteringConfig {
            algorithm: ClusteringAlgorithm::KMeans { k: 2, max_iterations: 10 },
            feature_strategy: FeatureStrategy::Combined {
                strategies: vec![
                    FeatureStrategy::LabelBased,
                    FeatureStrategy::PropertyBased { property_keys: vec!["age".to_string()] },
                    FeatureStrategy::Structural,
                ],
            },
            distance_metric: DistanceMetric::Euclidean,
            random_seed: Some(42),
        };

        let engine = ClusteringEngine::new(config);
        let result = engine.cluster(&graph).unwrap();

        assert!(!result.clusters.is_empty());
        assert!(result.converged);
    }

    #[test]
    fn test_jaccard_distance() {
        let config = ClusteringConfig {
            algorithm: ClusteringAlgorithm::LabelBased,
            feature_strategy: FeatureStrategy::LabelBased,
            distance_metric: DistanceMetric::Jaccard,
            random_seed: None,
        };
        let engine = ClusteringEngine::new(config);

        let features1 = vec![1.0, 0.0, 1.0, 0.0];
        let features2 = vec![0.0, 1.0, 1.0, 0.0];

        let jaccard = engine.calculate_distance(&features1, &features2);
        // Jaccard distance = 1 - Jaccard similarity
        // Jaccard similarity = intersection / union = 1 / 3 = 0.333...
        // Jaccard distance = 1 - 0.333... = 0.666...
        assert!((jaccard - 0.666).abs() < 0.01);
    }

    #[test]
    fn test_hamming_distance() {
        let config = ClusteringConfig {
            algorithm: ClusteringAlgorithm::LabelBased,
            feature_strategy: FeatureStrategy::LabelBased,
            distance_metric: DistanceMetric::Hamming,
            random_seed: None,
        };
        let engine = ClusteringEngine::new(config);

        let features1 = vec![1.0, 0.0, 1.0, 0.0];
        let features2 = vec![0.0, 1.0, 1.0, 0.0];

        let hamming = engine.calculate_distance(&features1, &features2);
        assert!((hamming - 2.0).abs() < 0.01); // 2 different positions
    }
}
