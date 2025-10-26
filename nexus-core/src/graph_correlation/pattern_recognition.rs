//! Pattern Recognition for Graph Correlation Analysis
//!
//! Detects common architectural and design patterns in code graphs:
//! - Pipeline patterns
//! - Event-driven patterns
//! - Architectural patterns
//! - Design patterns

use crate::graph_correlation::{CorrelationGraph, EdgeType, GraphNode, GraphType, NodeType};
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
        if edge.source == node_id {
            if !visited.contains(&edge.target) {
                let mut sub_chain = dfs_pipeline_chain(graph, &edge.target, visited);
                chain.append(&mut sub_chain);
            }
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
                .or_insert_with(Vec::new)
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
                .or_insert_with(Vec::new)
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

    #[test]
    fn test_pattern_statistics() {
        let stats = PatternStatistics::default();
        assert_eq!(stats.total_patterns, 0);
        assert_eq!(stats.avg_confidence, 0.0);
    }

    #[test]
    fn test_pattern_types() {
        let pattern = PatternType::Pipeline;
        assert_eq!(pattern, PatternType::Pipeline);
    }
}
