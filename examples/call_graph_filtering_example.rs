//! Call Graph Filtering and Search Example
//!
//! This example demonstrates the comprehensive filtering and search capabilities
//! for call graphs, including node filtering, edge filtering, text search,
//! and path finding.

use nexus_core::graph::correlation::{
    CallGraphFilter, CallGraphSearch, CorrelationGraph, EdgeFilter, EdgeType, GraphEdge, GraphNode,
    GraphType, NodeFilter, NodeType, PathSearch, RecursiveCallConfig,
};
use std::collections::HashMap;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Call Graph Filtering and Search Example");
    println!("==========================================");

    // Create a sample call graph
    let graph = create_sample_call_graph()?;
    println!(
        "📊 Created sample call graph with {} nodes and {} edges",
        graph.nodes.len(),
        graph.edges.len()
    );

    // Demonstrate node filtering
    println!("\n🎯 Node Filtering Examples:");
    demonstrate_node_filtering(&graph)?;

    // Demonstrate edge filtering
    println!("\n🔗 Edge Filtering Examples:");
    demonstrate_edge_filtering(&graph)?;

    // Demonstrate text search
    println!("\n🔍 Text Search Examples:");
    demonstrate_text_search(&graph)?;

    // Demonstrate path finding
    println!("\n🛤️  Path Finding Examples:");
    demonstrate_path_finding(&graph)?;

    // Demonstrate advanced filtering
    println!("\n⚡ Advanced Filtering Examples:");
    demonstrate_advanced_filtering(&graph)?;

    // Demonstrate recursive call detection
    println!("\n🔄 Recursive Call Detection:");
    demonstrate_recursive_calls(&graph)?;

    println!("\n🎉 Example completed successfully!");
    Ok(())
}

fn create_sample_call_graph() -> Result<CorrelationGraph, Box<dyn std::error::Error>> {
    let mut graph = CorrelationGraph::new(GraphType::Call, "Sample Call Graph".to_string());

    // Add module nodes
    let modules = vec![
        ("main.rs", "Main application module"),
        ("auth.rs", "Authentication module"),
        ("database.rs", "Database operations module"),
        ("utils.rs", "Utility functions module"),
        ("api.rs", "API endpoints module"),
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
                (
                    "lines_of_code".to_string(),
                    serde_json::Value::Number(serde_json::Number::from(100 + (file.len() * 10))),
                ),
            ]),
            position: Some((0.0, 0.0)),
            size: Some(2.0),
            color: Some("#3498db".to_string()),
        };
        graph.add_node(node)?;
    }

    // Add function nodes with various characteristics
    let functions = vec![
        // Main module functions
        (
            "main",
            "file:main.rs",
            vec!["auth::login", "api::start_server"],
            "entry_point",
        ),
        (
            "init_app",
            "file:main.rs",
            vec!["database::connect", "utils::load_config"],
            "initialization",
        ),
        // Auth module functions
        (
            "login",
            "file:auth.rs",
            vec!["auth::validate_credentials", "database::get_user"],
            "authentication",
        ),
        (
            "validate_credentials",
            "file:auth.rs",
            vec!["utils::hash_password", "utils::verify_token"],
            "validation",
        ),
        (
            "logout",
            "file:auth.rs",
            vec!["utils::clear_session"],
            "session_management",
        ),
        (
            "refresh_token",
            "file:auth.rs",
            vec!["auth::refresh_token"],
            "token_management",
        ), // Recursive
        // Database module functions
        (
            "connect",
            "file:database.rs",
            vec!["database::init_connection", "database::test_connection"],
            "connection",
        ),
        ("init_connection", "file:database.rs", vec![], "setup"),
        ("test_connection", "file:database.rs", vec![], "testing"),
        (
            "get_user",
            "file:database.rs",
            vec!["database::query_user"],
            "query",
        ),
        ("query_user", "file:database.rs", vec![], "execution"),
        (
            "save_user",
            "file:database.rs",
            vec!["database::save_user"],
            "persistence",
        ), // Recursive
        // Utils module functions
        ("hash_password", "file:utils.rs", vec![], "crypto"),
        ("verify_token", "file:utils.rs", vec![], "crypto"),
        ("clear_session", "file:utils.rs", vec![], "session"),
        (
            "load_config",
            "file:utils.rs",
            vec!["utils::parse_config"],
            "configuration",
        ),
        ("parse_config", "file:utils.rs", vec![], "parsing"),
        (
            "factorial",
            "file:utils.rs",
            vec!["utils::factorial"],
            "math",
        ), // Recursive
        (
            "fibonacci",
            "file:utils.rs",
            vec!["utils::fibonacci"],
            "math",
        ), // Recursive
        // API module functions
        (
            "start_server",
            "file:api.rs",
            vec!["api::setup_routes", "api::bind_port"],
            "server",
        ),
        (
            "setup_routes",
            "file:api.rs",
            vec!["api::register_endpoints"],
            "routing",
        ),
        ("register_endpoints", "file:api.rs", vec![], "endpoints"),
        ("bind_port", "file:api.rs", vec![], "networking"),
    ];

    for (func_name, module_id, calls, category) in functions {
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
                (
                    "category".to_string(),
                    serde_json::Value::String(category.to_string()),
                ),
                (
                    "complexity".to_string(),
                    serde_json::Value::Number(serde_json::Number::from(calls.len() + 1)),
                ),
            ]),
            position: Some((0.0, 0.0)),
            size: Some(1.0),
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
            metadata: HashMap::from([(
                "relationship".to_string(),
                serde_json::Value::String("contains".to_string()),
            )]),
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
                    metadata: HashMap::from([(
                        "call_type".to_string(),
                        serde_json::Value::String("function_call".to_string()),
                    )]),
                    label: Some("calls".to_string()),
                };
                graph.add_edge(call_edge)?;
            }
        }
    }

    // Apply recursive call detection
    let config = RecursiveCallConfig {
        max_search_depth: 10,
        detect_indirect: true,
        detect_mutual: true,
        include_recursion_metadata: true,
        mark_recursive_edges: true,
    };
    graph.apply_recursive_call_detection(&config)?;

    Ok(graph)
}

