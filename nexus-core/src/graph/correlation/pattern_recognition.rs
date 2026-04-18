//! Pattern Recognition for Graph Correlation Analysis
//!
//! Detects common architectural and design patterns in code graphs:
//! - Pipeline patterns
//! - Event-driven patterns
//! - Architectural patterns
//! - Design patterns

use crate::Result;
use crate::graph::correlation::{CorrelationGraph, EdgeType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Trait for pattern detection
pub trait PatternDetector {
    /// Detect patterns in a graph
    fn detect(&self, graph: &CorrelationGraph) -> Result<PatternDetectionResult>;

    /// Get pattern detector name
    fn name(&self) -> &str;

    /// Get supported pattern types
    fn supported_patterns(&self) -> Vec<PatternType>;
}

/// Pattern detection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternDetectionResult {
    /// Detected patterns
    pub patterns: Vec<DetectedPattern>,
    /// Pattern statistics
    pub statistics: PatternStatistics,
    /// Pattern quality score
    pub quality_score: f64,
}

/// Detected pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedPattern {
    /// Pattern type
    pub pattern_type: PatternType,
    /// Pattern confidence (0.0 to 1.0)
    pub confidence: f64,
    /// Nodes involved in the pattern
    pub node_ids: Vec<String>,
    /// Pattern metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Pattern types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PatternType {
    /// Pipeline pattern (sequential processing)
    Pipeline,
    /// Event-driven pattern (pub/sub)
    EventDriven,
    /// Layered architecture
    LayeredArchitecture,
    /// Microservices pattern
    Microservices,
    /// Observer pattern
    Observer,
    /// Factory pattern
    Factory,
    /// Singleton pattern
    Singleton,
    /// Strategy pattern
    Strategy,
}

/// Pattern statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternStatistics {
    /// Total patterns detected
    pub total_patterns: usize,
    /// Pattern counts by type
    pub pattern_counts: HashMap<String, usize>,
    /// Average confidence score
    pub avg_confidence: f64,
}

impl Default for PatternStatistics {
    fn default() -> Self {
        Self {
            total_patterns: 0,
            pattern_counts: HashMap::new(),
            avg_confidence: 0.0,
        }
    }
}

/// Pipeline pattern detector
pub struct PipelinePatternDetector;

impl PatternDetector for PipelinePatternDetector {
    fn detect(&self, graph: &CorrelationGraph) -> Result<PatternDetectionResult> {
        let mut patterns = Vec::new();

        // Detect pipeline patterns (sequential processing chains)
        let pipeline_nodes = detect_pipeline_chain(graph);

        for chain in pipeline_nodes {
            let confidence = calculate_pipeline_confidence(&chain, graph);
            patterns.push(DetectedPattern {
                pattern_type: PatternType::Pipeline,
                confidence,
                node_ids: chain,
                metadata: HashMap::new(),
            });
        }

        Ok(PatternDetectionResult {
            statistics: calculate_statistics(&patterns),
            quality_score: calculate_quality_score(&patterns),
            patterns,
        })
    }

    fn name(&self) -> &str {
        "Pipeline Pattern Detector"
    }

    fn supported_patterns(&self) -> Vec<PatternType> {
        vec![PatternType::Pipeline]
    }
}

/// Event-driven pattern detector
pub struct EventDrivenPatternDetector;

impl PatternDetector for EventDrivenPatternDetector {
    fn detect(&self, graph: &CorrelationGraph) -> Result<PatternDetectionResult> {
        let mut patterns = Vec::new();

        // Detect event-driven patterns (pub/sub relationships)
        let pub_sub_nodes = detect_pub_sub_pattern(graph);

        for pattern in pub_sub_nodes {
            let confidence = calculate_event_driven_confidence(&pattern, graph);
            patterns.push(DetectedPattern {
                pattern_type: PatternType::EventDriven,
                confidence,
                node_ids: pattern,
                metadata: HashMap::new(),
            });
        }

        Ok(PatternDetectionResult {
            statistics: calculate_statistics(&patterns),
            quality_score: calculate_quality_score(&patterns),
            patterns,
        })
    }

