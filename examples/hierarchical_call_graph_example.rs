//! Hierarchical Call Graph Layout Example
//!
//! This example demonstrates how to use the hierarchical call graph layout
//! to visualize function call hierarchies in a more organized and readable way.

use nexus_core::graph::construction::LayoutDirection;
use nexus_core::graph::correlation::{
    CallGraphBuilder, CorrelationGraph, EdgeType, GraphBuilder, GraphEdge, GraphNode, GraphType,
    NodeType, hierarchical_layout::HierarchicalCallGraphConfig,
};
use std::collections::HashMap;
use tracing;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing::info!("🔗 Hierarchical Call Graph Layout Example");
    tracing::info!("==========================================");

    // Create a sample call graph
    let mut graph = create_sample_call_graph()?;

    tracing::info!("📊 Original Graph Statistics:");
    print_graph_statistics(&graph);

    // Apply hierarchical layout with default configuration
    tracing::info!("\n🎯 Applying hierarchical layout (default configuration)...");
    graph.apply_hierarchical_layout()?;

    tracing::info!("✅ Hierarchical layout applied successfully!");
    print_node_positions(&graph);

    // Create a new graph with custom hierarchical layout configuration
    tracing::info!("\n🎨 Creating graph with custom hierarchical layout...");
    let custom_config = HierarchicalCallGraphConfig {
        level_spacing: 150.0,
        node_spacing: 100.0,
        direction: LayoutDirection::LeftRight,
        group_by_module: true,
        show_call_flow: true,
        min_node_distance: 40.0,
        use_curved_edges: true,
        padding: 60.0,
    };

    let mut custom_graph = create_sample_call_graph()?;
    custom_graph.apply_hierarchical_layout_with_config(custom_config)?;

    tracing::info!("✅ Custom hierarchical layout applied!");
    print_node_positions(&custom_graph);

    // Demonstrate CallGraphBuilder with hierarchical layout and recursive call detection
    tracing::info!(
        "\n🏗️  Using CallGraphBuilder with hierarchical layout and recursive call detection..."
    );
    let builder = CallGraphBuilder::new_with_hierarchical_layout("Sample Call Graph".to_string())
        .enable_recursive_call_detection();
    let source_data = create_sample_source_data();
    let built_graph = builder.build(&source_data)?;

    tracing::info!("✅ Graph built with hierarchical layout and recursive call detection!");
    print_node_positions(&built_graph);

    // Show recursive call statistics
    let recursive_stats = built_graph.get_recursive_call_statistics();
    tracing::info!("\n📊 Recursive Call Statistics:");
    tracing::info!(
        "  • Total recursive functions: {}",
        recursive_stats.total_recursive_functions
    );
    tracing::info!(
        "  • Direct recursion count: {}",
        recursive_stats.direct_recursion_count
    );
    tracing::info!(
        "  • Indirect recursion count: {}",
        recursive_stats.indirect_recursion_count
    );
    tracing::info!(
        "  • Mutual recursion count: {}",
        recursive_stats.mutual_recursion_count
    );
    tracing::info!("  • Recursive edges: {}", recursive_stats.recursive_edges);
    tracing::info!(
        "  • Recursion percentage: {:.1}%",
        recursive_stats.recursion_percentage
    );
    tracing::info!(
        "  • Max recursion depth: {}",
        recursive_stats.max_recursion_depth
    );

    // Export the graph
    tracing::info!("\n📤 Exporting graph to JSON...");
    let json = graph.to_json()?;
    tracing::info!("✅ Graph exported to JSON ({} characters)", json.len());

    tracing::info!("\n🎉 Example completed successfully!");
    Ok(())
}

