//! Correlation test suite. Attached via `#[cfg(test)] mod tests;` in
//! the parent module; super::* pulls in every correlation type.

#![allow(unused_imports)]
use super::*;

#[test]
fn test_correlation_graph_creation() {
    let graph = CorrelationGraph::new(GraphType::Call, "Test Graph".to_string());
    assert_eq!(graph.graph_type, GraphType::Call);
    assert_eq!(graph.name, "Test Graph");
    assert!(graph.nodes.is_empty());
    assert!(graph.edges.is_empty());
}

#[test]
fn test_add_node() {
    let mut graph = CorrelationGraph::new(GraphType::Call, "Test Graph".to_string());

    let node = GraphNode {
        id: "node1".to_string(),
        node_type: NodeType::Function,
        label: "test_function".to_string(),
        metadata: HashMap::new(),
        position: None,
        size: None,
        color: None,
    };

    assert!(graph.add_node(node).is_ok());
    assert_eq!(graph.nodes.len(), 1);
    assert_eq!(graph.nodes[0].id, "node1");
}

#[test]
fn test_add_edge() {
    let mut graph = CorrelationGraph::new(GraphType::Call, "Test Graph".to_string());

    // Add nodes first
    let node1 = GraphNode {
        id: "node1".to_string(),
        node_type: NodeType::Function,
        label: "function1".to_string(),
        metadata: HashMap::new(),
        position: None,
        size: None,
        color: None,
    };

    let node2 = GraphNode {
        id: "node2".to_string(),
        node_type: NodeType::Function,
        label: "function2".to_string(),
        metadata: HashMap::new(),
        position: None,
        size: None,
        color: None,
    };

    graph.add_node(node1).unwrap();
    graph.add_node(node2).unwrap();

    // Add edge
    let edge = GraphEdge {
        id: "edge1".to_string(),
        source: "node1".to_string(),
        target: "node2".to_string(),
        edge_type: EdgeType::Calls,
        weight: 1.0,
        metadata: HashMap::new(),
        label: None,
    };

    assert!(graph.add_edge(edge).is_ok());
    assert_eq!(graph.edges.len(), 1);
    assert_eq!(graph.edges[0].id, "edge1");
}

#[test]
fn test_duplicate_node_id() {
    let mut graph = CorrelationGraph::new(GraphType::Call, "Test Graph".to_string());

    let node1 = GraphNode {
        id: "node1".to_string(),
        node_type: NodeType::Function,
        label: "function1".to_string(),
        metadata: HashMap::new(),
        position: None,
        size: None,
        color: None,
    };

    let node2 = GraphNode {
        id: "node1".to_string(), // Same ID
        node_type: NodeType::Function,
        label: "function2".to_string(),
        metadata: HashMap::new(),
        position: None,
        size: None,
        color: None,
    };

    graph.add_node(node1).unwrap();
    assert!(graph.add_node(node2).is_err());
}

#[test]
fn test_edge_with_nonexistent_node() {
    let mut graph = CorrelationGraph::new(GraphType::Call, "Test Graph".to_string());

    let edge = GraphEdge {
        id: "edge1".to_string(),
        source: "nonexistent".to_string(),
        target: "also_nonexistent".to_string(),
        edge_type: EdgeType::Calls,
        weight: 1.0,
        metadata: HashMap::new(),
        label: None,
    };

    assert!(graph.add_edge(edge).is_err());
}

#[test]
fn test_graph_statistics() {
    let mut graph = CorrelationGraph::new(GraphType::Call, "Test Graph".to_string());

    // Add nodes
    for i in 0..3 {
        let node = GraphNode {
            id: format!("node{}", i),
            node_type: NodeType::Function,
            label: format!("function{}", i),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };
        graph.add_node(node).unwrap();
    }

    // Add edges
    let edge = GraphEdge {
        id: "edge1".to_string(),
        source: "node0".to_string(),
        target: "node1".to_string(),
        edge_type: EdgeType::Calls,
        weight: 1.0,
        metadata: HashMap::new(),
        label: None,
    };
    graph.add_edge(edge).unwrap();

    let stats = graph.statistics();
    assert_eq!(stats.node_count, 3);
    assert_eq!(stats.edge_count, 1);
    assert_eq!(stats.avg_degree, 1.0 / 3.0);
}

#[test]
fn test_call_graph_builder() {
    let builder = CallGraphBuilder::new("Test Call Graph".to_string());
    let mut source_data = GraphSourceData::new();

    source_data.add_file("test.rs".to_string(), "fn test() {}".to_string());
    source_data.add_functions("test.rs".to_string(), vec!["test".to_string()]);

    let graph = builder.build(&source_data).unwrap();
    assert_eq!(graph.graph_type, GraphType::Call);
    assert_eq!(graph.nodes.len(), 2); // 1 file + 1 function
    assert_eq!(graph.edges.len(), 1); // 1 edge from file to function
}

#[test]
fn test_dependency_graph_builder() {
    let builder = DependencyGraphBuilder::new("Test Dependency Graph".to_string());
    let mut source_data = GraphSourceData::new();

    source_data.add_file("main.rs".to_string(), "use module;".to_string());
    source_data.add_file("module.rs".to_string(), "pub fn func() {}".to_string());
    source_data.add_imports("main.rs".to_string(), vec!["module.rs".to_string()]);

    let graph = builder.build(&source_data).unwrap();
    assert_eq!(graph.graph_type, GraphType::Dependency);
    assert_eq!(graph.nodes.len(), 2); // 2 files
    assert_eq!(graph.edges.len(), 1); // 1 import edge
}