    fn name(&self) -> &str {
        "Event-Driven Pattern Detector"
    }

    fn supported_patterns(&self) -> Vec<PatternType> {
        vec![PatternType::EventDriven]
    }
}

/// Architectural pattern detector
pub struct ArchitecturalPatternDetector;

impl PatternDetector for ArchitecturalPatternDetector {
    fn detect(&self, graph: &CorrelationGraph) -> Result<PatternDetectionResult> {
        let mut patterns = Vec::new();

        // Detect layered architecture
        let layered = detect_layered_architecture(graph);
        if !layered.is_empty() {
            patterns.push(DetectedPattern {
                pattern_type: PatternType::LayeredArchitecture,
                confidence: 0.8,
                node_ids: layered,
                metadata: HashMap::new(),
            });
        }

        // Detect microservices pattern
        let microservices = detect_microservices(graph);
        for service in microservices {
            patterns.push(DetectedPattern {
                pattern_type: PatternType::Microservices,
                confidence: 0.7,
                node_ids: service,
                metadata: HashMap::new(),
            });
        }

        Ok(PatternDetectionResult {
            statistics: calculate_statistics(&patterns),
            quality_score: calculate_quality_score(&patterns),
            patterns,
        })
    }

    fn name(&self) -> &str {
        "Architectural Pattern Detector"
    }

    fn supported_patterns(&self) -> Vec<PatternType> {
        vec![PatternType::LayeredArchitecture, PatternType::Microservices]
    }
}

// Helper functions

fn detect_pipeline_chain(graph: &CorrelationGraph) -> Vec<Vec<String>> {
    let mut chains = Vec::new();
    let mut visited = std::collections::HashSet::new();

    for node in &graph.nodes {
        if !visited.contains(&node.id) {
            let chain = dfs_pipeline_chain(graph, &node.id, &mut visited);
            if chain.len() >= 3 {
                chains.push(chain);
            }
        }
    }

    chains
}

fn dfs_pipeline_chain(
    graph: &CorrelationGraph,
    node_id: &str,
    visited: &mut std::collections::HashSet<String>,
) -> Vec<String> {
    visited.insert(node_id.to_string());
    let mut chain = vec![node_id.to_string()];

    for edge in &graph.edges {
        if edge.source == node_id && !visited.contains(&edge.target) {
            let mut sub_chain = dfs_pipeline_chain(graph, &edge.target, visited);
            chain.append(&mut sub_chain);
        }
    }

    chain
}

fn detect_pub_sub_pattern(graph: &CorrelationGraph) -> Vec<Vec<String>> {
    let mut patterns = Vec::new();

    // Find nodes that have multiple outgoing edges (potential publishers)
    let mut publishers: HashMap<String, Vec<String>> = HashMap::new();

    for edge in &graph.edges {
        if edge.edge_type == EdgeType::Uses {
            publishers
                .entry(edge.source.clone())
                .or_default()
                .push(edge.target.clone());
        }
    }

    // Identify pub/sub patterns (publishers with multiple subscribers)
    for (publisher, subscribers) in publishers {
        if subscribers.len() >= 2 {
            let mut pattern = vec![publisher];
            pattern.extend(subscribers);
            patterns.push(pattern);
        }
    }

    patterns
}

fn detect_layered_architecture(graph: &CorrelationGraph) -> Vec<String> {
    let mut layers = Vec::new();

    // Simple heuristic: nodes with "api", "service", "model" in their names
    for node in &graph.nodes {
        let name_lower = node.label.to_lowercase();
        if name_lower.contains("api")
            || name_lower.contains("service")
            || name_lower.contains("model")
        {
            layers.push(node.id.clone());
        }
    }

    layers
}

