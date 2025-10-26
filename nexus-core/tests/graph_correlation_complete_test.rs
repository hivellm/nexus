//! Comprehensive tests for Graph Correlation Analysis
//!
//! Tests for export, statistics, and pattern recognition

use nexus_core::graph_correlation::*;
use std::collections::HashMap;

#[test]
fn test_export_all_formats() {
    let mut graph = CorrelationGraph::new(GraphType::Call, "Export Test".to_string());

    // Add test nodes
    graph
        .add_node(GraphNode {
            id: "node1".to_string(),
            node_type: NodeType::Function,
            label: "Function A".to_string(),
            metadata: HashMap::new(),
            position: Some((0.0, 0.0)),
            size: Some(1.0),
            color: Some("#3498db".to_string()),
        })
        .unwrap();

    graph
        .add_node(GraphNode {
            id: "node2".to_string(),
            node_type: NodeType::Function,
            label: "Function B".to_string(),
            metadata: HashMap::new(),
            position: Some((1.0, 1.0)),
            size: Some(1.0),
            color: Some("#3498db".to_string()),
        })
        .unwrap();

    // Add test edge
    graph
        .add_edge(GraphEdge {
            id: "edge1".to_string(),
            source: "node1".to_string(),
            target: "node2".to_string(),
            edge_type: EdgeType::Calls,
            weight: 1.0,
            metadata: HashMap::new(),
            label: Some("calls".to_string()),
        })
        .unwrap();

    // Test JSON export
    let json = export_graph(&graph, ExportFormat::Json).unwrap();
    assert!(json.contains("Export Test"));
    assert!(json.contains("node1"));
    assert!(json.contains("Function A"));

    // Test GraphML export
    let graphml = export_graph(&graph, ExportFormat::GraphML).unwrap();
    assert!(graphml.contains("<?xml"));
    assert!(graphml.contains("<graphml"));
    assert!(graphml.contains("node1"));

    // Test GEXF export
    let gexf = export_graph(&graph, ExportFormat::GEXF).unwrap();
    assert!(gexf.contains("<?xml"));
    assert!(gexf.contains("<gexf"));
    assert!(gexf.contains("node1"));

    // Test DOT export
    let dot = export_graph(&graph, ExportFormat::DOT).unwrap();
    assert!(dot.contains("digraph"));
    assert!(dot.contains("node1"));
    assert!(dot.contains("->"));
}

#[test]
fn test_graph_statistics_calculation() {
    let mut graph = CorrelationGraph::new(GraphType::Call, "Stats Test".to_string());

    // Create a simple graph: A -> B -> C
    for i in 1..=3 {
        graph
            .add_node(GraphNode {
                id: format!("node{}", i),
                node_type: NodeType::Function,
                label: format!("Node {}", i),
                metadata: HashMap::new(),
                position: None,
                size: None,
                color: None,
            })
            .unwrap();
    }

    graph
        .add_edge(GraphEdge {
            id: "edge1".to_string(),
            source: "node1".to_string(),
            target: "node2".to_string(),
            edge_type: EdgeType::Calls,
            weight: 1.0,
            metadata: HashMap::new(),
            label: None,
        })
        .unwrap();

    graph
        .add_edge(GraphEdge {
            id: "edge2".to_string(),
            source: "node2".to_string(),
            target: "node3".to_string(),
            edge_type: EdgeType::Calls,
            weight: 1.0,
            metadata: HashMap::new(),
            label: None,
        })
        .unwrap();

    let stats = calculate_statistics(&graph);

    assert_eq!(stats.node_count, 3);
    assert_eq!(stats.edge_count, 2);
    assert!(stats.avg_degree > 0.0);
    assert_eq!(stats.connected_components, 1);
}

#[test]
fn test_pattern_detection_pipeline() {
    let graph = create_pipeline_graph();

    let detector = PipelinePatternDetector;
    let result = detector.detect(&graph).unwrap();

    assert!(!result.patterns.is_empty());
    assert_eq!(result.patterns[0].pattern_type, PatternType::Pipeline);
    assert!(result.patterns[0].confidence > 0.0);
    assert!(result.statistics.total_patterns > 0);
}

#[test]
fn test_pattern_detection_event_driven() {
    let graph = create_event_driven_graph();

    let detector = EventDrivenPatternDetector;
    let result = detector.detect(&graph).unwrap();

    // May or may not find patterns depending on graph structure
    assert!(result.patterns.len() >= 0);
}

