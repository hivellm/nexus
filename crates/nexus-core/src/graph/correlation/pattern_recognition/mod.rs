//! Pattern Recognition for Graph Correlation Analysis
//!
//! Detects common architectural and design patterns in code graphs:
//! - Pipeline patterns
//! - Event-driven patterns
//! - Architectural patterns
//! - Design patterns

mod detectors;
mod overlay;
mod quality;
mod recommendation;
mod types;

// ── Public re-exports ────────────────────────────────────────────────────────

pub use detectors::{
    ArchitecturalPatternDetector, DesignPatternDetector, EventDrivenPatternDetector,
    PipelinePatternDetector,
};
pub use overlay::{PatternOverlayConfig, apply_pattern_overlays};
pub use quality::{PatternMaturity, PatternQualityMetrics, calculate_pattern_quality_metrics};
pub use recommendation::{PatternDifficulty, PatternRecommendation, PatternRecommendationEngine};
pub use types::{
    DetectedPattern, PatternDetectionResult, PatternDetector, PatternStatistics, PatternType,
};

// ── Crate-visible helpers (previously private, needed by tests via `use super::*`) ──
pub(crate) use detectors::{
    calculate_quality_score, calculate_statistics, detect_factory_pattern, detect_observer_pattern,
    detect_singleton_pattern, detect_strategy_pattern,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::correlation::{
        CorrelationGraph, EdgeType, GraphEdge, GraphNode, GraphType, NodeType,
    };
    use std::collections::HashMap;

    fn create_test_graph() -> CorrelationGraph {
        let mut graph = CorrelationGraph::new(GraphType::Call, "Test".to_string());

        let node1 = GraphNode {
            id: "node1".to_string(),
            label: "Node 1".to_string(),
            node_type: NodeType::Function,
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };

        let node2 = GraphNode {
            id: "node2".to_string(),
            label: "Node 2".to_string(),
            node_type: NodeType::Function,
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };

        graph.add_node(node1).unwrap();
        graph.add_node(node2).unwrap();

        let edge = GraphEdge {
            id: "e1".to_string(),
            source: "node1".to_string(),
            target: "node2".to_string(),
            edge_type: EdgeType::Calls,
            weight: 1.0,
            metadata: HashMap::new(),
            label: None,
        };

        graph.add_edge(edge).unwrap();
        graph
    }

    #[test]
    fn test_pattern_statistics_default() {
        let stats = PatternStatistics::default();
        assert_eq!(stats.total_patterns, 0);
        assert_eq!(stats.avg_confidence, 0.0);
        assert!(stats.pattern_counts.is_empty());
    }

    #[test]
    fn test_pattern_types_all() {
        assert_eq!(format!("{:?}", PatternType::Pipeline), "Pipeline");
        assert_eq!(format!("{:?}", PatternType::EventDriven), "EventDriven");
        assert_eq!(
            format!("{:?}", PatternType::LayeredArchitecture),
            "LayeredArchitecture"
        );
        assert_eq!(format!("{:?}", PatternType::Microservices), "Microservices");
        assert_eq!(format!("{:?}", PatternType::Observer), "Observer");
        assert_eq!(format!("{:?}", PatternType::Factory), "Factory");
        assert_eq!(format!("{:?}", PatternType::Singleton), "Singleton");
        assert_eq!(format!("{:?}", PatternType::Strategy), "Strategy");
    }

    #[test]
    fn test_detected_pattern_creation() {
        let pattern = DetectedPattern {
            pattern_type: PatternType::Pipeline,
            node_ids: vec!["a".to_string(), "b".to_string()],
            confidence: 0.9,
            metadata: HashMap::new(),
        };

        assert_eq!(pattern.pattern_type, PatternType::Pipeline);
        assert_eq!(pattern.node_ids.len(), 2);
        assert_eq!(pattern.confidence, 0.9);
    }

    #[test]
    fn test_pipeline_detector_empty_graph() {
        let graph = CorrelationGraph::new(GraphType::Call, "Empty".to_string());

        let detector = PipelinePatternDetector;
        let result = detector.detect(&graph).unwrap();

        assert_eq!(result.patterns.len(), 0);
        assert_eq!(result.statistics.total_patterns, 0);
    }

    #[test]
    fn test_pipeline_detector_simple_chain() {
        let graph = create_test_graph();
        let detector = PipelinePatternDetector;
        let result = detector.detect(&graph).unwrap();

        assert!(result.quality_score >= 0.0);
        assert!(result.quality_score <= 1.0);
    }

    #[test]
    fn test_event_driven_detector_empty() {
        let graph = CorrelationGraph::new(GraphType::Call, "Empty".to_string());

        let detector = EventDrivenPatternDetector;
        let result = detector.detect(&graph).unwrap();

        assert_eq!(result.patterns.len(), 0);
    }

    #[test]
    fn test_architectural_detector_empty() {
        let graph = CorrelationGraph::new(GraphType::Call, "Empty".to_string());

        let detector = ArchitecturalPatternDetector;
        let result = detector.detect(&graph).unwrap();

        assert_eq!(result.patterns.len(), 0);
    }

    #[test]
    fn test_calculate_statistics_empty() {
        let patterns = vec![];
        let stats = calculate_statistics(&patterns);

        assert_eq!(stats.total_patterns, 0);
        assert_eq!(stats.avg_confidence, 0.0);
        assert!(stats.pattern_counts.is_empty());
    }

    #[test]
    fn test_calculate_statistics_with_patterns() {
        let patterns = vec![
            DetectedPattern {
                pattern_type: PatternType::Pipeline,
                node_ids: vec!["a".to_string()],
                confidence: 0.8,
                metadata: HashMap::new(),
            },
            DetectedPattern {
                pattern_type: PatternType::Pipeline,
                node_ids: vec!["b".to_string()],
                confidence: 0.6,
                metadata: HashMap::new(),
            },
        ];

        let stats = calculate_statistics(&patterns);

        assert_eq!(stats.total_patterns, 2);
        assert_eq!(stats.avg_confidence, 0.7);
        assert_eq!(stats.pattern_counts.get("Pipeline"), Some(&2));
    }

    #[test]
    fn test_calculate_quality_score_empty() {
        let patterns = vec![];
        let score = calculate_quality_score(&patterns);
        assert_eq!(score, 0.0);
    }

    #[test]
    fn test_calculate_quality_score_with_patterns() {
        let patterns = vec![
            DetectedPattern {
                pattern_type: PatternType::Pipeline,
                node_ids: vec![],
                confidence: 0.9,
                metadata: HashMap::new(),
            },
            DetectedPattern {
                pattern_type: PatternType::EventDriven,
                node_ids: vec![],
                confidence: 0.7,
                metadata: HashMap::new(),
            },
        ];

        let score = calculate_quality_score(&patterns);
        assert_eq!(score, 0.8);
    }

    #[test]
    fn test_pattern_detection_result_structure() {
        let result = PatternDetectionResult {
            patterns: vec![],
            statistics: PatternStatistics::default(),
            quality_score: 0.5,
        };

        assert_eq!(result.patterns.len(), 0);
        assert_eq!(result.quality_score, 0.5);
    }

    // ============================================================================
    // Design Pattern Tests (Task 13.9)
    // ============================================================================

    #[test]
    fn test_design_pattern_detector_name() {
        let detector = DesignPatternDetector;
        assert_eq!(detector.name(), "Design Pattern Detector");
    }

    #[test]
    fn test_design_pattern_detector_supported_patterns() {
        let detector = DesignPatternDetector;
        let supported = detector.supported_patterns();
        assert!(supported.contains(&PatternType::Observer));
        assert!(supported.contains(&PatternType::Factory));
        assert!(supported.contains(&PatternType::Singleton));
        assert!(supported.contains(&PatternType::Strategy));
    }

    #[test]
    fn test_detect_observer_pattern() {
        let mut graph = CorrelationGraph::new(GraphType::Component, "Observer".to_string());

        let subject = GraphNode {
            id: "subject".to_string(),
            label: "Subject".to_string(),
            node_type: NodeType::Class,
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };

        let observer1 = GraphNode {
            id: "observer1".to_string(),
            label: "Observer1".to_string(),
            node_type: NodeType::Class,
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };

        let observer2 = GraphNode {
            id: "observer2".to_string(),
            label: "Observer2".to_string(),
            node_type: NodeType::Class,
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };

        graph.add_node(subject).unwrap();
        graph.add_node(observer1).unwrap();
        graph.add_node(observer2).unwrap();

        graph
            .add_edge(GraphEdge {
                id: "e1".to_string(),
                source: "subject".to_string(),
                target: "observer1".to_string(),
                edge_type: EdgeType::Uses,
                weight: 1.0,
                metadata: HashMap::new(),
                label: None,
            })
            .unwrap();

        graph
            .add_edge(GraphEdge {
                id: "e2".to_string(),
                source: "subject".to_string(),
                target: "observer2".to_string(),
                edge_type: EdgeType::Uses,
                weight: 1.0,
                metadata: HashMap::new(),
                label: None,
            })
            .unwrap();

        let patterns = detect_observer_pattern(&graph);
        assert!(!patterns.is_empty());
        assert_eq!(patterns[0].pattern_type, PatternType::Observer);
    }

    #[test]
    fn test_detect_factory_pattern() {
        let mut graph = CorrelationGraph::new(GraphType::Component, "Factory".to_string());

        let factory = GraphNode {
            id: "factory".to_string(),
            label: "Factory".to_string(),
            node_type: NodeType::Class,
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };

        let product = GraphNode {
            id: "product".to_string(),
            label: "Product".to_string(),
            node_type: NodeType::Class,
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };

        graph.add_node(factory).unwrap();
        graph.add_node(product).unwrap();

        graph
            .add_edge(GraphEdge {
                id: "e1".to_string(),
                source: "factory".to_string(),
                target: "product".to_string(),
                edge_type: EdgeType::Uses,
                weight: 1.0,
                metadata: HashMap::new(),
                label: None,
            })
            .unwrap();

        let patterns = detect_factory_pattern(&graph);
        assert!(!patterns.is_empty());
        assert_eq!(patterns[0].pattern_type, PatternType::Factory);
    }

    #[test]
    fn test_detect_singleton_pattern() {
        let mut graph = CorrelationGraph::new(GraphType::Component, "Singleton".to_string());

        let singleton = GraphNode {
            id: "singleton".to_string(),
            label: "Singleton".to_string(),
            node_type: NodeType::Class,
            metadata: {
                let mut m = HashMap::new();
                m.insert("is_singleton".to_string(), serde_json::Value::Bool(true));
                m
            },
            position: None,
            size: None,
            color: None,
        };

        graph.add_node(singleton).unwrap();

        let patterns = detect_singleton_pattern(&graph);
        assert!(!patterns.is_empty());
        assert_eq!(patterns[0].pattern_type, PatternType::Singleton);
    }

    #[test]
    fn test_detect_strategy_pattern() {
        let mut graph = CorrelationGraph::new(GraphType::Component, "Strategy".to_string());

        let context = GraphNode {
            id: "context".to_string(),
            label: "Context".to_string(),
            node_type: NodeType::Class,
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };

        let strategy = GraphNode {
            id: "strategy".to_string(),
            label: "Strategy".to_string(),
            node_type: NodeType::Class,
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };

        graph.add_node(context).unwrap();
        graph.add_node(strategy).unwrap();

        graph
            .add_edge(GraphEdge {
                id: "e1".to_string(),
                source: "context".to_string(),
                target: "strategy".to_string(),
                edge_type: EdgeType::Uses,
                weight: 1.0,
                metadata: HashMap::new(),
                label: None,
            })
            .unwrap();

        let patterns = detect_strategy_pattern(&graph);
        assert!(!patterns.is_empty());
        assert_eq!(patterns[0].pattern_type, PatternType::Strategy);
    }

    // ============================================================================
    // Pattern Visualization Overlay Tests (Task 13.9)
    // ============================================================================

    #[test]
    fn test_pattern_overlay_config_default() {
        let config = PatternOverlayConfig::default();
        assert_eq!(config.pipeline_color, "#3498db");
        assert_eq!(config.observer_color, "#f39c12");
        assert!(config.highlight_nodes);
        assert!(config.highlight_edges);
    }

    #[test]
    fn test_apply_pattern_overlays() {
        let mut graph = CorrelationGraph::new(GraphType::Call, "Test".to_string());

        let node = GraphNode {
            id: "node1".to_string(),
            label: "Node1".to_string(),
            node_type: NodeType::Function,
            metadata: HashMap::new(),
            position: None,
            size: Some(8.0),
            color: None,
        };
        graph.add_node(node).unwrap();

        let patterns = vec![DetectedPattern {
            pattern_type: PatternType::Observer,
            node_ids: vec!["node1".to_string()],
            confidence: 0.8,
            metadata: HashMap::new(),
        }];

        let config = PatternOverlayConfig::default();
        let result = apply_pattern_overlays(&mut graph, &patterns, &config);
        assert!(result.is_ok());

        let node = graph.nodes.iter().find(|n| n.id == "node1").unwrap();
        assert_eq!(node.color, Some("#f39c12".to_string())); // Observer color
    }

    // ============================================================================
    // Pattern Quality Metrics Tests (Task 13.9)
    // ============================================================================

    #[test]
    fn test_calculate_pattern_quality_metrics() {
        let mut graph = CorrelationGraph::new(GraphType::Component, "Test".to_string());

        let node1 = GraphNode {
            id: "node1".to_string(),
            label: "Node1".to_string(),
            node_type: NodeType::Class,
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };

        let node2 = GraphNode {
            id: "node2".to_string(),
            label: "Node2".to_string(),
            node_type: NodeType::Class,
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };

        graph.add_node(node1).unwrap();
        graph.add_node(node2).unwrap();

        graph
            .add_edge(GraphEdge {
                id: "e1".to_string(),
                source: "node1".to_string(),
                target: "node2".to_string(),
                edge_type: EdgeType::Uses,
                weight: 1.0,
                metadata: HashMap::new(),
                label: None,
            })
            .unwrap();

        let pattern = DetectedPattern {
            pattern_type: PatternType::Observer,
            node_ids: vec!["node1".to_string(), "node2".to_string()],
            confidence: 0.8,
            metadata: HashMap::new(),
        };

        let metrics = calculate_pattern_quality_metrics(&pattern, &graph);
        assert_eq!(metrics.pattern_type, PatternType::Observer);
        assert_eq!(metrics.confidence, 0.8);
        assert!(metrics.quality_score >= 0.0 && metrics.quality_score <= 1.0);
        assert_eq!(metrics.node_count, 2);
    }

    #[test]
    fn test_pattern_maturity_levels() {
        assert!(PatternMaturity::Emerging < PatternMaturity::Developing);
        assert!(PatternMaturity::Developing < PatternMaturity::Mature);
        assert!(PatternMaturity::Mature < PatternMaturity::Established);
    }

    // ============================================================================
    // Pattern Recommendation Engine Tests (Task 13.9)
    // ============================================================================

    #[test]
    fn test_pattern_recommendation_engine_recommend_observer() {
        let mut graph = CorrelationGraph::new(GraphType::Call, "Test".to_string());

        let node1 = GraphNode {
            id: "node1".to_string(),
            label: "Node1".to_string(),
            node_type: NodeType::Function,
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };

        let node2 = GraphNode {
            id: "node2".to_string(),
            label: "Node2".to_string(),
            node_type: NodeType::Function,
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };

        let node3 = GraphNode {
            id: "node3".to_string(),
            label: "Node3".to_string(),
            node_type: NodeType::Function,
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };

        graph.add_node(node1).unwrap();
        graph.add_node(node2).unwrap();
        graph.add_node(node3).unwrap();

        // Both node2 and node3 depend on node1
        graph
            .add_edge(GraphEdge {
                id: "e1".to_string(),
                source: "node2".to_string(),
                target: "node1".to_string(),
                edge_type: EdgeType::Uses,
                weight: 1.0,
                metadata: HashMap::new(),
                label: None,
            })
            .unwrap();

        graph
            .add_edge(GraphEdge {
                id: "e2".to_string(),
                source: "node3".to_string(),
                target: "node1".to_string(),
                edge_type: EdgeType::Uses,
                weight: 1.0,
                metadata: HashMap::new(),
                label: None,
            })
            .unwrap();

        let recommendations = PatternRecommendationEngine::recommend_patterns(&graph, &[]);
        assert!(!recommendations.is_empty());
        let observer_rec = recommendations
            .iter()
            .find(|r| r.pattern_type == PatternType::Observer);
        assert!(observer_rec.is_some());
    }

    #[test]
    fn test_pattern_recommendation_priority_sorting() {
        let graph = CorrelationGraph::new(GraphType::Call, "Test".to_string());
        let recommendations = PatternRecommendationEngine::recommend_patterns(&graph, &[]);

        // Check that recommendations are sorted by priority (descending)
        for i in 1..recommendations.len() {
            assert!(recommendations[i - 1].priority >= recommendations[i].priority);
        }
    }

    #[test]
    fn test_pattern_difficulty_ordering() {
        assert!(PatternDifficulty::Easy < PatternDifficulty::Moderate);
        assert!(PatternDifficulty::Moderate < PatternDifficulty::Hard);
    }
}

