//! API Integration Tests for Nexus Server
//!
//! These tests verify the complete HTTP API functionality including:
//! - REST endpoints
//! - Server-Sent Events (SSE)
//! - Error handling
//! - Performance benchmarks
//! - MCP protocol integration

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    Router,
};
use hyper::body::to_bytes;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::RwLock;
use tower::ServiceExt;

use nexus_core::{
    catalog::Catalog,
    executor::Executor,
    index::{KnnIndex, LabelIndex},
    storage::RecordStore,
};
use nexus_server::{api, main::NexusServer};

/// Create a test server instance
async fn create_test_server() -> (Router, Arc<NexusServer>) {
    let temp_dir = TempDir::new().unwrap();
    
    // Initialize core components
    let catalog = Catalog::new(temp_dir.path().join("catalog")).unwrap();
    let catalog_arc = Arc::new(RwLock::new(catalog));
    
    let store = RecordStore::new(temp_dir.path()).unwrap();
    let store_arc = Arc::new(RwLock::new(store));
    
    let label_index = LabelIndex::new();
    let label_index_arc = Arc::new(RwLock::new(label_index));
    
    let knn_index = KnnIndex::new(128).unwrap();
    let knn_index_arc = Arc::new(RwLock::new(knn_index));
    
    let executor = Executor::new(
        catalog_arc.clone(),
        store_arc.clone(),
        label_index_arc.clone(),
        knn_index_arc.clone(),
    ).unwrap();
    let executor_arc = Arc::new(RwLock::new(executor));
    
    // Initialize API modules
    api::cypher::init_executor(executor_arc.clone()).unwrap();
    api::knn::init_executor(executor_arc.clone()).unwrap();
    api::ingest::init_executor(executor_arc.clone()).unwrap();
    api::schema::init_catalog(catalog_arc.clone()).unwrap();
    api::data::init_catalog(catalog_arc.clone()).unwrap();
    api::stats::init_instances(
        catalog_arc.clone(),
        label_index_arc.clone(),
        knn_index_arc.clone(),
    ).unwrap();
    api::health::init();
    
    // Create server state
    let server = Arc::new(NexusServer {
        executor: executor_arc,
        catalog: catalog_arc,
        label_index: label_index_arc,
        knn_index: knn_index_arc,
    });
    
    // Create MCP router
    let mcp_router = create_mcp_router(server.clone()).await.unwrap();
    
    // Build main router
    let app = Router::new()
        .route("/", axum::routing::get(api::health::health_check))
        .route("/health", axum::routing::get(api::health::health_check))
        .route("/metrics", axum::routing::get(api::health::metrics))
        .route("/cypher", axum::routing::post(api::cypher::execute_cypher))
        .route("/knn_traverse", axum::routing::post(api::knn::knn_traverse))
        .route("/ingest", axum::routing::post(api::ingest::ingest_data))
        .route("/schema/labels", axum::routing::post(api::schema::create_label))
        .route("/schema/labels", axum::routing::get(api::schema::list_labels))
        .route("/schema/rel_types", axum::routing::post(api::schema::create_rel_type))
        .route("/schema/rel_types", axum::routing::get(api::schema::list_rel_types))
        .route("/data/nodes", axum::routing::post(api::data::create_node))
        .route("/data/relationships", axum::routing::post(api::data::create_rel))
        .route("/data/nodes", axum::routing::put(api::data::update_node))
        .route("/data/nodes", axum::routing::delete(api::data::delete_node))
        .route("/stats", axum::routing::get(api::stats::get_stats))
        .route("/clustering/algorithms", axum::routing::get(api::clustering::get_algorithms))
        .route("/clustering/cluster", axum::routing::post({
            let server = server.clone();
            move |request| api::clustering::cluster_nodes(axum::extract::State(server), request)
        }))
        .route("/clustering/group-by-label", axum::routing::post({
            let server = server.clone();
            move |request| api::clustering::group_by_label(axum::extract::State(server), request)
        }))
        .route("/clustering/group-by-property", axum::routing::post({
            let server = server.clone();
            move |request| api::clustering::group_by_property(axum::extract::State(server), request)
        }))
        .route("/sse/cypher", axum::routing::get({
            let server = server.clone();
            move |query| api::streaming::stream_cypher_query(query, server)
        }))
        .route("/sse/stats", axum::routing::get({
            let server = server.clone();
            move |query| api::streaming::stream_stats(query, server)
        }))
        .route("/sse/heartbeat", axum::routing::get(api::streaming::stream_heartbeat))
        .nest("/mcp", mcp_router);
    
    (app, server)
}