#[test]
fn test_graph_correlation_manager() {
    let manager = GraphCorrelationManager::new();
    let available_types = manager.available_graph_types();

    assert!(available_types.contains(&GraphType::Call));
    assert!(available_types.contains(&GraphType::Dependency));
}

#[test]
fn test_json_export() {
    let mut graph = CorrelationGraph::new(GraphType::Call, "Test Graph".to_string());

    let node = GraphNode {
        id: "node1".to_string(),
        node_type: NodeType::Function,
        label: "test_function".to_string(),
        metadata: HashMap::new(),
        position: None,
        size: None,
        color: None,
    };
    graph.add_node(node).unwrap();

    let json = graph.to_json().unwrap();
    assert!(json.contains("node1"));
    assert!(json.contains("test_function"));
}

#[test]
fn test_graphml_export() {
    let mut graph = CorrelationGraph::new(GraphType::Call, "Test Graph".to_string());

    let node = GraphNode {
        id: "node1".to_string(),
        node_type: NodeType::Function,
        label: "test_function".to_string(),
        metadata: HashMap::new(),
        position: None,
        size: None,
        color: None,
    };
    graph.add_node(node).unwrap();

    let graphml = graph.to_graphml().unwrap();
    assert!(graphml.contains("<?xml"));
    assert!(graphml.contains("node1"));
    assert!(graphml.contains("test_function"));
}

#[test]
fn test_node_type_enum_variants() {
    // Test all NodeType variants exist
    let function = NodeType::Function;
    let module = NodeType::Module;
    let class = NodeType::Class;
    let variable = NodeType::Variable;
    let api = NodeType::API;

    // Test debug formatting
    assert_eq!(format!("{:?}", function), "Function");
    assert_eq!(format!("{:?}", module), "Module");
    assert_eq!(format!("{:?}", class), "Class");
    assert_eq!(format!("{:?}", variable), "Variable");
    assert_eq!(format!("{:?}", api), "API");
}

#[test]
fn test_node_type_equality() {
    // Test equality between same variants
    assert_eq!(NodeType::Function, NodeType::Function);
    assert_eq!(NodeType::Module, NodeType::Module);
    assert_eq!(NodeType::Class, NodeType::Class);
    assert_eq!(NodeType::Variable, NodeType::Variable);
    assert_eq!(NodeType::API, NodeType::API);

    // Test inequality between different variants
    assert_ne!(NodeType::Function, NodeType::Module);
    assert_ne!(NodeType::Module, NodeType::Class);
    assert_ne!(NodeType::Class, NodeType::Variable);
    assert_ne!(NodeType::Variable, NodeType::API);
    assert_ne!(NodeType::API, NodeType::Function);
}

#[test]
fn test_node_type_clone() {
    let original = NodeType::Function;
    let cloned = original; // NodeType implements Copy, so no need for clone()
    assert_eq!(original, cloned);
}

#[test]
fn test_node_type_copy() {
    let original = NodeType::Module;
    let copied = original; // This should work because NodeType implements Copy
    assert_eq!(original, copied);
    assert_eq!(original, NodeType::Module); // original should still be valid
}

#[test]
fn test_node_type_serialization() {
    let node_types = vec![
        NodeType::Function,
        NodeType::Module,
        NodeType::Class,
        NodeType::Variable,
        NodeType::API,
    ];

    for node_type in node_types {
        // Test JSON serialization
        let json = serde_json::to_string(&node_type).unwrap();
        let deserialized: NodeType = serde_json::from_str(&json).unwrap();
        assert_eq!(node_type, deserialized);

        // Test that serialized JSON contains expected strings
        match node_type {
            NodeType::Function => assert!(json.contains("Function")),
            NodeType::Module => assert!(json.contains("Module")),
            NodeType::Class => assert!(json.contains("Class")),
            NodeType::Variable => assert!(json.contains("Variable")),
            NodeType::API => assert!(json.contains("API")),
        }
    }
}

#[test]
fn test_node_type_deserialization() {
    // Test deserialization from JSON strings
    let test_cases = vec![
        ("Function", NodeType::Function),
        ("Module", NodeType::Module),
        ("Class", NodeType::Class),
        ("Variable", NodeType::Variable),
        ("API", NodeType::API),
    ];

    for (json_str, expected) in test_cases {
        let deserialized: NodeType = serde_json::from_str(&format!("\"{}\"", json_str)).unwrap();
        assert_eq!(deserialized, expected);
    }
}

#[test]
fn test_node_type_in_graph_node() {
    // Test NodeType usage in GraphNode
    let node = GraphNode {
        id: "test_node".to_string(),
        node_type: NodeType::Function,
        label: "test_function".to_string(),
        metadata: HashMap::new(),
        position: None,
        size: None,
        color: None,
    };

    assert_eq!(node.node_type, NodeType::Function);
    assert_eq!(node.label, "test_function");
}

#[test]
fn test_node_type_pattern_matching() {
    let node_type = NodeType::API;

    let description = match node_type {
        NodeType::Function => "A function or method",
        NodeType::Module => "A module or file",
        NodeType::Class => "A class or struct",
        NodeType::Variable => "A variable or parameter",
        NodeType::API => "An API endpoint or service",
    };

    assert_eq!(description, "An API endpoint or service");
}

#[test]
fn test_node_type_all_variants() {
    // Test that we can iterate through all variants
    let all_variants = [
        NodeType::Function,
        NodeType::Module,
        NodeType::Class,
        NodeType::Variable,
        NodeType::API,
    ];

    assert_eq!(all_variants.len(), 5);

    // Test that all variants are unique
    for (i, variant1) in all_variants.iter().enumerate() {
        for (j, variant2) in all_variants.iter().enumerate() {
            if i != j {
                assert_ne!(variant1, variant2);
            }
        }
    }
}

