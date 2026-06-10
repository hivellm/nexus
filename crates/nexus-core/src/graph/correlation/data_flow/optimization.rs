//! Flow optimization suggestions (Task 11.7).

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::graph::correlation::CorrelationGraph;

use super::{DataFlowAnalyzer, TransformationType};

/// Priority level for optimization suggestions
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum OptimizationPriority {
    /// Low priority - minor improvements
    Low,
    /// Medium priority - moderate improvements
    Medium,
    /// High priority - significant improvements
    High,
    /// Critical priority - major performance issues
    Critical,
}

/// Impact level of optimization
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum OptimizationImpact {
    /// Low impact - minimal performance gain
    Low,
    /// Medium impact - moderate performance gain
    Medium,
    /// High impact - significant performance gain
    High,
}

/// Effort required for optimization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OptimizationEffort {
    /// Low effort - easy to implement
    Low,
    /// Medium effort - moderate complexity
    Medium,
    /// High effort - complex implementation
    High,
}

/// Flow optimization suggestion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowOptimizationSuggestion {
    /// Category of optimization
    pub category: String,
    /// Priority level
    pub priority: OptimizationPriority,
    /// Impact level
    pub impact: OptimizationImpact,
    /// Effort required
    pub effort: OptimizationEffort,
    /// Description of the issue
    pub description: String,
    /// Suggested optimization
    pub suggestion: String,
    /// Location in code (file, line)
    pub location: Option<String>,
    /// Estimated performance improvement (percentage)
    pub estimated_improvement: Option<f64>,
}

/// Flow optimization analyzer
pub struct FlowOptimizationAnalyzer;

impl FlowOptimizationAnalyzer {
    /// Analyze data flow graph and generate optimization suggestions
    pub fn analyze(
        graph: &CorrelationGraph,
        analyzer: &DataFlowAnalyzer,
    ) -> Vec<FlowOptimizationSuggestion> {
        let mut suggestions = Vec::new();

        // Analyze for redundant transformations
        suggestions.extend(Self::detect_redundant_transformations(graph, analyzer));

        // Analyze for inefficient type conversions
        suggestions.extend(Self::detect_inefficient_conversions(graph, analyzer));

        // Analyze for unused variables
        suggestions.extend(Self::detect_unused_variables(analyzer));

        // Analyze for long transformation chains
        suggestions.extend(Self::detect_long_chains(graph));

        // Analyze for parallelizable operations
        suggestions.extend(Self::detect_parallelization_opportunities(graph, analyzer));

        // Analyze for memory inefficiencies
        suggestions.extend(Self::detect_memory_inefficiencies(graph, analyzer));

        // Sort by priority and impact
        suggestions.sort_by(|a, b| match b.priority.cmp(&a.priority) {
            std::cmp::Ordering::Equal => b.impact.cmp(&a.impact),
            other => other,
        });

        suggestions
    }

    /// Detect redundant transformations (e.g., multiple conversions of same data)
    fn detect_redundant_transformations(
        _graph: &CorrelationGraph,
        analyzer: &DataFlowAnalyzer,
    ) -> Vec<FlowOptimizationSuggestion> {
        let mut suggestions = Vec::new();
        let mut conversion_chains: HashMap<String, Vec<String>> = HashMap::new();

        // Track type conversion chains
        for transformation in analyzer.transformations() {
            if transformation.transformation_type == TransformationType::TypeConversion {
                conversion_chains
                    .entry(transformation.source.clone())
                    .or_default()
                    .push(transformation.target.clone());
            }
        }

        // Detect chains with multiple conversions
        for (source, targets) in &conversion_chains {
            if targets.len() > 2 {
                suggestions.push(FlowOptimizationSuggestion {
                    category: "Redundant Conversions".to_string(),
                    priority: OptimizationPriority::Medium,
                    impact: OptimizationImpact::Medium,
                    effort: OptimizationEffort::Low,
                    description: format!(
                        "Multiple type conversions detected for variable '{}'",
                        source
                    ),
                    suggestion: format!(
                        "Consider combining conversions or using a single conversion path for '{}'",
                        source
                    ),
                    location: None,
                    estimated_improvement: Some(10.0),
                });
            }
        }

        suggestions
    }

    /// Detect inefficient type conversions
    fn detect_inefficient_conversions(
        _graph: &CorrelationGraph,
        analyzer: &DataFlowAnalyzer,
    ) -> Vec<FlowOptimizationSuggestion> {
        let mut suggestions = Vec::new();

        for transformation in analyzer.transformations() {
            if transformation.transformation_type == TransformationType::TypeConversion {
                // Check for string conversions in loops (would need more context)
                if transformation.target.contains("to_string") {
                    suggestions.push(FlowOptimizationSuggestion {
                        category: "Type Conversion".to_string(),
                        priority: OptimizationPriority::Low,
                        impact: OptimizationImpact::Low,
                        effort: OptimizationEffort::Low,
                        description: format!(
                            "Type conversion detected: {} -> {}",
                            transformation.source, transformation.target
                        ),
                        suggestion: "Consider if conversion is necessary or can be optimized"
                            .to_string(),
                        location: None,
                        estimated_improvement: Some(5.0),
                    });
                }
            }
        }

        suggestions
    }