/// Create MCP router for testing
async fn create_mcp_router(nexus_server: Arc<NexusServer>) -> anyhow::Result<Router> {
    use hyper::service::Service;
    use hyper_util::service::TowerToHyperService;
    use rmcp::transport::streamable_http_server::StreamableHttpService;
    use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use tracing;

    let server = nexus_server.clone();
    let streamable_service = StreamableHttpService::new(
        move || Ok(api::streaming::NexusMcpService::new(server.clone())),
        LocalSessionManager::default().into(),
        Default::default(),
    );

    let hyper_service = TowerToHyperService::new(streamable_service);
    let router = Router::new().route(
        "/",
        axum::routing::any(move |req: Request| {
            let service = hyper_service.clone();
            async move {
                match service.call(req).await {
                    Ok(response) => Ok(response),
                    Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
                }
            }
        }),
    );

    Ok(router)
}

// ============================================================================
// Health Check Tests
// ============================================================================

#[tokio::test]
async fn test_health_check_endpoint() {
    let (app, _server) = create_test_server().await;
    
    let request = Request::builder()
        .method(Method::GET)
        .uri("/health")
        .body(Body::empty())
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let health: Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(health["status"], "ok");
    assert!(health["version"].is_string());
    assert!(health["uptime_seconds"].is_number());
}

#[tokio::test]
async fn test_metrics_endpoint() {
    let (app, _server) = create_test_server().await;
    
    let request = Request::builder()
        .method(Method::GET)
        .uri("/metrics")
        .body(Body::empty())
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let metrics: Value = serde_json::from_slice(&body).unwrap();
    
    assert!(metrics["system"].is_object());
    assert!(metrics["components"].is_object());
    assert!(metrics["timestamp"].is_string());
}

// ============================================================================
// Cypher Query Tests
// ============================================================================

#[tokio::test]
async fn test_cypher_endpoint_success() {
    let (app, _server) = create_test_server().await;
    
    let query_body = json!({
        "query": "RETURN 1 as test",
        "params": {}
    });
    
    let request = Request::builder()
        .method(Method::POST)
        .uri("/cypher")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&query_body).unwrap()))
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let result: Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(result["status"], "success");
    assert!(result["columns"].is_array());
    assert!(result["rows"].is_array());
    assert!(result["execution_time_ms"].is_number());
}

#[tokio::test]
async fn test_cypher_endpoint_invalid_query() {
    let (app, _server) = create_test_server().await;
    
    let query_body = json!({
        "query": "INVALID CYPHER SYNTAX",
        "params": {}
    });
    
    let request = Request::builder()
        .method(Method::POST)
        .uri("/cypher")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&query_body).unwrap()))
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let error: Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(error["status"], "error");
    assert!(error["message"].is_string());
}

#[tokio::test]
async fn test_cypher_endpoint_missing_query() {
    let (app, _server) = create_test_server().await;
    
    let query_body = json!({
        "params": {}
    });
    
    let request = Request::builder()
        .method(Method::POST)
        .uri("/cypher")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&query_body).unwrap()))
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// ============================================================================
// Geospatial Point Serialization Tests
// ============================================================================

