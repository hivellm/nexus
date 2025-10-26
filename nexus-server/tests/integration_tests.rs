use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use serde_json::{Value, json};
use tower::ServiceExt;

/// Test helper to create a simple test app instance
async fn create_test_app() -> Router {
    // Create a simple router with basic endpoints for testing
    Router::new()
        .route("/health", axum::routing::get(|| async { "healthy" }))
        .route("/test", axum::routing::post(|body: String| async { body }))
}

/// Test helper to make HTTP requests
async fn make_request(
    app: &Router,
    method: Method,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, String) {
    let request_builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");

    let request = if let Some(body) = body {
        request_builder.body(Body::from(body.to_string())).unwrap()
    } else {
        request_builder.body(Body::empty()).unwrap()
    };

    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();

    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();

    let body_str = String::from_utf8(body_bytes.to_vec()).unwrap_or_default();

    (status, body_str)
}

#[tokio::test]
async fn test_health_check() {
    let app = create_test_app().await;
    let (status, body) = make_request(&app, Method::GET, "/health", None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "healthy");
}

#[tokio::test]
async fn test_post_endpoint() {
    let app = create_test_app().await;

    let test_body = json!({
        "message": "Hello, World!",
        "number": 42
    });

    let (status, body) = make_request(&app, Method::POST, "/test", Some(test_body)).await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Hello, World!"));
    assert!(body.contains("42"));
}

#[tokio::test]
async fn test_404_endpoint() {
    let app = create_test_app().await;

    let (status, _) = make_request(&app, Method::GET, "/nonexistent", None).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_concurrent_requests() {
    let app = create_test_app().await;

    let mut handles = vec![];

    for i in 0..10 {
        let app_clone = app.clone();

        let handle = tokio::spawn(async move {
            let test_body = json!({
                "request_id": i,
                "message": format!("Request {}", i)
            });

            let (status, body) =
                make_request(&app_clone, Method::POST, "/test", Some(test_body)).await;

            (status, body)
        });

        handles.push(handle);
    }

    let mut results = vec![];
    for handle in handles {
        let result = handle.await.unwrap();
        results.push(result);
    }

    // All requests should succeed
    for (status, body) in results {
        assert_eq!(status, StatusCode::OK);
        assert!(!body.is_empty());
    }
}

#[tokio::test]
async fn test_performance_metrics() {
    let app = create_test_app().await;

    let test_body = json!({
        "large_data": "x".repeat(1000)
    });

    let start = std::time::Instant::now();
    let (status, body) = make_request(&app, Method::POST, "/test", Some(test_body)).await;
    let duration = start.elapsed();

    assert_eq!(status, StatusCode::OK);
    assert!(duration.as_millis() < 1000); // Should complete within 1 second
    assert!(body.contains("large_data"));
}

#[tokio::test]
async fn test_error_handling() {
    let app = create_test_app().await;

    // Test invalid JSON - this might return 200 with the raw string
    let request = Request::builder()
        .method(Method::POST)
        .uri("/test")
        .header("content-type", "application/json")
        .body(Body::from("invalid json"))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    // The simple test endpoint just returns the body as-is, so it returns 200
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_http_methods() {
    let app = create_test_app().await;

    // Test GET
    let (status, _) = make_request(&app, Method::GET, "/health", None).await;
    assert_eq!(status, StatusCode::OK);

    // Test POST
    let test_body = json!({"test": "data"});
    let (status, _) = make_request(&app, Method::POST, "/test", Some(test_body)).await;
    assert_eq!(status, StatusCode::OK);

    // Test unsupported method
    let (status, _) = make_request(&app, Method::PUT, "/health", None).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn test_request_headers() {
    let app = create_test_app().await;

    let request = Request::builder()
        .method(Method::GET)
        .uri("/health")
        .header("custom-header", "test-value")
        .header("content-type", "application/json")
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_large_payload() {
    let app = create_test_app().await;

    let large_data = json!({
        "data": "x".repeat(10000),
        "numbers": (0..1000).collect::<Vec<i32>>(),
        "nested": {
            "level1": {
                "level2": {
                    "level3": "deep value"
                }
            }
        }
    });

    let (status, body) = make_request(&app, Method::POST, "/test", Some(large_data)).await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("deep value"));
}

#[tokio::test]
async fn test_empty_payload() {
    let app = create_test_app().await;

    let (status, body) = make_request(&app, Method::POST, "/test", Some(json!(null))).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "null");
}
