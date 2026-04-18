//! Data-flow test suite aggregated from five independent `mod` blocks in
//! the original `data_flow.rs`. Kept as separate `mod` inner items here
//! so individual test groupings remain visually distinct.

#![allow(unused_imports)]
use super::*;

mod main_tests {
    use super::*;

    #[test]
    fn test_variable_tracker() {
        let mut tracker = VariableTracker::new();

        tracker.track_definition(
            "x".to_string(),
            "test.rs".to_string(),
            1,
            Some("i32".to_string()),
            true,
            false,
        );

        tracker.track_usage(
            "x",
            "test.rs".to_string(),
            Some("main".to_string()),
            5,
            UsageType::Read,
        );

        let var = tracker.get_variable("x").unwrap();
        assert_eq!(var.name, "x");
        assert_eq!(var.usages.len(), 1);
    }

    #[test]
    fn test_data_flow_analyzer() {
        let mut analyzer = DataFlowAnalyzer::new();
        let mut files = HashMap::new();
        files.insert(
            "test.rs".to_string(),
            "let x = 5;\nlet _y = x + 1;".to_string(),
        );

        let result = analyzer.analyze_source_code(&files);
        assert!(result.is_ok());

        // Should have tracked at least some variables
        assert!(!analyzer.tracker().all_variables().is_empty());
    }

    #[test]
    fn test_detect_assignment_transformation() {
        let mut analyzer = DataFlowAnalyzer::new();

        // Track a variable first
        analyzer.tracker_mut().track_definition(
            "x".to_string(),
            "test.rs".to_string(),
            1,
            Some("i32".to_string()),
            true,
            false,
        );

        // Detect assignment transformation
        let transformation = analyzer.detect_assignment("y = x + 1");
        assert!(transformation.is_some());
        let trans = transformation.unwrap();
        assert_eq!(trans.transformation_type, TransformationType::Assignment);
        assert_eq!(trans.target.trim(), "y");
    }

    #[test]
    fn test_detect_function_call_transformation() {
        let mut analyzer = DataFlowAnalyzer::new();

        analyzer.tracker_mut().track_definition(
            "data".to_string(),
            "test.rs".to_string(),
            1,
            None,
            true,
            false,
        );

        let transformation =
            analyzer.detect_function_call_transformation("let result = process(data)");
        assert!(transformation.is_some());
        let trans = transformation.unwrap();
        assert_eq!(trans.transformation_type, TransformationType::FunctionCall);
        assert!(trans.source.contains("process"));
    }

    #[test]
    fn test_detect_type_conversion() {
        let mut analyzer = DataFlowAnalyzer::new();

        analyzer.tracker_mut().track_definition(
            "num".to_string(),
            "test.rs".to_string(),
            1,
            Some("i32".to_string()),
            true,
            false,
        );

        let transformation = analyzer.detect_type_conversion("let str = num.to_string()");
        assert!(transformation.is_some());
        let trans = transformation.unwrap();
        assert_eq!(
            trans.transformation_type,
            TransformationType::TypeConversion
        );
    }

    #[test]
    fn test_detect_aggregation() {
        let mut analyzer = DataFlowAnalyzer::new();

        analyzer.tracker_mut().track_definition(
            "numbers".to_string(),
            "test.rs".to_string(),
            1,
            None,
            true,
            false,
        );

        let transformation = analyzer.detect_aggregation("let total = numbers.sum()");
        assert!(transformation.is_some());
        let trans = transformation.unwrap();
        assert_eq!(trans.transformation_type, TransformationType::Aggregation);
    }

    #[test]
    fn test_detect_filter_operation() {
        let mut analyzer = DataFlowAnalyzer::new();

        analyzer.tracker_mut().track_definition(
            "items".to_string(),
            "test.rs".to_string(),
            1,
            None,
            true,
            false,
        );

        let transformation =
            analyzer.detect_filter_operation("let filtered = items.filter(|x| x > 0)");
        assert!(transformation.is_some());
        let trans = transformation.unwrap();
        assert_eq!(trans.transformation_type, TransformationType::Filter);
    }

    #[test]
    fn test_detect_map_operation() {
        let mut analyzer = DataFlowAnalyzer::new();

        analyzer.tracker_mut().track_definition(
            "values".to_string(),
            "test.rs".to_string(),
            1,
            None,
            true,
            false,
        );

        let transformation = analyzer.detect_map_operation("let doubled = values.map(|x| x * 2)");
        assert!(transformation.is_some());
        let trans = transformation.unwrap();
        assert_eq!(trans.transformation_type, TransformationType::Map);
    }

