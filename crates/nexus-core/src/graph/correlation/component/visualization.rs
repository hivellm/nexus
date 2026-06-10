use crate::Result;
use crate::graph::correlation::{CorrelationGraph, EdgeType, NodeType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::analyzer::ComponentAnalyzer;

// ============================================================================
// Component Relationship Visualization (Task 12.6)
// ============================================================================

/// Component visualization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentVisualizationConfig {
    /// Color for base classes
    pub base_class_color: String,
    /// Color for derived classes
    pub derived_class_color: String,
    /// Color for abstract classes
    pub abstract_class_color: String,
    /// Color for interfaces
    pub interface_color: String,
    /// Color for inheritance edges
    pub inheritance_edge_color: String,
    /// Color for implementation edges
    pub implementation_edge_color: String,
    /// Color for composition edges
    pub composition_edge_color: String,
    /// Show method counts on nodes
    pub show_method_counts: bool,
    /// Show field counts on nodes
    pub show_field_counts: bool,
    /// Highlight abstract classes
    pub highlight_abstract: bool,
}

impl Default for ComponentVisualizationConfig {
    fn default() -> Self {
        Self {
            base_class_color: "#3498db".to_string(),
            derived_class_color: "#2980b9".to_string(),
            abstract_class_color: "#9b59b6".to_string(),
            interface_color: "#2ecc71".to_string(),
            inheritance_edge_color: "#e74c3c".to_string(),
            implementation_edge_color: "#f39c12".to_string(),
            composition_edge_color: "#95a5a6".to_string(),
            show_method_counts: true,
            show_field_counts: true,
            highlight_abstract: true,
        }
    }
}

/// Apply component visualization styling to a graph
pub fn apply_component_visualization(
    graph: &mut CorrelationGraph,
    config: &ComponentVisualizationConfig,
    _analyzer: &ComponentAnalyzer,
) -> Result<()> {
    // Identify base classes (no incoming inheritance edges)
    let mut incoming_inheritance: HashMap<String, usize> = HashMap::new();
    for edge in &graph.edges {
        if edge.edge_type == EdgeType::Inherits {
            *incoming_inheritance.entry(edge.target.clone()).or_insert(0) += 1;
        }
    }

    // Apply styling to nodes
    for node in &mut graph.nodes {
        if node.node_type == NodeType::Class {
            let is_base = !incoming_inheritance.contains_key(&node.id);
            let is_abstract = node
                .metadata
                .get("is_abstract")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            if is_abstract && config.highlight_abstract {
                node.color = Some(config.abstract_class_color.clone());
            } else if is_base {
                node.color = Some(config.base_class_color.clone());
            } else {
                node.color = Some(config.derived_class_color.clone());
            }

            // Add method/field counts to label if enabled
            if config.show_method_counts || config.show_field_counts {
                let mut label_parts = vec![node.label.clone()];

                if config.show_method_counts {
                    if let Some(method_count) =
                        node.metadata.get("method_count").and_then(|v| v.as_u64())
                    {
                        label_parts.push(format!("({} methods)", method_count));
                    }
                }

                if config.show_field_counts {
                    if let Some(field_count) =
                        node.metadata.get("field_count").and_then(|v| v.as_u64())
                    {
                        label_parts.push(format!("{} fields", field_count));
                    }
                }

                node.label = label_parts.join(" ");
            }

            // Set node size based on importance
            if is_base {
                node.size = Some(12.0);
            } else {
                node.size = Some(10.0);
            }
        } else if node.node_type == NodeType::API && node.label.contains("interface") {
            node.color = Some(config.interface_color.clone());
            node.size = Some(8.0);
        }
    }

    // Apply styling to edges
    for edge in &mut graph.edges {
        match edge.edge_type {
            EdgeType::Inherits => {
                edge.metadata.insert(
                    "color".to_string(),
                    serde_json::Value::String(config.inheritance_edge_color.clone()),
                );
                edge.weight = 2.0; // Thicker for inheritance
            }
            EdgeType::Uses => {
                edge.metadata.insert(
                    "color".to_string(),
                    serde_json::Value::String(config.implementation_edge_color.clone()),
                );
                edge.weight = 1.5;
            }
            EdgeType::Composes => {
                edge.metadata.insert(
                    "color".to_string(),
                    serde_json::Value::String(config.composition_edge_color.clone()),
                );
                edge.weight = 1.0;
            }
            _ => {}
        }
    }

    Ok(())
}