#[test]
fn test_edge_type_enum_variants() {
    // Test all EdgeType variants exist
    let calls = EdgeType::Calls;
    let imports = EdgeType::Imports;
    let inherits = EdgeType::Inherits;
    let composes = EdgeType::Composes;
    let transforms = EdgeType::Transforms;
    let uses = EdgeType::Uses;
    let depends = EdgeType::Depends;

    // Test debug formatting
    assert_eq!(format!("{:?}", calls), "Calls");
    assert_eq!(format!("{:?}", imports), "Imports");
    assert_eq!(format!("{:?}", inherits), "Inherits");
    assert_eq!(format!("{:?}", composes), "Composes");
    assert_eq!(format!("{:?}", transforms), "Transforms");
    assert_eq!(format!("{:?}", uses), "Uses");
    assert_eq!(format!("{:?}", depends), "Depends");
}

#[test]
fn test_edge_type_equality() {
    // Test equality between same variants
    assert_eq!(EdgeType::Calls, EdgeType::Calls);
    assert_eq!(EdgeType::Imports, EdgeType::Imports);
    assert_eq!(EdgeType::Inherits, EdgeType::Inherits);
    assert_eq!(EdgeType::Composes, EdgeType::Composes);
    assert_eq!(EdgeType::Transforms, EdgeType::Transforms);
    assert_eq!(EdgeType::Uses, EdgeType::Uses);
    assert_eq!(EdgeType::Depends, EdgeType::Depends);

    // Test inequality between different variants
    assert_ne!(EdgeType::Calls, EdgeType::Imports);
    assert_ne!(EdgeType::Imports, EdgeType::Inherits);
    assert_ne!(EdgeType::Inherits, EdgeType::Composes);
    assert_ne!(EdgeType::Composes, EdgeType::Transforms);
    assert_ne!(EdgeType::Transforms, EdgeType::Uses);
    assert_ne!(EdgeType::Uses, EdgeType::Depends);
    assert_ne!(EdgeType::Depends, EdgeType::Calls);
}

#[test]
fn test_edge_type_clone() {
    let original = EdgeType::Calls;
    let cloned = original; // EdgeType implements Copy, so no need for clone()
    assert_eq!(original, cloned);
}

#[test]
fn test_edge_type_copy() {
    let original = EdgeType::Imports;
    let copied = original; // This should work because EdgeType implements Copy
    assert_eq!(original, copied);
    assert_eq!(original, EdgeType::Imports); // original should still be valid
}

#[test]
fn test_edge_type_serialization() {
    let edge_types = vec![
        EdgeType::Calls,
        EdgeType::Imports,
        EdgeType::Inherits,
        EdgeType::Composes,
        EdgeType::Transforms,
        EdgeType::Uses,
        EdgeType::Depends,
    ];

    for edge_type in edge_types {
        // Test JSON serialization
        let json = serde_json::to_string(&edge_type).unwrap();
        let deserialized: EdgeType = serde_json::from_str(&json).unwrap();
        assert_eq!(edge_type, deserialized);

        // Test that serialized JSON contains expected strings
        match edge_type {
            EdgeType::Calls => assert!(json.contains("Calls")),
            EdgeType::Imports => assert!(json.contains("Imports")),
            EdgeType::Inherits => assert!(json.contains("Inherits")),
            EdgeType::Composes => assert!(json.contains("Composes")),
            EdgeType::Transforms => assert!(json.contains("Transforms")),
            EdgeType::Uses => assert!(json.contains("Uses")),
            EdgeType::Depends => assert!(json.contains("Depends")),
            EdgeType::RecursiveCall => assert!(json.contains("RecursiveCall")),
        }
    }
}

#[test]
fn test_edge_type_deserialization() {
    // Test deserialization from JSON strings
    let test_cases = vec![
        ("Calls", EdgeType::Calls),
        ("Imports", EdgeType::Imports),
        ("Inherits", EdgeType::Inherits),
        ("Composes", EdgeType::Composes),
        ("Transforms", EdgeType::Transforms),
        ("Uses", EdgeType::Uses),
        ("Depends", EdgeType::Depends),
    ];

    for (json_str, expected) in test_cases {
        let deserialized: EdgeType = serde_json::from_str(&format!("\"{}\"", json_str)).unwrap();
        assert_eq!(deserialized, expected);
    }
}

#[test]
fn test_edge_type_in_graph_edge() {
    // Test EdgeType usage in GraphEdge
    let edge = GraphEdge {
        id: "test_edge".to_string(),
        source: "node1".to_string(),
        target: "node2".to_string(),
        edge_type: EdgeType::Calls,
        weight: 1.0,
        metadata: HashMap::new(),
        label: None,
    };

    assert_eq!(edge.edge_type, EdgeType::Calls);
    assert_eq!(edge.id, "test_edge");
}

#[test]
fn test_edge_type_pattern_matching() {
    let edge_type = EdgeType::Transforms;

    let description = match edge_type {
        EdgeType::Calls => "Function calls another function",
        EdgeType::Imports => "Module imports another module",
        EdgeType::Inherits => "Class inherits from another class",
        EdgeType::Composes => "Component composes another component",
        EdgeType::Transforms => "Data transforms from one format to another",
        EdgeType::Uses => "Uses or references another entity",
        EdgeType::Depends => "Depends on another entity",
        EdgeType::RecursiveCall => "Recursive call (function calls itself directly or indirectly)",
    };

    assert_eq!(description, "Data transforms from one format to another");
}

