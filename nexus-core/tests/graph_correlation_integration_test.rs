//! Integration tests for Graph Correlation Analysis
//!
//! These tests verify the complete graph correlation functionality:
//! - Graph generation from real code patterns
//! - Multiple graph types working together
//! - Performance with realistic codebase sizes
//! - Edge cases and error handling

use nexus_core::graph::correlation::{
    GraphCorrelationManager, GraphSourceData, GraphType, NodeType,
};

/// Helper function to create realistic Rust code source data
fn create_rust_project_data() -> GraphSourceData {
    let mut source_data = GraphSourceData::new();

    // main.rs - entry point
    source_data.add_file(
        "src/main.rs".to_string(),
        "mod utils;\nmod handlers;\nuse handlers::process;\nfn main() {\n    process();\n}"
            .to_string(),
    );
    source_data.add_functions("src/main.rs".to_string(), vec!["main".to_string()]);
    source_data.add_imports(
        "src/main.rs".to_string(),
        vec!["handlers".to_string(), "utils".to_string()],
    );

    // utils.rs - utility functions
    source_data.add_file(
        "src/utils.rs".to_string(),
        "pub fn validate_input(input: &str) -> bool {\n    !input.is_empty()\n}\npub fn format_output(s: &str) -> String {\n    s.to_uppercase()\n}".to_string(),
    );
    source_data.add_functions(
        "src/utils.rs".to_string(),
        vec!["validate_input".to_string(), "format_output".to_string()],
    );

    // handlers.rs - business logic
    source_data.add_file(
        "src/handlers.rs".to_string(),
        "use crate::utils::{validate_input, format_output};\npub fn process() {\n    let input = \"hello\";\n    if validate_input(input) {\n        let output = format_output(input);\n        println!(\"{}\", output);\n    }\n}".to_string(),
    );
    source_data.add_functions("src/handlers.rs".to_string(), vec!["process".to_string()]);
    source_data.add_imports(
        "src/handlers.rs".to_string(),
        vec!["validate_input".to_string(), "format_output".to_string()],
    );

    source_data
}

/// Test call graph generation with a realistic Rust project
#[test]
fn test_call_graph_integration() {
    let manager = GraphCorrelationManager::new();
    let source_data = create_rust_project_data();

    let graph = manager
        .build_graph(GraphType::Call, &source_data)
        .expect("Failed to build call graph");

    // Verify graph has nodes
    assert!(!graph.nodes.is_empty(), "Call graph should have nodes");

    // Verify we have relationships
    assert!(!graph.edges.is_empty(), "Call graph should have edges");

    // Verify main function is in the graph
    let has_main = graph
        .nodes
        .iter()
        .any(|n| n.label.contains("main") && n.node_type == NodeType::Function);
    assert!(has_main, "Call graph should include main function");

    println!(
        "Call graph integration test passed: {} nodes, {} edges",
        graph.nodes.len(),
        graph.edges.len()
    );
}

/// Test dependency graph generation
#[test]
fn test_dependency_graph_integration() {
    let manager = GraphCorrelationManager::new();
    let source_data = create_rust_project_data();

    let graph = manager
        .build_graph(GraphType::Dependency, &source_data)
        .expect("Failed to build dependency graph");

    // Verify graph has nodes
    assert!(
        !graph.nodes.is_empty(),
        "Dependency graph should have nodes"
    );

    // Verify module nodes
    let has_modules = graph.nodes.iter().any(|n| n.node_type == NodeType::Module);
    assert!(has_modules, "Dependency graph should have module nodes");

    println!(
        "Dependency graph integration test passed: {} nodes, {} edges",
        graph.nodes.len(),
        graph.edges.len()
    );
}

/// Test graph manager with multiple graph types
#[test]
fn test_multiple_graph_types() {
    let manager = GraphCorrelationManager::new();
    let source_data = create_rust_project_data();

    // Build call graph
    let call_graph = manager
        .build_graph(GraphType::Call, &source_data)
        .expect("Failed to build call graph");

    // Build dependency graph
    let dep_graph = manager
        .build_graph(GraphType::Dependency, &source_data)
        .expect("Failed to build dependency graph");

    // Both should succeed
    assert!(!call_graph.nodes.is_empty(), "Call graph should have nodes");
    assert!(
        !dep_graph.nodes.is_empty(),
        "Dependency graph should have nodes"
    );

    // They should be different graphs
    assert_ne!(
        call_graph.graph_type, dep_graph.graph_type,
        "Graphs should have different types"
    );
    assert_ne!(
        call_graph.nodes.len(),
        dep_graph.nodes.len(),
        "Different graphs should have different structures"
    );

    println!("Multiple graph types test passed");
}

