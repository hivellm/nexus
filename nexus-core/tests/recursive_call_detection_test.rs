//! Comprehensive tests for recursive call detection functionality

use nexus_core::graph_correlation::{
    CallGraphBuilder, CorrelationGraph, EdgeType, GraphBuilder, GraphEdge, GraphNode,
    GraphSourceData, NodeType, RecursionType, RecursiveCallConfig, RecursiveCallInfo,
};
use std::collections::HashMap;

/// Helper function to create a test call graph with recursive functions
fn create_recursive_test_graph() -> CorrelationGraph {
    let mut graph = CorrelationGraph::new(
        nexus_core::graph_correlation::GraphType::Call,
        "Test Recursive Graph".to_string(),
    );

    // Add function nodes
    let functions = vec![
        "factorial",
        "fibonacci",
        "gcd",
        "helper_func",
        "non_recursive_func",
        "mutual_a",
        "mutual_b",
    ];

    for func in functions {
        let node = GraphNode {
            id: format!("func:{}", func),
            node_type: NodeType::Function,
            label: func.to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };
        graph.add_node(node).unwrap();
    }

    // Add call edges to create different recursion patterns

    // Direct recursion: factorial calls itself
    let edge1 = GraphEdge {
        id: "edge:factorial->factorial".to_string(),
        source: "func:factorial".to_string(),
        target: "func:factorial".to_string(),
        edge_type: EdgeType::Calls,
        weight: 1.0,
        metadata: HashMap::new(),
        label: Some("calls".to_string()),
    };
    graph.add_edge(edge1).unwrap();

    // Indirect recursion: fibonacci calls itself through helper
    let edge2 = GraphEdge {
        id: "edge:fibonacci->fibonacci".to_string(),
        source: "func:fibonacci".to_string(),
        target: "func:fibonacci".to_string(),
        edge_type: EdgeType::Calls,
        weight: 1.0,
        metadata: HashMap::new(),
        label: Some("calls".to_string()),
    };
    graph.add_edge(edge2).unwrap();

    // Complex recursion: gcd calls helper, helper calls gcd
    let edge3 = GraphEdge {
        id: "edge:gcd->helper_func".to_string(),
        source: "func:gcd".to_string(),
        target: "func:helper_func".to_string(),
        edge_type: EdgeType::Calls,
        weight: 1.0,
        metadata: HashMap::new(),
        label: Some("calls".to_string()),
    };
    graph.add_edge(edge3).unwrap();

    let edge4 = GraphEdge {
        id: "edge:helper_func->gcd".to_string(),
        source: "func:helper_func".to_string(),
        target: "func:gcd".to_string(),
        edge_type: EdgeType::Calls,
        weight: 1.0,
        metadata: HashMap::new(),
        label: Some("calls".to_string()),
    };
    graph.add_edge(edge4).unwrap();

    // Mutual recursion: mutual_a calls mutual_b, mutual_b calls mutual_a
    let edge5 = GraphEdge {
        id: "edge:mutual_a->mutual_b".to_string(),
        source: "func:mutual_a".to_string(),
        target: "func:mutual_b".to_string(),
        edge_type: EdgeType::Calls,
        weight: 1.0,
        metadata: HashMap::new(),
        label: Some("calls".to_string()),
    };
    graph.add_edge(edge5).unwrap();

    let edge6 = GraphEdge {
        id: "edge:mutual_b->mutual_a".to_string(),
        source: "func:mutual_b".to_string(),
        target: "func:mutual_a".to_string(),
        edge_type: EdgeType::Calls,
        weight: 1.0,
        metadata: HashMap::new(),
        label: Some("calls".to_string()),
    };
    graph.add_edge(edge6).unwrap();

    // Non-recursive call
    let edge7 = GraphEdge {
        id: "edge:non_recursive_func->helper_func".to_string(),
        source: "func:non_recursive_func".to_string(),
        target: "func:helper_func".to_string(),
        edge_type: EdgeType::Calls,
        weight: 1.0,
        metadata: HashMap::new(),
        label: Some("calls".to_string()),
    };
    graph.add_edge(edge7).unwrap();

    graph
}