#[test]
fn test_edge_type_all_variants() {
    // Test that we can iterate through all variants
    let all_variants = [
        EdgeType::Calls,
        EdgeType::Imports,
        EdgeType::Inherits,
        EdgeType::Composes,
        EdgeType::Transforms,
        EdgeType::Uses,
        EdgeType::Depends,
    ];

    assert_eq!(all_variants.len(), 7);

    // Test that all variants are unique
    for (i, variant1) in all_variants.iter().enumerate() {
        for (j, variant2) in all_variants.iter().enumerate() {
            if i != j {
                assert_ne!(variant1, variant2);
            }
        }
    }
}

#[test]
fn test_graph_type_variants() {
    // Test all GraphType variants
    assert_eq!(GraphType::Call, GraphType::Call);
    assert_eq!(GraphType::Dependency, GraphType::Dependency);
    assert_eq!(GraphType::DataFlow, GraphType::DataFlow);
    assert_eq!(GraphType::Component, GraphType::Component);

    assert_ne!(GraphType::Call, GraphType::Dependency);
    assert_ne!(GraphType::Call, GraphType::DataFlow);
    assert_ne!(GraphType::Call, GraphType::Component);
    assert_ne!(GraphType::Dependency, GraphType::DataFlow);
    assert_ne!(GraphType::Dependency, GraphType::Component);
    assert_ne!(GraphType::DataFlow, GraphType::Component);

    // Test serialization
    let call_json = serde_json::to_string(&GraphType::Call).unwrap();
    assert!(call_json.contains("Call"));

    let dep_json = serde_json::to_string(&GraphType::Dependency).unwrap();
    assert!(dep_json.contains("Dependency"));

    let flow_json = serde_json::to_string(&GraphType::DataFlow).unwrap();
    assert!(flow_json.contains("DataFlow"));

    let comp_json = serde_json::to_string(&GraphType::Component).unwrap();
    assert!(comp_json.contains("Component"));
}

#[test]
fn test_graph_node_creation() {
    let mut metadata = HashMap::new();
    metadata.insert(
        "file".to_string(),
        serde_json::Value::String("test.rs".to_string()),
    );
    metadata.insert("line".to_string(), serde_json::Value::Number(42.into()));

    let node = GraphNode {
        id: "node1".to_string(),
        node_type: NodeType::Function,
        label: "test_function".to_string(),
        metadata: metadata.clone(),
        position: Some((10.0, 20.0)),
        size: Some(5.0),
        color: Some("#FF0000".to_string()),
    };

    assert_eq!(node.id, "node1");
    assert_eq!(node.node_type, NodeType::Function);
    assert_eq!(node.label, "test_function");
    assert_eq!(node.metadata.len(), 2);
    assert_eq!(node.position, Some((10.0, 20.0)));
    assert_eq!(node.size, Some(5.0));
    assert_eq!(node.color, Some("#FF0000".to_string()));

    // Test clone
    let cloned = node.clone();
    assert_eq!(node.id, cloned.id);
    assert_eq!(node.node_type, cloned.node_type);
    assert_eq!(node.label, cloned.label);
    assert_eq!(node.metadata, cloned.metadata);
    assert_eq!(node.position, cloned.position);
    assert_eq!(node.size, cloned.size);
    assert_eq!(node.color, cloned.color);
}

#[test]
fn test_graph_edge_creation() {
    let mut metadata = HashMap::new();
    metadata.insert(
        "weight".to_string(),
        serde_json::Value::Number(serde_json::Number::from_f64(1.5).unwrap()),
    );
    metadata.insert(
        "frequency".to_string(),
        serde_json::Value::Number(10.into()),
    );

    let edge = GraphEdge {
        id: "edge1".to_string(),
        source: "node1".to_string(),
        target: "node2".to_string(),
        edge_type: EdgeType::Calls,
        metadata: metadata.clone(),
        weight: 1.5,
        label: Some("#00FF00".to_string()),
    };

    assert_eq!(edge.id, "edge1");
    assert_eq!(edge.source, "node1");
    assert_eq!(edge.target, "node2");
    assert_eq!(edge.edge_type, EdgeType::Calls);
    assert_eq!(edge.metadata.len(), 2);
    assert_eq!(edge.weight, 1.5);
    assert_eq!(edge.label, Some("#00FF00".to_string()));

    // Test clone
    let cloned = edge.clone();
    assert_eq!(edge.id, cloned.id);
    assert_eq!(edge.source, cloned.source);
    assert_eq!(edge.target, cloned.target);
    assert_eq!(edge.edge_type, cloned.edge_type);
    assert_eq!(edge.metadata, cloned.metadata);
    assert_eq!(edge.weight, cloned.weight);
    assert_eq!(edge.label, cloned.label);
}

#[test]
fn test_graph_statistics_calculation() {
    let mut graph = CorrelationGraph::new(GraphType::Call, "Test Graph".to_string());

    // Add nodes
    let node1 = GraphNode {
        id: "node1".to_string(),
        node_type: NodeType::Function,
        label: "Function 1".to_string(),
        metadata: HashMap::new(),
        position: None,
        size: None,
        color: None,
    };
    let node2 = GraphNode {
        id: "node2".to_string(),
        node_type: NodeType::Function,
        label: "Function 2".to_string(),
        metadata: HashMap::new(),
        position: None,
        size: None,
        color: None,
    };
    let node3 = GraphNode {
        id: "node3".to_string(),
        node_type: NodeType::Module,
        label: "Module 1".to_string(),
        metadata: HashMap::new(),
        position: None,
        size: None,
        color: None,
    };

    graph.add_node(node1).unwrap();
    graph.add_node(node2).unwrap();
    graph.add_node(node3).unwrap();

    // Add edges
    let edge1 = GraphEdge {
        id: "edge1".to_string(),
        source: "node1".to_string(),
        target: "node2".to_string(),
        edge_type: EdgeType::Calls,
        metadata: HashMap::new(),
        weight: 1.0,
        label: None,
    };
    let edge2 = GraphEdge {
        id: "edge2".to_string(),
        source: "node2".to_string(),
        target: "node3".to_string(),
        edge_type: EdgeType::Imports,
        metadata: HashMap::new(),
        weight: 1.0,
        label: None,
    };

    graph.add_edge(edge1).unwrap();
    graph.add_edge(edge2).unwrap();

    let stats = graph.statistics();
    assert_eq!(stats.node_count, 3);
    assert_eq!(stats.edge_count, 2);
}