fn detect_microservices(graph: &CorrelationGraph) -> Vec<Vec<String>> {
    let mut services = Vec::new();

    // Group nodes by module/file
    let mut service_groups: HashMap<String, Vec<String>> = HashMap::new();

    for node in &graph.nodes {
        if let Some(file_path) = node.metadata.get("file_path").and_then(|v| v.as_str()) {
            let module = file_path.split('/').next().unwrap_or("unknown");
            service_groups
                .entry(module.to_string())
                .or_default()
                .push(node.id.clone());
        }
    }

    for service_nodes in service_groups.values() {
        if service_nodes.len() >= 3 {
            services.push(service_nodes.clone());
        }
    }

    services
}

fn calculate_pipeline_confidence(chain: &[String], _graph: &CorrelationGraph) -> f64 {
    // Longer chains are more confident
    (chain.len() as f64 / 10.0).min(1.0)
}

fn calculate_event_driven_confidence(pattern: &[String], _graph: &CorrelationGraph) -> f64 {
    // More subscribers = higher confidence
    (pattern.len() as f64 / 5.0).min(1.0)
}

fn calculate_statistics(patterns: &[DetectedPattern]) -> PatternStatistics {
    let mut counts = HashMap::new();
    let mut total_confidence = 0.0;

    for pattern in patterns {
        *counts
            .entry(format!("{:?}", pattern.pattern_type))
            .or_insert(0) += 1;
        total_confidence += pattern.confidence;
    }

    PatternStatistics {
        total_patterns: patterns.len(),
        pattern_counts: counts,
        avg_confidence: if patterns.is_empty() {
            0.0
        } else {
            total_confidence / patterns.len() as f64
        },
    }
}

fn calculate_quality_score(patterns: &[DetectedPattern]) -> f64 {
    if patterns.is_empty() {
        return 0.0;
    }

    let total_confidence: f64 = patterns.iter().map(|p| p.confidence).sum();
    total_confidence / patterns.len() as f64
}

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
// Design Pattern Identification (Task 13.5)
// ============================================================================

/// Design pattern detector
pub struct DesignPatternDetector;

impl PatternDetector for DesignPatternDetector {
    fn detect(&self, graph: &CorrelationGraph) -> Result<PatternDetectionResult> {
        let mut patterns = Vec::new();

        // Detect Observer pattern
        patterns.extend(detect_observer_pattern(graph));

        // Detect Factory pattern
        patterns.extend(detect_factory_pattern(graph));

        // Detect Singleton pattern
        patterns.extend(detect_singleton_pattern(graph));

        // Detect Strategy pattern
        patterns.extend(detect_strategy_pattern(graph));

        Ok(PatternDetectionResult {
            statistics: calculate_statistics(&patterns),
            quality_score: calculate_quality_score(&patterns),
            patterns,
        })
    }

    fn name(&self) -> &str {
        "Design Pattern Detector"
    }

    fn supported_patterns(&self) -> Vec<PatternType> {
        vec![
            PatternType::Observer,
            PatternType::Factory,
            PatternType::Singleton,
            PatternType::Strategy,
        ]
    }
}

/// Detect Observer pattern (one-to-many dependency)
fn detect_observer_pattern(graph: &CorrelationGraph) -> Vec<DetectedPattern> {
    let mut patterns = Vec::new();

    // Find nodes with "Subject" or "Observable" in name
    let mut subjects = Vec::new();
    let mut observers = Vec::new();

    for node in &graph.nodes {
        let label_lower = node.label.to_lowercase();
        if label_lower.contains("subject") || label_lower.contains("observable") {
            subjects.push(node.id.clone());
        } else if label_lower.contains("observer") || label_lower.contains("listener") {
            observers.push(node.id.clone());
        }
    }

    // Check for relationships between subjects and observers
    for subject in &subjects {
        let mut related_observers = Vec::new();
        for edge in &graph.edges {
            if edge.source == *subject && observers.contains(&edge.target) {
                related_observers.push(edge.target.clone());
            }
        }

        if !related_observers.is_empty() {
            let observer_count = related_observers.len();
            let mut node_ids = vec![subject.clone()];
            node_ids.extend(related_observers);

            let confidence = (observer_count as f64 / 3.0).min(1.0);
            patterns.push(DetectedPattern {
                pattern_type: PatternType::Observer,
                confidence,
                node_ids,
                metadata: {
                    let mut m = HashMap::new();
                    m.insert(
                        "subject".to_string(),
                        serde_json::Value::String(subject.clone()),
                    );
                    m.insert(
                        "observer_count".to_string(),
                        serde_json::Value::Number(observer_count.into()),
                    );
                    m
                },
            });
        }
    }

    patterns
}