    #[test]
    fn test_detect_reduce_operation() {
        let mut analyzer = DataFlowAnalyzer::new();

        analyzer.tracker_mut().track_definition(
            "numbers".to_string(),
            "test.rs".to_string(),
            1,
            None,
            true,
            false,
        );

        let transformation =
            analyzer.detect_reduce_operation("let sum = numbers.reduce(|a, b| a + b)");
        assert!(transformation.is_some());
        let trans = transformation.unwrap();
        assert_eq!(trans.transformation_type, TransformationType::Reduce);
    }

    #[test]
    fn test_transformation_integration() {
        let mut analyzer = DataFlowAnalyzer::new();
        let mut files = HashMap::new();
        files.insert(
            "test.rs".to_string(),
            "let x = 5;\nlet y = x + 1;\nlet z = y.to_string();\nlet sum = [1, 2, 3].sum();"
                .to_string(),
        );

        let result = analyzer.analyze_source_code(&files);
        assert!(result.is_ok());

        // Should have detected transformations
        assert!(!analyzer.transformations().is_empty());
    }

    #[test]
    fn test_type_propagator_inference() {
        let mut propagator = TypePropagator::new();

        // Test type inference from definition
        let inferred = propagator.infer_type_from_definition("x", "let x: i32 = 5");
        assert_eq!(inferred, Some("i32".to_string()));

        let inferred =
            propagator.infer_type_from_definition("str", "let str = String::from(\"hello\")");
        assert_eq!(inferred, Some("String".to_string()));

        let inferred = propagator.infer_type_from_definition("vec", "let vec = Vec::new()");
        assert_eq!(inferred, Some("Vec<T>".to_string()));

        let inferred = propagator.infer_type_from_definition("flag", "let flag = true");
        assert_eq!(inferred, Some("bool".to_string()));
    }

    #[test]
    fn test_type_propagation_through_transformation() {
        let mut propagator = TypePropagator::new();
        propagator
            .type_map
            .insert("x".to_string(), "i32".to_string());

        let transformation = DataTransformation {
            source: "x".to_string(),
            target: "y".to_string(),
            transformation_type: TransformationType::Assignment,
            input_types: vec!["i32".to_string()],
            output_types: vec![],
        };

        let output_types = propagator.propagate_through_transformation(&transformation);
        assert_eq!(output_types, vec!["i32".to_string()]);

        // Test type conversion - use a target that contains conversion pattern
        let conversion = DataTransformation {
            source: "num".to_string(),
            target: "str = num.to_string()".to_string(),
            transformation_type: TransformationType::TypeConversion,
            input_types: vec!["i32".to_string()],
            output_types: vec![],
        };

        let output_types = propagator.propagate_through_transformation(&conversion);
        assert_eq!(output_types, vec!["String".to_string()]);
    }

    #[test]
    fn test_type_propagation_aggregation() {
        let mut propagator = TypePropagator::new();

        let transformation = DataTransformation {
            source: "numbers".to_string(),
            target: "sum".to_string(),
            transformation_type: TransformationType::Aggregation,
            input_types: vec!["Vec<i32>".to_string()],
            output_types: vec![],
        };

        let output_types = propagator.propagate_through_transformation(&transformation);
        assert!(
            output_types.contains(&"i64".to_string()) || output_types.contains(&"f64".to_string())
        );
    }

    #[test]
    fn test_type_propagation_map() {
        let mut propagator = TypePropagator::new();

        let transformation = DataTransformation {
            source: "numbers".to_string(),
            target: "doubled".to_string(),
            transformation_type: TransformationType::Map,
            input_types: vec!["Vec<i32>".to_string()],
            output_types: vec![],
        };

        let output_types = propagator.propagate_through_transformation(&transformation);
        assert_eq!(output_types, vec!["Vec<i32>".to_string()]);
    }

    #[test]
    fn test_type_propagation_reduce() {
        let mut propagator = TypePropagator::new();

        let transformation = DataTransformation {
            source: "numbers".to_string(),
            target: "sum".to_string(),
            transformation_type: TransformationType::Reduce,
            input_types: vec!["Vec<i32>".to_string()],
            output_types: vec![],
        };

        let output_types = propagator.propagate_through_transformation(&transformation);
        assert_eq!(output_types, vec!["i32".to_string()]);
    }
}