#[tokio::test]
async fn test_point_serialization_in_return_via_http() {
    let (app, _server) = create_test_server().await;
    
    let query_body = json!({
        "query": "RETURN point({x: 1.0, y: 2.0, crs: 'cartesian'}) AS p",
        "params": {}
    });
    
    let request = Request::builder()
        .method(Method::POST)
        .uri("/cypher")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&query_body).unwrap()))
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let result: Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(result["status"], "success");
    assert!(result["columns"].is_array());
    assert_eq!(result["columns"][0], "p");
    assert!(result["rows"].is_array());
    assert_eq!(result["rows"].as_array().unwrap().len(), 1);
    
    // Verify Point serialization structure
    let point_value = &result["rows"][0][0];
    assert!(point_value.is_object());
    
    let point_obj = point_value.as_object().unwrap();
    assert!(point_obj.contains_key("x"));
    assert!(point_obj.contains_key("y"));
    assert!(point_obj.contains_key("crs"));
    
    assert_eq!(point_obj["x"].as_f64().unwrap(), 1.0);
    assert_eq!(point_obj["y"].as_f64().unwrap(), 2.0);
    assert_eq!(point_obj["crs"].as_str().unwrap(), "cartesian");
}

#[tokio::test]
async fn test_point_serialization_3d_in_return_via_http() {
    let (app, _server) = create_test_server().await;
    
    let query_body = json!({
        "query": "RETURN point({x: 1.0, y: 2.0, z: 3.0, crs: 'cartesian-3d'}) AS p",
        "params": {}
    });
    
    let request = Request::builder()
        .method(Method::POST)
        .uri("/cypher")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&query_body).unwrap()))
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let result: Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(result["status"], "success");
    
    // Verify 3D Point serialization
    let point_value = &result["rows"][0][0];
    let point_obj = point_value.as_object().unwrap();
    
    assert_eq!(point_obj["x"].as_f64().unwrap(), 1.0);
    assert_eq!(point_obj["y"].as_f64().unwrap(), 2.0);
    assert_eq!(point_obj["z"].as_f64().unwrap(), 3.0);
    assert_eq!(point_obj["crs"].as_str().unwrap(), "cartesian-3d");
}

#[tokio::test]
async fn test_point_serialization_in_node_properties_via_http() {
    let (app, _server) = create_test_server().await;
    
    // Create a node with Point property
    let create_query = json!({
        "query": "CREATE (n:Location {pos: point({x: 10.0, y: 20.0, crs: 'cartesian'})}) RETURN n",
        "params": {}
    });
    
    let request = Request::builder()
        .method(Method::POST)
        .uri("/cypher")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&create_query).unwrap()))
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let result: Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(result["status"], "success");
    
    // Verify node has Point property correctly serialized
    let node_value = &result["rows"][0][0];
    assert!(node_value.is_object());
    
    let node_obj = node_value.as_object().unwrap();
    assert!(node_obj.contains_key("pos"));
    
    let pos_value = &node_obj["pos"];
    assert!(pos_value.is_object());
    
    let pos_obj = pos_value.as_object().unwrap();
    assert_eq!(pos_obj["x"].as_f64().unwrap(), 10.0);
    assert_eq!(pos_obj["y"].as_f64().unwrap(), 20.0);
    assert_eq!(pos_obj["crs"].as_str().unwrap(), "cartesian");
}

#[tokio::test]
async fn test_point_serialization_wgs84_via_http() {
    let (app, _server) = create_test_server().await;
    
    let query_body = json!({
        "query": "RETURN point({longitude: -122.4194, latitude: 37.7749, crs: 'wgs-84'}) AS p",
        "params": {}
    });
    
    let request = Request::builder()
        .method(Method::POST)
        .uri("/cypher")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&query_body).unwrap()))
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let result: Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(result["status"], "success");
    
    // Verify WGS84 Point serialization
    let point_value = &result["rows"][0][0];
    let point_obj = point_value.as_object().unwrap();
    
    // WGS84 should serialize with x/y (longitude/latitude) and crs
    assert_eq!(point_obj["x"].as_f64().unwrap(), -122.4194);
    assert_eq!(point_obj["y"].as_f64().unwrap(), 37.7749);
    assert_eq!(point_obj["crs"].as_str().unwrap(), "wgs-84");
}