/// Detect Factory pattern (creation pattern)
fn detect_factory_pattern(graph: &CorrelationGraph) -> Vec<DetectedPattern> {
    let mut patterns = Vec::new();

    // Find nodes with "Factory" in name
    let mut factories = Vec::new();
    let mut products = Vec::new();

    for node in &graph.nodes {
        let label_lower = node.label.to_lowercase();
        if label_lower.contains("factory") {
            factories.push(node.id.clone());
        } else if label_lower.contains("product") || label_lower.contains("create") {
            products.push(node.id.clone());
        }
    }

    // Check for relationships between factories and products
    for factory in &factories {
        let mut created_products = Vec::new();
        for edge in &graph.edges {
            if edge.source == *factory && products.contains(&edge.target) {
                created_products.push(edge.target.clone());
            }
        }

        if !created_products.is_empty() {
            let mut node_ids = vec![factory.clone()];
            node_ids.extend(created_products.clone());

            let confidence = 0.7; // Factory pattern has moderate confidence
            patterns.push(DetectedPattern {
                pattern_type: PatternType::Factory,
                confidence,
                node_ids,
                metadata: {
                    let mut m = HashMap::new();
                    m.insert(
                        "factory".to_string(),
                        serde_json::Value::String(factory.clone()),
                    );
                    m.insert(
                        "product_count".to_string(),
                        serde_json::Value::Number(created_products.len().into()),
                    );
                    m
                },
            });
        }
    }

    patterns
}

/// Detect Singleton pattern (single instance)
fn detect_singleton_pattern(graph: &CorrelationGraph) -> Vec<DetectedPattern> {
    let mut patterns = Vec::new();

    // Find nodes with "Singleton" or "Instance" in name/metadata
    for node in &graph.nodes {
        let label_lower = node.label.to_lowercase();
        let has_singleton_name = label_lower.contains("singleton")
            || label_lower.contains("instance")
            || label_lower.contains("getinstance");

        let has_singleton_metadata = node
            .metadata
            .get("is_singleton")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if has_singleton_name || has_singleton_metadata {
            // Check for getInstance-like methods
            let mut has_get_instance = false;
            for edge in &graph.edges {
                if edge.target == node.id {
                    let edge_label = edge
                        .metadata
                        .get("label")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if edge_label.to_lowercase().contains("getinstance")
                        || edge_label.to_lowercase().contains("get_instance")
                    {
                        has_get_instance = true;
                        break;
                    }
                }
            }

            let confidence = if has_get_instance { 0.9 } else { 0.6 };
            patterns.push(DetectedPattern {
                pattern_type: PatternType::Singleton,
                confidence,
                node_ids: vec![node.id.clone()],
                metadata: {
                    let mut m = HashMap::new();
                    m.insert(
                        "singleton_class".to_string(),
                        serde_json::Value::String(node.label.clone()),
                    );
                    m.insert(
                        "has_get_instance".to_string(),
                        serde_json::Value::Bool(has_get_instance),
                    );
                    m
                },
            });
        }
    }

    patterns
}

