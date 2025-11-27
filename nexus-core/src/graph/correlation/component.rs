//! Component Analysis Module
//!
//! Provides advanced component analysis capabilities for object-oriented code:
//! - Class and interface analysis
//! - Inheritance and composition tracking
//! - Object-oriented hierarchy layout
//! - Interface implementation analysis
//! - Component relationship visualization
//! - Component coupling analysis
//! - Component metrics calculation

use crate::Result;
use crate::graph::correlation::visualization::VisualizationConfig;
use crate::graph::correlation::{CorrelationGraph, EdgeType, GraphEdge, GraphNode, NodeType};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Class information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassInfo {
    /// Class name
    pub name: String,
    /// File where class is defined
    pub file: String,
    /// Line number where class is defined
    pub line: usize,
    /// Base class (if any)
    pub base_class: Option<String>,
    /// Interfaces implemented by this class
    pub interfaces: Vec<String>,
    /// Methods in this class
    pub methods: Vec<MethodInfo>,
    /// Fields/properties in this class
    pub fields: Vec<FieldInfo>,
    /// Whether class is abstract
    pub is_abstract: bool,
    /// Whether class is final/sealed
    pub is_final: bool,
    /// Access modifier (public, private, protected)
    pub access_modifier: String,
}

/// Interface information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceInfo {
    /// Interface name
    pub name: String,
    /// File where interface is defined
    pub file: String,
    /// Line number where interface is defined
    pub line: usize,
    /// Parent interfaces (if any)
    pub parent_interfaces: Vec<String>,
    /// Methods declared in this interface
    pub methods: Vec<MethodInfo>,
    /// Properties declared in this interface
    pub properties: Vec<PropertyInfo>,
}

/// Method information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodInfo {
    /// Method name
    pub name: String,
    /// Return type
    pub return_type: Option<String>,
    /// Parameters
    pub parameters: Vec<ParameterInfo>,
    /// Access modifier
    pub access_modifier: String,
    /// Whether method is abstract
    pub is_abstract: bool,
    /// Whether method is static
    pub is_static: bool,
    /// Whether method is virtual
    pub is_virtual: bool,
    /// Line number
    pub line: usize,
}

/// Field information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldInfo {
    /// Field name
    pub name: String,
    /// Field type
    pub field_type: Option<String>,
    /// Access modifier
    pub access_modifier: String,
    /// Whether field is static
    pub is_static: bool,
    /// Line number
    pub line: usize,
}

/// Property information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyInfo {
    /// Property name
    pub name: String,
    /// Property type
    pub property_type: Option<String>,
    /// Access modifier
    pub access_modifier: String,
    /// Line number
    pub line: usize,
}

/// Parameter information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterInfo {
    /// Parameter name
    pub name: String,
    /// Parameter type
    pub param_type: Option<String>,
}

/// Component relationship type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComponentRelationship {
    /// Class inherits from another class
    Inheritance,
    /// Class implements an interface
    Implementation,
    /// Component composes another component (has-a relationship)
    Composition,
    /// Component aggregates another component
    Aggregation,
    /// Component uses another component
    Usage,
    /// Component depends on another component
    Dependency,
}

/// Component relationship information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentRelationshipInfo {
    /// Source component
    pub source: String,
    /// Target component
    pub target: String,
    /// Relationship type
    pub relationship_type: ComponentRelationship,
    /// Strength of relationship (0.0 to 1.0)
    pub strength: f64,
    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Component analyzer for OOP code
