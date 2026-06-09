//! Core types for pattern recognition: traits, result structs, enums.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::Result;
use crate::graph::correlation::CorrelationGraph;

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