fn demonstrate_node_filtering(graph: &CorrelationGraph) -> Result<(), Box<dyn std::error::Error>> {
    let filter = CallGraphFilter::new(graph.clone());

    // Filter by node type
    println!("  📋 Functions only:");
    let functions = filter.get_nodes_by_type(NodeType::Function);
    println!("    Found {} functions", functions.len());
    for func in functions.iter().take(3) {
        println!("      • {}", func.label);
    }

    // Filter by label contains
    println!("  🔍 Functions containing 'auth':");
    let auth_filter = NodeFilter {
        label_contains: Some(vec!["auth".to_string()]),
        ..Default::default()
    };
    let auth_functions = filter.filter_nodes(&auth_filter)?;
    for func in &auth_functions {
        println!("      • {} ({})", func.label, func.id);
    }

    // Filter by metadata
    println!("  🏷️  Functions with high complexity (>3):");
    let complexity_filter = NodeFilter {
        metadata: Some(HashMap::from([(
            "function_type".to_string(),
            serde_json::Value::String("function".to_string()),
        )])),
        ..Default::default()
    };
    let all_functions = filter.filter_nodes(&complexity_filter)?;
    let high_complexity: Vec<_> = all_functions
        .iter()
        .filter(|f| {
            if let Some(complexity) = f.metadata.get("complexity") {
                if let Some(num) = complexity.as_u64() {
                    num > 3
                } else {
                    false
                }
            } else {
                false
            }
        })
        .collect();
    for func in &high_complexity {
        println!(
            "      • {} (complexity: {})",
            func.label,
            func.metadata
                .get("complexity")
                .unwrap_or(&serde_json::Value::Null)
        );
    }

    // Filter by module
    println!("  📁 Functions in auth.rs:");
    let auth_module_functions = filter.get_nodes_by_module("auth.rs");
    for func in &auth_module_functions {
        println!("      • {}", func.label);
    }

    Ok(())
}

