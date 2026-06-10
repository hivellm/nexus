//! Node clustering and grouping algorithms
//!
//! This module provides various clustering algorithms and grouping strategies
//! for organizing nodes in the graph based on their properties, labels, and
//! structural relationships.

mod community;
mod engine;
mod grouping;
mod kmeans;
mod metrics;
mod rng;
mod types;

#[cfg(test)]
mod tests;

// Re-export everything that was previously reachable at crate::graph::clustering::*
pub use engine::ClusteringEngine;
pub use types::{
    Cluster, ClusteringAlgorithm, ClusteringConfig, ClusteringMetrics, ClusteringResult,
    DistanceMetric, FeatureStrategy, LinkageType,
};