    /// Detect unused variables
    fn detect_unused_variables(analyzer: &DataFlowAnalyzer) -> Vec<FlowOptimizationSuggestion> {
        let mut suggestions = Vec::new();

        for variable in analyzer.tracker().all_variables() {
            if variable.usages.is_empty() && !variable.is_parameter {
                suggestions.push(FlowOptimizationSuggestion {
                    category: "Unused Variables".to_string(),
                    priority: OptimizationPriority::Low,
                    impact: OptimizationImpact::Low,
                    effort: OptimizationEffort::Low,
                    description: format!("Variable '{}' is defined but never used", variable.name),
                    suggestion: format!("Consider removing unused variable '{}'", variable.name),
                    location: Some(format!("{}:{}", variable.file, variable.line)),
                    estimated_improvement: Some(2.0),
                });
            }
        }

        suggestions
    }

    /// Detect long transformation chains that could be optimized
    fn detect_long_chains(graph: &CorrelationGraph) -> Vec<FlowOptimizationSuggestion> {
        let mut suggestions = Vec::new();

        // Build adjacency map
        let mut outgoing: HashMap<String, Vec<String>> = HashMap::new();
        for edge in &graph.edges {
            outgoing
                .entry(edge.source.clone())
                .or_default()
                .push(edge.target.clone());
        }

        // Find longest paths (simplified - would use proper graph algorithms in production)
        let mut max_chain_length = 0;
        let mut longest_chain_start = None;

        for node in &graph.nodes {
            let chain_length = Self::calculate_chain_length(&node.id, &outgoing);
            if chain_length > max_chain_length {
                max_chain_length = chain_length;
                longest_chain_start = Some(node.id.clone());
            }
        }

        if max_chain_length > 5 {
            suggestions.push(FlowOptimizationSuggestion {
                category: "Long Transformation Chain".to_string(),
                priority: OptimizationPriority::Medium,
                impact: OptimizationImpact::Medium,
                effort: OptimizationEffort::Medium,
                description: format!(
                    "Long transformation chain detected (length: {})",
                    max_chain_length
                ),
                suggestion: "Consider breaking into smaller, more manageable transformations or combining operations".to_string(),
                location: longest_chain_start,
                estimated_improvement: Some(15.0),
            });
        }

        suggestions
    }

    /// Calculate chain length from a node
    pub fn calculate_chain_length(node_id: &str, outgoing: &HashMap<String, Vec<String>>) -> usize {
        let mut visited = HashSet::new();
        let mut max_length = 0;

        fn dfs(
            current: &str,
            outgoing: &HashMap<String, Vec<String>>,
            visited: &mut HashSet<String>,
            length: usize,
            max_length: &mut usize,
        ) {
            if visited.contains(current) {
                return;
            }
            visited.insert(current.to_string());

            if length > *max_length {
                *max_length = length;
            }

            if let Some(targets) = outgoing.get(current) {
                for target in targets {
                    dfs(target, outgoing, visited, length + 1, max_length);
                }
            }
        }

        dfs(node_id, outgoing, &mut visited, 1, &mut max_length);
        max_length
    }

    /// Detect opportunities for parallelization
    fn detect_parallelization_opportunities(
        graph: &CorrelationGraph,
        _analyzer: &DataFlowAnalyzer,
    ) -> Vec<FlowOptimizationSuggestion> {
        let mut suggestions = Vec::new();

        // Detect independent transformation chains
        let mut independent_chains = 0;
        let mut incoming: HashMap<String, usize> = HashMap::new();

        for edge in &graph.edges {
            *incoming.entry(edge.target.clone()).or_insert(0) += 1;
        }

        // Count source nodes (nodes with no incoming edges)
        for node in &graph.nodes {
            if incoming.get(&node.id).copied().unwrap_or(0) == 0 {
                independent_chains += 1;
            }
        }

        if independent_chains > 2 {
            suggestions.push(FlowOptimizationSuggestion {
                category: "Parallelization".to_string(),
                priority: OptimizationPriority::High,
                impact: OptimizationImpact::High,
                effort: OptimizationEffort::Medium,
                description: format!(
                    "{} independent data flow chains detected",
                    independent_chains
                ),
                suggestion: "Consider parallelizing independent transformation chains".to_string(),
                location: None,
                estimated_improvement: Some(30.0),
            });
        }

        suggestions
    }

    /// Detect memory inefficiencies
    fn detect_memory_inefficiencies(
        _graph: &CorrelationGraph,
        analyzer: &DataFlowAnalyzer,
    ) -> Vec<FlowOptimizationSuggestion> {
        let mut suggestions = Vec::new();

        // Detect multiple copies of large data structures
        let mut variable_copies: HashMap<String, usize> = HashMap::new();
        for transformation in analyzer.transformations() {
            if transformation.transformation_type == TransformationType::Assignment {
                *variable_copies
                    .entry(transformation.source.clone())
                    .or_insert(0) += 1;
            }
        }

        for (var, count) in &variable_copies {
            if *count > 3 {
                suggestions.push(FlowOptimizationSuggestion {
                    category: "Memory Efficiency".to_string(),
                    priority: OptimizationPriority::Medium,
                    impact: OptimizationImpact::Medium,
                    effort: OptimizationEffort::Low,
                    description: format!("Variable '{}' is copied {} times", var, count),
                    suggestion: format!(
                        "Consider using references or moving '{}' instead of copying",
                        var
                    ),
                    location: None,
                    estimated_improvement: Some(10.0),
                });
            }
        }

        suggestions
    }
}