#[test]
fn test_direct_recursion_detection() {
    let graph = create_recursive_test_graph();
    let config = RecursiveCallConfig::default();

    let recursive_info = graph.detect_recursive_calls(&config).unwrap();

    // Check that factorial is detected as directly recursive
    let factorial_info = recursive_info.get("func:factorial").unwrap();
    assert!(factorial_info.is_recursive);
    assert!(factorial_info.direct_recursion);
    assert!(!factorial_info.indirect_recursion);
    assert_eq!(factorial_info.recursion_type, RecursionType::Direct);
    assert!(
        factorial_info
            .cycle_functions
            .contains(&"func:factorial".to_string())
    );
}

#[test]
fn test_indirect_recursion_detection() {
    let graph = create_recursive_test_graph();
    let config = RecursiveCallConfig::default();

    let recursive_info = graph.detect_recursive_calls(&config).unwrap();

    // Check that gcd and helper_func are detected as mutually recursive
    let gcd_info = recursive_info.get("func:gcd").unwrap();
    assert!(gcd_info.is_recursive);
    assert!(!gcd_info.direct_recursion);
    assert!(gcd_info.indirect_recursion);
    assert_eq!(gcd_info.recursion_type, RecursionType::Mutual);
    assert!(gcd_info.cycle_functions.contains(&"func:gcd".to_string()));
    assert!(
        gcd_info
            .cycle_functions
            .contains(&"func:helper_func".to_string())
    );

    let helper_info = recursive_info.get("func:helper_func").unwrap();
    assert!(helper_info.is_recursive);
    assert!(!helper_info.direct_recursion);
    assert!(helper_info.indirect_recursion);
    assert_eq!(helper_info.recursion_type, RecursionType::Mutual);
}

#[test]
fn test_mutual_recursion_detection() {
    let graph = create_recursive_test_graph();
    let config = RecursiveCallConfig::default();

    let recursive_info = graph.detect_recursive_calls(&config).unwrap();

    // Check that mutual_a and mutual_b are detected as mutually recursive
    let mutual_a_info = recursive_info.get("func:mutual_a").unwrap();
    assert!(mutual_a_info.is_recursive);
    assert!(!mutual_a_info.direct_recursion);
    assert!(mutual_a_info.indirect_recursion);
    assert_eq!(mutual_a_info.recursion_type, RecursionType::Mutual);

    let mutual_b_info = recursive_info.get("func:mutual_b").unwrap();
    assert!(mutual_b_info.is_recursive);
    assert!(!mutual_b_info.direct_recursion);
    assert!(mutual_b_info.indirect_recursion);
    assert_eq!(mutual_b_info.recursion_type, RecursionType::Mutual);
}

#[test]
fn test_non_recursive_function() {
    let graph = create_recursive_test_graph();
    let config = RecursiveCallConfig::default();

    let recursive_info = graph.detect_recursive_calls(&config).unwrap();

    // Check that non_recursive_func is not detected as recursive
    let non_recursive_info = recursive_info.get("func:non_recursive_func").unwrap();
    assert!(!non_recursive_info.is_recursive);
    assert!(!non_recursive_info.direct_recursion);
    assert!(!non_recursive_info.indirect_recursion);
    assert_eq!(non_recursive_info.recursion_type, RecursionType::None);
    assert!(non_recursive_info.cycle_functions.is_empty());
}

