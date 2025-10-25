//! Graph Construction Algorithms Example
//!
//! This example demonstrates how to use the various graph construction algorithms
//! provided by the nexus-core library.

use nexus_core::graph_construction::*;
use std::collections::HashMap;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Graph Construction Algorithms Example");
    println!("=====================================");

    // Create a sample graph
    let mut graph = create_sample_graph();
    println!(
        "Created sample graph with {} nodes and {} edges",
        graph.nodes.len(),
        graph.edges.len()
    );

    // Demonstrate Force-Directed Layout
    println!("\n1. Force-Directed Layout");
    println!("------------------------");
    let force_layout = ForceDirectedLayout::new()
        .with_iterations(500)
        .with_temperature(50.0)
        .with_spring_constant(0.2);

    let force_result = force_layout.layout(graph.clone())?;
    print_layout_info(&force_result, "Force-Directed");

    // Demonstrate Hierarchical Layout
    println!("\n2. Hierarchical Layout");
    println!("----------------------");
    let hierarchical_layout = HierarchicalLayout::new()
        .with_level_spacing(80.0)
        .with_node_spacing(60.0)
        .with_direction(LayoutDirection::TopDown);

    let hierarchical_result = hierarchical_layout.layout(graph.clone())?;
    print_layout_info(&hierarchical_result, "Hierarchical");

    // Demonstrate Circular Layout
    println!("\n3. Circular Layout");
    println!("------------------");
    let circular_layout = CircularLayout::new()
        .with_radius(150.0)
        .with_start_angle(0.0)
        .with_clockwise(true);

    let circular_result = circular_layout.layout(graph.clone())?;
    print_layout_info(&circular_result, "Circular");

    // Demonstrate Grid Layout
    println!("\n4. Grid Layout");
    println!("---------------");
    let grid_layout = GridLayout::new()
        .with_cell_size(120.0, 80.0)
        .with_padding(30.0);

    let grid_result = grid_layout.layout(graph.clone())?;
    print_layout_info(&grid_result, "Grid");

    // Demonstrate K-Means Clustering
    println!("\n5. K-Means Clustering");
    println!("---------------------");
    let clustering = KMeansClustering::new(3)
        .with_max_iterations(50)
        .with_tolerance(1e-4);

    let assignments = clustering.cluster(&graph)?;
    print_clustering_info(&assignments, "K-Means");

    // Demonstrate Connected Components
    println!("\n6. Connected Components");
    println!("------------------------");
    let cc = ConnectedComponents::new().with_directed(false);
    let components = cc.find_components(&graph)?;
    print_components_info(&components, "Connected Components");

    // Demonstrate graph operations
    println!("\n7. Graph Operations");
    println!("-------------------");
    demonstrate_graph_operations(&mut graph);

    Ok(())
}

fn create_sample_graph() -> GraphLayout {
    let mut graph = GraphLayout::new(800.0, 600.0);

    // Add nodes with different properties
    let mut node_metadata = HashMap::new();
    node_metadata.insert("type".to_string(), "person".to_string());
    node_metadata.insert("department".to_string(), "engineering".to_string());

    graph.add_node(
        LayoutNode::new("Alice".to_string(), Point2D::new(0.0, 0.0))
            .with_size(20.0)
            .with_metadata(node_metadata.clone()),
    );

    node_metadata.insert("department".to_string(), "marketing".to_string());
    graph.add_node(
        LayoutNode::new("Bob".to_string(), Point2D::new(0.0, 0.0))
            .with_size(15.0)
            .with_metadata(node_metadata.clone()),
    );

    node_metadata.insert("department".to_string(), "engineering".to_string());
    graph.add_node(
        LayoutNode::new("Charlie".to_string(), Point2D::new(0.0, 0.0))
            .with_size(25.0)
            .with_metadata(node_metadata.clone()),
    );

    node_metadata.insert("department".to_string(), "sales".to_string());
    graph.add_node(
        LayoutNode::new("Diana".to_string(), Point2D::new(0.0, 0.0))
            .with_size(18.0)
            .with_metadata(node_metadata.clone()),
    );

    node_metadata.insert("department".to_string(), "engineering".to_string());
    graph.add_node(
        LayoutNode::new("Eve".to_string(), Point2D::new(0.0, 0.0))
            .with_size(22.0)
            .with_metadata(node_metadata.clone()),
    );

    // Add edges with different weights
    let mut edge_metadata = HashMap::new();
    edge_metadata.insert("relationship".to_string(), "colleague".to_string());
    edge_metadata.insert("strength".to_string(), "strong".to_string());

    graph.add_edge(
        LayoutEdge::new("AB".to_string(), "Alice".to_string(), "Bob".to_string())
            .with_weight(0.8)
            .with_length(100.0)
            .with_metadata(edge_metadata.clone()),
    );

    edge_metadata.insert("strength".to_string(), "medium".to_string());
    graph.add_edge(
        LayoutEdge::new("AC".to_string(), "Alice".to_string(), "Charlie".to_string())
            .with_weight(0.6)
            .with_length(80.0)
            .with_metadata(edge_metadata.clone()),
    );

    edge_metadata.insert("strength".to_string(), "weak".to_string());
    graph.add_edge(
        LayoutEdge::new("BD".to_string(), "Bob".to_string(), "Diana".to_string())
            .with_weight(0.3)
            .with_length(120.0)
            .with_metadata(edge_metadata.clone()),
    );

    edge_metadata.insert("strength".to_string(), "strong".to_string());
    graph.add_edge(
        LayoutEdge::new("CE".to_string(), "Charlie".to_string(), "Eve".to_string())
            .with_weight(0.9)
            .with_length(90.0)
            .with_metadata(edge_metadata.clone()),
    );

    edge_metadata.insert("strength".to_string(), "medium".to_string());
    graph.add_edge(
        LayoutEdge::new("DE".to_string(), "Diana".to_string(), "Eve".to_string())
            .with_weight(0.5)
            .with_length(110.0)
            .with_metadata(edge_metadata.clone()),
    );

    graph
}

