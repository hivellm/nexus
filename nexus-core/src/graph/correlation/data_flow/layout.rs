//! Flow-based graph layout for data-flow visualisations. Places nodes
//! according to their position in the flow graph (sources on the left,
//! sinks on the right) and groups correlated flows.

use super::*;

pub struct FlowBasedLayout;

impl FlowBasedLayout {
    /// Apply flow-based layout to a data flow graph
    pub fn apply_layout(
        graph: &mut crate::graph::correlation::CorrelationGraph,
        config: &VisualizationConfig,
    ) -> Result<()> {
        use std::collections::{HashMap, HashSet};

        if graph.nodes.is_empty() {
            return Ok(());
        }

        // Build adjacency lists (forward and backward)
        let mut outgoing: HashMap<String, Vec<String>> = HashMap::new();
        let mut incoming: HashMap<String, Vec<String>> = HashMap::new();
        let mut node_ids: HashSet<String> = HashSet::new();

        for node in &graph.nodes {
            node_ids.insert(node.id.clone());
            outgoing.insert(node.id.clone(), Vec::new());
            incoming.insert(node.id.clone(), Vec::new());
        }

        for edge in &graph.edges {
            outgoing
                .entry(edge.source.clone())
                .or_default()
                .push(edge.target.clone());
            incoming
                .entry(edge.target.clone())
                .or_default()
                .push(edge.source.clone());
        }

        // Calculate layers using topological sort (BFS-based)
        let mut layers: Vec<Vec<String>> = Vec::new();
        let mut assigned = HashSet::new();
        let mut in_degree: HashMap<String, usize> = HashMap::new();

        // Initialize in-degrees
        for node_id in &node_ids {
            in_degree.insert(
                node_id.clone(),
                incoming.get(node_id).map(|v| v.len()).unwrap_or(0),
            );
        }

        // Find source nodes (nodes with no incoming edges)
        let mut current_layer: Vec<String> = node_ids
            .iter()
            .filter(|id| in_degree.get(*id).copied().unwrap_or(0) == 0)
            .cloned()
            .collect();

        // If no source nodes, start with all nodes (for disconnected components)
        if current_layer.is_empty() {
            current_layer = node_ids.iter().cloned().collect();
        }

        // Build layers using topological ordering
        while !current_layer.is_empty() {
            layers.push(current_layer.clone());

            for node_id in &current_layer {
                assigned.insert(node_id.clone());
            }

            // Find next layer (nodes whose dependencies are all assigned)
            let mut next_layer = Vec::new();
            for node_id in &node_ids {
                if assigned.contains(node_id) {
                    continue;
                }

                let deps_assigned = incoming
                    .get(node_id)
                    .map(|deps| deps.iter().all(|dep| assigned.contains(dep)))
                    .unwrap_or(true);

                if deps_assigned {
                    next_layer.push(node_id.clone());
                }
            }

            current_layer = next_layer;
        }

        // Handle any remaining unassigned nodes (disconnected components)
        for node_id in &node_ids {
            if !assigned.contains(node_id) {
                if layers.is_empty() {
                    layers.push(vec![node_id.clone()]);
                } else {
                    layers.last_mut().unwrap().push(node_id.clone());
                }
            }
        }

        // Calculate positions for each layer
        let layer_count = layers.len();
        let layer_width = if layer_count > 1 {
            (config.width - 2.0 * config.padding) / (layer_count - 1) as f32
        } else {
            0.0
        };

        for (layer_idx, layer_nodes) in layers.iter().enumerate() {
            let x = if layer_count > 1 {
                config.padding + layer_idx as f32 * layer_width
            } else {
                config.width / 2.0
            };

            let node_count = layer_nodes.len();
            let node_spacing = if node_count > 1 {
                (config.height - 2.0 * config.padding) / (node_count - 1) as f32
            } else {
                0.0
            };

            for (node_idx, node_id) in layer_nodes.iter().enumerate() {
                let y = if node_count > 1 {
                    config.padding + node_idx as f32 * node_spacing
                } else {
                    config.height / 2.0
                };

                // Update node position
                if let Some(node) = graph.nodes.iter_mut().find(|n| n.id == *node_id) {
                    node.position = Some((x, y));
                }
            }
        }

        Ok(())
    }
}

/// Apply flow-based layout to a data flow graph
pub fn apply_flow_layout(
    graph: &mut crate::graph::correlation::CorrelationGraph,
    config: &VisualizationConfig,
) -> Result<()> {
    FlowBasedLayout::apply_layout(graph, config)
}

/// Data flow visualization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataFlowVisualizationConfig {
    /// Color for variable nodes
    pub variable_color: String,
    /// Color for transformation nodes
    pub transformation_color: String,
    /// Color for input/source nodes
    pub source_color: String,
    /// Color for output/sink nodes
    pub sink_color: String,
    /// Show type information on nodes
    pub show_types: bool,
    /// Show variable names on edges
    pub show_edge_labels: bool,
    /// Highlight critical paths
    pub highlight_critical_paths: bool,
    /// Color map for different data types
    pub type_colors: HashMap<String, String>,
}

