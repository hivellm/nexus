//! Comprehensive tests for UMICP Graph Correlation methods

use nexus_server::api::graph_correlation_umicp::{GraphUmicpHandler, UmicpRequest};
use serde_json::json;
use tracing;

#[tokio::test]
async fn test_umicp_graph_generate() {
    let handler = GraphUmicpHandler::new();

    let request = UmicpRequest {
        method: "graph.generate".to_string(),
        params: Some(json!({
            "graph_type": "Call",
            "files": {
                "test.rs": "fn main() { helper(); } fn helper() {}"
            }
        })),
        context: None,
    };

    let response = handler.handle_request(request).await;
    assert!(response.result.is_some());
    assert!(response.error.is_none());

    let result = response.result.unwrap();
    assert!(result.get("graph_id").is_some());
    assert!(result.get("graph").is_some());
    assert!(result.get("node_count").is_some());
    assert!(result.get("edge_count").is_some());
}

#[tokio::test]
async fn test_umicp_graph_get() {
    let handler = GraphUmicpHandler::new();

    // First generate a graph
    let generate_request = UmicpRequest {
        method: "graph.generate".to_string(),
        params: Some(json!({
            "graph_type": "Call",
            "files": {
                "test.rs": "fn main() {}"
            }
        })),
        context: None,
    };

    let generate_response = handler.handle_request(generate_request).await;
    let graph_id = generate_response
        .result
        .unwrap()
        .get("graph_id")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

    // Now get the graph
    let get_request = UmicpRequest {
        method: "graph.get".to_string(),
        params: Some(json!({
            "graph_id": graph_id
        })),
        context: None,
    };

    let get_response = handler.handle_request(get_request).await;
    assert!(get_response.result.is_some());
    assert!(get_response.error.is_none());
}

#[tokio::test]
async fn test_umicp_graph_get_not_found() {
    let handler = GraphUmicpHandler::new();

    let request = UmicpRequest {
        method: "graph.get".to_string(),
        params: Some(json!({
            "graph_id": "nonexistent"
        })),
        context: None,
    };

    let response = handler.handle_request(request).await;
    assert!(response.result.is_none());
    assert!(response.error.is_some());
    assert_eq!(response.error.unwrap().code, "GRAPH_NOT_FOUND");
}

#[tokio::test]
async fn test_umicp_graph_analyze() {
    let handler = GraphUmicpHandler::new();

    // Generate a graph first
    let generate_request = UmicpRequest {
        method: "graph.generate".to_string(),
        params: Some(json!({
            "graph_type": "Call",
            "files": {
                "test.rs": "fn main() { helper(); } fn helper() {}"
            }
        })),
        context: None,
    };

    let generate_response = handler.handle_request(generate_request).await;
    let graph_id = generate_response
        .result
        .unwrap()
        .get("graph_id")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

    // Analyze the graph
    let analyze_request = UmicpRequest {
        method: "graph.analyze".to_string(),
        params: Some(json!({
            "graph_id": graph_id,
            "analysis_type": "statistics"
        })),
        context: None,
    };

    let analyze_response = handler.handle_request(analyze_request).await;
    assert!(analyze_response.result.is_some());
    assert!(analyze_response.error.is_none());

    let result = analyze_response.result.unwrap();
    assert!(result.get("statistics").is_some());
}

#[tokio::test]
async fn test_umicp_graph_analyze_patterns() {
    let handler = GraphUmicpHandler::new();

    // Generate a graph
    let generate_request = UmicpRequest {
        method: "graph.generate".to_string(),
        params: Some(json!({
            "graph_type": "Call",
            "files": {
                "test.rs": "fn main() { helper(); } fn helper() {}"
            }
        })),
        context: None,
    };

    let generate_response = handler.handle_request(generate_request).await;
    let graph_id = generate_response
        .result
        .unwrap()
        .get("graph_id")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

    // Analyze patterns
    let analyze_request = UmicpRequest {
        method: "graph.analyze".to_string(),
        params: Some(json!({
            "graph_id": graph_id,
            "analysis_type": "patterns"
        })),
        context: None,
    };

    let analyze_response = handler.handle_request(analyze_request).await;
    assert!(analyze_response.result.is_some());
    let result = analyze_response.result.unwrap();
    assert!(result.get("patterns").is_some());
    assert!(result.get("pattern_count").is_some());
}

#[tokio::test]
async fn test_umicp_graph_visualize() {
    let handler = GraphUmicpHandler::new();

    // Generate a graph
    let generate_request = UmicpRequest {
        method: "graph.generate".to_string(),
        params: Some(json!({
            "graph_type": "Call",
            "files": {
                "test.rs": "fn main() { helper(); } fn helper() {}"
            }
        })),
        context: None,
    };

    let generate_response = handler.handle_request(generate_request).await;
    let graph_id = generate_response
        .result
        .unwrap()
        .get("graph_id")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

    // Visualize the graph
    let visualize_request = UmicpRequest {
        method: "graph.visualize".to_string(),
        params: Some(json!({
            "graph_id": graph_id,
            "width": 800,
            "height": 600
        })),
        context: None,
    };

    let visualize_response = handler.handle_request(visualize_request).await;
    if visualize_response.error.is_some() {
        etracing::info!("Visualization error: {:?}", visualize_response.error);
    }
    assert!(
        visualize_response.result.is_some(),
        "Visualization failed: {:?}",
        visualize_response.error
    );
    assert!(visualize_response.error.is_none());

    let result = visualize_response.result.unwrap();
    assert!(result.get("svg").is_some());
    assert!(result.get("width").is_some());
    assert!(result.get("height").is_some());
}