fn demonstrate_edge_filtering(graph: &CorrelationGraph) -> Result<(), Box<dyn std::error::Error>> {
    let filter = CallGraphFilter::new(graph.clone());

    // Filter by edge type
    println!("  📞 Call edges only:");
    let call_edges = filter.get_edges_by_type(EdgeType::Calls);
    println!("    Found {} call edges", call_edges.len());
    for edge in call_edges.iter().take(5) {
        println!(
            "      • {} -> {} ({})",
            edge.source.split(':').next_back().unwrap_or(&edge.source),
            edge.target.split(':').next_back().unwrap_or(&edge.target),
            edge.label.as_ref().unwrap_or(&"".to_string())
        );
    }

    // Filter by recursive calls
    println!("  🔄 Recursive call edges:");
    let recursive_edges = filter.get_recursive_calls();
    println!("    Found {} recursive calls", recursive_edges.len());
    for edge in &recursive_edges {
        println!(
            "      • {} -> {} (recursive)",
            edge.source.split(':').next_back().unwrap_or(&edge.source),
            edge.target.split(':').next_back().unwrap_or(&edge.target)
        );
    }

    // Filter by weight range
    println!("  ⚖️  Edges with weight > 0.5:");
    let weight_filter = EdgeFilter {
        weight_range: Some((0.5, 1.0)),
        ..Default::default()
    };
    let weighted_edges = filter.filter_edges(&weight_filter)?;
    println!("    Found {} edges with weight > 0.5", weighted_edges.len());

    Ok(())
}

fn demonstrate_text_search(graph: &CorrelationGraph) -> Result<(), Box<dyn std::error::Error>> {
    let filter = CallGraphFilter::new(graph.clone());

    // Search for "auth" in labels
    println!("  🔍 Searching for 'auth' in labels:");
    let search = CallGraphSearch {
        query: "auth".to_string(),
        search_labels: true,
        search_metadata: false,
        search_edge_labels: false,
        search_edge_metadata: false,
        ..Default::default()
    };
    let result = filter.search(&search)?;
    println!("    Found {} matches", result.total_matches);
    for node in &result.matching_nodes {
        println!("      • {} ({})", node.label, node.id);
    }

    // Search for "config" in metadata
    println!("  🔍 Searching for 'config' in metadata:");
    let search = CallGraphSearch {
        query: "config".to_string(),
        search_labels: false,
        search_metadata: true,
        search_edge_labels: false,
        search_edge_metadata: false,
        ..Default::default()
    };
    let result = filter.search(&search)?;
    println!("    Found {} matches", result.total_matches);
    for node in &result.matching_nodes {
        println!("      • {} ({})", node.label, node.id);
    }

    // Search for functions only
    println!("  🔍 Searching for 'user' in function names only:");
    let search = CallGraphSearch {
        query: "user".to_string(),
        search_labels: true,
        search_metadata: false,
        search_edge_labels: false,
        search_edge_metadata: false,
        function_names_only: true,
        ..Default::default()
    };
    let result = filter.search(&search)?;
    println!("    Found {} matches", result.total_matches);
    for node in &result.matching_nodes {
        println!("      • {} ({})", node.label, node.id);
    }

    Ok(())
}

