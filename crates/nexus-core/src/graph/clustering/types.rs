//! Shared types for the clustering subsystem.

use crate::graph::simple::{NodeId, PropertyValue};
use std::collections::HashMap;

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
