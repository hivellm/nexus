//! Clustering quality metrics: silhouette, WCSS/BCSS, Davies-Bouldin.

use crate::error::Result;
use crate::graph::simple::{Graph, Node};

use super::engine::ClusteringEngine;
use super::types::{Cluster, ClusteringMetrics};

impl ClusteringEngine {
    /// Calculate clustering quality metrics
    pub(super) fn calculate_metrics(
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