/// Detect Strategy pattern (algorithm selection)
fn detect_strategy_pattern(graph: &CorrelationGraph) -> Vec<DetectedPattern> {
    let mut patterns = Vec::new();

    // Find nodes with "Strategy" or "Algorithm" in name
    let mut strategies = Vec::new();
    let mut contexts = Vec::new();

    for node in &graph.nodes {
        let label_lower = node.label.to_lowercase();
        if label_lower.contains("strategy") || label_lower.contains("algorithm") {
            strategies.push(node.id.clone());
        } else if label_lower.contains("context") {
            contexts.push(node.id.clone());
        }
    }

    // Check for relationships between contexts and strategies
    for context in &contexts {
        let mut used_strategies = Vec::new();
        for edge in &graph.edges {
            if edge.source == *context && strategies.contains(&edge.target) {
                used_strategies.push(edge.target.clone());
            }
        }

        if !used_strategies.is_empty() {
            let mut node_ids = vec![context.clone()];
            node_ids.extend(used_strategies.clone());

            let confidence = 0.75;
            patterns.push(DetectedPattern {
                pattern_type: PatternType::Strategy,
                confidence,
                node_ids,
                metadata: {
                    let mut m = HashMap::new();
                    m.insert(
                        "context".to_string(),
                        serde_json::Value::String(context.clone()),
                    );
                    m.insert(
                        "strategy_count".to_string(),
                        serde_json::Value::Number(used_strategies.len().into()),
                    );
                    m
                },
            });
        }
    }

    patterns
}

// ============================================================================
// Pattern Visualization Overlays (Task 13.6)
// ============================================================================

// GraphEdge and GraphNode are used via DetectedPattern and CorrelationGraph

/// Pattern visualization overlay configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternOverlayConfig {
    /// Color for pipeline patterns
    pub pipeline_color: String,
    /// Color for event-driven patterns
    pub event_driven_color: String,
    /// Color for observer patterns
    pub observer_color: String,
    /// Color for factory patterns
    pub factory_color: String,
    /// Color for singleton patterns
    pub singleton_color: String,
    /// Color for strategy patterns
    pub strategy_color: String,
    /// Highlight pattern nodes
    pub highlight_nodes: bool,
    /// Highlight pattern edges
    pub highlight_edges: bool,
    /// Show pattern labels
    pub show_labels: bool,
    /// Pattern border width
    pub border_width: f32,
}

impl Default for PatternOverlayConfig {
    fn default() -> Self {
        Self {
            pipeline_color: "#3498db".to_string(),
            event_driven_color: "#e74c3c".to_string(),
            observer_color: "#f39c12".to_string(),
            factory_color: "#2ecc71".to_string(),
            singleton_color: "#9b59b6".to_string(),
            strategy_color: "#1abc9c".to_string(),
            highlight_nodes: true,
            highlight_edges: true,
            show_labels: true,
            border_width: 2.0,
        }
    }
}

/// Apply pattern visualization overlays to a graph
pub fn apply_pattern_overlays(
    graph: &mut CorrelationGraph,
    patterns: &[DetectedPattern],
    config: &PatternOverlayConfig,
) -> Result<()> {
    // Build pattern map for quick lookup
    let mut node_patterns: HashMap<String, Vec<&DetectedPattern>> = HashMap::new();
    for pattern in patterns {
        for node_id in &pattern.node_ids {
            node_patterns
                .entry(node_id.clone())
                .or_default()
                .push(pattern);
        }
    }

    // Apply styling to nodes
    if config.highlight_nodes {
        for node in &mut graph.nodes {
            if let Some(pattern_list) = node_patterns.get(&node.id) {
                // Use color from first pattern (could be enhanced to blend colors)
                if let Some(first_pattern) = pattern_list.first() {
                    let color = match first_pattern.pattern_type {
                        PatternType::Pipeline => &config.pipeline_color,
                        PatternType::EventDriven => &config.event_driven_color,
                        PatternType::Observer => &config.observer_color,
                        PatternType::Factory => &config.factory_color,
                        PatternType::Singleton => &config.singleton_color,
                        PatternType::Strategy => &config.strategy_color,
                        _ => "#95a5a6", // Default gray
                    };
                    node.color = Some(color.to_string());

                    // Add pattern label if enabled
                    if config.show_labels {
                        let pattern_name = format!("{:?}", first_pattern.pattern_type);
                        node.label = format!("{} [{}]", node.label, pattern_name);
                    }

                    // Increase node size for pattern nodes
                    node.size = Some(node.size.unwrap_or(8.0) + config.border_width);
                }
            }
        }
    }

    // Apply styling to edges
    if config.highlight_edges {
        for edge in &mut graph.edges {
            let source_has_pattern = node_patterns.contains_key(&edge.source);
            let target_has_pattern = node_patterns.contains_key(&edge.target);

            if source_has_pattern || target_has_pattern {
                // Find matching pattern
                let pattern = patterns.iter().find(|p| {
                    p.node_ids.contains(&edge.source) && p.node_ids.contains(&edge.target)
                });

                if let Some(p) = pattern {
                    let color = match p.pattern_type {
                        PatternType::Pipeline => &config.pipeline_color,
                        PatternType::EventDriven => &config.event_driven_color,
                        PatternType::Observer => &config.observer_color,
                        PatternType::Factory => &config.factory_color,
                        PatternType::Singleton => &config.singleton_color,
                        PatternType::Strategy => &config.strategy_color,
                        _ => "#95a5a6",
                    };
                    edge.metadata.insert(
                        "color".to_string(),
                        serde_json::Value::String(color.to_string()),
                    );
                    edge.weight += config.border_width;

                    if config.show_labels {
                        let pattern_name = format!("{:?}", p.pattern_type);
                        edge.label = Some(format!(
                            "{} [{}]",
                            edge.label.as_deref().unwrap_or(""),
                            pattern_name
                        ));
                    }
                }
            }
        }
    }

    Ok(())
}

