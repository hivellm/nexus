use crate::Result;
use crate::graph::correlation::{CorrelationGraph, EdgeType, GraphEdge, GraphNode, NodeType};
use std::collections::HashMap;

use super::types::{
    ClassInfo, ComponentRelationship, ComponentRelationshipInfo, FieldInfo, InterfaceInfo,
};

/// Component analyzer for OOP code
pub struct ComponentAnalyzer {
    /// Map of class name to class information
    pub(super) classes: HashMap<String, ClassInfo>,
    /// Map of interface name to interface information
    pub(super) interfaces: HashMap<String, InterfaceInfo>,
    /// Relationships between components
    pub(super) relationships: Vec<ComponentRelationshipInfo>,
}

impl ComponentAnalyzer {
    /// Create a new component analyzer
    pub fn new() -> Self {
        Self {
            classes: HashMap::new(),
            interfaces: HashMap::new(),
            relationships: Vec::new(),
        }
    }

    /// Analyze source code for component patterns
    pub fn analyze_source_code(&mut self, files: &HashMap<String, String>) -> Result<()> {
        for (file_path, content) in files {
            self.analyze_file(file_path, content)?;
        }
        Ok(())
    }

    /// Analyze a single file for component patterns
    fn analyze_file(&mut self, file_path: &str, content: &str) -> Result<()> {
        let lines: Vec<&str> = content.lines().collect();

        for (line_num, line) in lines.iter().enumerate() {
            let line = line.trim();

            // Detect class definitions (simplified pattern matching)
            if let Some(class_info) = self.detect_class(line, file_path, line_num + 1) {
                self.classes.insert(class_info.name.clone(), class_info);
            }

            // Detect interface definitions
            if let Some(interface_info) = self.detect_interface(line, file_path, line_num + 1) {
                self.interfaces
                    .insert(interface_info.name.clone(), interface_info);
            }

            // Detect inheritance relationships
            if let Some(rel) = self.detect_inheritance(line, file_path) {
                self.relationships.push(rel);
            }

            // Detect interface implementations
            if let Some(rel) = self.detect_interface_implementation(line, file_path) {
                self.relationships.push(rel);
            }

            // Detect composition relationships
            if let Some(rel) = self.detect_composition(line, file_path) {
                self.relationships.push(rel);
            }
        }

        Ok(())
    }