#[test]
fn test_apply_recursive_call_detection() {
    let mut graph = create_recursive_test_graph();
    let config = RecursiveCallConfig {
        include_recursion_metadata: true,
        mark_recursive_edges: true,
        ..Default::default()
    };

    graph.apply_recursive_call_detection(&config).unwrap();

    // Check that recursive functions have metadata
    let factorial_node = graph
        .nodes
        .iter()
        .find(|n| n.id == "func:factorial")
        .unwrap();
    assert!(factorial_node.metadata.contains_key("recursive_call_info"));
    assert!(factorial_node.metadata.contains_key("is_recursive"));
    assert_eq!(factorial_node.color, Some("#ff6b6b".to_string()));

    // Check that recursive edges are marked
    let recursive_edges: Vec<_> = graph
        .edges
        .iter()
        .filter(|e| e.edge_type == EdgeType::RecursiveCall)
        .collect();
    assert!(!recursive_edges.is_empty());

    for edge in recursive_edges {
        assert!(edge.metadata.contains_key("is_recursive_call"));
        assert_eq!(
            edge.metadata.get("color"),
            Some(&serde_json::Value::String("#ff6b6b".to_string()))
        );
    }
}

#[test]
fn test_recursive_call_statistics() {
    let mut graph = create_recursive_test_graph();
    let config = RecursiveCallConfig::default();

    graph.apply_recursive_call_detection(&config).unwrap();
    let stats = graph.get_recursive_call_statistics();

    assert!(stats.total_recursive_functions > 0);
    assert!(stats.direct_recursion_count > 0);
    assert!(stats.indirect_recursion_count > 0);
    assert!(stats.mutual_recursion_count > 0);
    assert!(stats.recursive_edges > 0);
    assert!(stats.recursion_percentage > 0.0);
    assert!(stats.max_recursion_depth > 0);
}

#[test]
fn test_recursive_call_config_options() {
    let graph = create_recursive_test_graph();

    // Test with disabled recursion detection
    let disabled_config = RecursiveCallConfig {
        max_search_depth: 0,
        detect_indirect: false,
        detect_mutual: false,
        include_recursion_metadata: false,
        mark_recursive_edges: false,
    };

    let recursive_info = graph.detect_recursive_calls(&disabled_config).unwrap();

    // All functions should be detected as non-recursive with disabled config
    for (_, info) in recursive_info {
        assert!(!info.is_recursive);
        assert_eq!(info.recursion_type, RecursionType::None);
    }
}

#[test]
fn test_call_graph_builder_with_recursion_detection() {
    let mut source_data = GraphSourceData::new();
    source_data.add_file(
        "test.rs".to_string(),
        "fn factorial(n: u32) -> u32 { if n <= 1 { 1 } else { n * factorial(n - 1) } }".to_string(),
    );
    source_data.add_functions("test.rs".to_string(), vec!["factorial".to_string()]);

    let builder = CallGraphBuilder::new("Test Graph".to_string()).enable_recursive_call_detection();

    let graph = builder.build(&source_data).unwrap();

    // Check that the graph has recursive call detection applied
    let factorial_node = graph.nodes.iter().find(|n| n.label == "factorial").unwrap();
    assert!(factorial_node.metadata.contains_key("recursive_call_info"));
}

#[test]
fn test_recursive_call_config_serialization() {
    let config = RecursiveCallConfig {
        max_search_depth: 15,
        detect_indirect: true,
        detect_mutual: true,
        include_recursion_metadata: true,
        mark_recursive_edges: true,
    };

    // Test JSON serialization
    let json = serde_json::to_string(&config).unwrap();
    assert!(json.contains("max_search_depth"));
    assert!(json.contains("detect_indirect"));
    assert!(json.contains("detect_mutual"));
    assert!(json.contains("include_recursion_metadata"));
    assert!(json.contains("mark_recursive_edges"));

    // Test deserialization
    let deserialized: RecursiveCallConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.max_search_depth, config.max_search_depth);
    assert_eq!(deserialized.detect_indirect, config.detect_indirect);
    assert_eq!(deserialized.detect_mutual, config.detect_mutual);
    assert_eq!(
        deserialized.include_recursion_metadata,
        config.include_recursion_metadata
    );
    assert_eq!(
        deserialized.mark_recursive_edges,
        config.mark_recursive_edges
    );
}

