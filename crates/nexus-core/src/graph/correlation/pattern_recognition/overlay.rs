//! Pattern Visualization Overlays (Task 13.6)

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::Result;
use crate::graph::correlation::CorrelationGraph;

use super::types::{DetectedPattern, PatternType};

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