#[tokio::test]
async fn test_point_serialization_in_match_query_via_http() {
    let (app, _server) = create_test_server().await;
    
    // First create a node with Point property
    let create_query = json!({
        "query": "CREATE (n:Location {name: 'SF', pos: point({x: -122.4194, y: 37.7749, crs: 'wgs-84'})}) RETURN n",
        "params": {}
    });
    
    let request = Request::builder()
        .method(Method::POST)
        .uri("/cypher")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&create_query).unwrap()))
        .unwrap();
    
    let _response = app.clone().oneshot(request).await.unwrap();
    
    // Now query it back
    let match_query = json!({
        "query": "MATCH (n:Location {name: 'SF'}) RETURN n.pos AS location",
        "params": {}
    });
    
    let request = Request::builder()
        .method(Method::POST)
        .uri("/cypher")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&match_query).unwrap()))
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let result: Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(result["status"], "success");
    assert_eq!(result["rows"].as_array().unwrap().len(), 1);
    
    // Verify Point is correctly serialized in response
    let location_value = &result["rows"][0][0];
    assert!(location_value.is_object());
    
    let location_obj = location_value.as_object().unwrap();
    assert_eq!(location_obj["x"].as_f64().unwrap(), -122.4194);
    assert_eq!(location_obj["y"].as_f64().unwrap(), 37.7749);
    assert_eq!(location_obj["crs"].as_str().unwrap(), "wgs-84");
}

// ============================================================================
// KNN Traverse Tests
// ============================================================================

#[tokio::test]
async fn test_knn_traverse_endpoint() {
    let (app, _server) = create_test_server().await;
    
    let knn_body = json!({
        "label": "Person",
        "vector": [0.1, 0.2, 0.3, 0.4],
        "k": 5,
        "limit": 10
    });
    
    let request = Request::builder()
        .method(Method::POST)
        .uri("/knn_traverse")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&knn_body).unwrap()))
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let result: Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(result["status"], "success");
    assert!(result["results"].is_array());
    assert!(result["execution_time_ms"].is_number());
}

#[tokio::test]
async fn test_knn_traverse_invalid_vector() {
    let (app, _server) = create_test_server().await;
    
    let knn_body = json!({
        "label": "Person",
        "vector": "invalid_vector",
        "k": 5
    });
    
    let request = Request::builder()
        .method(Method::POST)
        .uri("/knn_traverse")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&knn_body).unwrap()))
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// ============================================================================
// Schema Management Tests
// ============================================================================

#[tokio::test]
async fn test_create_label() {
    let (app, _server) = create_test_server().await;
    
    let label_body = json!({
        "name": "Person"
    });
    
    let request = Request::builder()
        .method(Method::POST)
        .uri("/schema/labels")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&label_body).unwrap()))
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let result: Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(result["status"], "success");
    assert!(result["label_id"].is_number());
}

#[tokio::test]
async fn test_list_labels() {
    let (app, _server) = create_test_server().await;
    
    let request = Request::builder()
        .method(Method::GET)
        .uri("/schema/labels")
        .body(Body::empty())
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let result: Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(result["status"], "success");
    assert!(result["labels"].is_array());
}

#[tokio::test]
async fn test_create_rel_type() {
    let (app, _server) = create_test_server().await;
    
    let rel_type_body = json!({
        "name": "KNOWS"
    });
    
    let request = Request::builder()
        .method(Method::POST)
        .uri("/schema/rel_types")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&rel_type_body).unwrap()))
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let result: Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(result["status"], "success");
    assert!(result["type_id"].is_number());
}