#[test]
fn test_graph_serialization() {
    let mut graph = CorrelationGraph::new(GraphType::Call, "Test Graph".to_string());

    // Add test data
    let node1 = GraphNode {
        id: "node1".to_string(),
        node_type: NodeType::Function,
        label: "Function 1".to_string(),
        metadata: HashMap::new(),
        position: None,
        size: None,
        color: None,
    };
    let node2 = GraphNode {
        id: "node2".to_string(),
        node_type: NodeType::Function,
        label: "Function 2".to_string(),
        metadata: HashMap::new(),
        position: None,
        size: None,
        color: None,
    };
    let edge1 = GraphEdge {
        id: "edge1".to_string(),
        source: "node1".to_string(),
        target: "node2".to_string(),
        edge_type: EdgeType::Calls,
        metadata: HashMap::new(),
        weight: 1.0,
        label: None,
    };

    graph.add_node(node1).unwrap();
    graph.add_node(node2).unwrap();
    graph.add_edge(edge1).unwrap();

    // Test JSON serialization
    let json = graph.to_json().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert!(parsed.is_object());
    assert!(parsed.get("nodes").is_some());
    assert!(parsed.get("edges").is_some());

    let nodes = parsed.get("nodes").unwrap().as_array().unwrap();
    let edges = parsed.get("edges").unwrap().as_array().unwrap();

    assert_eq!(nodes.len(), 2);
    assert_eq!(edges.len(), 1);
}

#[test]
fn test_graph_node_metadata() {
    let mut metadata = HashMap::new();
    metadata.insert(
        "complexity".to_string(),
        serde_json::Value::Number(5.into()),
    );
    metadata.insert("lines".to_string(), serde_json::Value::Number(100.into()));
    metadata.insert("is_public".to_string(), serde_json::Value::Bool(true));

    let node = GraphNode {
        id: "complex_node".to_string(),
        node_type: NodeType::Function,
        label: "complex_function".to_string(),
        metadata,
        position: None,
        size: None,
        color: None,
    };

    assert_eq!(
        node.metadata.get("complexity").unwrap().as_i64().unwrap(),
        5
    );
    assert_eq!(node.metadata.get("lines").unwrap().as_i64().unwrap(), 100);
    assert!(node.metadata.get("is_public").unwrap().as_bool().unwrap());
}

#[test]
fn test_graph_edge_metadata() {
    let mut metadata = HashMap::new();
    metadata.insert(
        "call_count".to_string(),
        serde_json::Value::Number(50.into()),
    );
    metadata.insert(
        "avg_duration".to_string(),
        serde_json::Value::Number(serde_json::Number::from_f64(0.5).unwrap()),
    );
    metadata.insert("is_async".to_string(), serde_json::Value::Bool(false));

    let edge = GraphEdge {
        id: "frequent_call".to_string(),
        source: "caller".to_string(),
        target: "callee".to_string(),
        edge_type: EdgeType::Calls,
        metadata,
        weight: 50.0,
        label: None,
    };

    assert_eq!(
        edge.metadata.get("call_count").unwrap().as_i64().unwrap(),
        50
    );
    assert_eq!(
        edge.metadata.get("avg_duration").unwrap().as_f64().unwrap(),
        0.5
    );
    assert!(!edge.metadata.get("is_async").unwrap().as_bool().unwrap());
}

#[test]
fn test_graph_manager_operations() {
    let manager = GraphCorrelationManager::new();

    // Test available graph types
    let graph_types = manager.available_graph_types();
    assert!(!graph_types.is_empty());
    assert!(graph_types.contains(&GraphType::Call));
    assert!(graph_types.contains(&GraphType::Dependency));

    // Test building graphs
    let source_data = GraphSourceData::new();
    let call_graph = manager.build_graph(GraphType::Call, &source_data);
    assert!(call_graph.is_ok());

    let dep_graph = manager.build_graph(GraphType::Dependency, &source_data);
    assert!(dep_graph.is_ok());
}

#[test]
fn test_graph_visualization_properties() {
    let mut graph = CorrelationGraph::new(GraphType::Call, "Test Graph".to_string());

    // Add nodes with visualization properties
    let mut node1_metadata = HashMap::new();
    node1_metadata.insert(
        "importance".to_string(),
        serde_json::Value::String("high".to_string()),
    );

    let node = GraphNode {
        id: "important_func".to_string(),
        node_type: NodeType::Function,
        label: "Important Function".to_string(),
        metadata: node1_metadata,
        position: Some((10.0, 20.0)),
        size: Some(5.0),
        color: Some("#FF0000".to_string()),
    };
    graph.add_node(node).unwrap();

    // Add edge with weight
    let mut edge_metadata = HashMap::new();
    edge_metadata.insert(
        "frequency".to_string(),
        serde_json::Value::Number(100.into()),
    );

    // Add the target node first
    let other_node = GraphNode {
        id: "other_func".to_string(),
        node_type: NodeType::Function,
        label: "Other Function".to_string(),
        metadata: HashMap::new(),
        position: None,
        size: None,
        color: None,
    };
    graph.add_node(other_node).unwrap();

    let edge = GraphEdge {
        id: "frequent_call".to_string(),
        source: "important_func".to_string(),
        target: "other_func".to_string(),
        edge_type: EdgeType::Calls,
        metadata: edge_metadata,
        weight: 1.0,
        label: Some("frequent".to_string()),
    };
    graph.add_edge(edge).unwrap();

    // Test that visualization properties are preserved
    let important_node = graph.get_node("important_func").unwrap();
    assert_eq!(important_node.label, "Important Function");
    assert_eq!(important_node.node_type, NodeType::Function);

    let frequent_edge = graph.get_edge("frequent_call").unwrap();
    assert_eq!(frequent_edge.edge_type, EdgeType::Calls);
    assert_eq!(frequent_edge.source, "important_func");
    assert_eq!(frequent_edge.target, "other_func");
}

