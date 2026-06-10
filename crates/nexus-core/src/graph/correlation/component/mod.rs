// Submodule declarations
mod analyzer;
mod layout;
mod metrics;
mod types;
mod visualization;

// Re-export everything previously reachable at crate::graph::correlation::component::*
pub use analyzer::ComponentAnalyzer;
pub use layout::{OOHierarchyLayout, apply_oop_hierarchy_layout};
pub use metrics::{ComponentCouplingAnalyzer, ComponentCouplingMetrics, ComponentStatistics};
pub use types::{
    ClassInfo, ComponentRelationship, ComponentRelationshipInfo, FieldInfo, InterfaceInfo,
    MethodInfo, ParameterInfo, PropertyInfo,
};
pub use visualization::{ComponentVisualizationConfig, apply_component_visualization};

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
        let mut files = std::collections::HashMap::new();
        files.insert("test.rs".to_string(),
            "class BaseClass {}\ninterface MyInterface {}\nclass Child extends BaseClass implements MyInterface {}".to_string());

        let result = analyzer.analyze_source_code(&files);
        assert!(result.is_ok());
        assert!(!analyzer.classes().is_empty());
        assert!(!analyzer.interfaces().is_empty());
    }

    #[test]
    fn test_oop_hierarchy_layout() {
        let mut graph = crate::graph::correlation::CorrelationGraph::new(
            crate::graph::correlation::GraphType::Component,
            "Test".to_string(),
        );

        // Create inheritance hierarchy: Base -> Child -> GrandChild
        let base = crate::graph::correlation::GraphNode {
            id: "Base".to_string(),
            node_type: crate::graph::correlation::NodeType::Class,
            label: "Base".to_string(),
            metadata: {
                let mut m = std::collections::HashMap::new();
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
        let child = crate::graph::correlation::GraphNode {
            id: "Child".to_string(),
            node_type: crate::graph::correlation::NodeType::Class,
            label: "Child".to_string(),
            metadata: {
                let mut m = std::collections::HashMap::new();
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
            .add_edge(crate::graph::correlation::GraphEdge {
                id: "e1".to_string(),
                source: "Child".to_string(),
                target: "Base".to_string(),
                edge_type: crate::graph::correlation::EdgeType::Inherits,
                weight: 1.0,
                metadata: std::collections::HashMap::new(),
                label: None,
            })
            .unwrap();

        let config = crate::graph::correlation::visualization::VisualizationConfig {
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

        let graph = crate::graph::correlation::CorrelationGraph::new(
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
        let mut graph = crate::graph::correlation::CorrelationGraph::new(
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
            .add_node(crate::graph::correlation::GraphNode {
                id: "A".to_string(),
                node_type: crate::graph::correlation::NodeType::Class,
                label: "A".to_string(),
                metadata: std::collections::HashMap::new(),
                position: None,
                size: None,
                color: None,
            })
            .unwrap();

        graph
            .add_node(crate::graph::correlation::GraphNode {
                id: "B".to_string(),
                node_type: crate::graph::correlation::NodeType::Class,
                label: "B".to_string(),
                metadata: std::collections::HashMap::new(),
                position: None,
                size: None,
                color: None,
            })
            .unwrap();

        graph
            .add_edge(crate::graph::correlation::GraphEdge {
                id: "e1".to_string(),
                source: "B".to_string(),
                target: "A".to_string(),
                edge_type: crate::graph::correlation::EdgeType::Inherits,
                weight: 1.0,
                metadata: std::collections::HashMap::new(),
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
        let config = crate::graph::correlation::visualization::VisualizationConfig {
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