// ============================================================================
// Data Management Tests
// ============================================================================

#[tokio::test]
async fn test_create_node() {
    let (app, _server) = create_test_server().await;
    
    let node_body = json!({
        "labels": ["Person"],
        "properties": {
            "name": "Alice",
            "age": 30
        }
    });
    
    let request = Request::builder()
        .method(Method::POST)
        .uri("/data/nodes")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&node_body).unwrap()))
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let result: Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(result["status"], "success");
    assert!(result["node_id"].is_number());
}

#[tokio::test]
async fn test_create_relationship() {
    let (app, _server) = create_test_server().await;
    
    let rel_body = json!({
        "source_id": 1,
        "target_id": 2,
        "rel_type": "KNOWS",
        "properties": {
            "since": "2020"
        }
    });
    
    let request = Request::builder()
        .method(Method::POST)
        .uri("/data/relationships")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&rel_body).unwrap()))
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let result: Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(result["status"], "success");
    assert!(result["rel_id"].is_number());
}

// ============================================================================
// Statistics Tests
// ============================================================================

#[tokio::test]
async fn test_stats_endpoint() {
    let (app, _server) = create_test_server().await;
    
    let request = Request::builder()
        .method(Method::GET)
        .uri("/stats")
        .body(Body::empty())
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let result: Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(result["status"], "success");
    assert!(result["stats"].is_object());
    assert!(result["stats"]["node_count"].is_number());
    assert!(result["stats"]["relationship_count"].is_number());
}

// ============================================================================
// Clustering Tests
// ============================================================================

#[tokio::test]
async fn test_clustering_algorithms() {
    let (app, _server) = create_test_server().await;
    
    let request = Request::builder()
        .method(Method::GET)
        .uri("/clustering/algorithms")
        .body(Body::empty())
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let result: Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(result["status"], "success");
    assert!(result["algorithms"].is_array());
}

#[tokio::test]
async fn test_clustering_cluster() {
    let (app, _server) = create_test_server().await;
    
    let cluster_body = json!({
        "algorithm": "kmeans",
        "k": 3,
        "label": "Person"
    });
    
    let request = Request::builder()
        .method(Method::POST)
        .uri("/clustering/cluster")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&cluster_body).unwrap()))
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let result: Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(result["status"], "success");
    assert!(result["clusters"].is_array());
}

// ============================================================================
// Ingest Tests
// ============================================================================