#[test]
fn test_graph_error_handling() {
    let mut graph = CorrelationGraph::new(GraphType::Call, "Test Graph".to_string());

    // Test adding duplicate node
    let node1 = GraphNode {
        id: "node1".to_string(),
        node_type: NodeType::Function,
        label: "Function 1".to_string(),
        metadata: HashMap::new(),
        position: None,
        size: None,
        color: None,
    };
    graph.add_node(node1).unwrap();

    let node1_dup = GraphNode {
        id: "node1".to_string(),
        node_type: NodeType::Function,
        label: "Function 1".to_string(),
        metadata: HashMap::new(),
        position: None,
        size: None,
        color: None,
    };
    let result = graph.add_node(node1_dup);
    assert!(result.is_err());

    // Test adding edge with non-existent nodes
    let edge = GraphEdge {
        id: "edge1".to_string(),
        source: "nonexistent1".to_string(),
        target: "nonexistent2".to_string(),
        edge_type: EdgeType::Calls,
        metadata: HashMap::new(),
        weight: 1.0,
        label: None,
    };
    let result = graph.add_edge(edge);
    assert!(result.is_err());
}

#[test]
fn test_graph_clear_operations() {
    let mut graph = CorrelationGraph::new(GraphType::Call, "Test Graph".to_string());

    // Add some data
    let node1 = GraphNode {
        id: "node1".to_string(),
        node_type: NodeType::Function,
        label: "Function 1".to_string(),
        metadata: HashMap::new(),
        position: None,
        size: None,
        color: None,
    };
    let node2 = GraphNode {
        id: "node2".to_string(),
        node_type: NodeType::Function,
        label: "Function 2".to_string(),
        metadata: HashMap::new(),
        position: None,
        size: None,
        color: None,
    };
    graph.add_node(node1).unwrap();
    graph.add_node(node2).unwrap();

    let edge = GraphEdge {
        id: "edge1".to_string(),
        source: "node1".to_string(),
        target: "node2".to_string(),
        edge_type: EdgeType::Calls,
        metadata: HashMap::new(),
        weight: 1.0,
        label: None,
    };
    graph.add_edge(edge).unwrap();

    // Verify data exists
    let stats = graph.statistics();
    assert_eq!(stats.node_count, 2);
    assert_eq!(stats.edge_count, 1);

    // Clear and verify
    graph.nodes.clear();
    graph.edges.clear();
    let stats_after_clear = graph.statistics();
    assert_eq!(stats_after_clear.node_count, 0);
    assert_eq!(stats_after_clear.edge_count, 0);

    // Should be able to add new data after clear
    let node3 = GraphNode {
        id: "node3".to_string(),
        node_type: NodeType::Function,
        label: "Function 3".to_string(),
        metadata: HashMap::new(),
        position: None,
        size: None,
        color: None,
    };
    graph.add_node(node3).unwrap();
    let stats_final = graph.statistics();
    assert_eq!(stats_final.node_count, 1);
}

// VectorizerGraphExtractor tests
#[test]
fn test_vectorizer_graph_extractor_new() {
    let extractor = VectorizerGraphExtractor::new();
    assert!(extractor.mcp_client.is_none());
    assert!(extractor.query_cache.is_empty());
    assert_eq!(extractor.config.max_results, 1000);
    assert_eq!(extractor.config.similarity_threshold, 0.7);
    assert!(extractor.config.enable_caching);
}

#[test]
fn test_vectorizer_graph_extractor_with_config() {
    let config = VectorizerExtractorConfig {
        max_results: 500,
        similarity_threshold: 0.8,
        enable_caching: false,
        cache_ttl_seconds: 1800,
        collections: VectorizerCollections {
            functions: "custom_functions".to_string(),
            imports: "custom_imports".to_string(),
            calls: "custom_calls".to_string(),
            types: "custom_types".to_string(),
            codebase: "custom_codebase".to_string(),
        },
    };

    let extractor = VectorizerGraphExtractor::with_config(config.clone());
    assert_eq!(extractor.config.max_results, 500);
    assert_eq!(extractor.config.similarity_threshold, 0.8);
    assert!(!extractor.config.enable_caching);
    assert_eq!(extractor.config.cache_ttl_seconds, 1800);
    assert_eq!(extractor.config.collections.functions, "custom_functions");
}

#[test]
fn test_vectorizer_extractor_config_default() {
    let config = VectorizerExtractorConfig::default();
    assert_eq!(config.max_results, 1000);
    assert_eq!(config.similarity_threshold, 0.7);
    assert!(config.enable_caching);
    assert_eq!(config.cache_ttl_seconds, 3600);
    assert_eq!(config.collections.functions, "functions");
    assert_eq!(config.collections.imports, "imports");
    assert_eq!(config.collections.calls, "calls");
    assert_eq!(config.collections.types, "types");
    assert_eq!(config.collections.codebase, "codebase");
}

