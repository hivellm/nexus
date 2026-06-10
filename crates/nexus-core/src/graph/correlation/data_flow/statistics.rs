//! Data flow statistics (Task 11.8) and convenience methods on
//! `DataFlowAnalyzer` that delegate to the optimization and statistics types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::graph::correlation::CorrelationGraph;

use super::{
    DataFlowAnalyzer, FlowOptimizationAnalyzer, FlowOptimizationSuggestion, TransformationType,
};

/// Statistics about data flow in a graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataFlowStatistics {
    /// Total number of variables tracked
    pub total_variables: usize,
    /// Number of variables with types inferred
    pub typed_variables: usize,
    /// Total number of transformations
    pub total_transformations: usize,
    /// Transformation counts by type
    pub transformation_counts: HashMap<String, usize>,
    /// Average transformation chain length
    pub average_chain_length: f64,
    /// Maximum transformation chain length
    pub max_chain_length: usize,
    /// Number of source nodes (no incoming edges)
    pub source_nodes: usize,
    /// Number of sink nodes (no outgoing edges)
    pub sink_nodes: usize,
    /// Number of type conversions
    pub type_conversions: usize,
    /// Number of unused variables
    pub unused_variables: usize,
    /// Number of variables with multiple usages
    pub multi_usage_variables: usize,
    /// Average usages per variable
    pub average_usages_per_variable: f64,
}

impl DataFlowStatistics {
    /// Calculate statistics from a data flow graph and analyzer
    pub fn calculate(graph: &CorrelationGraph, analyzer: &DataFlowAnalyzer) -> Self {
        let variables = analyzer.tracker().all_variables();
        let total_variables = variables.len();
        let typed_variables = variables.iter().filter(|v| v.var_type.is_some()).count();

        let transformations = analyzer.transformations();
        let total_transformations = transformations.len();

        // Count transformations by type
        let mut transformation_counts = HashMap::new();
        for trans in transformations {
            let type_name = format!("{:?}", trans.transformation_type);
            *transformation_counts.entry(type_name).or_insert(0) += 1;
        }

        // Calculate chain lengths
        let mut outgoing: HashMap<String, Vec<String>> = HashMap::new();
        for edge in &graph.edges {
            outgoing
                .entry(edge.source.clone())
                .or_default()
                .push(edge.target.clone());
        }

        let mut chain_lengths = Vec::new();
        for node in &graph.nodes {
            let length = FlowOptimizationAnalyzer::calculate_chain_length(&node.id, &outgoing);
            chain_lengths.push(length);
        }

        let average_chain_length = if !chain_lengths.is_empty() {
            chain_lengths.iter().sum::<usize>() as f64 / chain_lengths.len() as f64
        } else {
            0.0
        };
        let max_chain_length = chain_lengths.iter().max().copied().unwrap_or(0);

        // Count source and sink nodes
        let mut incoming: HashMap<String, usize> = HashMap::new();
        let mut outgoing_count: HashMap<String, usize> = HashMap::new();

        for edge in &graph.edges {
            *incoming.entry(edge.target.clone()).or_insert(0) += 1;
            *outgoing_count.entry(edge.source.clone()).or_insert(0) += 1;
        }

        let source_nodes = graph
            .nodes
            .iter()
            .filter(|n| incoming.get(&n.id).copied().unwrap_or(0) == 0)
            .count();
        let sink_nodes = graph
            .nodes
            .iter()
            .filter(|n| outgoing_count.get(&n.id).copied().unwrap_or(0) == 0)
            .count();

        // Count type conversions
        let type_conversions = transformations
            .iter()
            .filter(|t| t.transformation_type == TransformationType::TypeConversion)
            .count();

        // Count unused variables
        let unused_variables = variables
            .iter()
            .filter(|v| v.usages.is_empty() && !v.is_parameter)
            .count();

        // Count variables with multiple usages
        let multi_usage_variables = variables.iter().filter(|v| v.usages.len() > 1).count();

        // Calculate average usages per variable
        let total_usages: usize = variables.iter().map(|v| v.usages.len()).sum();
        let average_usages_per_variable = if total_variables > 0 {
            total_usages as f64 / total_variables as f64
        } else {
            0.0
        };

        Self {
            total_variables,
            typed_variables,
            total_transformations,
            transformation_counts,
            average_chain_length,
            max_chain_length,
            source_nodes,
            sink_nodes,
            type_conversions,
            unused_variables,
            multi_usage_variables,
            average_usages_per_variable,
        }
    }
}

impl DataFlowAnalyzer {
    /// Get flow optimization suggestions for this analyzer's graph
    pub fn get_optimization_suggestions(
        &self,
        graph: &CorrelationGraph,
    ) -> Vec<FlowOptimizationSuggestion> {
        FlowOptimizationAnalyzer::analyze(graph, self)
    }

    /// Calculate data flow statistics
    pub fn calculate_statistics(&self, graph: &CorrelationGraph) -> DataFlowStatistics {
        DataFlowStatistics::calculate(graph, self)
    }
}