#[test]
fn test_recursive_call_info_serialization() {
    let info = RecursiveCallInfo {
        is_recursive: true,
        direct_recursion: true,
        indirect_recursion: false,
        max_depth: 5,
        cycle_functions: vec!["func:factorial".to_string()],
        recursion_type: RecursionType::Direct,
    };

    // Test JSON serialization
    let json = serde_json::to_string(&info).unwrap();
    assert!(json.contains("is_recursive"));
    assert!(json.contains("direct_recursion"));
    assert!(json.contains("max_depth"));
    assert!(json.contains("cycle_functions"));
    assert!(json.contains("recursion_type"));

    // Test deserialization
    let deserialized: RecursiveCallInfo = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.is_recursive, info.is_recursive);
    assert_eq!(deserialized.direct_recursion, info.direct_recursion);
    assert_eq!(deserialized.indirect_recursion, info.indirect_recursion);
    assert_eq!(deserialized.max_depth, info.max_depth);
    assert_eq!(deserialized.cycle_functions, info.cycle_functions);
    assert_eq!(deserialized.recursion_type, info.recursion_type);
}

#[test]
fn test_complex_recursion_scenario() {
    let mut graph = CorrelationGraph::new(
        nexus_core::graph_correlation::GraphType::Call,
        "Complex Recursion".to_string(),
    );

    // Create a complex recursion scenario: A -> B -> C -> A
    let functions = vec!["func_a", "func_b", "func_c"];
    for func in &functions {
        let node = GraphNode {
            id: format!("func:{}", func),
            node_type: NodeType::Function,
            label: func.to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };
        graph.add_node(node).unwrap();
    }

    // Add edges: A -> B -> C -> A
    let edges = [
        ("func:func_a", "func:func_b"),
        ("func:func_b", "func:func_c"),
        ("func:func_c", "func:func_a"),
    ];

    for (source, target) in edges.iter() {
        let edge = GraphEdge {
            id: format!("edge:{}->{}", source, target),
            source: source.to_string(),
            target: target.to_string(),
            edge_type: EdgeType::Calls,
            weight: 1.0,
            metadata: HashMap::new(),
            label: Some("calls".to_string()),
        };
        graph.add_edge(edge).unwrap();
    }

    let config = RecursiveCallConfig::default();
    let recursive_info = graph.detect_recursive_calls(&config).unwrap();

    // All functions should be detected as recursively calling each other
    for func in &functions {
        let info = recursive_info.get(&format!("func:{}", func)).unwrap();
        assert!(info.is_recursive);
        assert!(!info.direct_recursion);
        assert!(info.indirect_recursion);
        assert_eq!(info.recursion_type, RecursionType::Indirect);
        assert_eq!(info.cycle_functions.len(), 3);
    }
}

#[test]
fn test_max_depth_limit() {
    let mut graph = CorrelationGraph::new(
        nexus_core::graph_correlation::GraphType::Call,
        "Depth Limit Test".to_string(),
    );

    // Create a long chain: A -> B -> C -> D -> E -> F
    let functions = vec!["func_a", "func_b", "func_c", "func_d", "func_e", "func_f"];
    for func in &functions {
        let node = GraphNode {
            id: format!("func:{}", func),
            node_type: NodeType::Function,
            label: func.to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };
        graph.add_node(node).unwrap();
    }

    // Add edges: A -> B -> C -> D -> E -> F
    for i in 0..functions.len() - 1 {
        let edge = GraphEdge {
            id: format!("edge:{}->{}", functions[i], functions[i + 1]),
            source: format!("func:{}", functions[i]),
            target: format!("func:{}", functions[i + 1]),
            edge_type: EdgeType::Calls,
            weight: 1.0,
            metadata: HashMap::new(),
            label: Some("calls".to_string()),
        };
        graph.add_edge(edge).unwrap();
    }

    // Test with limited depth
    let config = RecursiveCallConfig {
        max_search_depth: 3,
        ..Default::default()
    };

    let recursive_info = graph.detect_recursive_calls(&config).unwrap();

    // No recursion should be detected due to depth limit
    for (_, info) in recursive_info {
        assert!(!info.is_recursive);
        assert_eq!(info.recursion_type, RecursionType::None);
    }
}