#[test]
fn test_vectorizer_collections_default() {
    let collections = VectorizerCollections::default();
    assert_eq!(collections.functions, "functions");
    assert_eq!(collections.imports, "imports");
    assert_eq!(collections.calls, "calls");
    assert_eq!(collections.types, "types");
    assert_eq!(collections.codebase, "codebase");
}

#[test]
fn test_set_mcp_client() {
    let mut extractor = VectorizerGraphExtractor::new();
    assert!(extractor.mcp_client.is_none());

    let client = serde_json::json!({"test": "client"});
    extractor.set_mcp_client(client.clone());
    assert!(extractor.mcp_client.is_some());
    assert_eq!(extractor.mcp_client.unwrap(), client);
}

#[test]
fn test_create_function_node() {
    let extractor = VectorizerGraphExtractor::new();

    let func_data = serde_json::json!({
        "id": "func_123",
        "name": "test_function",
        "signature": "fn test_function() -> i32",
        "file": "src/main.rs"
    });

    let node = extractor.create_function_node(func_data).unwrap();
    assert_eq!(node.id, "func_123");
    assert_eq!(node.label, "test_function");
    assert_eq!(node.node_type, NodeType::Function);
    assert_eq!(
        node.metadata.get("signature").unwrap().as_str().unwrap(),
        "fn test_function() -> i32"
    );
    assert_eq!(
        node.metadata.get("file").unwrap().as_str().unwrap(),
        "src/main.rs"
    );
}

#[test]
fn test_create_function_node_minimal() {
    let extractor = VectorizerGraphExtractor::new();

    let func_data = serde_json::json!({});

    let node = extractor.create_function_node(func_data).unwrap();
    assert_eq!(node.id, "unknown");
    assert_eq!(node.label, "Unknown Function");
    assert_eq!(node.node_type, NodeType::Function);
    assert!(node.metadata.is_empty());
}

#[test]
fn test_create_call_edge() {
    let extractor = VectorizerGraphExtractor::new();

    let call_data = serde_json::json!({
        "id": "call_123",
        "caller": "function_a",
        "callee": "function_b",
        "frequency": 42
    });

    let edge = extractor.create_call_edge(call_data).unwrap();
    assert_eq!(edge.id, "call_123");
    assert_eq!(edge.source, "function_a");
    assert_eq!(edge.target, "function_b");
    assert_eq!(edge.edge_type, EdgeType::Calls);
    assert_eq!(edge.weight, 1.0);
    assert_eq!(
        edge.metadata.get("frequency").unwrap().as_number().unwrap(),
        &serde_json::Number::from(42)
    );
}

#[test]
fn test_create_call_edge_minimal() {
    let extractor = VectorizerGraphExtractor::new();

    let call_data = serde_json::json!({});

    let edge = extractor.create_call_edge(call_data).unwrap();
    assert_eq!(edge.id, "unknown");
    assert_eq!(edge.source, "unknown");
    assert_eq!(edge.target, "unknown");
    assert_eq!(edge.edge_type, EdgeType::Calls);
    assert_eq!(edge.weight, 1.0);
    assert!(edge.metadata.is_empty());
}

#[test]
fn test_create_import_relationship() {
    let extractor = VectorizerGraphExtractor::new();

    let import_data = serde_json::json!({
        "source": "module_a",
        "target": "module_b"
    });

    let (source_node, target_node, edge) =
        extractor.create_import_relationship(import_data).unwrap();

    assert_eq!(source_node.id, "module_a");
    assert_eq!(source_node.label, "module_a");
    assert_eq!(source_node.node_type, NodeType::Module);

    assert_eq!(target_node.id, "module_b");
    assert_eq!(target_node.label, "module_b");
    assert_eq!(target_node.node_type, NodeType::Module);

    assert_eq!(edge.id, "module_a->module_b");
    assert_eq!(edge.source, "module_a");
    assert_eq!(edge.target, "module_b");
    assert_eq!(edge.edge_type, EdgeType::Imports);
}

#[test]
fn test_create_variable_node() {
    let extractor = VectorizerGraphExtractor::new();

    let var_data = serde_json::json!({
        "id": "var_123",
        "name": "counter",
        "type": "i32"
    });

    let node = extractor.create_variable_node(var_data).unwrap();
    assert_eq!(node.id, "var_123");
    assert_eq!(node.label, "counter");
    assert_eq!(node.node_type, NodeType::Variable);
    assert_eq!(node.metadata.get("type").unwrap().as_str().unwrap(), "i32");
}

#[test]
fn test_create_transformation_edge() {
    let extractor = VectorizerGraphExtractor::new();

    let transform_data = serde_json::json!({
        "id": "transform_123",
        "source": "input_data",
        "target": "output_data"
    });

    let edge = extractor
        .create_transformation_edge(transform_data)
        .unwrap();
    assert_eq!(edge.id, "transform_123");
    assert_eq!(edge.source, "input_data");
    assert_eq!(edge.target, "output_data");
    assert_eq!(edge.edge_type, EdgeType::Transforms);
}

#[test]
fn test_create_class_node() {
    let extractor = VectorizerGraphExtractor::new();

    let class_data = serde_json::json!({
        "id": "class_123",
        "name": "MyClass",
        "base_class": "BaseClass"
    });

    let node = extractor.create_class_node(class_data).unwrap();
    assert_eq!(node.id, "class_123");
    assert_eq!(node.label, "MyClass");
    assert_eq!(node.node_type, NodeType::Class);
    assert_eq!(
        node.metadata.get("base_class").unwrap().as_str().unwrap(),
        "BaseClass"
    );
}

