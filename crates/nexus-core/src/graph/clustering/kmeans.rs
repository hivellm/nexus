//! K-means clustering implementation.

use crate::error::Result;
use crate::graph::simple::{Graph, Node};

use super::engine::ClusteringEngine;
use super::rng::SimpleRng;
use super::types::{Cluster, ClusteringMetrics, ClusteringResult};

impl ClusteringEngine {
    /// K-means clustering implementation
    pub(super) fn kmeans_clustering(
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
    pub(super) fn initialize_centroids(
        &self,
        features: &[Vec<f64>],
        k: usize,
    ) -> Result<Vec<Vec<f64>>> {
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
}