fn demonstrate_path_finding(graph: &CorrelationGraph) -> Result<(), Box<dyn std::error::Error>> {
    let filter = CallGraphFilter::new(graph.clone());

    // Find paths from main to specific functions
    println!("  🛤️  Paths from main to database functions:");
    let path_search = PathSearch {
        start_node: Some("func:main.rs:main".to_string()),
        end_node: Some("func:database.rs:get_user".to_string()),
        max_length: Some(5),
        ..Default::default()
    };
    let paths = filter.find_paths(&path_search)?;
    println!("    Found {} paths", paths.len());
    for (i, path) in paths.iter().enumerate().take(3) {
        println!("      Path {}: {}", i + 1, path);
    }

    // Find all paths from a specific function
    println!("  🛤️  All paths from login function:");
    let path_search = PathSearch {
        start_node: Some("func:auth.rs:login".to_string()),
        max_length: Some(3),
        ..Default::default()
    };
    let paths = filter.find_paths(&path_search)?;
    println!("    Found {} paths", paths.len());
    for (i, path) in paths.iter().enumerate().take(5) {
        println!("      Path {}: {}", i + 1, path);
    }

    // Find call chains
    println!("  🔗 Call chain from main:");
    let call_chains = filter.get_call_chain("func:main.rs:main")?;
    println!("    Found {} call chains", call_chains.len());
    for (i, chain) in call_chains.iter().enumerate().take(3) {
        println!("      Chain {}: {}", i + 1, chain);
    }

    Ok(())
}

fn demonstrate_advanced_filtering(
    graph: &CorrelationGraph,
) -> Result<(), Box<dyn std::error::Error>> {
    let filter = CallGraphFilter::new(graph.clone());

    // Complex node filter
    println!("  ⚡ Complex node filtering (auth functions with high complexity):");
    let complex_filter = NodeFilter {
        node_types: Some(vec![NodeType::Function]),
        label_contains: Some(vec!["auth".to_string()]),
        metadata: Some(HashMap::from([(
            "function_type".to_string(),
            serde_json::Value::String("function".to_string()),
        )])),
        ..Default::default()
    };
    let auth_functions = filter.filter_nodes(&complex_filter)?;
    for func in &auth_functions {
        let complexity = func
            .metadata
            .get("complexity")
            .unwrap_or(&serde_json::Value::Null);
        println!("      • {} (complexity: {})", func.label, complexity);
    }

    // Complex edge filter
    println!("  ⚡ Complex edge filtering (call edges with specific metadata):");
    let edge_filter = EdgeFilter {
        edge_types: Some(vec![EdgeType::Calls]),
        metadata: Some(HashMap::from([(
            "call_type".to_string(),
            serde_json::Value::String("function_call".to_string()),
        )])),
        ..Default::default()
    };
    let call_edges = filter.filter_edges(&edge_filter)?;
    println!(
        "    Found {} call edges with specific metadata",
        call_edges.len()
    );

    // Connected nodes
    println!("  🔗 Nodes connected to main:");
    let connected = filter.get_connected_nodes("func:main.rs:main")?;
    for node in &connected {
        println!("      • {} ({:?})", node.label, node.node_type);
    }

    Ok(())
}

fn demonstrate_recursive_calls(graph: &CorrelationGraph) -> Result<(), Box<dyn std::error::Error>> {
    let filter = CallGraphFilter::new(graph.clone());

    // Get recursive call statistics
    let recursive_stats = graph.get_recursive_call_statistics();
    println!("  📊 Recursive Call Statistics:");
    println!(
        "    • Total recursive functions: {}",
        recursive_stats.total_recursive_functions
    );
    println!(
        "    • Direct recursion count: {}",
        recursive_stats.direct_recursion_count
    );
    println!(
        "    • Indirect recursion count: {}",
        recursive_stats.indirect_recursion_count
    );
    println!(
        "    • Mutual recursion count: {}",
        recursive_stats.mutual_recursion_count
    );
    println!("    • Recursive edges: {}", recursive_stats.recursive_edges);
    println!(
        "    • Recursion percentage: {:.1}%",
        recursive_stats.recursion_percentage
    );
    println!(
        "    • Max recursion depth: {}",
        recursive_stats.max_recursion_depth
    );

    // Find recursive functions
    println!("  🔄 Recursive functions found:");
    let recursive_edges = filter.get_recursive_calls();
    let mut recursive_functions = std::collections::HashSet::new();
    for edge in &recursive_edges {
        recursive_functions.insert(edge.source.split(':').next_back().unwrap_or(&edge.source));
    }
    for func in &recursive_functions {
        println!("      • {}", func);
    }

    Ok(())
}