fn create_sample_call_graph() -> Result<CorrelationGraph, Box<dyn std::error::Error>> {
    let mut graph = CorrelationGraph::new(GraphType::Call, "Sample Call Graph".to_string());

    // Add module nodes
    let modules = vec![
        ("main.rs", "Main Module"),
        ("auth.rs", "Authentication Module"),
        ("database.rs", "Database Module"),
        ("utils.rs", "Utilities Module"),
    ];

    for (file, description) in modules {
        let node = GraphNode {
            id: format!("file:{}", file),
            node_type: NodeType::Module,
            label: file.to_string(),
            metadata: HashMap::from([
                (
                    "description".to_string(),
                    serde_json::Value::String(description.to_string()),
                ),
                (
                    "file_type".to_string(),
                    serde_json::Value::String("rust".to_string()),
                ),
            ]),
            position: None,
            size: None,
            color: Some("#3498db".to_string()),
        };
        graph.add_node(node)?;
    }

    // Add function nodes with call relationships (including some recursive functions)
    let functions = vec![
        (
            "main",
            "file:main.rs",
            vec!["auth::login", "database::connect", "utils::factorial"],
        ),
        (
            "login",
            "file:auth.rs",
            vec!["auth::validate_credentials", "database::get_user"],
        ),
        (
            "validate_credentials",
            "file:auth.rs",
            vec!["utils::hash_password"],
        ),
        (
            "connect",
            "file:database.rs",
            vec!["database::init_connection"],
        ),
        ("get_user", "file:database.rs", vec!["database::query_user"]),
        ("init_connection", "file:database.rs", vec![]),
        ("query_user", "file:database.rs", vec![]),
        ("hash_password", "file:utils.rs", vec![]),
        ("factorial", "file:utils.rs", vec!["utils::factorial"]), // Direct recursion
        ("fibonacci", "file:utils.rs", vec!["utils::fibonacci"]), // Direct recursion
        ("gcd", "file:utils.rs", vec!["utils::gcd_helper"]),      // Indirect recursion
        ("gcd_helper", "file:utils.rs", vec!["utils::gcd"]),      // Indirect recursion
    ];

    for (func_name, module_id, calls) in functions {
        let node = GraphNode {
            id: format!("func:{}:{}", module_id.replace("file:", ""), func_name),
            node_type: NodeType::Function,
            label: func_name.to_string(),
            metadata: HashMap::from([
                (
                    "module".to_string(),
                    serde_json::Value::String(module_id.replace("file:", "")),
                ),
                (
                    "function_type".to_string(),
                    serde_json::Value::String("function".to_string()),
                ),
            ]),
            position: None,
            size: None,
            color: Some("#e74c3c".to_string()),
        };
        let node_id = node.id.clone();
        graph.add_node(node)?;

        // Add edge from module to function
        let edge = GraphEdge {
            id: format!("edge:{}:{}", module_id, node_id),
            source: module_id.to_string(),
            target: node_id.clone(),
            edge_type: EdgeType::Uses,
            weight: 1.0,
            metadata: HashMap::new(),
            label: Some("contains".to_string()),
        };
        graph.add_edge(edge)?;

        // Add call relationships
        for call in calls {
            let call_id = format!("func:{}:{}", module_id.replace("file:", ""), call);
            if graph.nodes.iter().any(|n| n.id == call_id) {
                let call_edge = GraphEdge {
                    id: format!("call:{}:{}", node_id, call_id),
                    source: node_id.clone(),
                    target: call_id,
                    edge_type: EdgeType::Calls,
                    weight: 1.0,
                    metadata: HashMap::new(),
                    label: Some("calls".to_string()),
                };
                graph.add_edge(call_edge)?;
            }
        }
    }

    Ok(graph)
}

fn create_sample_source_data() -> nexus_core::graph::correlation::GraphSourceData {
    let mut source_data = nexus_core::graph::correlation::GraphSourceData::new();

    // Add files with recursive functions
    source_data.add_file(
        "main.rs".to_string(),
        "fn main() { auth::login(); database::connect(); utils::factorial(5); }".to_string(),
    );
    source_data.add_file(
        "auth.rs".to_string(),
        "fn login() { validate_credentials(); get_user(); }".to_string(),
    );
    source_data.add_file(
        "database.rs".to_string(),
        "fn connect() { init_connection(); }".to_string(),
    );
    source_data.add_file(
        "utils.rs".to_string(),
        r#"
        fn hash_password() { /* implementation */ }
        fn factorial(n: u32) -> u32 { if n <= 1 { 1 } else { n * factorial(n - 1) } }
        fn fibonacci(n: u32) -> u32 { if n <= 1 { n } else { fibonacci(n - 1) + fibonacci(n - 2) } }
        fn gcd(a: u32, b: u32) -> u32 { if b == 0 { a } else { gcd_helper(b, a % b) } }
        fn gcd_helper(a: u32, b: u32) -> u32 { if b == 0 { a } else { gcd(b, a % b) } }
    "#
        .to_string(),
    );

    // Add functions
    source_data.add_functions("main.rs".to_string(), vec!["main".to_string()]);
    source_data.add_functions(
        "auth.rs".to_string(),
        vec!["login".to_string(), "validate_credentials".to_string()],
    );
    source_data.add_functions(
        "database.rs".to_string(),
        vec!["connect".to_string(), "init_connection".to_string()],
    );
    source_data.add_functions(
        "utils.rs".to_string(),
        vec![
            "hash_password".to_string(),
            "factorial".to_string(),
            "fibonacci".to_string(),
            "gcd".to_string(),
            "gcd_helper".to_string(),
        ],
    );

    source_data
}

fn print_graph_statistics(graph: &CorrelationGraph) {
    let stats = graph.statistics();
    tracing::info!("  • Total nodes: {}", stats.node_count);
    tracing::info!("  • Total edges: {}", stats.edge_count);
    tracing::info!("  • Average degree: {:.2}", stats.avg_degree);
    tracing::info!("  • Max degree: {}", stats.max_degree);
    tracing::info!("  • Graph density: {:.2}", stats.graph_density);
}

fn print_node_positions(graph: &CorrelationGraph) {
    tracing::info!("  📍 Node Positions:");
    for node in &graph.nodes {
        if let Some((x, y)) = node.position {
            tracing::info!("    • {}: ({:.1}, {:.1})", node.label, x, y);
        } else {
            tracing::info!("    • {}: (not positioned)", node.label);
        }
    }
}