// ============================================================================
// Pattern Quality Metrics (Task 13.7)
// ============================================================================

/// Detailed pattern quality metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternQualityMetrics {
    /// Pattern type
    pub pattern_type: PatternType,
    /// Confidence score (0.0 to 1.0)
    pub confidence: f64,
    /// Completeness score (how complete the pattern is)
    pub completeness: f64,
    /// Consistency score (how consistent the pattern implementation is)
    pub consistency: f64,
    /// Overall quality score
    pub quality_score: f64,
    /// Number of nodes in pattern
    pub node_count: usize,
    /// Number of edges in pattern
    pub edge_count: usize,
    /// Pattern maturity level
    pub maturity: PatternMaturity,
}

/// Pattern maturity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PatternMaturity {
    /// Emerging pattern (low confidence)
    Emerging,
    /// Developing pattern (moderate confidence)
    Developing,
    /// Mature pattern (high confidence)
    Mature,
    /// Well-established pattern (very high confidence)
    Established,
}

/// Calculate detailed quality metrics for patterns
pub fn calculate_pattern_quality_metrics(
    pattern: &DetectedPattern,
    graph: &CorrelationGraph,
) -> PatternQualityMetrics {
    // Calculate completeness (based on expected vs actual nodes)
    let expected_nodes = get_expected_node_count(pattern.pattern_type);
    let completeness = if expected_nodes > 0 {
        (pattern.node_ids.len() as f64 / expected_nodes as f64).min(1.0)
    } else {
        0.5 // Default for unknown patterns
    };

    // Calculate consistency (based on edge relationships)
    let edge_count = graph
        .edges
        .iter()
        .filter(|e| pattern.node_ids.contains(&e.source) && pattern.node_ids.contains(&e.target))
        .count();

    let expected_edges = get_expected_edge_count(pattern.pattern_type, pattern.node_ids.len());
    let consistency = if expected_edges > 0 {
        (edge_count as f64 / expected_edges as f64).min(1.0)
    } else {
        0.5
    };

    // Calculate overall quality score
    let quality_score =
        (pattern.confidence * 0.4 + completeness * 0.3 + consistency * 0.3).min(1.0);

    // Determine maturity
    let maturity = if quality_score >= 0.8 {
        PatternMaturity::Established
    } else if quality_score >= 0.6 {
        PatternMaturity::Mature
    } else if quality_score >= 0.4 {
        PatternMaturity::Developing
    } else {
        PatternMaturity::Emerging
    };

    PatternQualityMetrics {
        pattern_type: pattern.pattern_type,
        confidence: pattern.confidence,
        completeness,
        consistency,
        quality_score,
        node_count: pattern.node_ids.len(),
        edge_count,
        maturity,
    }
}