// ============================================================================
// Integration Tests (Task 13.10)
// ============================================================================

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::graph::correlation::{
        CorrelationGraph, EdgeType, GraphEdge, GraphNode, GraphType, NodeType,
    };
    use std::collections::HashMap;

    fn create_observer_pattern_graph() -> CorrelationGraph {
        let mut graph = CorrelationGraph::new(GraphType::Component, "Observer Pattern".to_string());

        let subject = GraphNode {
            id: "subject".to_string(),
            label: "Subject".to_string(),
            node_type: NodeType::Class,
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };

        let observer1 = GraphNode {
            id: "observer1".to_string(),
            label: "Observer1".to_string(),
            node_type: NodeType::Class,
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };

        let observer2 = GraphNode {
            id: "observer2".to_string(),
            label: "Observer2".to_string(),
            node_type: NodeType::Class,
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };

        graph.add_node(subject).unwrap();
        graph.add_node(observer1).unwrap();
        graph.add_node(observer2).unwrap();

        graph
            .add_edge(GraphEdge {
                id: "e1".to_string(),
                source: "subject".to_string(),
                target: "observer1".to_string(),
                edge_type: EdgeType::Uses,
                weight: 1.0,
                metadata: HashMap::new(),
                label: None,
            })
            .unwrap();

        graph
            .add_edge(GraphEdge {
                id: "e2".to_string(),
                source: "subject".to_string(),
                target: "observer2".to_string(),
                edge_type: EdgeType::Uses,
                weight: 1.0,
                metadata: HashMap::new(),
                label: None,
            })
            .unwrap();

        graph
    }

    fn create_factory_pattern_graph() -> CorrelationGraph {
        let mut graph = CorrelationGraph::new(GraphType::Component, "Factory Pattern".to_string());

        let factory = GraphNode {
            id: "factory".to_string(),
            label: "Factory".to_string(),
            node_type: NodeType::Class,
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };

        let product1 = GraphNode {
            id: "product1".to_string(),
            label: "Product1".to_string(),
            node_type: NodeType::Class,
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };

        let product2 = GraphNode {
            id: "product2".to_string(),
            label: "Product2".to_string(),
            node_type: NodeType::Class,
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };

        graph.add_node(factory).unwrap();
        graph.add_node(product1).unwrap();
        graph.add_node(product2).unwrap();

        graph
            .add_edge(GraphEdge {
                id: "e1".to_string(),
                source: "factory".to_string(),
                target: "product1".to_string(),
                edge_type: EdgeType::Uses,
                weight: 1.0,
                metadata: HashMap::new(),
                label: None,
            })
            .unwrap();

        graph
            .add_edge(GraphEdge {
                id: "e2".to_string(),
                source: "factory".to_string(),
                target: "product2".to_string(),
                edge_type: EdgeType::Uses,
                weight: 1.0,
                metadata: HashMap::new(),
                label: None,
            })
            .unwrap();

        graph
    }

    #[test]
    fn test_complete_pattern_detection_workflow() {
        // Test complete workflow: detect patterns -> calculate metrics -> apply overlays -> get recommendations
        let graph = create_observer_pattern_graph();

        // Detect patterns
        let detector = DesignPatternDetector;
        let result = detector.detect(&graph).unwrap();

        assert!(!result.patterns.is_empty());
        assert!(result.quality_score >= 0.0 && result.quality_score <= 1.0);

        // Calculate quality metrics
        for pattern in &result.patterns {
            let metrics = calculate_pattern_quality_metrics(pattern, &graph);
            assert!(metrics.quality_score >= 0.0 && metrics.quality_score <= 1.0);
            assert!(metrics.completeness >= 0.0 && metrics.completeness <= 1.0);
            assert!(metrics.consistency >= 0.0 && metrics.consistency <= 1.0);
        }

        // Apply visualization overlays
        let mut overlay_graph = graph.clone();
        let config = PatternOverlayConfig::default();
        let overlay_result = apply_pattern_overlays(&mut overlay_graph, &result.patterns, &config);
        assert!(overlay_result.is_ok());

        // Get recommendations
        let recommendations =
            PatternRecommendationEngine::recommend_patterns(&graph, &result.patterns);
        // Should have fewer recommendations since Observer pattern already exists
        assert!(recommendations.len() <= 4); // Max 4 pattern types
    }

    #[test]
    fn test_multiple_pattern_types_detection() {
        // Test detection of multiple pattern types in one graph
        let mut graph = create_observer_pattern_graph();

        // Add singleton node
        let singleton = GraphNode {
            id: "singleton".to_string(),
            label: "Singleton".to_string(),
            node_type: NodeType::Class,
            metadata: {
                let mut m = HashMap::new();
                m.insert("is_singleton".to_string(), serde_json::Value::Bool(true));
                m
            },
            position: None,
            size: None,
            color: None,
        };
        graph.add_node(singleton).unwrap();

        let detector = DesignPatternDetector;
        let result = detector.detect(&graph).unwrap();

        // Should detect both Observer and Singleton patterns
        let pattern_types: Vec<PatternType> =
            result.patterns.iter().map(|p| p.pattern_type).collect();
        assert!(
            pattern_types.contains(&PatternType::Observer)
                || pattern_types.contains(&PatternType::Singleton)
        );
    }

    #[test]
    fn test_pattern_recommendation_with_existing_patterns() {
        let graph = create_factory_pattern_graph();

        // Detect existing patterns
        let detector = DesignPatternDetector;
        let detection_result = detector.detect(&graph).unwrap();

        // Get recommendations (should not recommend Factory since it exists)
        let recommendations =
            PatternRecommendationEngine::recommend_patterns(&graph, &detection_result.patterns);

        // Verify recommendations don't include Factory if it was detected
        let has_factory = detection_result
            .patterns
            .iter()
            .any(|p| p.pattern_type == PatternType::Factory);
        if has_factory {
            let factory_recommendations: Vec<_> = recommendations
                .iter()
                .filter(|r| r.pattern_type == PatternType::Factory)
                .collect();
            assert!(
                factory_recommendations.is_empty(),
                "Should not recommend Factory pattern if it already exists"
            );
        }
    }

    #[test]
    fn test_pattern_visualization_overlay_with_multiple_patterns() {
        let mut graph = create_observer_pattern_graph();

        // Add singleton node
        let singleton = GraphNode {
            id: "singleton".to_string(),
            label: "Singleton".to_string(),
            node_type: NodeType::Class,
            metadata: {
                let mut m = HashMap::new();
                m.insert("is_singleton".to_string(), serde_json::Value::Bool(true));
                m
            },
            position: None,
            size: None,
            color: None,
        };
        graph.add_node(singleton).unwrap();

        // Detect patterns
        let detector = DesignPatternDetector;
        let result = detector.detect(&graph).unwrap();

        // Apply overlays
        let config = PatternOverlayConfig::default();
        let overlay_result = apply_pattern_overlays(&mut graph, &result.patterns, &config);
        assert!(overlay_result.is_ok());

        // Verify nodes have been styled
        let styled_nodes: Vec<_> = graph.nodes.iter().filter(|n| n.color.is_some()).collect();
        assert!(
            !styled_nodes.is_empty(),
            "At least some nodes should have colors applied"
        );
    }

    #[test]
    fn test_pattern_quality_metrics_for_known_patterns() {
        let observer_graph = create_observer_pattern_graph();
        let factory_graph = create_factory_pattern_graph();

        // Test Observer pattern metrics
        let observer_detector = DesignPatternDetector;
        let observer_result = observer_detector.detect(&observer_graph).unwrap();

        for pattern in &observer_result.patterns {
            if pattern.pattern_type == PatternType::Observer {
                let metrics = calculate_pattern_quality_metrics(pattern, &observer_graph);
                assert_eq!(metrics.pattern_type, PatternType::Observer);
                assert!(metrics.node_count >= 2); // At least subject + observer
            }
        }

        // Test Factory pattern metrics
        let factory_result = observer_detector.detect(&factory_graph).unwrap();
        for pattern in &factory_result.patterns {
            if pattern.pattern_type == PatternType::Factory {
                let metrics = calculate_pattern_quality_metrics(pattern, &factory_graph);
                assert_eq!(metrics.pattern_type, PatternType::Factory);
                assert!(metrics.node_count >= 2); // At least factory + product
            }
        }
    }
}
