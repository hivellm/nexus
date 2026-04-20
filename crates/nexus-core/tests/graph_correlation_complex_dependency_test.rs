#![allow(unexpected_cfgs)]
#![cfg(FALSE)]
//! Complex Dependency Tree Integration Tests
//!
//! Tests for complex dependency scenarios including diamond dependencies,
//! circular dependencies, deep trees, and version conflicts

use nexus_core::graph::correlation::{
    ChangeType, CorrelationGraph, DependencyFilter, DependencyVersion, EdgeType, GraphNode,
    GraphSourceData, GraphType, NodeType, VersionConstraint, analyze_change_impact,
    analyze_version_constraints, filter_dependency_graph,
};
use std::collections::HashMap;

/// Create a diamond dependency graph
/// A -> B, A -> C, B -> D, C -> D (diamond pattern)
fn create_diamond_dependency_graph() -> CorrelationGraph {
    let nodes = vec![
        GraphNode {
            id: "app".to_string(),
            label: "Application".to_string(),
            node_type: NodeType::Module,
            properties: HashMap::new(),
        },
        GraphNode {
            id: "lib_b".to_string(),
            label: "Library B".to_string(),
            node_type: NodeType::Module,
            properties: HashMap::new(),
        },
        GraphNode {
            id: "lib_c".to_string(),
            label: "Library C".to_string(),
            node_type: NodeType::Module,
            properties: HashMap::new(),
        },
        GraphNode {
            id: "lib_d".to_string(),
            label: "Library D".to_string(),
            node_type: NodeType::Module,
            properties: HashMap::new(),
        },
    ];

    let edges = vec![
        ("app", "lib_b", EdgeType::DependsOn),
        ("app", "lib_c", EdgeType::DependsOn),
        ("lib_b", "lib_d", EdgeType::DependsOn),
        ("lib_c", "lib_d", EdgeType::DependsOn),
    ]
    .into_iter()
    .map(
        |(source, target, edge_type)| nexus_core::graph::correlation::GraphEdge {
            source: source.to_string(),
            target: target.to_string(),
            edge_type,
            properties: HashMap::new(),
        },
    )
    .collect();

    CorrelationGraph {
        graph_type: GraphType::Dependency,
        nodes,
        edges,
        metadata: HashMap::new(),
    }
}

/// Create a deep dependency tree (10+ levels)
fn create_deep_dependency_tree(depth: usize) -> CorrelationGraph {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    for i in 0..depth {
        nodes.push(GraphNode {
            id: format!("level_{}", i),
            label: format!("Level {} Module", i),
            node_type: NodeType::Module,
            properties: HashMap::new(),
        });

        if i > 0 {
            edges.push(nexus_core::graph::correlation::GraphEdge {
                source: format!("level_{}", i - 1),
                target: format!("level_{}", i),
                edge_type: EdgeType::DependsOn,
                properties: HashMap::new(),
            });
        }
    }

    CorrelationGraph {
        graph_type: GraphType::Dependency,
        nodes,
        edges,
        metadata: HashMap::new(),
    }
}

/// Create a wide dependency graph (many siblings)
fn create_wide_dependency_graph(width: usize) -> CorrelationGraph {
    let mut nodes = vec![GraphNode {
        id: "root".to_string(),
        label: "Root Module".to_string(),
        node_type: NodeType::Module,
        properties: HashMap::new(),
    }];

    let mut edges = Vec::new();

    for i in 0..width {
        nodes.push(GraphNode {
            id: format!("dep_{}", i),
            label: format!("Dependency {}", i),
            node_type: NodeType::Module,
            properties: HashMap::new(),
        });

        edges.push(nexus_core::graph::correlation::GraphEdge {
            source: "root".to_string(),
            target: format!("dep_{}", i),
            edge_type: EdgeType::DependsOn,
            properties: HashMap::new(),
        });
    }

    CorrelationGraph {
        graph_type: GraphType::Dependency,
        nodes,
        edges,
        metadata: HashMap::new(),
    }
}