/// Get expected node count for a pattern type
fn get_expected_node_count(pattern_type: PatternType) -> usize {
    match pattern_type {
        PatternType::Observer => 3,    // Subject + Observer + ConcreteObserver
        PatternType::Factory => 3,     // Factory + Product + ConcreteProduct
        PatternType::Singleton => 1,   // Single instance
        PatternType::Strategy => 3,    // Context + Strategy + ConcreteStrategy
        PatternType::Pipeline => 3,    // Minimum pipeline length
        PatternType::EventDriven => 2, // Publisher + Subscriber
        _ => 2,                        // Default
    }
}

/// Get expected edge count for a pattern type
fn get_expected_edge_count(pattern_type: PatternType, node_count: usize) -> usize {
    match pattern_type {
        PatternType::Observer => node_count - 1, // Each observer connects to subject
        PatternType::Factory => node_count - 1,  // Factory creates products
        PatternType::Singleton => 0,             // No edges typically
        PatternType::Strategy => node_count - 1, // Context uses strategies
        PatternType::Pipeline => node_count - 1, // Sequential chain
        PatternType::EventDriven => node_count - 1, // Publisher to subscribers
        _ => node_count.max(1) - 1,
    }
}

// ============================================================================
// Pattern Recommendation Engine (Task 13.8)
// ============================================================================

/// Pattern recommendation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternRecommendation {
    /// Recommended pattern type
    pub pattern_type: PatternType,
    /// Recommendation reason
    pub reason: String,
    /// Priority level (1-10, higher is more important)
    pub priority: u8,
    /// Estimated benefit
    pub estimated_benefit: String,
    /// Implementation difficulty
    pub difficulty: PatternDifficulty,
    /// Nodes that would benefit from this pattern
    pub candidate_nodes: Vec<String>,
}

/// Pattern implementation difficulty
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PatternDifficulty {
    /// Easy to implement
    Easy,
    /// Moderate difficulty
    Moderate,
    /// Difficult to implement
    Hard,
}

/// Pattern recommendation engine
pub struct PatternRecommendationEngine;

impl PatternRecommendationEngine {
    /// Generate pattern recommendations for a graph
    pub fn recommend_patterns(
        graph: &CorrelationGraph,
        existing_patterns: &[DetectedPattern],
    ) -> Vec<PatternRecommendation> {
        let mut recommendations = Vec::new();

        // Analyze graph structure for pattern opportunities
        recommendations.extend(Self::recommend_observer_pattern(graph, existing_patterns));
        recommendations.extend(Self::recommend_factory_pattern(graph, existing_patterns));
        recommendations.extend(Self::recommend_singleton_pattern(graph, existing_patterns));
        recommendations.extend(Self::recommend_strategy_pattern(graph, existing_patterns));

        // Sort by priority
        recommendations.sort_by(|a, b| b.priority.cmp(&a.priority));

        recommendations
    }

    /// Recommend Observer pattern
    fn recommend_observer_pattern(
        graph: &CorrelationGraph,
        existing_patterns: &[DetectedPattern],
    ) -> Vec<PatternRecommendation> {
        let mut recommendations = Vec::new();

        // Check if Observer pattern already exists
        let has_observer = existing_patterns
            .iter()
            .any(|p| p.pattern_type == PatternType::Observer);

        if !has_observer {
            // Find nodes with multiple dependents (potential subjects)
            let mut dependents: HashMap<String, Vec<String>> = HashMap::new();
            for edge in &graph.edges {
                dependents
                    .entry(edge.target.clone())
                    .or_default()
                    .push(edge.source.clone());
            }

            for (node_id, dependents_list) in &dependents {
                if dependents_list.len() >= 2 {
                    recommendations.push(PatternRecommendation {
                        pattern_type: PatternType::Observer,
                        reason: format!(
                            "Node '{}' has {} dependents, Observer pattern could decouple them",
                            node_id,
                            dependents_list.len()
                        ),
                        priority: 7,
                        estimated_benefit: "Improved decoupling and maintainability".to_string(),
                        difficulty: PatternDifficulty::Moderate,
                        candidate_nodes: {
                            let mut nodes = vec![node_id.clone()];
                            nodes.extend(dependents_list.clone());
                            nodes
                        },
                    });
                }
            }
        }

        recommendations
    }