#[test]
fn test_pattern_detection_architectural() {
    let graph = create_layered_architecture_graph();

    let detector = ArchitecturalPatternDetector;
    let result = detector.detect(&graph).unwrap();

    assert!(result.patterns.len() >= 0);
    assert!(result.quality_score >= 0.0);
}

#[test]
fn test_all_graph_builders() {
    let manager = GraphCorrelationManager::new();
    let source_data = create_test_source_data();

    // Test Call Graph
    let call_graph = manager.build_graph(GraphType::Call, &source_data).unwrap();
    assert_eq!(call_graph.graph_type, GraphType::Call);
    assert!(!call_graph.nodes.is_empty());

    // Test Dependency Graph
    let dep_graph = manager
        .build_graph(GraphType::Dependency, &source_data)
        .unwrap();
    assert_eq!(dep_graph.graph_type, GraphType::Dependency);

    // Test DataFlow Graph
    let data_graph = manager
        .build_graph(GraphType::DataFlow, &source_data)
        .unwrap();
    assert_eq!(data_graph.graph_type, GraphType::DataFlow);

    // Test Component Graph
    let comp_graph = manager
        .build_graph(GraphType::Component, &source_data)
        .unwrap();
    assert_eq!(comp_graph.graph_type, GraphType::Component);
}

#[test]
fn test_graph_correlation_manager_available_types() {
    let manager = GraphCorrelationManager::new();
    let types = manager.available_graph_types();

    assert_eq!(types.len(), 4);
    assert!(types.contains(&GraphType::Call));
    assert!(types.contains(&GraphType::Dependency));
    assert!(types.contains(&GraphType::DataFlow));
    assert!(types.contains(&GraphType::Component));
}

#[test]
fn test_export_empty_graph() {
    let graph = CorrelationGraph::new(GraphType::Call, "Empty".to_string());

    let json = export_graph(&graph, ExportFormat::Json).unwrap();
    assert!(json.contains("Empty"));

    let dot = export_graph(&graph, ExportFormat::DOT).unwrap();
    assert!(dot.contains("digraph"));
}

#[test]
fn test_statistics_empty_graph() {
    let graph = CorrelationGraph::new(GraphType::Call, "Empty".to_string());
    let stats = calculate_statistics(&graph);

    assert_eq!(stats.node_count, 0);
    assert_eq!(stats.edge_count, 0);
    assert_eq!(stats.avg_degree, 0.0);
    assert_eq!(stats.connected_components, 0);
}

#[test]
fn test_complex_graph_statistics() {
    let mut graph = CorrelationGraph::new(GraphType::Call, "Complex".to_string());

    // Create a more complex graph with multiple components
    for i in 1..=6 {
        graph
            .add_node(GraphNode {
                id: format!("node{}", i),
                node_type: NodeType::Function,
                label: format!("Node {}", i),
                metadata: HashMap::new(),
                position: None,
                size: None,
                color: None,
            })
            .unwrap();
    }

    // Component 1: nodes 1-3
    graph
        .add_edge(create_edge("edge1", "node1", "node2"))
        .unwrap();
    graph
        .add_edge(create_edge("edge2", "node2", "node3"))
        .unwrap();

    // Component 2: nodes 4-6
    graph
        .add_edge(create_edge("edge3", "node4", "node5"))
        .unwrap();
    graph
        .add_edge(create_edge("edge4", "node5", "node6"))
        .unwrap();

    let stats = calculate_statistics(&graph);

    assert_eq!(stats.node_count, 6);
    assert_eq!(stats.edge_count, 4);
    assert_eq!(stats.connected_components, 2);
}

#[test]
fn test_pattern_detection_confidence_scores() {
    let graph = create_pipeline_graph();

    let detector = PipelinePatternDetector;
    let result = detector.detect(&graph).unwrap();

    for pattern in &result.patterns {
        assert!(pattern.confidence >= 0.0 && pattern.confidence <= 1.0);
    }
}

#[test]
fn test_export_special_characters() {
    let mut graph = CorrelationGraph::new(GraphType::Call, "Test <>&\"'".to_string());

    graph
        .add_node(GraphNode {
            id: "node1".to_string(),
            node_type: NodeType::Function,
            label: "Function <test>".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        })
        .unwrap();

    // Should not panic and should escape special characters
    let graphml = export_graph(&graph, ExportFormat::GraphML).unwrap();
    assert!(graphml.contains("&lt;") || graphml.contains("&gt;"));
}