impl Default for DataFlowVisualizationConfig {
    fn default() -> Self {
        let mut type_colors = HashMap::new();
        type_colors.insert("String".to_string(), "#e74c3c".to_string());
        type_colors.insert("i32".to_string(), "#3498db".to_string());
        type_colors.insert("i64".to_string(), "#3498db".to_string());
        type_colors.insert("f64".to_string(), "#9b59b6".to_string());
        type_colors.insert("bool".to_string(), "#f39c12".to_string());
        type_colors.insert("Vec<T>".to_string(), "#1abc9c".to_string());
        type_colors.insert("HashMap<K, V>".to_string(), "#e67e22".to_string());

        Self {
            variable_color: "#3498db".to_string(),
            transformation_color: "#9b59b6".to_string(),
            source_color: "#2ecc71".to_string(),
            sink_color: "#e74c3c".to_string(),
            show_types: true,
            show_edge_labels: true,
            highlight_critical_paths: true,
            type_colors,
        }
    }
}

/// Apply data flow visualization styling to a graph
pub fn apply_data_flow_visualization(
    graph: &mut crate::graph::correlation::CorrelationGraph,
    config: &DataFlowVisualizationConfig,
    analyzer: &DataFlowAnalyzer,
) -> Result<()> {
    // EdgeType, GraphEdge, GraphNode, NodeType are used via CorrelationGraph

    // Build node type map from analyzer
    let type_map = analyzer.type_propagator().all_types();

    // Identify source and sink nodes
    let mut incoming_count: HashMap<String, usize> = HashMap::new();
    let mut outgoing_count: HashMap<String, usize> = HashMap::new();

    for edge in &graph.edges {
        *incoming_count.entry(edge.target.clone()).or_insert(0) += 1;
        *outgoing_count.entry(edge.source.clone()).or_insert(0) += 1;
    }

    // Apply styling to nodes
    for node in &mut graph.nodes {
        // Determine if node is source or sink
        let is_source = incoming_count.get(&node.id).copied().unwrap_or(0) == 0;
        let is_sink = outgoing_count.get(&node.id).copied().unwrap_or(0) == 0;

        // Set color based on node type and role
        if is_source {
            node.color = Some(config.source_color.clone());
        } else if is_sink {
            node.color = Some(config.sink_color.clone());
        } else if node.id.starts_with("trans:") {
            node.color = Some(config.transformation_color.clone());
        } else if node.id.starts_with("var:") {
            // Use type-based color if available
            if let Some(var_name) = node.metadata.get("variable_name").and_then(|v| v.as_str()) {
                if let Some(type_name) = type_map.get(var_name) {
                    if let Some(type_color) = config.type_colors.get(type_name) {
                        node.color = Some(type_color.clone());
                    } else {
                        node.color = Some(config.variable_color.clone());
                    }
                } else {
                    node.color = Some(config.variable_color.clone());
                }
            } else {
                node.color = Some(config.variable_color.clone());
            }
        } else {
            node.color = Some(config.variable_color.clone());
        }

        // Add type information to label if enabled
        if config.show_types {
            if let Some(var_name) = node.metadata.get("variable_name").and_then(|v| v.as_str()) {
                if let Some(type_name) = type_map.get(var_name) {
                    node.label = format!("{}: {}", node.label, type_name);
                }
            }
        }

        // Set node size based on importance
        if is_source || is_sink {
            node.size = Some(12.0); // Larger for sources/sinks
        } else {
            node.size = Some(8.0);
        }
    }

    // Apply styling to edges
    for edge in &mut graph.edges {
        // Add variable name to edge label if enabled
        if config.show_edge_labels {
            if let Some(var_name) = edge.metadata.get("variable").and_then(|v| v.as_str()) {
                edge.label = Some(var_name.to_string());
            }
        }

        // Set edge width based on flow type
        if edge.edge_type == EdgeType::Transforms {
            edge.weight = 2.0; // Thicker for transformations
        }
    }

    Ok(())
}

/// Generate data flow visualization with enhanced styling
pub fn visualize_data_flow(
    graph: &mut crate::graph::correlation::CorrelationGraph,
    visualization_config: &VisualizationConfig,
    data_flow_config: &DataFlowVisualizationConfig,
    analyzer: &DataFlowAnalyzer,
) -> Result<String> {
    use crate::graph::correlation::visualization::render_graph_to_svg;

    // Apply flow-based layout
    apply_flow_layout(graph, visualization_config)?;

    // Apply data flow visualization styling
    apply_data_flow_visualization(graph, data_flow_config, analyzer)?;

    // Render to SVG
    render_graph_to_svg(graph, visualization_config)
}