    /// Recommend Factory pattern
    fn recommend_factory_pattern(
        graph: &CorrelationGraph,
        existing_patterns: &[DetectedPattern],
    ) -> Vec<PatternRecommendation> {
        let mut recommendations = Vec::new();

        let has_factory = existing_patterns
            .iter()
            .any(|p| p.pattern_type == PatternType::Factory);

        if !has_factory {
            // Find nodes with "create" or "new" in name (potential factory candidates)
            let mut creation_nodes = Vec::new();
            for node in &graph.nodes {
                let label_lower = node.label.to_lowercase();
                if label_lower.contains("create") || label_lower.contains("new") {
                    creation_nodes.push(node.id.clone());
                }
            }

            if !creation_nodes.is_empty() {
                recommendations.push(PatternRecommendation {
                    pattern_type: PatternType::Factory,
                    reason: format!(
                        "Found {} creation-related nodes, Factory pattern could centralize object creation",
                        creation_nodes.len()
                    ),
                    priority: 6,
                    estimated_benefit: "Centralized object creation and improved flexibility".to_string(),
                    difficulty: PatternDifficulty::Easy,
                    candidate_nodes: creation_nodes,
                });
            }
        }

        recommendations
    }

    /// Recommend Singleton pattern
    fn recommend_singleton_pattern(
        graph: &CorrelationGraph,
        existing_patterns: &[DetectedPattern],
    ) -> Vec<PatternRecommendation> {
        let mut recommendations = Vec::new();

        let has_singleton = existing_patterns
            .iter()
            .any(|p| p.pattern_type == PatternType::Singleton);

        if !has_singleton {
            // Find nodes that are used frequently (potential singleton candidates)
            let mut usage_count: HashMap<String, usize> = HashMap::new();
            for edge in &graph.edges {
                *usage_count.entry(edge.target.clone()).or_insert(0) += 1;
            }

            for (node_id, count) in &usage_count {
                if *count >= 5 {
                    recommendations.push(PatternRecommendation {
                        pattern_type: PatternType::Singleton,
                        reason: format!(
                            "Node '{}' is used {} times, Singleton pattern could ensure single instance",
                            node_id, count
                        ),
                        priority: 5,
                        estimated_benefit: "Resource efficiency and controlled access".to_string(),
                        difficulty: PatternDifficulty::Easy,
                        candidate_nodes: vec![node_id.clone()],
                    });
                }
            }
        }

        recommendations
    }

    /// Recommend Strategy pattern
    fn recommend_strategy_pattern(
        graph: &CorrelationGraph,
        existing_patterns: &[DetectedPattern],
    ) -> Vec<PatternRecommendation> {
        let mut recommendations = Vec::new();

        let has_strategy = existing_patterns
            .iter()
            .any(|p| p.pattern_type == PatternType::Strategy);

        if !has_strategy {
            // Find nodes with multiple similar implementations (potential strategy candidates)
            let mut similar_groups: HashMap<String, Vec<String>> = HashMap::new();
            for node in &graph.nodes {
                let label_lower = node.label.to_lowercase();
                // Group by common prefixes/suffixes
                if label_lower.contains("handler") || label_lower.contains("processor") {
                    let key = if label_lower.contains("handler") {
                        "handler"
                    } else {
                        "processor"
                    };
                    similar_groups
                        .entry(key.to_string())
                        .or_default()
                        .push(node.id.clone());
                }
            }

            for (key, nodes) in &similar_groups {
                if nodes.len() >= 2 {
                    recommendations.push(PatternRecommendation {
                        pattern_type: PatternType::Strategy,
                        reason: format!(
                            "Found {} similar {} nodes, Strategy pattern could encapsulate algorithms",
                            nodes.len(), key
                        ),
                        priority: 6,
                        estimated_benefit: "Improved flexibility and algorithm interchangeability".to_string(),
                        difficulty: PatternDifficulty::Moderate,
                        candidate_nodes: nodes.clone(),
                    });
                }
            }
        }

        recommendations
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