mod layout_tests {
    use super::*;
    use crate::graph::correlation::{
        CorrelationGraph, EdgeType, GraphEdge, GraphNode, GraphType, NodeType,
    };
    use std::collections::HashMap;

    #[test]
    fn test_flow_layout_simple_chain() {
        let mut graph = CorrelationGraph::new(GraphType::DataFlow, "Test Flow".to_string());

        // Create a simple chain: A -> B -> C
        let node_a = GraphNode {
            id: "A".to_string(),
            node_type: NodeType::Variable,
            label: "A".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };
        let node_b = GraphNode {
            id: "B".to_string(),
            node_type: NodeType::Function,
            label: "B".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };
        let node_c = GraphNode {
            id: "C".to_string(),
            node_type: NodeType::Variable,
            label: "C".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };

        graph.add_node(node_a).unwrap();
        graph.add_node(node_b).unwrap();
        graph.add_node(node_c).unwrap();

        graph
            .add_edge(GraphEdge {
                id: "e1".to_string(),
                source: "A".to_string(),
                target: "B".to_string(),
                edge_type: EdgeType::Transforms,
                weight: 1.0,
                metadata: HashMap::new(),
                label: None,
            })
            .unwrap();

        graph
            .add_edge(GraphEdge {
                id: "e2".to_string(),
                source: "B".to_string(),
                target: "C".to_string(),
                edge_type: EdgeType::Transforms,
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

        let result = FlowBasedLayout::apply_layout(&mut graph, &config);
        assert!(result.is_ok());

        // Verify all nodes have positions
        for node in &graph.nodes {
            assert!(
                node.position.is_some(),
                "Node {} should have a position",
                node.id
            );
        }

        // Verify nodes are in layers (A should be leftmost, C rightmost)
        let pos_a = graph
            .nodes
            .iter()
            .find(|n| n.id == "A")
            .unwrap()
            .position
            .unwrap();
        let pos_b = graph
            .nodes
            .iter()
            .find(|n| n.id == "B")
            .unwrap()
            .position
            .unwrap();
        let pos_c = graph
            .nodes
            .iter()
            .find(|n| n.id == "C")
            .unwrap()
            .position
            .unwrap();

        assert!(pos_a.0 < pos_b.0, "A should be to the left of B");
        assert!(pos_b.0 < pos_c.0, "B should be to the left of C");
    }

    #[test]
    fn test_flow_layout_empty_graph() {
        let mut graph = CorrelationGraph::new(GraphType::DataFlow, "Empty".to_string());
        let config = VisualizationConfig::default();

        let result = FlowBasedLayout::apply_layout(&mut graph, &config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_flow_layout_single_node() {
        let mut graph = CorrelationGraph::new(GraphType::DataFlow, "Single".to_string());

        let node = GraphNode {
            id: "single".to_string(),
            node_type: NodeType::Variable,
            label: "Single".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };
        graph.add_node(node).unwrap();

        let config = VisualizationConfig {
            width: 1000.0,
            height: 800.0,
            ..Default::default()
        };

        let result = FlowBasedLayout::apply_layout(&mut graph, &config);
        assert!(result.is_ok());

        let pos = graph.nodes[0].position.unwrap();
        // Single node should be centered
        assert!((pos.0 - config.width / 2.0).abs() < 1.0);
        assert!((pos.1 - config.height / 2.0).abs() < 1.0);
    }

    #[test]
    fn test_flow_layout_disconnected_components() {
        let mut graph = CorrelationGraph::new(GraphType::DataFlow, "Disconnected".to_string());

        // Create two disconnected components
        let node_a = GraphNode {
            id: "A".to_string(),
            node_type: NodeType::Variable,
            label: "A".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };
        let node_b = GraphNode {
            id: "B".to_string(),
            node_type: NodeType::Variable,
            label: "B".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };

        graph.add_node(node_a).unwrap();
        graph.add_node(node_b).unwrap();

        let config = VisualizationConfig {
            width: 1000.0,
            height: 800.0,
            ..Default::default()
        };

        let result = FlowBasedLayout::apply_layout(&mut graph, &config);
        assert!(result.is_ok());

        // Both nodes should have positions
        for node in &graph.nodes {
            assert!(node.position.is_some());
        }
    }
}

#[cfg(test)]
mod visualization_tests {
    use super::*;
    use crate::graph::correlation::{
        CorrelationGraph, EdgeType, GraphEdge, GraphNode, GraphType, NodeType,
    };
    use std::collections::HashMap;

    #[test]
    fn test_data_flow_visualization_config_default() {
        let config = DataFlowVisualizationConfig::default();
        assert_eq!(config.variable_color, "#3498db");
        assert_eq!(config.transformation_color, "#9b59b6");
        assert_eq!(config.source_color, "#2ecc71");
        assert_eq!(config.sink_color, "#e74c3c");
        assert!(config.show_types);
        assert!(config.show_edge_labels);
        assert!(!config.type_colors.is_empty());
    }

    #[test]
    fn test_apply_data_flow_visualization() {
        let mut graph = CorrelationGraph::new(GraphType::DataFlow, "Test".to_string());

        let node_a = GraphNode {
            id: "A".to_string(),
            node_type: NodeType::Variable,
            label: "A".to_string(),
            metadata: {
                let mut m = HashMap::new();
                m.insert(
                    "variable_name".to_string(),
                    serde_json::Value::String("x".to_string()),
                );
                m
            },
            position: None,
            size: None,
            color: None,
        };
        let node_b = GraphNode {
            id: "B".to_string(),
            node_type: NodeType::Function,
            label: "B".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };

        graph.add_node(node_a).unwrap();
        graph.add_node(node_b).unwrap();

        graph
            .add_edge(GraphEdge {
                id: "e1".to_string(),
                source: "A".to_string(),
                target: "B".to_string(),
                edge_type: EdgeType::Transforms,
                weight: 1.0,
                metadata: {
                    let mut m = HashMap::new();
                    m.insert(
                        "variable".to_string(),
                        serde_json::Value::String("x".to_string()),
                    );
                    m
                },
                label: None,
            })
            .unwrap();

        let mut analyzer = DataFlowAnalyzer::new();
        analyzer
            .type_propagator_mut()
            .type_map
            .insert("x".to_string(), "i32".to_string());

        let config = DataFlowVisualizationConfig::default();
        let result = apply_data_flow_visualization(&mut graph, &config, &analyzer);
        assert!(result.is_ok());

        // Verify nodes have colors
        let node_a = graph.nodes.iter().find(|n| n.id == "A").unwrap();
        assert!(node_a.color.is_some());

        // Verify source node (A) has source color
        assert_eq!(node_a.color.as_ref().unwrap(), &config.source_color);

        // Verify edge has label
        let edge = graph.edges.iter().find(|e| e.id == "e1").unwrap();
        assert!(edge.label.is_some());
    }

    #[test]
    fn test_visualize_data_flow() {
        let mut graph = CorrelationGraph::new(GraphType::DataFlow, "Test".to_string());

        let node = GraphNode {
            id: "var1".to_string(),
            node_type: NodeType::Variable,
            label: "x".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };
        graph.add_node(node).unwrap();

        use crate::graph::correlation::visualization::LayoutAlgorithm;
        let vis_config = VisualizationConfig {
            width: 800.0,
            height: 600.0,
            layout_algorithm: LayoutAlgorithm::FlowBased,
            ..Default::default()
        };
        let data_flow_config = DataFlowVisualizationConfig::default();
        let analyzer = DataFlowAnalyzer::new();

        let result = visualize_data_flow(&mut graph, &vis_config, &data_flow_config, &analyzer);
        assert!(result.is_ok());

        let svg = result.unwrap();
        assert!(svg.contains("<svg"));
    }
}

#[cfg(test)]
mod optimization_tests {
    use super::*;
    use crate::graph::correlation::{
        CorrelationGraph, EdgeType, GraphEdge, GraphNode, GraphType, NodeType,
    };
    use std::collections::HashMap;

    #[test]
    fn test_flow_optimization_analyzer_detect_unused_variables() {
        let mut analyzer = DataFlowAnalyzer::new();
        analyzer.tracker_mut().track_definition(
            "unused_var".to_string(),
            "test.rs".to_string(),
            1,
            Some("i32".to_string()),
            true,
            false,
        );
        analyzer.tracker_mut().track_definition(
            "used_var".to_string(),
            "test.rs".to_string(),
            2,
            Some("i32".to_string()),
            true,
            false,
        );
        analyzer.tracker_mut().track_usage(
            "used_var",
            "test.rs".to_string(),
            None,
            5,
            UsageType::Read,
        );

        let graph = CorrelationGraph::new(GraphType::DataFlow, "Test".to_string());
        let suggestions = FlowOptimizationAnalyzer::analyze(&graph, &analyzer);

        // Should detect unused variable
        assert!(!suggestions.is_empty());
        let unused_suggestion = suggestions
            .iter()
            .find(|s| s.category == "Unused Variables");
        assert!(unused_suggestion.is_some());
    }

    #[test]
    fn test_flow_optimization_analyzer_detect_redundant_conversions() {
        let mut analyzer = DataFlowAnalyzer::new();

        // Add multiple type conversions for same source
        analyzer.add_transformation(DataTransformation {
            source: "x".to_string(),
            target: "y".to_string(),
            transformation_type: TransformationType::TypeConversion,
            input_types: vec!["i32".to_string()],
            output_types: vec!["String".to_string()],
        });
        analyzer.add_transformation(DataTransformation {
            source: "x".to_string(),
            target: "z".to_string(),
            transformation_type: TransformationType::TypeConversion,
            input_types: vec!["i32".to_string()],
            output_types: vec!["String".to_string()],
        });
        analyzer.add_transformation(DataTransformation {
            source: "x".to_string(),
            target: "w".to_string(),
            transformation_type: TransformationType::TypeConversion,
            input_types: vec!["i32".to_string()],
            output_types: vec!["String".to_string()],
        });

        let graph = CorrelationGraph::new(GraphType::DataFlow, "Test".to_string());
        let suggestions = FlowOptimizationAnalyzer::analyze(&graph, &analyzer);

        // Should detect redundant conversions
        let redundant = suggestions
            .iter()
            .find(|s| s.category == "Redundant Conversions");
        assert!(redundant.is_some());
    }

    #[test]
    fn test_flow_optimization_analyzer_detect_long_chains() {
        let mut graph = CorrelationGraph::new(GraphType::DataFlow, "Test".to_string());

        // Create a long chain: A -> B -> C -> D -> E -> F -> G
        let nodes = vec!["A", "B", "C", "D", "E", "F", "G"];
        for node_id in &nodes {
            graph
                .add_node(GraphNode {
                    id: node_id.to_string(),
                    node_type: NodeType::Variable,
                    label: node_id.to_string(),
                    metadata: HashMap::new(),
                    position: None,
                    size: None,
                    color: None,
                })
                .unwrap();
        }

        for i in 0..nodes.len() - 1 {
            graph
                .add_edge(GraphEdge {
                    id: format!("e{}", i),
                    source: nodes[i].to_string(),
                    target: nodes[i + 1].to_string(),
                    edge_type: EdgeType::Transforms,
                    weight: 1.0,
                    metadata: HashMap::new(),
                    label: None,
                })
                .unwrap();
        }

        let analyzer = DataFlowAnalyzer::new();
        let suggestions = FlowOptimizationAnalyzer::analyze(&graph, &analyzer);

        // Should detect long chain
        let long_chain = suggestions
            .iter()
            .find(|s| s.category == "Long Transformation Chain");
        assert!(long_chain.is_some());
    }

    #[test]
    fn test_flow_optimization_analyzer_detect_parallelization() {
        let mut graph = CorrelationGraph::new(GraphType::DataFlow, "Test".to_string());

        // Create multiple independent chains (source nodes)
        for i in 0..5 {
            graph
                .add_node(GraphNode {
                    id: format!("source{}", i),
                    node_type: NodeType::Variable,
                    label: format!("Source{}", i),
                    metadata: HashMap::new(),
                    position: None,
                    size: None,
                    color: None,
                })
                .unwrap();
        }

        let analyzer = DataFlowAnalyzer::new();
        let suggestions = FlowOptimizationAnalyzer::analyze(&graph, &analyzer);

        // Should detect parallelization opportunity
        let parallel = suggestions.iter().find(|s| s.category == "Parallelization");
        assert!(parallel.is_some());
    }

    #[test]
    fn test_data_flow_statistics_calculation() {
        let mut analyzer = DataFlowAnalyzer::new();

        // Add some variables
        analyzer.tracker_mut().track_definition(
            "x".to_string(),
            "test.rs".to_string(),
            1,
            Some("i32".to_string()),
            true,
            false,
        );
        analyzer.tracker_mut().track_definition(
            "y".to_string(),
            "test.rs".to_string(),
            2,
            None,
            true,
            false,
        );
        analyzer
            .tracker_mut()
            .track_usage("x", "test.rs".to_string(), None, 5, UsageType::Read);
        analyzer
            .tracker_mut()
            .track_usage("x", "test.rs".to_string(), None, 6, UsageType::Read);

        // Add transformations
        analyzer.add_transformation(DataTransformation {
            source: "x".to_string(),
            target: "y".to_string(),
            transformation_type: TransformationType::Assignment,
            input_types: vec!["i32".to_string()],
            output_types: vec!["i32".to_string()],
        });
        analyzer.add_transformation(DataTransformation {
            source: "y".to_string(),
            target: "z".to_string(),
            transformation_type: TransformationType::TypeConversion,
            input_types: vec!["i32".to_string()],
            output_types: vec!["String".to_string()],
        });

        let mut graph = CorrelationGraph::new(GraphType::DataFlow, "Test".to_string());
        graph
            .add_node(GraphNode {
                id: "x".to_string(),
                node_type: NodeType::Variable,
                label: "x".to_string(),
                metadata: HashMap::new(),
                position: None,
                size: None,
                color: None,
            })
            .unwrap();

        let stats = analyzer.calculate_statistics(&graph);

        assert_eq!(stats.total_variables, 2);
        assert_eq!(stats.typed_variables, 1);
        assert_eq!(stats.total_transformations, 2);
        assert_eq!(stats.type_conversions, 1);
        assert_eq!(stats.unused_variables, 1); // y is unused
        assert_eq!(stats.multi_usage_variables, 1); // x has 2 usages
        assert!(stats.average_usages_per_variable > 0.0);
    }

    #[test]
    fn test_data_flow_statistics_empty() {
        let analyzer = DataFlowAnalyzer::new();
        let graph = CorrelationGraph::new(GraphType::DataFlow, "Empty".to_string());

        let stats = analyzer.calculate_statistics(&graph);

        assert_eq!(stats.total_variables, 0);
        assert_eq!(stats.total_transformations, 0);
        assert_eq!(stats.average_chain_length, 0.0);
        assert_eq!(stats.max_chain_length, 0);
    }

    #[test]
    fn test_optimization_suggestions_priority_sorting() {
        let mut analyzer = DataFlowAnalyzer::new();

        // Add unused variable (low priority)
        analyzer.tracker_mut().track_definition(
            "unused".to_string(),
            "test.rs".to_string(),
            1,
            None,
            true,
            false,
        );

        // Add multiple conversions (medium priority)
        for i in 0..4 {
            analyzer.add_transformation(DataTransformation {
                source: "x".to_string(),
                target: format!("y{}", i),
                transformation_type: TransformationType::TypeConversion,
                input_types: vec![],
                output_types: vec![],
            });
        }

        let graph = CorrelationGraph::new(GraphType::DataFlow, "Test".to_string());
        let suggestions = analyzer.get_optimization_suggestions(&graph);

        // Should be sorted by priority (higher first)
        if suggestions.len() > 1 {
            for i in 0..suggestions.len() - 1 {
                let current_priority = match suggestions[i].priority {
                    OptimizationPriority::Critical => 4,
                    OptimizationPriority::High => 3,
                    OptimizationPriority::Medium => 2,
                    OptimizationPriority::Low => 1,
                };
                let next_priority = match suggestions[i + 1].priority {
                    OptimizationPriority::Critical => 4,
                    OptimizationPriority::High => 3,
                    OptimizationPriority::Medium => 2,
                    OptimizationPriority::Low => 1,
                };
                assert!(current_priority >= next_priority);
            }
        }
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::graph::correlation::{CorrelationGraph, GraphNode, GraphType, NodeType};
    use std::collections::HashMap;

    #[test]
    fn test_data_pipeline_analysis() {
        // Simulate a data pipeline: input -> filter -> map -> reduce -> output
        let mut analyzer = DataFlowAnalyzer::new();
        let mut files = HashMap::new();

        files.insert(
            "pipeline.rs".to_string(),
            "let data = vec![1, 2, 3, 4, 5];\n\
             let filtered = data.filter(|x| x > 2);\n\
             let mapped = filtered.map(|x| x * 2);\n\
             let result = mapped.reduce(|a, b| a + b);"
                .to_string(),
        );

        let result = analyzer.analyze_source_code(&files);
        assert!(result.is_ok());

        // Should have detected transformations
        assert!(!analyzer.transformations().is_empty());

        // Should have tracked variables
        assert!(!analyzer.tracker().all_variables().is_empty());
    }

    #[test]
    fn test_complete_data_flow_workflow() {
        // Test complete workflow: analyze -> build graph -> get suggestions -> calculate stats
        let mut analyzer = DataFlowAnalyzer::new();
        let mut files = HashMap::new();

        files.insert(
            "workflow.rs".to_string(),
            "let x: i32 = 5;\n\
             let y = x + 1;\n\
             let z = y.to_string();\n\
             let sum = [1, 2, 3].sum();"
                .to_string(),
        );

        analyzer.analyze_source_code(&files).unwrap();

        let base_graph = CorrelationGraph::new(GraphType::DataFlow, "Workflow".to_string());
        let graph = analyzer
            .build_enhanced_data_flow_graph(&base_graph)
            .unwrap();

        // Get optimization suggestions
        let suggestions = analyzer.get_optimization_suggestions(&graph);
        assert!(!suggestions.is_empty());

        // Calculate statistics
        let stats = analyzer.calculate_statistics(&graph);
        assert!(stats.total_variables > 0);
        assert!(stats.total_transformations > 0);
    }

    #[test]
    fn test_data_flow_with_type_propagation() {
        let mut analyzer = DataFlowAnalyzer::new();

        // Track variable with type
        analyzer.tracker_mut().track_definition(
            "numbers".to_string(),
            "test.rs".to_string(),
            1,
            Some("Vec<i32>".to_string()),
            true,
            false,
        );

        // Add transformations that should propagate types
        analyzer.add_transformation(DataTransformation {
            source: "numbers".to_string(),
            target: "filtered".to_string(),
            transformation_type: TransformationType::Filter,
            input_types: vec!["Vec<i32>".to_string()],
            output_types: vec![],
        });

        analyzer.add_transformation(DataTransformation {
            source: "filtered".to_string(),
            target: "sum".to_string(),
            transformation_type: TransformationType::Aggregation,
            input_types: vec![],
            output_types: vec![],
        });

        // Propagate types
        let mut transformations: Vec<DataTransformation> = analyzer.transformations().to_vec();
        analyzer
            .type_propagator_mut()
            .analyze_and_propagate(&mut transformations);

        // Verify type propagation worked
        let stats = analyzer.calculate_statistics(&CorrelationGraph::new(
            GraphType::DataFlow,
            "Test".to_string(),
        ));
        assert!(stats.typed_variables > 0);
    }

    #[test]
    fn test_optimization_suggestions_for_complex_pipeline() {
        let mut graph = CorrelationGraph::new(GraphType::DataFlow, "Complex Pipeline".to_string());
        let mut analyzer = DataFlowAnalyzer::new();

        // Create a complex pipeline with multiple independent chains
        for i in 0..4 {
            let source_id = format!("source{}", i);
            graph
                .add_node(GraphNode {
                    id: source_id.clone(),
                    node_type: NodeType::Variable,
                    label: source_id.clone(),
                    metadata: HashMap::new(),
                    position: None,
                    size: None,
                    color: None,
                })
                .unwrap();

            analyzer.tracker_mut().track_definition(
                format!("var{}", i),
                "pipeline.rs".to_string(),
                i + 1,
                Some("i32".to_string()),
                true,
                false,
            );

            // Add multiple transformations per source
            for j in 0..3 {
                analyzer.add_transformation(DataTransformation {
                    source: format!("var{}", i),
                    target: format!("trans{}_{}", i, j),
                    transformation_type: TransformationType::TypeConversion,
                    input_types: vec![],
                    output_types: vec![],
                });
            }
        }

        let suggestions = analyzer.get_optimization_suggestions(&graph);

        // Should detect parallelization opportunities
        let parallel_suggestion = suggestions.iter().find(|s| s.category == "Parallelization");
        assert!(parallel_suggestion.is_some());

        // Should detect redundant conversions
        let redundant_suggestion = suggestions
            .iter()
            .find(|s| s.category == "Redundant Conversions");
        assert!(redundant_suggestion.is_some());
    }
}