#[test]
fn test_create_interface_node() {
    let extractor = VectorizerGraphExtractor::new();

    let interface_data = serde_json::json!({
        "id": "interface_123",
        "name": "MyInterface"
    });

    let node = extractor.create_interface_node(interface_data).unwrap();
    assert_eq!(node.id, "interface_123");
    assert_eq!(node.label, "MyInterface");
    assert_eq!(node.node_type, NodeType::API);
}

#[test]
fn test_create_relationship_edge() {
    let extractor = VectorizerGraphExtractor::new();

    // Test inheritance
    let inherits_data = serde_json::json!({
        "id": "rel_123",
        "source": "ChildClass",
        "target": "ParentClass",
        "type": "inherits"
    });

    let edge = extractor.create_relationship_edge(inherits_data).unwrap();
    assert_eq!(edge.edge_type, EdgeType::Inherits);

    // Test implementation
    let implements_data = serde_json::json!({
        "id": "rel_456",
        "source": "MyClass",
        "target": "MyInterface",
        "type": "implements"
    });

    let edge = extractor.create_relationship_edge(implements_data).unwrap();
    assert_eq!(edge.edge_type, EdgeType::Composes);

    // Test uses
    let uses_data = serde_json::json!({
        "id": "rel_789",
        "source": "MyClass",
        "target": "OtherClass",
        "type": "uses"
    });

    let edge = extractor.create_relationship_edge(uses_data).unwrap();
    assert_eq!(edge.edge_type, EdgeType::Uses);

    // Test default (depends)
    let depends_data = serde_json::json!({
        "id": "rel_999",
        "source": "MyClass",
        "target": "OtherClass",
        "type": "unknown"
    });

    let edge = extractor.create_relationship_edge(depends_data).unwrap();
    assert_eq!(edge.edge_type, EdgeType::Depends);
}

#[test]
fn test_cache_operations() {
    let mut extractor = VectorizerGraphExtractor::new();

    // Test initial cache stats
    let (cache_entries, total_items) = extractor.cache_stats();
    assert_eq!(cache_entries, 0);
    assert_eq!(total_items, 0);

    // Test cache clear
    extractor.clear_cache();
    let (cache_entries, total_items) = extractor.cache_stats();
    assert_eq!(cache_entries, 0);
    assert_eq!(total_items, 0);
}

#[test]
fn test_vectorizer_graph_extractor_default() {
    let extractor = VectorizerGraphExtractor::default();
    assert!(extractor.mcp_client.is_none());
    assert!(extractor.query_cache.is_empty());
    assert_eq!(extractor.config.max_results, 1000);
}

#[tokio::test]
async fn test_extract_call_graph_empty() {
    let mut extractor = VectorizerGraphExtractor::new();

    // Test with empty query (should return empty graph)
    let graph = extractor.extract_call_graph("", None).await.unwrap();
    assert_eq!(graph.graph_type, GraphType::Call);
    assert_eq!(graph.name, "Call Graph");
    assert_eq!(graph.statistics().node_count, 0);
    assert_eq!(graph.statistics().edge_count, 0);
}

#[tokio::test]
async fn test_extract_dependency_graph_empty() {
    let mut extractor = VectorizerGraphExtractor::new();

    // Test with empty query (should return empty graph)
    let graph = extractor.extract_dependency_graph("").await.unwrap();
    assert_eq!(graph.graph_type, GraphType::Dependency);
    assert_eq!(graph.name, "Dependency Graph");
    assert_eq!(graph.statistics().node_count, 0);
    assert_eq!(graph.statistics().edge_count, 0);
}

#[tokio::test]
async fn test_extract_data_flow_graph_empty() {
    let mut extractor = VectorizerGraphExtractor::new();

    // Test with empty query (should return empty graph)
    let graph = extractor.extract_data_flow_graph("").await.unwrap();
    assert_eq!(graph.graph_type, GraphType::DataFlow);
    assert_eq!(graph.name, "Data Flow Graph");
    assert_eq!(graph.statistics().node_count, 0);
    assert_eq!(graph.statistics().edge_count, 0);
}

#[tokio::test]
async fn test_extract_component_graph_empty() {
    let mut extractor = VectorizerGraphExtractor::new();

    // Test with empty query (should return empty graph)
    let graph = extractor.extract_component_graph("").await.unwrap();
    assert_eq!(graph.graph_type, GraphType::Component);
    assert_eq!(graph.name, "Component Graph");
    assert_eq!(graph.statistics().node_count, 0);
    assert_eq!(graph.statistics().edge_count, 0);
}

#[test]
fn test_vectorizer_extractor_config_serialization() {
    let config = VectorizerExtractorConfig::default();
    let json = serde_json::to_string(&config).unwrap();
    let deserialized: VectorizerExtractorConfig = serde_json::from_str(&json).unwrap();

    assert_eq!(config.max_results, deserialized.max_results);
    assert_eq!(
        config.similarity_threshold,
        deserialized.similarity_threshold
    );
    assert_eq!(config.enable_caching, deserialized.enable_caching);
    assert_eq!(config.cache_ttl_seconds, deserialized.cache_ttl_seconds);
    assert_eq!(
        config.collections.functions,
        deserialized.collections.functions
    );
}

#[test]
fn test_vectorizer_collections_serialization() {
    let collections = VectorizerCollections::default();
    let json = serde_json::to_string(&collections).unwrap();
    let deserialized: VectorizerCollections = serde_json::from_str(&json).unwrap();

    assert_eq!(collections.functions, deserialized.functions);
    assert_eq!(collections.imports, deserialized.imports);
    assert_eq!(collections.calls, deserialized.calls);
    assert_eq!(collections.types, deserialized.types);
    assert_eq!(collections.codebase, deserialized.codebase);
}