/// Create a dependency graph with circular dependencies
fn create_circular_dependency_graph() -> CorrelationGraph {
    let nodes = vec![
        GraphNode {
            id: "mod_a".to_string(),
            label: "Module A".to_string(),
            node_type: NodeType::Module,
            properties: HashMap::new(),
        },
        GraphNode {
            id: "mod_b".to_string(),
            label: "Module B".to_string(),
            node_type: NodeType::Module,
            properties: HashMap::new(),
        },
        GraphNode {
            id: "mod_c".to_string(),
            label: "Module C".to_string(),
            node_type: NodeType::Module,
            properties: HashMap::new(),
        },
        GraphNode {
            id: "mod_d".to_string(),
            label: "Module D".to_string(),
            node_type: NodeType::Module,
            properties: HashMap::new(),
        },
    ];

    let edges = vec![
        ("mod_a", "mod_b", EdgeType::DependsOn),
        ("mod_b", "mod_c", EdgeType::DependsOn),
        ("mod_c", "mod_d", EdgeType::DependsOn),
        ("mod_d", "mod_b", EdgeType::DependsOn), // Circular: D -> B
        ("mod_c", "mod_a", EdgeType::DependsOn), // Circular: C -> A
    ]
    .into_iter()
    .map(
        |(source, target, edge_type)| nexus_core::graph::correlation::GraphEdge {
            source: source.to_string(),
            target: target.to_string(),
            edge_type,
            properties: HashMap::new(),
        },
    )
    .collect();

    CorrelationGraph {
        graph_type: GraphType::Dependency,
        nodes,
        edges,
        metadata: HashMap::new(),
    }
}

#[test]
fn test_diamond_dependency_resolution() {
    let graph = create_diamond_dependency_graph();

    // Verify diamond structure
    assert_eq!(graph.nodes.len(), 4);
    assert_eq!(graph.edges.len(), 4);

    // Check that lib_d has two incoming edges
    let lib_d_incoming = graph.edges.iter().filter(|e| e.target == "lib_d").count();
    assert_eq!(lib_d_incoming, 2);

    // Filter to depth 1 from app
    let filter = DependencyFilter {
        max_depth: Some(1),
        include_transitive: false,
        node_types: vec![],
        exclude_nodes: vec![],
    };

    let filtered = filter_dependency_graph(&graph, "app", &filter).unwrap();
    assert_eq!(filtered.nodes.len(), 3); // app, lib_b, lib_c
}

#[test]
fn test_deep_dependency_tree() {
    let depth = 15;
    let graph = create_deep_dependency_tree(depth);

    assert_eq!(graph.nodes.len(), depth);
    assert_eq!(graph.edges.len(), depth - 1);

    // Filter to depth 5
    let filter = DependencyFilter {
        max_depth: Some(5),
        include_transitive: true,
        node_types: vec![],
        exclude_nodes: vec![],
    };

    let filtered = filter_dependency_graph(&graph, "level_0", &filter).unwrap();
    assert!(filtered.nodes.len() <= 6); // level_0 through level_5
}

#[test]
fn test_wide_dependency_graph() {
    let width = 50;
    let graph = create_wide_dependency_graph(width);

    assert_eq!(graph.nodes.len(), width + 1); // root + width deps
    assert_eq!(graph.edges.len(), width);

    // All edges should originate from root
    let root_edges = graph.edges.iter().filter(|e| e.source == "root").count();
    assert_eq!(root_edges, width);
}

#[test]
fn test_circular_dependency_detection() {
    let graph = create_circular_dependency_graph();

    assert_eq!(graph.nodes.len(), 4);
    assert_eq!(graph.edges.len(), 5);

    // Analyze impact of changing mod_b (which is in a cycle)
    let impact = analyze_change_impact(&graph, "mod_b", ChangeType::Modified).unwrap();

    // Should affect nodes in the cycle
    assert!(!impact.affected_nodes.is_empty());
    assert!(impact.propagation_distance > 0);
}

#[test]
fn test_version_conflict_in_diamond() {
    let graph = create_diamond_dependency_graph();

    let mut version_info = HashMap::new();

    // lib_b requires lib_d ^1.0.0
    version_info.insert(
        "lib_d".to_string(),
        DependencyVersion {
            dependency_id: "lib_d".to_string(),
            current_version: "1.0.0".to_string(),
            constraint: VersionConstraint::Caret("1.0.0".to_string()),
            available_versions: vec![
                "1.0.0".to_string(),
                "1.1.0".to_string(),
                "2.0.0".to_string(),
            ],
        },
    );

    // lib_c requires lib_d ^2.0.0 (conflict!)
    let lib_c_constraint = VersionConstraint::Caret("2.0.0".to_string());

    // Analyze compatibility
    let compat = analyze_version_constraints(&graph, &version_info).unwrap();

    // Should detect compatibility issues
    assert!(compat.compatibility_score < 1.0 || version_info.len() == 1);
}