#[tokio::test]
async fn test_umicp_graph_export() {
    let handler = GraphUmicpHandler::new();

    // Generate a graph
    let generate_request = UmicpRequest {
        method: "graph.generate".to_string(),
        params: Some(json!({
            "graph_type": "Call",
            "files": {
                "test.rs": "fn main() { helper(); } fn helper() {}"
            }
        })),
        context: None,
    };

    let generate_response = handler.handle_request(generate_request).await;
    let graph_id = generate_response
        .result
        .unwrap()
        .get("graph_id")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

    // Export to JSON
    let export_request = UmicpRequest {
        method: "graph.export".to_string(),
        params: Some(json!({
            "graph_id": graph_id,
            "format": "JSON"
        })),
        context: None,
    };

    let export_response = handler.handle_request(export_request).await;
    assert!(export_response.result.is_some());
    assert!(export_response.error.is_none());

    let result = export_response.result.unwrap();
    assert!(result.get("format").is_some());
    assert!(result.get("content").is_some());
    assert_eq!(result.get("format").unwrap().as_str().unwrap(), "JSON");
}

#[tokio::test]
async fn test_umicp_graph_export_all_formats() {
    let handler = GraphUmicpHandler::new();

    // Generate a graph
    let generate_request = UmicpRequest {
        method: "graph.generate".to_string(),
        params: Some(json!({
            "graph_type": "Call",
            "files": {
                "test.rs": "fn main() { helper(); } fn helper() {}"
            }
        })),
        context: None,
    };

    let generate_response = handler.handle_request(generate_request).await;
    let graph_id = generate_response
        .result
        .unwrap()
        .get("graph_id")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

    // Test all export formats
    let formats = ["JSON", "GraphML", "GEXF", "DOT"];
    for format in formats {
        let export_request = UmicpRequest {
            method: "graph.export".to_string(),
            params: Some(json!({
                "graph_id": graph_id,
                "format": format
            })),
            context: None,
        };

        let export_response = handler.handle_request(export_request).await;
        assert!(
            export_response.result.is_some(),
            "Failed to export to {}",
            format
        );
        assert!(
            export_response.error.is_none(),
            "Error exporting to {}: {:?}",
            format,
            export_response.error
        );
    }
}

#[tokio::test]
async fn test_umicp_graph_patterns() {
    let handler = GraphUmicpHandler::new();

    // Generate a graph
    let generate_request = UmicpRequest {
        method: "graph.generate".to_string(),
        params: Some(json!({
            "graph_type": "Call",
            "files": {
                "test.rs": "fn main() { helper(); } fn helper() {}"
            }
        })),
        context: None,
    };

    let generate_response = handler.handle_request(generate_request).await;
    let graph_id = generate_response
        .result
        .unwrap()
        .get("graph_id")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

    // Get patterns
    let patterns_request = UmicpRequest {
        method: "graph.patterns".to_string(),
        params: Some(json!({
            "graph_id": graph_id
        })),
        context: None,
    };

    let patterns_response = handler.handle_request(patterns_request).await;
    assert!(patterns_response.result.is_some());
    let result = patterns_response.result.unwrap();
    assert!(result.get("patterns").is_some());
}

#[tokio::test]
async fn test_umicp_graph_search() {
    let handler = GraphUmicpHandler::new();

    let request = UmicpRequest {
        method: "graph.search".to_string(),
        params: Some(json!({
            "query": "function calls"
        })),
        context: None,
    };

    let response = handler.handle_request(request).await;
    assert!(response.result.is_some());
    let result = response.result.unwrap();
    assert!(result.get("query").is_some());
    assert!(result.get("results").is_some());
    assert!(result.get("count").is_some());
}

#[tokio::test]
async fn test_umicp_invalid_method() {
    let handler = GraphUmicpHandler::new();

    let request = UmicpRequest {
        method: "invalid.method".to_string(),
        params: None,
        context: None,
    };

    let response = handler.handle_request(request).await;
    assert!(response.result.is_none());
    assert!(response.error.is_some());
    assert_eq!(response.error.unwrap().code, "METHOD_NOT_FOUND");
}

#[tokio::test]
async fn test_umicp_missing_params() {
    let handler = GraphUmicpHandler::new();

    let request = UmicpRequest {
        method: "graph.generate".to_string(),
        params: None,
        context: None,
    };

    let response = handler.handle_request(request).await;
    assert!(response.result.is_none());
    assert!(response.error.is_some());
    assert_eq!(response.error.unwrap().code, "INVALID_PARAMS");
}

#[tokio::test]
async fn test_umicp_invalid_graph_type() {
    let handler = GraphUmicpHandler::new();

    let request = UmicpRequest {
        method: "graph.generate".to_string(),
        params: Some(json!({
            "graph_type": "InvalidType",
            "files": {}
        })),
        context: None,
    };

    let response = handler.handle_request(request).await;
    assert!(response.result.is_none());
    assert!(response.error.is_some());
    assert_eq!(response.error.unwrap().code, "INVALID_PARAMS");
}
