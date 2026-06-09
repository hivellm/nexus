//! Pattern Recommendation Engine (Task 13.8)

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::graph::correlation::CorrelationGraph;

use super::types::{DetectedPattern, PatternType};

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