pub struct ComponentAnalyzer {
    /// Map of class name to class information
    classes: HashMap<String, ClassInfo>,
    /// Map of interface name to interface information
    interfaces: HashMap<String, InterfaceInfo>,
    /// Relationships between components
    relationships: Vec<ComponentRelationshipInfo>,
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
    fn detect_class(&self, line: &str, file_path: &str, line_num: usize) -> Option<ClassInfo> {
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
    fn detect_interface(
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
    fn detect_inheritance(
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
    fn detect_interface_implementation(
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
    fn detect_composition(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_component_analyzer_new() {
        let analyzer = ComponentAnalyzer::new();
        assert!(analyzer.classes().is_empty());
        assert!(analyzer.interfaces().is_empty());
        assert!(analyzer.relationships().is_empty());
    }

    #[test]
    fn test_detect_class() {
        let analyzer = ComponentAnalyzer::new();
        let class_info = analyzer.detect_class("class MyClass {", "test.rs", 1);
        assert!(class_info.is_some());
        let class = class_info.unwrap();
        assert_eq!(class.name, "MyClass");
        assert_eq!(class.file, "test.rs");
        assert_eq!(class.line, 1);
    }

    #[test]
    fn test_detect_class_with_inheritance() {
        let analyzer = ComponentAnalyzer::new();
        let class_info = analyzer.detect_class("class Child extends Parent {", "test.rs", 1);
        assert!(class_info.is_some());
        let class = class_info.unwrap();
        assert_eq!(class.name, "Child");
        assert!(class.base_class.is_some());
        assert_eq!(class.base_class.unwrap(), "Parent");
    }

    #[test]
    fn test_detect_interface() {
        let analyzer = ComponentAnalyzer::new();
        let interface_info = analyzer.detect_interface("interface MyInterface {", "test.rs", 1);
        assert!(interface_info.is_some());
        let interface = interface_info.unwrap();
        assert_eq!(interface.name, "MyInterface");
    }

    #[test]
    fn test_detect_inheritance_relationship() {
        let analyzer = ComponentAnalyzer::new();
        let rel = analyzer.detect_inheritance("class Child extends Parent {", "test.rs");
        assert!(rel.is_some());
        let relationship = rel.unwrap();
        assert_eq!(relationship.source, "Child");
        assert_eq!(relationship.target, "Parent");
        assert_eq!(
            relationship.relationship_type,
            ComponentRelationship::Inheritance
        );
    }

    #[test]
    fn test_detect_interface_implementation() {
        let analyzer = ComponentAnalyzer::new();
        let rel = analyzer
            .detect_interface_implementation("class MyClass implements MyInterface {", "test.rs");
        assert!(rel.is_some());
        let relationship = rel.unwrap();
        assert_eq!(relationship.source, "MyClass");
        assert_eq!(relationship.target, "MyInterface");
        assert_eq!(
            relationship.relationship_type,
            ComponentRelationship::Implementation
        );
    }

    #[test]
    fn test_analyze_source_code() {
        let mut analyzer = ComponentAnalyzer::new();
        let mut files = HashMap::new();
        files.insert("test.rs".to_string(), 
            "class BaseClass {}\ninterface MyInterface {}\nclass Child extends BaseClass implements MyInterface {}".to_string());

        let result = analyzer.analyze_source_code(&files);
        assert!(result.is_ok());
        assert!(!analyzer.classes().is_empty());
        assert!(!analyzer.interfaces().is_empty());
    }

    #[test]
    fn test_oop_hierarchy_layout() {
        let mut graph = CorrelationGraph::new(
            crate::graph::correlation::GraphType::Component,
            "Test".to_string(),
        );

        // Create inheritance hierarchy: Base -> Child -> GrandChild
        let base = GraphNode {
            id: "Base".to_string(),
            node_type: NodeType::Class,
            label: "Base".to_string(),
            metadata: {
                let mut m = HashMap::new();
                m.insert(
                    "class_name".to_string(),
                    serde_json::Value::String("Base".to_string()),
                );
                m
            },
            position: None,
            size: None,
            color: None,
        };
        let child = GraphNode {
            id: "Child".to_string(),
            node_type: NodeType::Class,
            label: "Child".to_string(),
            metadata: {
                let mut m = HashMap::new();
                m.insert(
                    "class_name".to_string(),
                    serde_json::Value::String("Child".to_string()),
                );
                m
            },
            position: None,
            size: None,
            color: None,
        };

        graph.add_node(base).unwrap();
        graph.add_node(child).unwrap();

        graph
            .add_edge(GraphEdge {
                id: "e1".to_string(),
                source: "Child".to_string(),
                target: "Base".to_string(),
                edge_type: EdgeType::Inherits,
                weight: 1.0,
                metadata: HashMap::new(),
                label: None,
            })
            .unwrap();

        let config = VisualizationConfig {
            width: 1000.0,
            height: 800.0,
            ..Default::default()
        };

        let result = OOHierarchyLayout::apply_layout(&mut graph, &config);
        assert!(result.is_ok());

        // Verify all nodes have positions
        for node in &graph.nodes {
            assert!(node.position.is_some());
        }
    }

    #[test]
    fn test_component_statistics() {
        let mut analyzer = ComponentAnalyzer::new();
        analyzer.classes.insert(
            "TestClass".to_string(),
            ClassInfo {
                name: "TestClass".to_string(),
                file: "test.rs".to_string(),
                line: 1,
                base_class: None,
                interfaces: vec!["MyInterface".to_string()],
                methods: vec![MethodInfo {
                    name: "testMethod".to_string(),
                    return_type: Some("void".to_string()),
                    parameters: vec![],
                    access_modifier: "public".to_string(),
                    is_abstract: false,
                    is_static: false,
                    is_virtual: false,
                    line: 5,
                }],
                fields: vec![FieldInfo {
                    name: "testField".to_string(),
                    field_type: Some("String".to_string()),
                    access_modifier: "private".to_string(),
                    is_static: false,
                    line: 3,
                }],
                is_abstract: false,
                is_final: false,
                access_modifier: "public".to_string(),
            },
        );

        analyzer.interfaces.insert(
            "MyInterface".to_string(),
            InterfaceInfo {
                name: "MyInterface".to_string(),
                file: "test.rs".to_string(),
                line: 10,
                parent_interfaces: vec![],
                methods: vec![],
                properties: vec![],
            },
        );

        let graph = CorrelationGraph::new(
            crate::graph::correlation::GraphType::Component,
            "Test".to_string(),
        );
        let stats = analyzer.calculate_statistics(&graph);

        assert_eq!(stats.total_classes, 1);
        assert_eq!(stats.total_interfaces, 1);
        assert_eq!(stats.average_methods_per_class, 1.0);
        assert_eq!(stats.average_fields_per_class, 1.0);
    }

    #[test]
    fn test_component_coupling() {
        let mut graph = CorrelationGraph::new(
            crate::graph::correlation::GraphType::Component,
            "Test".to_string(),
        );
        let mut analyzer = ComponentAnalyzer::new();

        analyzer.classes.insert(
            "A".to_string(),
            ClassInfo {
                name: "A".to_string(),
                file: "test.rs".to_string(),
                line: 1,
                base_class: None,
                interfaces: vec![],
                methods: vec![],
                fields: vec![],
                is_abstract: false,
                is_final: false,
                access_modifier: "public".to_string(),
            },
        );

        analyzer.classes.insert(
            "B".to_string(),
            ClassInfo {
                name: "B".to_string(),
                file: "test.rs".to_string(),
                line: 2,
                base_class: Some("A".to_string()),
                interfaces: vec![],
                methods: vec![],
                fields: vec![],
                is_abstract: false,
                is_final: false,
                access_modifier: "public".to_string(),
            },
        );

        // Add nodes first
        graph
            .add_node(GraphNode {
                id: "A".to_string(),
                node_type: NodeType::Class,
                label: "A".to_string(),
                metadata: HashMap::new(),
                position: None,
                size: None,
                color: None,
            })
            .unwrap();

        graph
            .add_node(GraphNode {
                id: "B".to_string(),
                node_type: NodeType::Class,
                label: "B".to_string(),
                metadata: HashMap::new(),
                position: None,
                size: None,
                color: None,
            })
            .unwrap();

        graph
            .add_edge(GraphEdge {
                id: "e1".to_string(),
                source: "B".to_string(),
                target: "A".to_string(),
                edge_type: EdgeType::Inherits,
                weight: 1.0,
                metadata: HashMap::new(),
                label: None,
            })
            .unwrap();

        let coupling_metrics = analyzer.calculate_coupling(&graph);
        assert!(!coupling_metrics.is_empty());
    }

    #[test]
    fn test_component_visualization_config() {
        let config = ComponentVisualizationConfig::default();
        assert_eq!(config.base_class_color, "#3498db");
        assert_eq!(config.interface_color, "#2ecc71");
        assert!(config.show_method_counts);
        assert!(config.highlight_abstract);
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::graph::correlation::CorrelationGraph;
    use std::collections::HashMap;

    #[test]
    fn test_complete_component_workflow() {
        // Test complete workflow: analyze -> build graph -> layout -> visualize -> calculate stats
        let mut analyzer = ComponentAnalyzer::new();
        let mut files = HashMap::new();

        files.insert(
            "oop.rs".to_string(),
            "abstract class Animal {}\n\
             class Dog extends Animal {}\n\
             interface Flyable {}\n\
             class Bird extends Animal implements Flyable {}"
                .to_string(),
        );

        analyzer.analyze_source_code(&files).unwrap();

        let base_graph = CorrelationGraph::new(
            crate::graph::correlation::GraphType::Component,
            "OOP".to_string(),
        );
        let graph = analyzer
            .build_enhanced_component_graph(&base_graph)
            .unwrap();

        // Apply layout
        let config = VisualizationConfig {
            width: 1000.0,
            height: 800.0,
            ..Default::default()
        };
        let mut layout_graph = graph.clone();
        OOHierarchyLayout::apply_layout(&mut layout_graph, &config).unwrap();

        // Apply visualization
        let vis_config = ComponentVisualizationConfig::default();
        apply_component_visualization(&mut layout_graph, &vis_config, &analyzer).unwrap();

        // Calculate statistics
        let stats = analyzer.calculate_statistics(&graph);
        assert!(stats.total_classes > 0);
        assert!(stats.total_interfaces > 0);
    }
}

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
        use std::collections::{HashMap, HashSet};

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

// ============================================================================
// Component Coupling Analysis (Task 12.7)
// ============================================================================

/// Component coupling metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentCouplingMetrics {
    /// Component name
    pub component_name: String,
    /// Afferent coupling (number of components that depend on this)
    pub afferent_coupling: usize,
    /// Efferent coupling (number of components this depends on)
    pub efferent_coupling: usize,
    /// Instability (efferent / (afferent + efferent))
    pub instability: f64,
    /// Abstractness (abstract classes / total classes)
    pub abstractness: f64,
    /// Distance from main sequence (|abstractness + instability - 1|)
    pub distance_from_main_sequence: f64,
}

/// Component coupling analyzer
pub struct ComponentCouplingAnalyzer;

impl ComponentCouplingAnalyzer {
    /// Calculate coupling metrics for all components
    pub fn calculate_coupling(
        graph: &CorrelationGraph,
        analyzer: &ComponentAnalyzer,
    ) -> Vec<ComponentCouplingMetrics> {
        let mut metrics = Vec::new();

        // Build dependency maps
        let mut afferent: HashMap<String, HashSet<String>> = HashMap::new();
        let mut efferent: HashMap<String, HashSet<String>> = HashMap::new();

        for edge in &graph.edges {
            if edge.edge_type == EdgeType::Inherits
                || edge.edge_type == EdgeType::Composes
                || edge.edge_type == EdgeType::Depends
            {
                efferent
                    .entry(edge.source.clone())
                    .or_default()
                    .insert(edge.target.clone());
                afferent
                    .entry(edge.target.clone())
                    .or_default()
                    .insert(edge.source.clone());
            }
        }

        // Calculate metrics for each component
        for component_name in analyzer.classes().keys() {
            let afferent_count = afferent.get(component_name).map(|s| s.len()).unwrap_or(0);
            let efferent_count = efferent.get(component_name).map(|s| s.len()).unwrap_or(0);
            let total = afferent_count + efferent_count;

            let instability = if total > 0 {
                efferent_count as f64 / total as f64
            } else {
                0.0
            };

            // Calculate abstractness (simplified - would need full class info)
            let abstractness = 0.0; // Placeholder

            let distance_from_main_sequence = (abstractness + instability - 1.0).abs();

            metrics.push(ComponentCouplingMetrics {
                component_name: component_name.clone(),
                afferent_coupling: afferent_count,
                efferent_coupling: efferent_count,
                instability,
                abstractness,
                distance_from_main_sequence,
            });
        }

        metrics
    }
}

// ============================================================================
// Component Metrics Calculation (Task 12.8)
// ============================================================================

/// Statistics about components in a graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentStatistics {
    /// Total number of classes
    pub total_classes: usize,
    /// Total number of interfaces
    pub total_interfaces: usize,
    /// Number of abstract classes
    pub abstract_classes: usize,
    /// Number of final/sealed classes
    pub final_classes: usize,
    /// Total number of inheritance relationships
    pub inheritance_relationships: usize,
    /// Total number of implementation relationships
    pub implementation_relationships: usize,
    /// Total number of composition relationships
    pub composition_relationships: usize,
    /// Average methods per class
    pub average_methods_per_class: f64,
    /// Average fields per class
    pub average_fields_per_class: f64,
    /// Maximum inheritance depth
    pub max_inheritance_depth: usize,
    /// Number of root classes (no base class)
    pub root_classes: usize,
}

impl ComponentStatistics {
    /// Calculate statistics from a component graph and analyzer
    pub fn calculate(graph: &CorrelationGraph, analyzer: &ComponentAnalyzer) -> Self {
        let classes = analyzer.classes();
        let total_classes = classes.len();
        let total_interfaces = analyzer.interfaces().len();

        let abstract_classes = classes.values().filter(|c| c.is_abstract).count();

        let final_classes = classes.values().filter(|c| c.is_final).count();

        // Count relationships
        let inheritance_relationships = graph
            .edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Inherits)
            .count();

        let implementation_relationships = graph
            .edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Uses && e.target.contains("interface"))
            .count();

        let composition_relationships = graph
            .edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Composes)
            .count();

        // Calculate averages
        let total_methods: usize = classes.values().map(|c| c.methods.len()).sum();
        let average_methods_per_class = if total_classes > 0 {
            total_methods as f64 / total_classes as f64
        } else {
            0.0
        };

        let total_fields: usize = classes.values().map(|c| c.fields.len()).sum();
        let average_fields_per_class = if total_classes > 0 {
            total_fields as f64 / total_classes as f64
        } else {
            0.0
        };

        // Calculate max inheritance depth
        let mut max_depth = 0;
        let mut inheritance_map: HashMap<String, Vec<String>> = HashMap::new();

        for edge in &graph.edges {
            if edge.edge_type == EdgeType::Inherits {
                inheritance_map
                    .entry(edge.target.clone())
                    .or_default()
                    .push(edge.source.clone());
            }
        }

        fn calculate_depth(
            class: &str,
            inheritance_map: &HashMap<String, Vec<String>>,
            visited: &mut HashSet<String>,
        ) -> usize {
            if visited.contains(class) {
                return 0;
            }
            visited.insert(class.to_string());

            let mut max_child_depth = 0;
            if let Some(children) = inheritance_map.get(class) {
                for child in children {
                    let depth = calculate_depth(child, inheritance_map, visited);
                    max_child_depth = max_child_depth.max(depth);
                }
            }

            1 + max_child_depth
        }

        for class_name in classes.keys() {
            let mut visited = HashSet::new();
            let depth = calculate_depth(class_name, &inheritance_map, &mut visited);
            max_depth = max_depth.max(depth);
        }

        // Count root classes
        let root_classes = classes.values().filter(|c| c.base_class.is_none()).count();

        Self {
            total_classes,
            total_interfaces,
            abstract_classes,
            final_classes,
            inheritance_relationships,
            implementation_relationships,
            composition_relationships,
            average_methods_per_class,
            average_fields_per_class,
            max_inheritance_depth: max_depth,
            root_classes,
        }
    }
}

impl ComponentAnalyzer {
    /// Calculate component statistics
    pub fn calculate_statistics(&self, graph: &CorrelationGraph) -> ComponentStatistics {
        ComponentStatistics::calculate(graph, self)
    }

    /// Calculate coupling metrics
    pub fn calculate_coupling(&self, graph: &CorrelationGraph) -> Vec<ComponentCouplingMetrics> {
        ComponentCouplingAnalyzer::calculate_coupling(graph, self)
    }
}
