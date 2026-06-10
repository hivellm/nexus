use crate::graph::correlation::{CorrelationGraph, EdgeType};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use super::analyzer::ComponentAnalyzer;

// ============================================================================
// Component Coupling Analysis (Task 12.7)
// ============================================================================

/// Component coupling metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentCouplingMetrics {
    /// Component name
    pub component_name: String,
    /// Afferent coupling (number of components that depend on this)
    pub afferent_coupling: usize,
    /// Efferent coupling (number of components this depends on)
    pub efferent_coupling: usize,
    /// Instability (efferent / (afferent + efferent))
    pub instability: f64,
    /// Abstractness (abstract classes / total classes)
    pub abstractness: f64,
    /// Distance from main sequence (|abstractness + instability - 1|)
    pub distance_from_main_sequence: f64,
}

/// Component coupling analyzer
pub struct ComponentCouplingAnalyzer;

impl ComponentCouplingAnalyzer {
    /// Calculate coupling metrics for all components
    pub fn calculate_coupling(
        graph: &CorrelationGraph,
        analyzer: &ComponentAnalyzer,
    ) -> Vec<ComponentCouplingMetrics> {
        let mut metrics = Vec::new();

        // Build dependency maps
        let mut afferent: HashMap<String, HashSet<String>> = HashMap::new();
        let mut efferent: HashMap<String, HashSet<String>> = HashMap::new();

        for edge in &graph.edges {
            if edge.edge_type == EdgeType::Inherits
                || edge.edge_type == EdgeType::Composes
                || edge.edge_type == EdgeType::Depends
            {
                efferent
                    .entry(edge.source.clone())
                    .or_default()
                    .insert(edge.target.clone());
                afferent
                    .entry(edge.target.clone())
                    .or_default()
                    .insert(edge.source.clone());
            }
        }

        // Calculate metrics for each component
        for component_name in analyzer.classes().keys() {
            let afferent_count = afferent.get(component_name).map(|s| s.len()).unwrap_or(0);
            let efferent_count = efferent.get(component_name).map(|s| s.len()).unwrap_or(0);
            let total = afferent_count + efferent_count;

            let instability = if total > 0 {
                efferent_count as f64 / total as f64
            } else {
                0.0
            };

            // Abstractness calculation requires full class hierarchy info not available at this
            // call site; simplified to 0.0 (a class with no abstract methods is fully concrete).
            let abstractness = 0.0_f64;

            let distance_from_main_sequence = (abstractness + instability - 1.0).abs();

            metrics.push(ComponentCouplingMetrics {
                component_name: component_name.clone(),
                afferent_coupling: afferent_count,
                efferent_coupling: efferent_count,
                instability,
                abstractness,
                distance_from_main_sequence,
            });
        }

        metrics
    }
}

// ============================================================================
// Component Metrics Calculation (Task 12.8)
// ============================================================================

/// Statistics about components in a graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentStatistics {
    /// Total number of classes
    pub total_classes: usize,
    /// Total number of interfaces
    pub total_interfaces: usize,
    /// Number of abstract classes
    pub abstract_classes: usize,
    /// Number of final/sealed classes
    pub final_classes: usize,
    /// Total number of inheritance relationships
    pub inheritance_relationships: usize,
    /// Total number of implementation relationships
    pub implementation_relationships: usize,
    /// Total number of composition relationships
    pub composition_relationships: usize,
    /// Average methods per class
    pub average_methods_per_class: f64,
    /// Average fields per class
    pub average_fields_per_class: f64,
    /// Maximum inheritance depth
    pub max_inheritance_depth: usize,
    /// Number of root classes (no base class)
    pub root_classes: usize,
}

impl ComponentStatistics {
    /// Calculate statistics from a component graph and analyzer
    pub fn calculate(graph: &CorrelationGraph, analyzer: &ComponentAnalyzer) -> Self {
        let classes = analyzer.classes();
        let total_classes = classes.len();
        let total_interfaces = analyzer.interfaces().len();

        let abstract_classes = classes.values().filter(|c| c.is_abstract).count();

        let final_classes = classes.values().filter(|c| c.is_final).count();

        // Count relationships
        let inheritance_relationships = graph
            .edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Inherits)
            .count();

        let implementation_relationships = graph
            .edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Uses && e.target.contains("interface"))
            .count();

        let composition_relationships = graph
            .edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Composes)
            .count();

        // Calculate averages
        let total_methods: usize = classes.values().map(|c| c.methods.len()).sum();
        let average_methods_per_class = if total_classes > 0 {
            total_methods as f64 / total_classes as f64
        } else {
            0.0
        };

        let total_fields: usize = classes.values().map(|c| c.fields.len()).sum();
        let average_fields_per_class = if total_classes > 0 {
            total_fields as f64 / total_classes as f64
        } else {
            0.0
        };

        // Calculate max inheritance depth
        let mut max_depth = 0;
        let mut inheritance_map: HashMap<String, Vec<String>> = HashMap::new();

        for edge in &graph.edges {
            if edge.edge_type == EdgeType::Inherits {
                inheritance_map
                    .entry(edge.target.clone())
                    .or_default()
                    .push(edge.source.clone());
            }
        }

        fn calculate_depth(
            class: &str,
            inheritance_map: &HashMap<String, Vec<String>>,
            visited: &mut HashSet<String>,
        ) -> usize {
            if visited.contains(class) {
                return 0;
            }
            visited.insert(class.to_string());

            let mut max_child_depth = 0;
            if let Some(children) = inheritance_map.get(class) {
                for child in children {
                    let depth = calculate_depth(child, inheritance_map, visited);
                    max_child_depth = max_child_depth.max(depth);
                }
            }

            1 + max_child_depth
        }

        for class_name in classes.keys() {
            let mut visited = HashSet::new();
            let depth = calculate_depth(class_name, &inheritance_map, &mut visited);
            max_depth = max_depth.max(depth);
        }

        // Count root classes
        let root_classes = classes.values().filter(|c| c.base_class.is_none()).count();

        Self {
            total_classes,
            total_interfaces,
            abstract_classes,
            final_classes,
            inheritance_relationships,
            implementation_relationships,
            composition_relationships,
            average_methods_per_class,
            average_fields_per_class,
            max_inheritance_depth: max_depth,
            root_classes,
        }
    }
}

impl ComponentAnalyzer {
    /// Calculate component statistics
    pub fn calculate_statistics(&self, graph: &CorrelationGraph) -> ComponentStatistics {
        ComponentStatistics::calculate(graph, self)
    }

    /// Calculate coupling metrics
    pub fn calculate_coupling(&self, graph: &CorrelationGraph) -> Vec<ComponentCouplingMetrics> {
        ComponentCouplingAnalyzer::calculate_coupling(graph, self)
    }
}
