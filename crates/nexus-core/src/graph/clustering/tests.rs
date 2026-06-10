//! Tests for the clustering module.
//! Declared via `#[cfg(test)] mod tests;` in mod.rs — this file IS the tests module.

use super::*;
use crate::graph::simple::{Graph, NodeId, PropertyValue};

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
    cluster.set_metadata(
        "key1".to_string(),
        PropertyValue::String("value1".to_string()),
    );
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
    assert_eq!(
        cluster.get_metadata("key2"),
        Some(&PropertyValue::Int64(42))
    );
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
    cluster.set_metadata(
        "test".to_string(),
        PropertyValue::String("value".to_string()),
    );

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
    assert!(matches!(
        config.algorithm,
        ClusteringAlgorithm::KMeans {
            k: 3,
            max_iterations: 100
        }
    ));
    assert!(matches!(
        config.feature_strategy,
        FeatureStrategy::LabelBased
    ));
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
    assert!(matches!(
        config.feature_strategy,
        FeatureStrategy::Structural
    ));
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
    let metrics = ClusteringMetrics {
        silhouette_score: 0.8,
        ..Default::default()
    };
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
        algorithm: ClusteringAlgorithm::DBSCAN {
            eps: 0.5,
            min_points: 2,
        },
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
        algorithm: ClusteringAlgorithm::Hierarchical {
            linkage: LinkageType::Single,
        },
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
    assert!(result.converged);
}

#[test]
fn test_combined_feature_strategy() {
    let graph = create_test_graph();
    let config = ClusteringConfig {
        algorithm: ClusteringAlgorithm::KMeans {
            k: 2,
            max_iterations: 10,
        },
        feature_strategy: FeatureStrategy::Combined {
            strategies: vec![
                FeatureStrategy::LabelBased,
                FeatureStrategy::PropertyBased {
                    property_keys: vec!["age".to_string()],
                },
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