    /// Detect class definition (simplified heuristic-based detection)
    pub(super) fn detect_class(
        &self,
        line: &str,
        file_path: &str,
        line_num: usize,
    ) -> Option<ClassInfo> {
        // Pattern: class ClassName or class ClassName extends BaseClass
        if line.starts_with("class ") || line.starts_with("pub class ") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let class_name = parts[1].trim_end_matches(['{', '(']);

                // Extract base class if present
                let base_class = if line.contains("extends ") {
                    line.split("extends ")
                        .nth(1)
                        .and_then(|s| s.split_whitespace().next())
                        .map(|s| s.trim_end_matches(['{', '(']).to_string())
                } else if line.contains(": ") {
                    line.split(": ")
                        .nth(1)
                        .and_then(|s| s.split_whitespace().next())
                        .map(|s| s.trim_end_matches(['{', '(']).to_string())
                } else {
                    None
                };

                let is_abstract = line.contains("abstract");
                let is_final = line.contains("final") || line.contains("sealed");
                let access_modifier = if line.starts_with("pub ") {
                    "public".to_string()
                } else {
                    "private".to_string()
                };

                return Some(ClassInfo {
                    name: class_name.to_string(),
                    file: file_path.to_string(),
                    line: line_num,
                    base_class,
                    interfaces: Vec::new(),
                    methods: Vec::new(),
                    fields: Vec::new(),
                    is_abstract,
                    is_final,
                    access_modifier,
                });
            }
        }

        None
    }

    /// Detect interface definition
    pub(super) fn detect_interface(
        &self,
        line: &str,
        file_path: &str,
        line_num: usize,
    ) -> Option<InterfaceInfo> {
        // Pattern: interface InterfaceName or trait TraitName
        if line.starts_with("interface ")
            || line.starts_with("pub interface ")
            || line.starts_with("trait ")
            || line.starts_with("pub trait ")
        {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let interface_name = parts[1].trim_end_matches(['{', '(']);

                // Extract parent interfaces if present
                let parent_interfaces = if line.contains("extends ") {
                    line.split("extends ")
                        .nth(1)
                        .map(|s| {
                            s.split(',')
                                .map(|i| i.trim().trim_end_matches(['{', '(']).to_string())
                                .collect()
                        })
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };

                return Some(InterfaceInfo {
                    name: interface_name.to_string(),
                    file: file_path.to_string(),
                    line: line_num,
                    parent_interfaces,
                    methods: Vec::new(),
                    properties: Vec::new(),
                });
            }
        }

        None
    }

    /// Detect inheritance relationship
    pub(super) fn detect_inheritance(
        &self,
        line: &str,
        _file_path: &str,
    ) -> Option<ComponentRelationshipInfo> {
        // Pattern: class Child extends Parent or class Child : Parent
        if line.contains("extends ") || (line.contains(": ") && line.contains("class ")) {
            if let Some(class_part) = line.split("class ").nth(1) {
                let child = class_part.split_whitespace().next()?;

                let parent = if line.contains("extends ") {
                    line.split("extends ").nth(1)?.split_whitespace().next()?
                } else if line.contains(": ") {
                    line.split(": ").nth(1)?.split_whitespace().next()?
                } else {
                    return None;
                };

                return Some(ComponentRelationshipInfo {
                    source: child.trim_end_matches(['{', '(']).to_string(),
                    target: parent.trim_end_matches(['{', '(']).to_string(),
                    relationship_type: ComponentRelationship::Inheritance,
                    strength: 1.0,
                    metadata: HashMap::new(),
                });
            }
        }

        None
    }

    /// Detect interface implementation
    pub(super) fn detect_interface_implementation(
        &self,
        line: &str,
        _file_path: &str,
    ) -> Option<ComponentRelationshipInfo> {
        // Pattern: class ClassName implements InterfaceName
        if line.contains("implements ") {
            if let Some(class_part) = line.split("class ").nth(1) {
                let class_name = class_part.split_whitespace().next()?;

                if let Some(interface_part) = line.split("implements ").nth(1) {
                    let interface_name = interface_part.split_whitespace().next()?;

                    return Some(ComponentRelationshipInfo {
                        source: class_name.trim_end_matches(['{', '(']).to_string(),
                        target: interface_name.trim_end_matches(['{', '(']).to_string(),
                        relationship_type: ComponentRelationship::Implementation,
                        strength: 1.0,
                        metadata: HashMap::new(),
                    });
                }
            }
        }

        None
    }

    /// Detect composition relationship (has-a relationship)
    pub(super) fn detect_composition(
        &self,
        line: &str,
        _file_path: &str,
    ) -> Option<ComponentRelationshipInfo> {
        // Pattern: field declarations like "private ComponentType component;"
        // This is a simplified heuristic - full implementation would need proper parsing
        if line.contains("private ") || line.contains("protected ") || line.contains("pub ") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                // Check if it looks like a field declaration
                let field_type = parts[1];
                let field_name = parts[2].trim_end_matches(';');

                // Check if field_type is a known class
                if self.classes.contains_key(field_type) {
                    // Try to find the containing class (would need context in full implementation)
                    // For now, we'll create a generic relationship
                    return Some(ComponentRelationshipInfo {
                        source: "Unknown".to_string(), // Would need class context
                        target: field_type.to_string(),
                        relationship_type: ComponentRelationship::Composition,
                        strength: 0.8,
                        metadata: {
                            let mut m = HashMap::new();
                            m.insert(
                                "field_name".to_string(),
                                serde_json::Value::String(field_name.to_string()),
                            );
                            m
                        },
                    });
                }
            }
        }

        None
    }

    /// Get all classes
    pub fn classes(&self) -> &HashMap<String, ClassInfo> {
        &self.classes
    }

    /// Get all interfaces
    pub fn interfaces(&self) -> &HashMap<String, InterfaceInfo> {
        &self.interfaces
    }

    /// Get all relationships
    pub fn relationships(&self) -> &[ComponentRelationshipInfo] {
        &self.relationships
    }

    /// Build enhanced component graph
    pub fn build_enhanced_component_graph(
        &self,
        base_graph: &CorrelationGraph,
    ) -> Result<CorrelationGraph> {
        let mut graph = base_graph.clone();

        // Add class nodes
        for (class_name, class_info) in &self.classes {
            let node_id = format!(
                "class:{}:{}:{}",
                class_info.file, class_name, class_info.line
            );

            if graph.nodes.iter().any(|n| n.id == node_id) {
                continue;
            }

            let mut metadata = HashMap::new();
            metadata.insert(
                "class_name".to_string(),
                serde_json::Value::String(class_name.clone()),
            );
            metadata.insert(
                "file".to_string(),
                serde_json::Value::String(class_info.file.clone()),
            );
            metadata.insert(
                "line".to_string(),
                serde_json::Value::Number(class_info.line.into()),
            );
            metadata.insert(
                "is_abstract".to_string(),
                serde_json::Value::Bool(class_info.is_abstract),
            );
            metadata.insert(
                "is_final".to_string(),
                serde_json::Value::Bool(class_info.is_final),
            );
            metadata.insert(
                "access_modifier".to_string(),
                serde_json::Value::String(class_info.access_modifier.clone()),
            );
            metadata.insert(
                "method_count".to_string(),
                serde_json::Value::Number(class_info.methods.len().into()),
            );
            metadata.insert(
                "field_count".to_string(),
                serde_json::Value::Number(class_info.fields.len().into()),
            );

            if let Some(ref base_class) = class_info.base_class {
                metadata.insert(
                    "base_class".to_string(),
                    serde_json::Value::String(base_class.clone()),
                );
            }

            if !class_info.interfaces.is_empty() {
                metadata.insert(
                    "interfaces".to_string(),
                    serde_json::Value::Array(
                        class_info
                            .interfaces
                            .iter()
                            .map(|i| serde_json::Value::String(i.clone()))
                            .collect(),
                    ),
                );
            }

            let node = GraphNode {
                id: node_id.clone(),
                node_type: NodeType::Class,
                label: class_name.clone(),
                metadata,
                position: None,
                size: Some(10.0),
                color: Some(if class_info.is_abstract {
                    "#9b59b6".to_string()
                } else {
                    "#3498db".to_string()
                }),
            };

            graph.add_node(node)?;
        }

        // Add interface nodes
        for (interface_name, interface_info) in &self.interfaces {
            let node_id = format!(
                "interface:{}:{}:{}",
                interface_info.file, interface_name, interface_info.line
            );

            if graph.nodes.iter().any(|n| n.id == node_id) {
                continue;
            }

            let mut metadata = HashMap::new();
            metadata.insert(
                "interface_name".to_string(),
                serde_json::Value::String(interface_name.clone()),
            );
            metadata.insert(
                "file".to_string(),
                serde_json::Value::String(interface_info.file.clone()),
            );
            metadata.insert(
                "line".to_string(),
                serde_json::Value::Number(interface_info.line.into()),
            );
            metadata.insert(
                "method_count".to_string(),
                serde_json::Value::Number(interface_info.methods.len().into()),
            );

            if !interface_info.parent_interfaces.is_empty() {
                metadata.insert(
                    "parent_interfaces".to_string(),
                    serde_json::Value::Array(
                        interface_info
                            .parent_interfaces
                            .iter()
                            .map(|i| serde_json::Value::String(i.clone()))
                            .collect(),
                    ),
                );
            }

            let node = GraphNode {
                id: node_id.clone(),
                node_type: NodeType::API, // Using API as interface type
                label: format!("{} (interface)", interface_name),
                metadata,
                position: None,
                size: Some(8.0),
                color: Some("#2ecc71".to_string()),
            };

            graph.add_node(node)?;
        }

        // Add relationship edges
        for rel in &self.relationships {
            let source_id = format!("class:{}", rel.source);
            let target_id = match rel.relationship_type {
                ComponentRelationship::Inheritance | ComponentRelationship::Composition => {
                    format!("class:{}", rel.target)
                }
                ComponentRelationship::Implementation => {
                    format!("interface:{}", rel.target)
                }
                _ => format!("class:{}", rel.target),
            };

            // Check if nodes exist
            let source_exists = graph
                .nodes
                .iter()
                .any(|n| n.id.starts_with(&source_id) || n.id.contains(&rel.source));
            let target_exists = graph
                .nodes
                .iter()
                .any(|n| n.id.starts_with(&target_id) || n.id.contains(&rel.target));

            if source_exists && target_exists {
                let edge_type = match rel.relationship_type {
                    ComponentRelationship::Inheritance => EdgeType::Inherits,
                    ComponentRelationship::Implementation => EdgeType::Uses,
                    ComponentRelationship::Composition | ComponentRelationship::Aggregation => {
                        EdgeType::Composes
                    }
                    _ => EdgeType::Depends,
                };

                let edge_id = format!(
                    "rel:{}:{}:{:?}",
                    rel.source, rel.target, rel.relationship_type
                );

                let mut metadata = HashMap::new();
                metadata.insert(
                    "relationship_type".to_string(),
                    serde_json::Value::String(format!("{:?}", rel.relationship_type)),
                );
                metadata.insert(
                    "strength".to_string(),
                    serde_json::Value::Number(serde_json::Number::from_f64(rel.strength).unwrap()),
                );
                metadata.extend(rel.metadata.clone());

                let edge = GraphEdge {
                    id: edge_id,
                    source: rel.source.clone(),
                    target: rel.target.clone(),
                    edge_type,
                    weight: rel.strength as f32,
                    metadata,
                    label: Some(format!("{:?}", rel.relationship_type)),
                };

                let _ = graph.add_edge(edge);
            }
        }

        Ok(graph)
    }
}

impl Default for ComponentAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}