#[tokio::test]
async fn test_ingest_endpoint() {
    let (app, _server) = create_test_server().await;
    
    let ingest_body = json!({
        "nodes": [
            {
                "labels": ["Person"],
                "properties": {"name": "Alice"}
            }
        ],
        "relationships": []
    });
    
    let request = Request::builder()
        .method(Method::POST)
        .uri("/ingest")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&ingest_body).unwrap()))
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let result: Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(result["status"], "success");
    assert!(result["nodes_created"].is_number());
    assert!(result["relationships_created"].is_number());
    assert!(result["execution_time_ms"].is_number());
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[tokio::test]
async fn test_404_endpoint() {
    let (app, _server) = create_test_server().await;
    
    let request = Request::builder()
        .method(Method::GET)
        .uri("/nonexistent")
        .body(Body::empty())
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_invalid_json() {
    let (app, _server) = create_test_server().await;
    
    let request = Request::builder()
        .method(Method::POST)
        .uri("/cypher")
        .header("content-type", "application/json")
        .body(Body::from("invalid json"))
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_missing_content_type() {
    let (app, _server) = create_test_server().await;
    
    let request = Request::builder()
        .method(Method::POST)
        .uri("/cypher")
        .body(Body::from("{}"))
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// ============================================================================
// Performance Tests
// ============================================================================

#[tokio::test]
async fn test_health_check_performance() {
    let (app, _server) = create_test_server().await;
    
    let start = std::time::Instant::now();
    let mut success_count = 0;
    let total_requests = 100;
    
    for _ in 0..total_requests {
        let request = Request::builder()
            .method(Method::GET)
            .uri("/health")
            .body(Body::empty())
            .unwrap();
        
        let response = app.clone().oneshot(request).await.unwrap();
        if response.status() == StatusCode::OK {
            success_count += 1;
        }
    }
    
    let elapsed = start.elapsed();
    let throughput = total_requests as f64 / elapsed.as_secs_f64();
    
    tracing::info!("Health check performance: {} requests in {:?}", total_requests, elapsed);
    tracing::info!("Throughput: {:.0} requests/sec", throughput);
    
    assert_eq!(success_count, total_requests);
    assert!(throughput > 1000.0, "Throughput too low: {:.0} req/sec", throughput);
}

#[tokio::test]
async fn test_concurrent_requests() {
    let (app, _server) = create_test_server().await;
    
    let mut handles = vec![];
    
    // Spawn 10 concurrent requests
    for i in 0..10 {
        let app_clone = app.clone();
        let handle = tokio::spawn(async move {
            let request = Request::builder()
                .method(Method::GET)
                .uri("/health")
                .body(Body::empty())
                .unwrap();
            
            let response = app_clone.oneshot(request).await.unwrap();
            (i, response.status())
        });
        handles.push(handle);
    }
    
    let mut success_count = 0;
    for handle in handles {
        let (_, status) = handle.await.unwrap();
        if status == StatusCode::OK {
            success_count += 1;
        }
    }
    
    assert_eq!(success_count, 10);
}

// ============================================================================
// MCP Protocol Tests
// ============================================================================

#[tokio::test]
async fn test_mcp_endpoint() {
    let (app, _server) = create_test_server().await;
    
    let request = Request::builder()
        .method(Method::POST)
        .uri("/mcp/")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"jsonrpc": "2.0", "method": "tools/list", "id": 1}"#))
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    // MCP endpoint should respond (status may vary based on implementation)
    assert!(response.status().is_success() || response.status().is_client_error());
}

// ============================================================================
// Integration Test Helpers
// ============================================================================

/// Test that all major endpoints are accessible
#[tokio::test]
async fn test_all_endpoints_accessible() {
    let (app, _server) = create_test_server().await;
    
    let endpoints = vec![
        ("/", Method::GET),
        ("/health", Method::GET),
        ("/metrics", Method::GET),
        ("/stats", Method::GET),
        ("/schema/labels", Method::GET),
        ("/schema/rel_types", Method::GET),
        ("/clustering/algorithms", Method::GET),
    ];
    
    for (endpoint, method) in endpoints {
        let request = Request::builder()
            .method(method)
            .uri(endpoint)
            .body(Body::empty())
            .unwrap();
        
        let response = app.clone().oneshot(request).await.unwrap();
        
        // Most endpoints should return 200 or 400 (for missing parameters)
        assert!(
            response.status().is_success() || response.status() == StatusCode::BAD_REQUEST,
            "Endpoint {} {} returned unexpected status: {}",
            method,
            endpoint,
            response.status()
        );
    }
}

/// Test that the server can handle malformed requests gracefully
#[tokio::test]
async fn test_graceful_error_handling() {
    let (app, _server) = create_test_server().await;
    
    let malformed_requests = vec![
        ("/cypher", Method::POST, "not json"),
        ("/knn_traverse", Method::POST, r#"{"invalid": "structure"}"#),
        ("/data/nodes", Method::POST, r#"{"labels": "not_array"}"#),
    ];
    
    for (endpoint, method, body) in malformed_requests {
        let request = Request::builder()
            .method(method)
            .uri(endpoint)
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        
        let response = app.clone().oneshot(request).await.unwrap();
        
        // Should return 400 for malformed requests
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "Endpoint {} {} should return 400 for malformed request",
            method,
            endpoint
        );
    }
}