fn print_layout_info(graph: &GraphLayout, layout_type: &str) {
    println!("{} Layout Results:", layout_type);

    for node in &graph.nodes {
        println!(
            "  Node {}: ({:.1}, {:.1})",
            node.id, node.position.x, node.position.y
        );
    }

    // Calculate some basic statistics
    let mut total_distance = 0.0;
    let mut edge_count = 0;

    for edge in &graph.edges {
        if let (Some(source), Some(target)) =
            (graph.get_node(&edge.source), graph.get_node(&edge.target))
        {
            let distance = source.position.distance_to(&target.position);
            total_distance += distance;
            edge_count += 1;
        }
    }

    if edge_count > 0 {
        let avg_distance = total_distance / edge_count as f64;
        println!("  Average edge length: {:.1}", avg_distance);
    }
}

fn print_clustering_info(assignments: &[usize], clustering_type: &str) {
    println!("{} Results:", clustering_type);

    let mut cluster_counts = std::collections::HashMap::new();
    for &cluster in assignments {
        *cluster_counts.entry(cluster).or_insert(0) += 1;
    }

    for (cluster_id, count) in cluster_counts {
        println!("  Cluster {}: {} nodes", cluster_id, count);
    }
}

fn print_components_info(components: &[usize], component_type: &str) {
    println!("{} Results:", component_type);

    let mut component_counts = std::collections::HashMap::new();
    for &component in components {
        *component_counts.entry(component).or_insert(0) += 1;
    }

    for (component_id, count) in component_counts {
        println!("  Component {}: {} nodes", component_id, count);
    }
}

fn demonstrate_graph_operations(graph: &mut GraphLayout) {
    println!("Graph Operations:");

    // Center the nodes
    graph.center_nodes();
    println!("  - Centered nodes");

    // Scale to fit
    graph.scale_to_fit(50.0);
    println!("  - Scaled to fit with 50px padding");

    // Get neighbors of a specific node
    if let Some(_alice) = graph.get_node("Alice") {
        let neighbors = graph.get_neighbors("Alice");
        println!("  - Alice's neighbors: {}", neighbors.len());
        for neighbor in neighbors {
            println!("    * {}", neighbor.id);
        }
    }

    // Get edges for a specific node
    let alice_edges = graph.get_edges_for_node("Alice");
    println!("  - Alice's edges: {}", alice_edges.len());
    for edge in alice_edges {
        println!(
            "    * {} -> {} (weight: {:.1})",
            edge.source, edge.target, edge.weight
        );
    }

    // Calculate graph density
    let n = graph.nodes.len();
    let m = graph.edges.len();
    let max_edges = n * (n - 1) / 2;
    let density = if max_edges > 0 {
        m as f64 / max_edges as f64
    } else {
        0.0
    };
    println!("  - Graph density: {:.3}", density);
}
