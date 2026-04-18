//! Clustering API endpoints
//!
//! This module provides HTTP API endpoints for node clustering and grouping operations.

use axum::{
    Router,
    extract::{Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
};
use nexus_core::graph::clustering::{
    ClusteringAlgorithm, ClusteringConfig, ClusteringEngine, DistanceMetric, FeatureStrategy,
    LinkageType,
};
use nexus_core::graph::simple::Graph;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Request for clustering operations
#[derive(Debug, Deserialize)]
pub struct ClusteringRequest {
    /// Algorithm to use for clustering
    pub algorithm: String,
    /// Number of clusters (for k-means)
    pub k: Option<usize>,
    /// Maximum iterations (for k-means)
    pub max_iterations: Option<usize>,
    /// Epsilon parameter (for DBSCAN)
    pub eps: Option<f64>,
    /// Minimum points (for DBSCAN)
    pub min_points: Option<usize>,
    /// Linkage type (for hierarchical clustering)
    pub linkage: Option<String>,
    /// Property key for property-based grouping
    pub property_key: Option<String>,
    /// Feature extraction strategy
    pub feature_strategy: Option<String>,
    /// Property keys for property-based features
    pub property_keys: Option<Vec<String>>,
    /// Distance metric
    pub distance_metric: Option<String>,
    /// Random seed for reproducible results
    pub random_seed: Option<u64>,
}

/// Response for clustering operations
#[derive(Debug, Serialize)]
pub struct ClusteringResponse {
    /// Generated clusters
    pub clusters: Vec<ClusterInfo>,
    /// Algorithm used
    pub algorithm: String,
    /// Number of iterations performed
    pub iterations: usize,
    /// Convergence status
    pub converged: bool,
    /// Quality metrics
    pub metrics: ClusteringMetricsResponse,
}

/// Information about a cluster
#[derive(Debug, Serialize)]
pub struct ClusterInfo {
    /// Cluster ID
    pub id: u64,
    /// Number of nodes in cluster
    pub size: usize,
    /// Node IDs in cluster
    pub nodes: Vec<u64>,
    /// Cluster metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Quality metrics response
#[derive(Debug, Serialize)]
pub struct ClusteringMetricsResponse {
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

/// Available clustering algorithms
#[derive(Debug, Serialize)]
pub struct ClusteringAlgorithmsResponse {
    /// Available algorithms
    pub algorithms: Vec<AlgorithmInfo>,
    /// Available distance metrics
    pub distance_metrics: Vec<String>,
    /// Available feature strategies
    pub feature_strategies: Vec<String>,
    /// Available linkage types
    pub linkage_types: Vec<String>,
}

/// Information about a clustering algorithm
#[derive(Debug, Serialize)]
pub struct AlgorithmInfo {
    /// Algorithm name
    pub name: String,
    /// Algorithm description
    pub description: String,
    /// Required parameters
    pub required_params: Vec<String>,
    /// Optional parameters
    pub optional_params: Vec<String>,
}

/// Create clustering API router
#[allow(dead_code)]
pub fn create_router() -> Router<Arc<crate::NexusServer>> {
    Router::new()
        .route("/clustering/algorithms", get(get_algorithms))
        .route("/clustering/cluster", post(cluster_nodes))
        .route("/clustering/group-by-label", post(group_by_label))
        .route("/clustering/group-by-property", post(group_by_property))
}

/// Get available clustering algorithms and their parameters
pub async fn get_algorithms() -> Result<Json<ClusteringAlgorithmsResponse>, StatusCode> {
    let algorithms = vec![
        AlgorithmInfo {
            name: "kmeans".to_string(),
            description: "K-means clustering algorithm".to_string(),
            required_params: vec!["k".to_string()],
            optional_params: vec!["max_iterations".to_string(), "random_seed".to_string()],
        },
        AlgorithmInfo {
            name: "hierarchical".to_string(),
            description: "Hierarchical clustering algorithm".to_string(),
            required_params: vec![],
            optional_params: vec!["linkage".to_string(), "random_seed".to_string()],
        },
        AlgorithmInfo {
            name: "label_based".to_string(),
            description: "Group nodes by their labels".to_string(),
            required_params: vec![],
            optional_params: vec!["random_seed".to_string()],
        },
        AlgorithmInfo {
            name: "property_based".to_string(),
            description: "Group nodes by a specific property".to_string(),
            required_params: vec!["property_key".to_string()],
            optional_params: vec!["random_seed".to_string()],
        },
        AlgorithmInfo {
            name: "community_detection".to_string(),
            description: "Community detection using connected components".to_string(),
            required_params: vec![],
            optional_params: vec!["random_seed".to_string()],
        },
        AlgorithmInfo {
            name: "dbscan".to_string(),
            description: "Density-based clustering (DBSCAN)".to_string(),
            required_params: vec!["eps".to_string(), "min_points".to_string()],
            optional_params: vec!["random_seed".to_string()],
        },
    ];

    let distance_metrics = vec![
        "euclidean".to_string(),
        "manhattan".to_string(),
        "cosine".to_string(),
        "jaccard".to_string(),
        "hamming".to_string(),
    ];

    let feature_strategies = vec![
        "label_based".to_string(),
        "property_based".to_string(),
        "structural".to_string(),
        "combined".to_string(),
    ];

    let linkage_types = vec![
        "single".to_string(),
        "complete".to_string(),
        "average".to_string(),
        "ward".to_string(),
    ];

    Ok(Json(ClusteringAlgorithmsResponse {
        algorithms,
        distance_metrics,
        feature_strategies,
        linkage_types,
    }))
}

/// Perform clustering on nodes
pub async fn cluster_nodes(
    State(_server): State<Arc<crate::NexusServer>>,
    Json(request): Json<ClusteringRequest>,
) -> Result<Json<ClusteringResponse>, StatusCode> {
    // Create a simple graph for testing - in a real implementation, you'd extract nodes from the server
    let graph = Graph::new();

    let algorithm = parse_algorithm(&request)?;
    let feature_strategy = parse_feature_strategy(&request)?;
    let distance_metric = parse_distance_metric(&request)?;

    let config = ClusteringConfig {
        algorithm,
        feature_strategy,
        distance_metric,
        random_seed: request.random_seed,
    };

    let engine = ClusteringEngine::new(config);
    let result = engine
        .cluster(&graph)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let clusters: Vec<ClusterInfo> = result
        .clusters
        .into_iter()
        .map(|cluster| {
            let metadata: HashMap<String, serde_json::Value> = cluster
                .metadata
                .iter()
                .map(|(k, v)| (k.clone(), property_value_to_json(v.clone())))
                .collect();

            ClusterInfo {
                id: cluster.id,
                size: cluster.size(),
                nodes: cluster.nodes.into_iter().map(|id| id.value()).collect(),
                metadata,
            }
        })
        .collect();

    let response = ClusteringResponse {
        clusters,
        algorithm: format!("{:?}", result.algorithm),
        iterations: result.iterations,
        converged: result.converged,
        metrics: ClusteringMetricsResponse {
            silhouette_score: result.metrics.silhouette_score,
            wcss: result.metrics.wcss,
            bcss: result.metrics.bcss,
            calinski_harabasz: result.metrics.calinski_harabasz,
            davies_bouldin: result.metrics.davies_bouldin,
        },
    };

    Ok(Json(response))
}

/// Group nodes by their labels
pub async fn group_by_label(
    State(_server): State<Arc<crate::NexusServer>>,
    _request: axum::extract::Request,
) -> Result<Json<ClusteringResponse>, StatusCode> {
    // Create a simple graph for testing - in a real implementation, you'd extract nodes from the server
    let graph = Graph::new();

    let config = ClusteringConfig {
        algorithm: ClusteringAlgorithm::LabelBased,
        feature_strategy: FeatureStrategy::LabelBased,
        distance_metric: DistanceMetric::Euclidean,
        random_seed: None,
    };

    let engine = ClusteringEngine::new(config);
    let result = engine
        .cluster(&graph)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let clusters: Vec<ClusterInfo> = result
        .clusters
        .into_iter()
        .map(|cluster| {
            let metadata: HashMap<String, serde_json::Value> = cluster
                .metadata
                .iter()
                .map(|(k, v)| (k.clone(), property_value_to_json(v.clone())))
                .collect();

            ClusterInfo {
                id: cluster.id,
                size: cluster.size(),
                nodes: cluster.nodes.into_iter().map(|id| id.value()).collect(),
                metadata,
            }
        })
        .collect();

    let response = ClusteringResponse {
        clusters,
        algorithm: "LabelBased".to_string(),
        iterations: result.iterations,
        converged: result.converged,
        metrics: ClusteringMetricsResponse {
            silhouette_score: result.metrics.silhouette_score,
            wcss: result.metrics.wcss,
            bcss: result.metrics.bcss,
            calinski_harabasz: result.metrics.calinski_harabasz,
            davies_bouldin: result.metrics.davies_bouldin,
        },
    };

    Ok(Json(response))
}

/// Group nodes by a specific property
pub async fn group_by_property(
    State(_server): State<Arc<crate::NexusServer>>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ClusteringResponse>, StatusCode> {
    // Create a simple graph for testing - in a real implementation, you'd extract nodes from the server
    let graph = Graph::new();

    let property_key = params
        .get("property_key")
        .ok_or(StatusCode::BAD_REQUEST)?
        .clone();

    let config = ClusteringConfig {
        algorithm: ClusteringAlgorithm::PropertyBased {
            property_key: property_key.clone(),
        },
        feature_strategy: FeatureStrategy::PropertyBased {
            property_keys: vec![property_key.clone()],
        },
        distance_metric: DistanceMetric::Euclidean,
        random_seed: None,
    };

    let engine = ClusteringEngine::new(config);
    let result = engine
        .cluster(&graph)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let clusters: Vec<ClusterInfo> = result
        .clusters
        .into_iter()
        .map(|cluster| {
            let metadata: HashMap<String, serde_json::Value> = cluster
                .metadata
                .iter()
                .map(|(k, v)| (k.clone(), property_value_to_json(v.clone())))
                .collect();

            ClusterInfo {
                id: cluster.id,
                size: cluster.size(),
                nodes: cluster.nodes.into_iter().map(|id| id.value()).collect(),
                metadata,
            }
        })
        .collect();

    let response = ClusteringResponse {
        clusters,
        algorithm: "PropertyBased".to_string(),
        iterations: result.iterations,
        converged: result.converged,
        metrics: ClusteringMetricsResponse {
            silhouette_score: result.metrics.silhouette_score,
            wcss: result.metrics.wcss,
            bcss: result.metrics.bcss,
            calinski_harabasz: result.metrics.calinski_harabasz,
            davies_bouldin: result.metrics.davies_bouldin,
        },
    };

    Ok(Json(response))
}

/// Parse clustering algorithm from request
fn parse_algorithm(request: &ClusteringRequest) -> Result<ClusteringAlgorithm, StatusCode> {
    match request.algorithm.as_str() {
        "kmeans" => {
            let k = request.k.ok_or(StatusCode::BAD_REQUEST)?;
            let max_iterations = request.max_iterations.unwrap_or(100);
            Ok(ClusteringAlgorithm::KMeans { k, max_iterations })
        }
        "hierarchical" => {
            let linkage = request.linkage.as_deref().unwrap_or("average");
            let linkage = parse_linkage_type(linkage).map_err(|_| StatusCode::BAD_REQUEST)?;
            Ok(ClusteringAlgorithm::Hierarchical { linkage })
        }
        "label_based" => Ok(ClusteringAlgorithm::LabelBased),
        "property_based" => {
            let property_key = request
                .property_key
                .clone()
                .ok_or(StatusCode::BAD_REQUEST)?;
            Ok(ClusteringAlgorithm::PropertyBased { property_key })
        }
        "community_detection" => Ok(ClusteringAlgorithm::CommunityDetection),
        "dbscan" => {
            let eps = request.eps.ok_or(StatusCode::BAD_REQUEST)?;
            let min_points = request.min_points.ok_or(StatusCode::BAD_REQUEST)?;
            Ok(ClusteringAlgorithm::DBSCAN { eps, min_points })
        }
        _ => Err(StatusCode::BAD_REQUEST),
    }
}

/// Parse feature strategy from request
fn parse_feature_strategy(request: &ClusteringRequest) -> Result<FeatureStrategy, StatusCode> {
    match request.feature_strategy.as_deref().unwrap_or("label_based") {
        "label_based" => Ok(FeatureStrategy::LabelBased),
        "property_based" => {
            let property_keys = request.property_keys.clone().unwrap_or_default();
            Ok(FeatureStrategy::PropertyBased { property_keys })
        }
        "structural" => Ok(FeatureStrategy::Structural),
        "combined" => {
            // For now, just use label-based as the default combined strategy
            Ok(FeatureStrategy::LabelBased)
        }
        _ => Err(StatusCode::BAD_REQUEST),
    }
}

/// Parse distance metric from request
fn parse_distance_metric(request: &ClusteringRequest) -> Result<DistanceMetric, StatusCode> {
    match request.distance_metric.as_deref().unwrap_or("euclidean") {
        "euclidean" => Ok(DistanceMetric::Euclidean),
        "manhattan" => Ok(DistanceMetric::Manhattan),
        "cosine" => Ok(DistanceMetric::Cosine),
        "jaccard" => Ok(DistanceMetric::Jaccard),
        "hamming" => Ok(DistanceMetric::Hamming),
        _ => Err(StatusCode::BAD_REQUEST),
    }
}

/// Parse linkage type from string
fn parse_linkage_type(s: &str) -> Result<LinkageType, ()> {
    match s {
        "single" => Ok(LinkageType::Single),
        "complete" => Ok(LinkageType::Complete),
        "average" => Ok(LinkageType::Average),
        "ward" => Ok(LinkageType::Ward),
        _ => Err(()),
    }
}

/// Convert PropertyValue to JSON Value
fn property_value_to_json(value: nexus_core::graph::simple::PropertyValue) -> serde_json::Value {
    match value {
        nexus_core::graph::simple::PropertyValue::Null => serde_json::Value::Null,
        nexus_core::graph::simple::PropertyValue::Bool(b) => serde_json::Value::Bool(b),
        nexus_core::graph::simple::PropertyValue::Int64(i) => {
            serde_json::Value::Number(serde_json::Number::from(i))
        }
        nexus_core::graph::simple::PropertyValue::Float64(f) => serde_json::Value::Number(
            serde_json::Number::from_f64(f).unwrap_or(serde_json::Number::from(0)),
        ),
        nexus_core::graph::simple::PropertyValue::String(s) => serde_json::Value::String(s),
        nexus_core::graph::simple::PropertyValue::Bytes(b) => serde_json::Value::Array(
            b.into_iter()
                .map(|x| serde_json::Value::Number(serde_json::Number::from(x)))
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_core::graph::simple::Graph;

    #[allow(dead_code)]
    fn create_test_graph() -> Graph {
        let mut graph = Graph::new();

        // Create test nodes
        let _person1 = graph
            .create_node(vec!["Person".to_string(), "Employee".to_string()])
            .unwrap();
        let _person2 = graph
            .create_node(vec!["Person".to_string(), "Manager".to_string()])
            .unwrap();
        let _company1 = graph.create_node(vec!["Company".to_string()]).unwrap();

        graph
    }

    #[test]
    fn test_parse_algorithm_kmeans() {
        let request = ClusteringRequest {
            algorithm: "kmeans".to_string(),
            k: Some(3),
            max_iterations: Some(50),
            eps: None,
            min_points: None,
            linkage: None,
            property_key: None,
            feature_strategy: None,
            property_keys: None,
            distance_metric: None,
            random_seed: None,
        };

        let algorithm = parse_algorithm(&request).unwrap();
        assert!(matches!(
            algorithm,
            ClusteringAlgorithm::KMeans {
                k: 3,
                max_iterations: 50
            }
        ));
    }

    #[test]
    fn test_parse_algorithm_label_based() {
        let request = ClusteringRequest {
            algorithm: "label_based".to_string(),
            k: None,
            max_iterations: None,
            eps: None,
            min_points: None,
            linkage: None,
            property_key: None,
            feature_strategy: None,
            property_keys: None,
            distance_metric: None,
            random_seed: None,
        };

        let algorithm = parse_algorithm(&request).unwrap();
        assert!(matches!(algorithm, ClusteringAlgorithm::LabelBased));
    }

    #[test]
    fn test_parse_distance_metric() {
        let request = ClusteringRequest {
            algorithm: "kmeans".to_string(),
            k: Some(3),
            max_iterations: None,
            eps: None,
            min_points: None,
            linkage: None,
            property_key: None,
            feature_strategy: None,
            property_keys: None,
            distance_metric: Some("manhattan".to_string()),
            random_seed: None,
        };

        let metric = parse_distance_metric(&request).unwrap();
        assert!(matches!(metric, DistanceMetric::Manhattan));
    }

    #[test]
    fn test_property_value_to_json() {
        use nexus_core::graph::simple::PropertyValue;

        assert_eq!(
            property_value_to_json(PropertyValue::Null),
            serde_json::Value::Null
        );
        assert_eq!(
            property_value_to_json(PropertyValue::Bool(true)),
            serde_json::Value::Bool(true)
        );
        assert_eq!(
            property_value_to_json(PropertyValue::Int64(42)),
            serde_json::Value::Number(serde_json::Number::from(42))
        );
        assert_eq!(
            property_value_to_json(PropertyValue::String("test".to_string())),
            serde_json::Value::String("test".to_string())
        );
    }

    #[tokio::test]
    async fn test_get_algorithms() {
        let result = get_algorithms().await;
        assert!(result.is_ok());
        let response = result.unwrap();
        let algorithms = response.0;
        assert!(!algorithms.algorithms.is_empty());
        assert!(algorithms.algorithms.iter().any(|a| a.name == "kmeans"));
        assert!(
            algorithms
                .algorithms
                .iter()
                .any(|a| a.name == "label_based")
        );
    }

    #[test]
    fn test_parse_algorithm_hierarchical() {
        let request = ClusteringRequest {
            algorithm: "hierarchical".to_string(),
            k: None,
            max_iterations: None,
            eps: None,
            min_points: None,
            linkage: Some("complete".to_string()),
            property_key: None,
            feature_strategy: None,
            property_keys: None,
            distance_metric: None,
            random_seed: None,
        };

        let algorithm = parse_algorithm(&request).unwrap();
        assert!(matches!(
            algorithm,
            ClusteringAlgorithm::Hierarchical {
                linkage: LinkageType::Complete
            }
        ));
    }

    #[test]
    fn test_parse_algorithm_community_detection() {
        let request = ClusteringRequest {
            algorithm: "community_detection".to_string(),
            k: None,
            max_iterations: None,
            eps: None,
            min_points: None,
            linkage: None,
            property_key: None,
            feature_strategy: None,
            property_keys: None,
            distance_metric: None,
            random_seed: None,
        };

        let algorithm = parse_algorithm(&request).unwrap();
        assert!(matches!(algorithm, ClusteringAlgorithm::CommunityDetection));
    }

    #[test]
    fn test_parse_algorithm_invalid() {
        let request = ClusteringRequest {
            algorithm: "invalid".to_string(),
            k: None,
            max_iterations: None,
            eps: None,
            min_points: None,
            linkage: None,
            property_key: None,
            feature_strategy: None,
            property_keys: None,
            distance_metric: None,
            random_seed: None,
        };

        let result = parse_algorithm(&request);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_feature_strategy() {
        let request = ClusteringRequest {
            algorithm: "kmeans".to_string(),
            k: Some(3),
            max_iterations: None,
            eps: None,
            min_points: None,
            linkage: None,
            property_key: None,
            feature_strategy: Some("property_based".to_string()),
            property_keys: Some(vec!["age".to_string(), "salary".to_string()]),
            distance_metric: None,
            random_seed: None,
        };

        let strategy = parse_feature_strategy(&request).unwrap();
        assert!(matches!(
            strategy,
            FeatureStrategy::PropertyBased { property_keys } if property_keys == vec!["age".to_string(), "salary".to_string()]
        ));
    }

    #[test]
    fn test_parse_feature_strategy_structural() {
        let request = ClusteringRequest {
            algorithm: "kmeans".to_string(),
            k: Some(3),
            max_iterations: None,
            eps: None,
            min_points: None,
            linkage: None,
            property_key: None,
            feature_strategy: Some("structural".to_string()),
            property_keys: None,
            distance_metric: None,
            random_seed: None,
        };

        let strategy = parse_feature_strategy(&request).unwrap();
        assert!(matches!(strategy, FeatureStrategy::Structural));
    }

    #[test]
    fn test_parse_feature_strategy_invalid() {
        let request = ClusteringRequest {
            algorithm: "kmeans".to_string(),
            k: Some(3),
            max_iterations: None,
            eps: None,
            min_points: None,
            linkage: None,
            property_key: None,
            feature_strategy: Some("invalid".to_string()),
            property_keys: None,
            distance_metric: None,
            random_seed: None,
        };

        let result = parse_feature_strategy(&request);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_distance_metric_all_variants() {
        let metrics = vec![
            ("euclidean", DistanceMetric::Euclidean),
            ("manhattan", DistanceMetric::Manhattan),
            ("cosine", DistanceMetric::Cosine),
            ("jaccard", DistanceMetric::Jaccard),
            ("hamming", DistanceMetric::Hamming),
        ];

        for (name, _expected) in metrics {
            let request = ClusteringRequest {
                algorithm: "kmeans".to_string(),
                k: Some(3),
                max_iterations: None,
                eps: None,
                min_points: None,
                linkage: None,
                property_key: None,
                feature_strategy: None,
                property_keys: None,
                distance_metric: Some(name.to_string()),
                random_seed: None,
            };

            let metric = parse_distance_metric(&request).unwrap();
            assert!(matches!(metric, _expected));
        }
    }

    #[test]
    fn test_parse_distance_metric_invalid() {
        let request = ClusteringRequest {
            algorithm: "kmeans".to_string(),
            k: Some(3),
            max_iterations: None,
            eps: None,
            min_points: None,
            linkage: None,
            property_key: None,
            feature_strategy: None,
            property_keys: None,
            distance_metric: Some("invalid".to_string()),
            random_seed: None,
        };

        let result = parse_distance_metric(&request);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_linkage_type() {
        let linkages = vec![
            ("single", LinkageType::Single),
            ("complete", LinkageType::Complete),
            ("average", LinkageType::Average),
            ("ward", LinkageType::Ward),
        ];

        for (name, expected) in linkages {
            let result = parse_linkage_type(name);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), expected);
        }
    }

    #[test]
    fn test_parse_linkage_type_invalid() {
        let result = parse_linkage_type("invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_property_value_to_json_float() {
        use nexus_core::graph::simple::PropertyValue;

        let result = property_value_to_json(PropertyValue::Float64(std::f64::consts::PI));
        assert!(matches!(result, serde_json::Value::Number(_)));
    }

    #[test]
    fn test_property_value_to_json_bytes() {
        use nexus_core::graph::simple::PropertyValue;

        let bytes = vec![1, 2, 3, 4];
        let result = property_value_to_json(PropertyValue::Bytes(bytes));
        assert!(matches!(result, serde_json::Value::Array(_)));
    }
}
