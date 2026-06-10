use crate::Result;
use crate::graph::correlation::visualization::VisualizationConfig;
use crate::graph::correlation::{CorrelationGraph, EdgeType, NodeType};
use std::collections::{HashMap, HashSet};

// ============================================================================
// Object-Oriented Hierarchy Layout (Task 12.4)
// ============================================================================

/// Hierarchical layout algorithm for OOP component graphs
///
/// Organizes components in a tree structure based on inheritance:
/// - Base classes at the top
/// - Derived classes below their parents
/// - Interfaces grouped separately
pub struct OOHierarchyLayout;

impl OOHierarchyLayout {
    /// Apply hierarchical layout to a component graph
    pub fn apply_layout(graph: &mut CorrelationGraph, config: &VisualizationConfig) -> Result<()> {
        if graph.nodes.is_empty() {
            return Ok(());
        }

        // Build inheritance tree
        let mut inheritance_map: HashMap<String, Vec<String>> = HashMap::new();
        let mut all_classes: HashSet<String> = HashSet::new();
        let mut base_classes: HashSet<String> = HashSet::new();

        // Collect all class nodes
        for node in &graph.nodes {
            if node.node_type == NodeType::Class {
                if let Some(class_name) = node.metadata.get("class_name").and_then(|v| v.as_str()) {
                    all_classes.insert(class_name.to_string());
                }
            }
        }

        // Build inheritance relationships from edges
        for edge in &graph.edges {
            if edge.edge_type == EdgeType::Inherits {
                inheritance_map
                    .entry(edge.target.clone())
                    .or_default()
                    .push(edge.source.clone());
            }
        }

        // Find base classes (classes with no incoming inheritance edges)
        for class in &all_classes {
            let has_parent = graph
                .edges
                .iter()
                .any(|e| e.edge_type == EdgeType::Inherits && e.source == *class);
            if !has_parent {
                base_classes.insert(class.clone());
            }
        }

        // Calculate levels using BFS
        let mut levels: HashMap<String, usize> = HashMap::new();
        let mut queue: Vec<(String, usize)> = base_classes.iter().map(|c| (c.clone(), 0)).collect();

        while let Some((class_name, level)) = queue.pop() {
            levels.insert(class_name.clone(), level);

            if let Some(children) = inheritance_map.get(&class_name) {
                for child in children {
                    if !levels.contains_key(child) {
                        queue.push((child.clone(), level + 1));
                    }
                }
            }
        }

        // Assign positions based on levels
        let max_level = levels.values().max().copied().unwrap_or(0);
        let level_height = if max_level > 0 {
            (config.height - 2.0 * config.padding) / max_level as f32
        } else {
            0.0
        };

        // Group nodes by level
        let mut nodes_by_level: HashMap<usize, Vec<String>> = HashMap::new();
        for (class_name, level) in &levels {
            nodes_by_level
                .entry(*level)
                .or_default()
                .push(class_name.clone());
        }

        // Position nodes
        for (level, class_names) in &nodes_by_level {
            let y = config.padding + (*level as f32 * level_height);
            let node_count = class_names.len();
            let node_spacing = if node_count > 1 {
                (config.width - 2.0 * config.padding) / (node_count - 1) as f32
            } else {
                0.0
            };

            for (idx, class_name) in class_names.iter().enumerate() {
                let x = if node_count > 1 {
                    config.padding + idx as f32 * node_spacing
                } else {
                    config.width / 2.0
                };

                // Find and update node position
                for node in &mut graph.nodes {
                    if let Some(node_class_name) =
                        node.metadata.get("class_name").and_then(|v| v.as_str())
                    {
                        if node_class_name == class_name {
                            node.position = Some((x, y));
                            break;
                        }
                    }
                }
            }
        }

        // Position interface nodes (grouped separately)
        let interface_node_ids: Vec<String> = graph
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::API && n.label.contains("interface"))
            .map(|n| n.id.clone())
            .collect();

        let interface_count = interface_node_ids.len();
        if interface_count > 0 {
            let interface_y = config.height - config.padding - 50.0; // Bottom area
            let interface_spacing = if interface_count > 1 {
                (config.width - 2.0 * config.padding) / (interface_count - 1) as f32
            } else {
                0.0
            };

            for (idx, node_id) in interface_node_ids.iter().enumerate() {
                let x = if interface_count > 1 {
                    config.padding + idx as f32 * interface_spacing
                } else {
                    config.width / 2.0
                };

                // Find and update node position
                for node in &mut graph.nodes {
                    if node.id == *node_id {
                        node.position = Some((x, interface_y));
                        break;
                    }
                }
            }
        }

        // Position any remaining nodes (not in inheritance hierarchy)
        for node in &mut graph.nodes {
            if node.position.is_none() {
                node.position = Some((config.width / 2.0, config.height / 2.0));
            }
        }

        Ok(())
    }
}

/// Apply OOP hierarchy layout to a component graph
pub fn apply_oop_hierarchy_layout(
    graph: &mut CorrelationGraph,
    config: &VisualizationConfig,
) -> Result<()> {
    OOHierarchyLayout::apply_layout(graph, config)
}