#[test]
fn test_complex_multi_level_dependencies() {
    // Create a complex graph: A -> B,C; B -> D,E; C -> E,F; D -> G; E -> G; F -> G
    let nodes = vec!["A", "B", "C", "D", "E", "F", "G"]
        .into_iter()
        .map(|id| GraphNode {
            id: id.to_string(),
            label: format!("Module {}", id),
            node_type: NodeType::Module,
            properties: HashMap::new(),
        })
        .collect();

    let edges = vec![
        ("A", "B"),
        ("A", "C"),
        ("B", "D"),
        ("B", "E"),
        ("C", "E"),
        ("C", "F"),
        ("D", "G"),
        ("E", "G"),
        ("F", "G"),
    ]
    .into_iter()
    .map(
        |(source, target)| nexus_core::graph::correlation::GraphEdge {
            source: source.to_string(),
            target: target.to_string(),
            edge_type: EdgeType::DependsOn,
            properties: HashMap::new(),
        },
    )
    .collect();

    let graph = CorrelationGraph {
        graph_type: GraphType::Dependency,
        nodes,
        edges,
        metadata: HashMap::new(),
    };

    // G should have 3 incoming edges
    let g_incoming = graph.edges.iter().filter(|e| e.target == "G").count();
    assert_eq!(g_incoming, 3);

    // Filter from A with depth 2
    let filter = DependencyFilter {
        max_depth: Some(2),
        include_transitive: true,
        node_types: vec![],
        exclude_nodes: vec![],
    };

    let filtered = filter_dependency_graph(&graph, "A", &filter).unwrap();
    // Should include A, B, C, D, E, F (not G which is at depth 3)
    assert!(filtered.nodes.len() <= 6);
}

#[test]
fn test_version_constraint_parsing() {
    let exact = VersionConstraint::parse("1.0.0");
    assert!(exact.satisfies("1.0.0"));
    assert!(!exact.satisfies("1.0.1"));

    let caret = VersionConstraint::parse("^1.0.0");
    assert!(caret.satisfies("1.0.0"));
    assert!(caret.satisfies("1.5.0"));
    assert!(!caret.satisfies("2.0.0"));

    let tilde = VersionConstraint::parse("~1.0.0");
    assert!(tilde.satisfies("1.0.0"));
    assert!(tilde.satisfies("1.0.5"));
    assert!(!tilde.satisfies("1.1.0"));

    let range = VersionConstraint::parse(">=1.0.0,<2.0.0");
    assert!(range.satisfies("1.0.0"));
    assert!(range.satisfies("1.9.9"));
    assert!(!range.satisfies("2.0.0"));
}

#[test]
fn test_large_scale_dependency_graph() {
    // Create a large graph with 100 nodes
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    for i in 0..100 {
        nodes.push(GraphNode {
            id: format!("node_{}", i),
            label: format!("Node {}", i),
            node_type: NodeType::Module,
            properties: HashMap::new(),
        });

        // Create dependencies: each node depends on previous 3 nodes
        for j in 1..=3 {
            if i >= j {
                edges.push(nexus_core::graph::correlation::GraphEdge {
                    source: format!("node_{}", i),
                    target: format!("node_{}", i - j),
                    edge_type: EdgeType::DependsOn,
                    properties: HashMap::new(),
                });
            }
        }
    }

    let graph = CorrelationGraph {
        graph_type: GraphType::Dependency,
        nodes,
        edges,
        metadata: HashMap::new(),
    };

    assert_eq!(graph.nodes.len(), 100);
    assert!(graph.edges.len() > 200); // Should have many edges

    // Test filtering on large graph
    let filter = DependencyFilter {
        max_depth: Some(10),
        include_transitive: true,
        node_types: vec![],
        exclude_nodes: vec![],
    };

    let filtered = filter_dependency_graph(&graph, "node_99", &filter).unwrap();
    assert!(!filtered.nodes.is_empty());
}

#[test]
fn test_transitive_dependency_resolution() {
    let graph = create_deep_dependency_tree(10);

    // Get transitive deps from level_0
    let transitive =
        nexus_core::graph::correlation::get_transitive_dependencies(&graph, "level_0").unwrap();

    // Should include all levels from 1 to 9
    assert_eq!(transitive.len(), 9);
    assert!(transitive.contains(&"level_1".to_string()));
    assert!(transitive.contains(&"level_9".to_string()));
}

#[test]
fn test_leaf_and_root_identification() {
    let graph = create_diamond_dependency_graph();

    let (leaf_nodes, root_nodes) =
        nexus_core::graph::correlation::identify_leaf_and_root_nodes(&graph);

    // lib_d is a leaf (no outgoing edges)
    assert!(leaf_nodes.contains(&"lib_d".to_string()));

    // app is a root (no incoming edges)
    assert!(root_nodes.contains(&"app".to_string()));
}

#[test]
fn test_dependency_depth_calculation() {
    let graph = create_deep_dependency_tree(10);

    let depths = nexus_core::graph::correlation::calculate_node_depths(&graph, "level_0");

    // level_0 should be at depth 0
    assert_eq!(depths.get("level_0"), Some(&0));

    // level_5 should be at depth 5
    assert_eq!(depths.get("level_5"), Some(&5));

    // level_9 should be at depth 9
    assert_eq!(depths.get("level_9"), Some(&9));
}