/// Test graph with circular dependencies
#[test]
fn test_circular_dependencies() {
    let manager = GraphCorrelationManager::new();
    let mut source_data = GraphSourceData::new();

    // Create circular dependencies
    source_data.add_file(
        "src/a.rs".to_string(),
        "pub fn func_a() { func_b(); }".to_string(),
    );
    source_data.add_functions("src/a.rs".to_string(), vec!["func_a".to_string()]);
    source_data.add_imports("src/a.rs".to_string(), vec!["b".to_string()]);

    source_data.add_file(
        "src/b.rs".to_string(),
        "pub fn func_b() { func_c(); }".to_string(),
    );
    source_data.add_functions("src/b.rs".to_string(), vec!["func_b".to_string()]);
    source_data.add_imports("src/b.rs".to_string(), vec!["c".to_string()]);

    source_data.add_file(
        "src/c.rs".to_string(),
        "pub fn func_c() { func_a(); }".to_string(),
    );
    source_data.add_functions("src/c.rs".to_string(), vec!["func_c".to_string()]);
    source_data.add_imports("src/c.rs".to_string(), vec!["a".to_string()]);

    let graph = manager
        .build_graph(GraphType::Call, &source_data)
        .expect("Failed to build graph with circular dependencies");

    // Graph should be created even with circular dependencies
    assert!(
        !graph.nodes.is_empty(),
        "Graph should handle circular dependencies"
    );
    assert!(graph.nodes.len() >= 3, "Should have at least 3 functions");

    println!(
        "Circular dependencies test passed: {} nodes, {} edges",
        graph.nodes.len(),
        graph.edges.len()
    );
}

/// Test large-scale graph generation
#[test]
fn test_large_scale_graph() {
    let manager = GraphCorrelationManager::new();
    let mut source_data = GraphSourceData::new();

    // Create a large codebase with 100 files
    for i in 0..100 {
        let file_path = format!("src/module_{}.rs", i);
        let content = format!("pub fn func_{}() {{}}\npub fn helper_{}() {{}}", i, i);
        source_data.add_file(file_path.clone(), content);

        let functions = vec![format!("func_{}", i), format!("helper_{}", i)];
        source_data.add_functions(file_path, functions);

        if i > 0 {
            // Each module depends on the previous one
            source_data.add_imports(
                format!("src/module_{}.rs", i),
                vec![format!("module_{}", i - 1)],
            );
        }
    }

    let graph = manager
        .build_graph(GraphType::Dependency, &source_data)
        .expect("Failed to build large-scale graph");

    // Verify it can handle large codebases
    assert!(
        graph.nodes.len() >= 200,
        "Large graph should have many nodes"
    );
    assert!(
        graph.edges.len() >= 100,
        "Large graph should have many edges"
    );

    println!(
        "Large-scale graph test passed: {} nodes, {} edges",
        graph.nodes.len(),
        graph.edges.len()
    );
}

/// Test graph filtering functionality
#[test]
fn test_graph_filtering() {
    let manager = GraphCorrelationManager::new();
    let source_data = create_rust_project_data();

    let graph = manager
        .build_graph(GraphType::Call, &source_data)
        .expect("Failed to build graph for filtering");

    // Filter by function name
    let filtered_nodes: Vec<_> = graph
        .nodes
        .iter()
        .filter(|node| node.label.contains("process"))
        .collect();

    assert!(
        !filtered_nodes.is_empty(),
        "Should find nodes matching filter"
    );

    // Check node count
    let shallow_nodes_count = graph.nodes.len();

    assert!(shallow_nodes_count > 0, "Should find shallow nodes");

    println!(
        "Graph filtering test passed: {} filtered nodes found",
        filtered_nodes.len()
    );
}

/// Test error handling with empty source data
#[test]
fn test_empty_source_data() {
    let manager = GraphCorrelationManager::new();
    let source_data = GraphSourceData::new(); // Empty

    let result = manager.build_graph(GraphType::Call, &source_data);

    // Should either succeed with empty graph or return error
    match result {
        Ok(graph) => {
            assert_eq!(
                graph.nodes.len(),
                0,
                "Empty source should produce empty graph"
            );
            println!("Empty source data handled gracefully");
        }
        Err(e) => {
            // Error is acceptable for empty source
            assert!(e.to_string().contains("No source"));
            println!("Empty source data correctly rejected: {}", e);
        }
    }
}

/// Test graph statistics
#[test]
fn test_graph_statistics() {
    let manager = GraphCorrelationManager::new();
    let source_data = create_rust_project_data();

    let graph = manager
        .build_graph(GraphType::Call, &source_data)
        .expect("Failed to build graph");

    // Calculate basic statistics
    let node_count = graph.nodes.len();
    let edge_count = graph.edges.len();
    let function_count = graph
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Function)
        .count();
    let module_count = graph
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Module)
        .count();

    assert!(node_count > 0, "Should have nodes");
    assert!(edge_count > 0, "Should have edges");
    assert!(function_count > 0, "Should have functions");
    assert!(module_count > 0, "Module count should be positive");

    println!("Graph statistics test passed:");
    println!("  Total nodes: {}", node_count);
    println!("  Total edges: {}", edge_count);
    println!("  Functions: {}", function_count);
    println!("  Modules: {}", module_count);
}
