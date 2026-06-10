//! Pattern detector implementations and their private helper functions.

use std::collections::HashMap;

use crate::Result;
use crate::graph::correlation::{CorrelationGraph, EdgeType};

use super::types::{
    DetectedPattern, PatternDetectionResult, PatternDetector, PatternStatistics, PatternType,
};

// ============================================================================
// Detector structs
// ============================================================================

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

// ============================================================================
// Helper functions (pipeline / architectural)
// ============================================================================

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

pub(crate) fn calculate_statistics(patterns: &[DetectedPattern]) -> PatternStatistics {
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

pub(crate) fn calculate_quality_score(patterns: &[DetectedPattern]) -> f64 {
    if patterns.is_empty() {
        return 0.0;
    }

    let total_confidence: f64 = patterns.iter().map(|p| p.confidence).sum();
    total_confidence / patterns.len() as f64
}

// ============================================================================
// Design pattern helper functions
// ============================================================================

/// Detect Observer pattern (one-to-many dependency)
pub(crate) fn detect_observer_pattern(graph: &CorrelationGraph) -> Vec<DetectedPattern> {
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
pub(crate) fn detect_factory_pattern(graph: &CorrelationGraph) -> Vec<DetectedPattern> {
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
pub(crate) fn detect_singleton_pattern(graph: &CorrelationGraph) -> Vec<DetectedPattern> {
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
pub(crate) fn detect_strategy_pattern(graph: &CorrelationGraph) -> Vec<DetectedPattern> {
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
