//! Pattern Quality Metrics (Task 13.7)

use serde::{Deserialize, Serialize};

use crate::graph::correlation::CorrelationGraph;

use super::types::{DetectedPattern, PatternType};

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
