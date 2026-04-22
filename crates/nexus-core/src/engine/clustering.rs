//! Engine-level clustering and graph-conversion methods.
//!
//! `convert_to_simple_graph` materialises the storage-backed graph
//! into the in-memory `graph::simple::Graph` that the clustering
//! algorithms consume. The public entry points (`cluster_nodes`,
//! `group_nodes_by_*`, `kmeans_cluster_nodes`, `detect_communities`)
//! preset a `ClusteringConfig` and forward to `cluster_nodes`, which
//! owns the only real work. Extracted from `engine/mod.rs` during the
//! split — public API unchanged, methods are still `Engine`'s via an
//! `impl Engine` block that cross-references the struct defined in
//! `engine/mod.rs`.

use super::Engine;
use crate::Result;
use crate::graph;
use crate::graph::clustering::{
    ClusteringAlgorithm, ClusteringConfig, ClusteringEngine, ClusteringResult, DistanceMetric,
    FeatureStrategy,
};

impl Engine {
    /// Perform node clustering on the graph.
    pub fn cluster_nodes(&mut self, config: ClusteringConfig) -> Result<ClusteringResult> {
        let simple_graph = self.convert_to_simple_graph()?;
        let engine = ClusteringEngine::new(config);
        engine.cluster(&simple_graph)
    }

    /// Convert the storage to a simple graph for clustering and analysis.
    ///
    /// Scans every node and relationship out of `RecordStore` and
    /// rebuilds them inside a fresh `graph::simple::Graph`. Properties
    /// are loaded but not yet projected onto the simple graph (tracked
    /// as a future property-integration follow-up).
    pub fn convert_to_simple_graph(&mut self) -> Result<graph::simple::Graph> {
        let mut simple_graph = graph::simple::Graph::new();

        for node_id in 0..self.storage.node_count() {
            if let Ok(Some(node_record)) = self.get_node(node_id) {
                let labels = self
                    .catalog
                    .get_labels_from_bitmap(node_record.label_bits)?;

                let simple_node_id = graph::simple::NodeId::new(node_id);
                let node = graph::simple::Node::new(simple_node_id, labels);

                if node_record.prop_ptr != 0 {
                    if let Ok(Some(_properties)) = self.storage.load_node_properties(node_id) {
                        // Properties loaded but not projected onto the
                        // simple graph yet — property integration is a
                        // follow-up.
                    }
                }

                simple_graph.update_node(node)?;
            }
        }

        for rel_id in 0..self.storage.relationship_count() {
            if let Ok(Some(rel_record)) = self.get_relationship(rel_id) {
                let rel_type = self
                    .catalog
                    .get_type_name(rel_record.type_id)
                    .unwrap_or_else(|_| Some("UNKNOWN".to_string()))
                    .unwrap_or_else(|| "UNKNOWN".to_string());

                if rel_record.prop_ptr != 0 {
                    if let Ok(Some(_properties)) = self.storage.load_relationship_properties(rel_id)
                    {
                        // Property integration follow-up, as above.
                    }
                }

                let source_id = graph::simple::NodeId::new(rel_record.src_id);
                let target_id = graph::simple::NodeId::new(rel_record.dst_id);

                simple_graph.create_edge(source_id, target_id, rel_type)?;
            }
        }

        Ok(simple_graph)
    }

    /// Perform label-based grouping of nodes.
    pub fn group_nodes_by_labels(&mut self) -> Result<ClusteringResult> {
        let config = ClusteringConfig {
            algorithm: ClusteringAlgorithm::LabelBased,
            feature_strategy: FeatureStrategy::LabelBased,
            distance_metric: DistanceMetric::Euclidean,
            random_seed: None,
        };
        self.cluster_nodes(config)
    }

    /// Perform property-based grouping of nodes.
    pub fn group_nodes_by_property(&mut self, property_key: &str) -> Result<ClusteringResult> {
        let config = ClusteringConfig {
            algorithm: ClusteringAlgorithm::PropertyBased {
                property_key: property_key.to_string(),
            },
            feature_strategy: FeatureStrategy::PropertyBased {
                property_keys: vec![property_key.to_string()],
            },
            distance_metric: DistanceMetric::Euclidean,
            random_seed: None,
        };
        self.cluster_nodes(config)
    }

    /// Perform K-means clustering on nodes.
    pub fn kmeans_cluster_nodes(
        &mut self,
        k: usize,
        max_iterations: usize,
    ) -> Result<ClusteringResult> {
        let config = ClusteringConfig {
            algorithm: ClusteringAlgorithm::KMeans { k, max_iterations },
            feature_strategy: FeatureStrategy::Structural,
            distance_metric: DistanceMetric::Euclidean,
            random_seed: Some(42),
        };
        self.cluster_nodes(config)
    }

    /// Perform community detection on nodes.
    pub fn detect_communities(&mut self) -> Result<ClusteringResult> {
        let config = ClusteringConfig {
            algorithm: ClusteringAlgorithm::CommunityDetection,
            feature_strategy: FeatureStrategy::Structural,
            distance_metric: DistanceMetric::Euclidean,
            random_seed: None,
        };
        self.cluster_nodes(config)
    }
}