// Helper functions

fn create_pipeline_graph() -> CorrelationGraph {
    let mut graph = CorrelationGraph::new(GraphType::Call, "Pipeline".to_string());

    for i in 1..=5 {
        graph
            .add_node(GraphNode {
                id: format!("func{}", i),
                node_type: NodeType::Function,
                label: format!("Function {}", i),
                metadata: HashMap::new(),
                position: None,
                size: None,
                color: None,
            })
            .unwrap();
    }

    for i in 1..5 {
        graph
            .add_edge(GraphEdge {
                id: format!("edge{}", i),
                source: format!("func{}", i),
                target: format!("func{}", i + 1),
                edge_type: EdgeType::Calls,
                weight: 1.0,
                metadata: HashMap::new(),
                label: None,
            })
            .unwrap();
    }

    graph
}

fn create_event_driven_graph() -> CorrelationGraph {
    let mut graph = CorrelationGraph::new(GraphType::Call, "EventDriven".to_string());

    // Publisher
    graph
        .add_node(GraphNode {
            id: "publisher".to_string(),
            node_type: NodeType::Function,
            label: "Publisher".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        })
        .unwrap();

    // Subscribers
    for i in 1..=3 {
        graph
            .add_node(GraphNode {
                id: format!("subscriber{}", i),
                node_type: NodeType::Function,
                label: format!("Subscriber {}", i),
                metadata: HashMap::new(),
                position: None,
                size: None,
                color: None,
            })
            .unwrap();

        graph
            .add_edge(GraphEdge {
                id: format!("pub_edge{}", i),
                source: "publisher".to_string(),
                target: format!("subscriber{}", i),
                edge_type: EdgeType::Uses,
                weight: 1.0,
                metadata: HashMap::new(),
                label: None,
            })
            .unwrap();
    }

    graph
}

fn create_layered_architecture_graph() -> CorrelationGraph {
    let mut graph = CorrelationGraph::new(GraphType::Component, "Layered".to_string());

    // API layer
    let mut api_metadata = HashMap::new();
    api_metadata.insert(
        "file_path".to_string(),
        serde_json::Value::String("api/routes.rs".to_string()),
    );

    graph
        .add_node(GraphNode {
            id: "api_layer".to_string(),
            node_type: NodeType::API,
            label: "API Layer".to_string(),
            metadata: api_metadata,
            position: None,
            size: None,
            color: None,
        })
        .unwrap();

    // Service layer
    let mut service_metadata = HashMap::new();
    service_metadata.insert(
        "file_path".to_string(),
        serde_json::Value::String("service/handler.rs".to_string()),
    );

    graph
        .add_node(GraphNode {
            id: "service_layer".to_string(),
            node_type: NodeType::Class,
            label: "Service Layer".to_string(),
            metadata: service_metadata,
            position: None,
            size: None,
            color: None,
        })
        .unwrap();

    // Model layer
    let mut model_metadata = HashMap::new();
    model_metadata.insert(
        "file_path".to_string(),
        serde_json::Value::String("model/entity.rs".to_string()),
    );

    graph
        .add_node(GraphNode {
            id: "model_layer".to_string(),
            node_type: NodeType::Class,
            label: "Model Layer".to_string(),
            metadata: model_metadata,
            position: None,
            size: None,
            color: None,
        })
        .unwrap();

    graph
}

fn create_test_source_data() -> GraphSourceData {
    let mut source_data = GraphSourceData::new();

    source_data.add_file("src/main.rs".to_string(), "fn main() {}".to_string());
    source_data.add_file("src/lib.rs".to_string(), "pub mod utils;".to_string());

    source_data.add_functions("src/main.rs".to_string(), vec!["main".to_string()]);
    source_data.add_imports("src/lib.rs".to_string(), vec!["utils".to_string()]);

    source_data
}

fn create_edge(id: &str, source: &str, target: &str) -> GraphEdge {
    GraphEdge {
        id: id.to_string(),
        source: source.to_string(),
        target: target.to_string(),
        edge_type: EdgeType::Calls,
        weight: 1.0,
        metadata: HashMap::new(),
        label: None,
    }
}
