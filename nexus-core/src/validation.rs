//! Graph validation and integrity checks
//!
//! This module provides comprehensive validation for graph data structures,
//! ensuring data integrity, consistency, and correctness across the entire graph.

use crate::error::Result;
use crate::graph::simple::PropertyValue;
use crate::graph::{Edge, Graph, Node};
use std::collections::{HashMap, HashSet};

/// Validation result containing detailed information about validation checks
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Whether the validation passed
    pub is_valid: bool,
    /// List of validation errors found
    pub errors: Vec<ValidationError>,
    /// List of validation warnings
    pub warnings: Vec<ValidationWarning>,
    /// Statistics about the validation process
    pub stats: ValidationStats,
}

/// Validation error with detailed information
#[derive(Debug, Clone)]
pub struct ValidationError {
    /// Type of validation error
    pub error_type: ValidationErrorType,
    /// Detailed error message
    pub message: String,
    /// ID of the entity that caused the error (if applicable)
    pub entity_id: Option<String>,
    /// Severity level
    pub severity: ValidationSeverity,
}

/// Validation warning with detailed information
#[derive(Debug, Clone)]
pub struct ValidationWarning {
    /// Type of validation warning
    pub warning_type: ValidationWarningType,
    /// Detailed warning message
    pub message: String,
    /// ID of the entity that caused the warning (if applicable)
    pub entity_id: Option<String>,
}

/// Types of validation errors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationErrorType {
    /// Node validation errors
    NodeNotFound,
    NodeHasInvalidId,
    NodeHasEmptyLabels,
    NodeHasInvalidProperties,
    NodeHasDuplicateProperties,
    NodeHasDuplicateLabels,

    /// Edge validation errors
    EdgeNotFound,
    EdgeHasInvalidId,
    EdgeHasInvalidSource,
    EdgeHasInvalidTarget,
    EdgeHasEmptyType,
    EdgeHasInvalidProperties,
    EdgeHasDuplicateProperties,
    EdgeReferencesNonExistentNode,

    /// Graph consistency errors
    OrphanedEdge,
    DuplicateEdge,
    SelfLoop,
    InvalidGraphStructure,

    /// Storage consistency errors
    StorageInconsistency,
    CatalogInconsistency,
    IndexInconsistency,
}

/// Types of validation warnings
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationWarningType {
    /// Performance warnings
    LargePropertyValue,
    ExcessiveLabels,
    ExcessiveProperties,

    /// Data quality warnings
    EmptyNode,
    EmptyEdge,
    UnusedLabel,
    UnusedRelationshipType,

    /// Structural warnings
    IsolatedNode,
    DenseSubgraph,
    SparseGraph,
}

/// Severity levels for validation errors
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ValidationSeverity {
    /// Low severity - minor issues
    Low,
    /// Medium severity - significant issues
    Medium,
    /// High severity - serious issues
    High,
    /// Critical severity - data corruption
    Critical,
}

/// Statistics about the validation process
#[derive(Debug, Clone)]
pub struct ValidationStats {
    /// Number of nodes validated
    pub nodes_checked: usize,
    /// Number of edges validated
    pub edges_checked: usize,
    /// Number of properties validated
    pub properties_checked: usize,
    /// Number of labels validated
    pub labels_checked: usize,
    /// Number of relationship types validated
    pub relationship_types_checked: usize,
    /// Total validation time in milliseconds
    pub validation_time_ms: u64,
}

/// Graph validator that performs comprehensive integrity checks
pub struct GraphValidator {
    /// Configuration for validation
    config: ValidationConfig,
}

/// Configuration for graph validation
#[derive(Debug, Clone)]
pub struct ValidationConfig {
    /// Whether to validate node integrity
    pub validate_nodes: bool,
    /// Whether to validate edge integrity
    pub validate_edges: bool,
    /// Whether to validate graph consistency
    pub validate_consistency: bool,
    /// Whether to validate storage consistency
    pub validate_storage: bool,
    /// Whether to validate catalog consistency
    pub validate_catalog: bool,
    /// Whether to validate index consistency
    pub validate_indexes: bool,
    /// Maximum number of errors to report
    pub max_errors: usize,
    /// Maximum number of warnings to report
    pub max_warnings: usize,
    /// Whether to stop on first critical error
    pub stop_on_critical: bool,
    /// Whether to validate properties
    pub validate_properties: bool,
    /// Whether to check for orphaned entities
    pub check_orphaned: bool,
    /// Whether to check for duplicates
    pub check_duplicates: bool,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            validate_nodes: true,
            validate_edges: true,
            validate_consistency: true,
            validate_storage: false,
            validate_catalog: false,
            validate_indexes: false,
            max_errors: 1000,
            max_warnings: 500,
            stop_on_critical: false,
            validate_properties: true,
            check_orphaned: true,
            check_duplicates: true,
        }
    }
}

impl GraphValidator {
    /// Create a new graph validator with default configuration
    pub fn new() -> Self {
        Self {
            config: ValidationConfig::default(),
        }
    }

    /// Create a new graph validator with custom configuration
    pub fn with_config(config: ValidationConfig) -> Self {
        Self { config }
    }

    /// Validate the entire graph
    pub fn validate_graph(&self, graph: &Graph) -> Result<ValidationResult> {
        let start_time = std::time::Instant::now();
        let mut result = ValidationResult {
            is_valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
            stats: ValidationStats {
                nodes_checked: 0,
                edges_checked: 0,
                properties_checked: 0,
                labels_checked: 0,
                relationship_types_checked: 0,
                validation_time_ms: 0,
            },
        };

        // Validate nodes if enabled
        if self.config.validate_nodes {
            self.validate_nodes(graph, &mut result)?;
        }

        // Validate edges if enabled
        if self.config.validate_edges {
            self.validate_edges(graph, &mut result)?;
        }

        // Validate graph consistency if enabled
        if self.config.validate_consistency {
            self.validate_consistency(graph, &mut result)?;
        }

        // Update validation time
        result.stats.validation_time_ms = start_time.elapsed().as_millis() as u64;

        // Determine overall validity
        result.is_valid = result.errors.is_empty()
            || result
                .errors
                .iter()
                .all(|e| e.severity < ValidationSeverity::Critical);

        Ok(result)
    }

    /// Validate all nodes in the graph
    fn validate_nodes(&self, graph: &Graph, result: &mut ValidationResult) -> Result<()> {
        let nodes = graph.get_all_nodes()?;
        result.stats.nodes_checked = nodes.len();

        for node in nodes {
            self.validate_node(&node, graph, result)?;

            // Check if we should stop due to critical errors
            if self.config.stop_on_critical
                && result
                    .errors
                    .iter()
                    .any(|e| e.severity == ValidationSeverity::Critical)
            {
                break;
            }

            // Check if we've reached the error limit
            if result.errors.len() >= self.config.max_errors {
                break;
            }
        }

        Ok(())
    }

    /// Validate a single node
    fn validate_node(
        &self,
        node: &Node,
        _graph: &Graph,
        result: &mut ValidationResult,
    ) -> Result<()> {
        // Validate node ID
        if node.id.value() == u64::MAX {
            result.errors.push(ValidationError {
                error_type: ValidationErrorType::NodeHasInvalidId,
                message: "Node has invalid ID (u64::MAX)".to_string(),
                entity_id: Some(format!("node:{}", node.id.value())),
                severity: ValidationSeverity::High,
            });
        }

        // Validate labels
        if node.labels.is_empty() {
            result.warnings.push(ValidationWarning {
                warning_type: ValidationWarningType::EmptyNode,
                message: "Node has no labels".to_string(),
                entity_id: Some(format!("node:{}", node.id.value())),
            });
        }

        // Check for duplicate labels
        let mut seen_labels = HashSet::new();
        for label in &node.labels {
            if !seen_labels.insert(label) {
                result.errors.push(ValidationError {
                    error_type: ValidationErrorType::NodeHasDuplicateLabels,
                    message: format!("Node has duplicate label: {}", label),
                    entity_id: Some(format!("node:{}", node.id.value())),
                    severity: ValidationSeverity::Medium,
                });
            }
        }

        result.stats.labels_checked += node.labels.len();

        // Validate properties if enabled
        if self.config.validate_properties {
            self.validate_node_properties(node, result)?;
        }

        Ok(())
    }

    /// Validate node properties
    fn validate_node_properties(&self, node: &Node, result: &mut ValidationResult) -> Result<()> {
        result.stats.properties_checked += node.properties.len();

        // Check for duplicate property keys
        let mut seen_keys = HashSet::new();
        for key in node.properties.keys() {
            if !seen_keys.insert(key) {
                result.errors.push(ValidationError {
                    error_type: ValidationErrorType::NodeHasDuplicateProperties,
                    message: format!("Node has duplicate property key: {}", key),
                    entity_id: Some(format!("node:{}", node.id.value())),
                    severity: ValidationSeverity::Medium,
                });
            }
        }

        // Validate property values
        for (key, value) in &node.properties {
            self.validate_property_value(key, value, result)?;
        }

        Ok(())
    }

    /// Validate all edges in the graph
    fn validate_edges(&self, graph: &Graph, result: &mut ValidationResult) -> Result<()> {
        let edges = graph.get_all_edges()?;
        result.stats.edges_checked = edges.len();

        for edge in edges {
            self.validate_edge(&edge, graph, result)?;

            // Check if we should stop due to critical errors
            if self.config.stop_on_critical
                && result
                    .errors
                    .iter()
                    .any(|e| e.severity == ValidationSeverity::Critical)
            {
                break;
            }

            // Check if we've reached the error limit
            if result.errors.len() >= self.config.max_errors {
                break;
            }
        }

        Ok(())
    }

    /// Validate a single edge
    fn validate_edge(
        &self,
        edge: &Edge,
        graph: &Graph,
        result: &mut ValidationResult,
    ) -> Result<()> {
        // Validate edge ID
        if edge.id.value() == u64::MAX {
            result.errors.push(ValidationError {
                error_type: ValidationErrorType::EdgeHasInvalidId,
                message: "Edge has invalid ID (u64::MAX)".to_string(),
                entity_id: Some(format!("edge:{}", edge.id.value())),
                severity: ValidationSeverity::High,
            });
        }

        // Validate source node exists
        if graph.get_node(edge.source)?.is_none() {
            result.errors.push(ValidationError {
                error_type: ValidationErrorType::EdgeReferencesNonExistentNode,
                message: format!(
                    "Edge references non-existent source node: {}",
                    edge.source.value()
                ),
                entity_id: Some(format!("edge:{}", edge.id.value())),
                severity: ValidationSeverity::Critical,
            });
        }

        // Validate target node exists
        if graph.get_node(edge.target)?.is_none() {
            result.errors.push(ValidationError {
                error_type: ValidationErrorType::EdgeReferencesNonExistentNode,
                message: format!(
                    "Edge references non-existent target node: {}",
                    edge.target.value()
                ),
                entity_id: Some(format!("edge:{}", edge.id.value())),
                severity: ValidationSeverity::Critical,
            });
        }

        // Check for self-loops
        if edge.source == edge.target {
            result.warnings.push(ValidationWarning {
                warning_type: ValidationWarningType::DenseSubgraph,
                message: "Edge is a self-loop".to_string(),
                entity_id: Some(format!("edge:{}", edge.id.value())),
            });
        }

        // Validate relationship type
        if edge.relationship_type.is_empty() {
            result.errors.push(ValidationError {
                error_type: ValidationErrorType::EdgeHasEmptyType,
                message: "Edge has empty relationship type".to_string(),
                entity_id: Some(format!("edge:{}", edge.id.value())),
                severity: ValidationSeverity::High,
            });
        }

        result.stats.relationship_types_checked += 1;

        // Validate properties if enabled
        if self.config.validate_properties {
            self.validate_edge_properties(edge, result)?;
        }

        Ok(())
    }

    /// Validate edge properties
    fn validate_edge_properties(&self, edge: &Edge, result: &mut ValidationResult) -> Result<()> {
        result.stats.properties_checked += edge.properties.len();

        // Check for duplicate property keys
        let mut seen_keys = HashSet::new();
        for key in edge.properties.keys() {
            if !seen_keys.insert(key) {
                result.errors.push(ValidationError {
                    error_type: ValidationErrorType::EdgeHasDuplicateProperties,
                    message: format!("Edge has duplicate property key: {}", key),
                    entity_id: Some(format!("edge:{}", edge.id.value())),
                    severity: ValidationSeverity::Medium,
                });
            }
        }

        // Validate property values
        for (key, value) in &edge.properties {
            self.validate_property_value(key, value, result)?;
        }

        Ok(())
    }

    /// Validate a property value
    fn validate_property_value(
        &self,
        key: &str,
        value: &PropertyValue,
        result: &mut ValidationResult,
    ) -> Result<()> {
        match value {
            PropertyValue::String(_) => {
                // String references are valid
            }
            PropertyValue::Int64(val) => {
                if *val == i64::MIN || *val == i64::MAX {
                    result.warnings.push(ValidationWarning {
                        warning_type: ValidationWarningType::LargePropertyValue,
                        message: format!("Property '{}' has extreme integer value: {}", key, val),
                        entity_id: None,
                    });
                }
            }
            PropertyValue::Float64(val) => {
                if val.is_infinite() || val.is_nan() {
                    result.errors.push(ValidationError {
                        error_type: ValidationErrorType::NodeHasInvalidProperties,
                        message: format!("Property '{}' has invalid float value: {}", key, val),
                        entity_id: None,
                        severity: ValidationSeverity::Medium,
                    });
                }
            }
            PropertyValue::Bool(_) => {
                // Boolean values are always valid
            }
            PropertyValue::Null => {
                // Null values are valid
            }
            PropertyValue::Bytes(_) => {
                // Bytes values are valid
            }
        }

        Ok(())
    }

    /// Validate graph consistency
    fn validate_consistency(&self, graph: &Graph, result: &mut ValidationResult) -> Result<()> {
        if self.config.check_orphaned {
            self.check_orphaned_edges(graph, result)?;
        }

        if self.config.check_duplicates {
            self.check_duplicate_edges(graph, result)?;
        }

        self.check_graph_structure(graph, result)?;

        Ok(())
    }

    /// Check for orphaned edges (edges that reference non-existent nodes)
    fn check_orphaned_edges(&self, graph: &Graph, result: &mut ValidationResult) -> Result<()> {
        let edges = graph.get_all_edges()?;
        let mut node_ids = HashSet::new();

        // Collect all valid node IDs
        let nodes = graph.get_all_nodes()?;
        for node in nodes {
            node_ids.insert(node.id);
        }

        for edge in edges {
            if !node_ids.contains(&edge.source) || !node_ids.contains(&edge.target) {
                result.errors.push(ValidationError {
                    error_type: ValidationErrorType::OrphanedEdge,
                    message: format!("Edge {} references non-existent nodes", edge.id.value()),
                    entity_id: Some(format!("edge:{}", edge.id.value())),
                    severity: ValidationSeverity::Critical,
                });
            }
        }

        Ok(())
    }

    /// Check for duplicate edges
    fn check_duplicate_edges(&self, graph: &Graph, result: &mut ValidationResult) -> Result<()> {
        let edges = graph.get_all_edges()?;
        let mut edge_map = HashMap::new();

        for edge in edges {
            let key = (edge.source, edge.target, edge.relationship_type.clone());
            if let Some(_existing_id) = edge_map.get(&key) {
                result.errors.push(ValidationError {
                    error_type: ValidationErrorType::DuplicateEdge,
                    message: format!(
                        "Duplicate edge found: {} -> {} (type: {})",
                        edge.source.value(),
                        edge.target.value(),
                        edge.relationship_type
                    ),
                    entity_id: Some(format!("edge:{}", edge.id.value())),
                    severity: ValidationSeverity::Medium,
                });
            } else {
                edge_map.insert(key, edge.id);
            }
        }

        Ok(())
    }

    /// Check overall graph structure
    fn check_graph_structure(&self, graph: &Graph, result: &mut ValidationResult) -> Result<()> {
        let nodes = graph.get_all_nodes()?;
        let edges = graph.get_all_edges()?;

        // Check for isolated nodes
        let mut connected_nodes = HashSet::new();
        for edge in &edges {
            connected_nodes.insert(edge.source);
            connected_nodes.insert(edge.target);
        }

        for node in &nodes {
            if !connected_nodes.contains(&node.id) {
                result.warnings.push(ValidationWarning {
                    warning_type: ValidationWarningType::IsolatedNode,
                    message: format!("Node {} is isolated (no edges)", node.id.value()),
                    entity_id: Some(format!("node:{}", node.id.value())),
                });
            }
        }

        // Check graph density
        let node_count = nodes.len();
        let edge_count = edges.len();

        if node_count > 0 {
            let max_edges = node_count * (node_count - 1) / 2;
            let density = if max_edges > 0 {
                edge_count as f64 / max_edges as f64
            } else {
                0.0
            };

            if density > 0.8 {
                result.warnings.push(ValidationWarning {
                    warning_type: ValidationWarningType::DenseSubgraph,
                    message: format!("Graph is very dense: {:.2}%", density * 100.0),
                    entity_id: None,
                });
            } else if density < 0.01 && node_count > 100 {
                result.warnings.push(ValidationWarning {
                    warning_type: ValidationWarningType::SparseGraph,
                    message: format!("Graph is very sparse: {:.2}%", density * 100.0),
                    entity_id: None,
                });
            }
        }

        Ok(())
    }
}

impl Default for GraphValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::simple::PropertyValue;
    use tempfile::TempDir;

    fn create_test_graph() -> (Graph, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = crate::storage::RecordStore::new(dir.path()).unwrap();
        let catalog =
            std::sync::Arc::new(crate::catalog::Catalog::new(dir.path().join("catalog")).unwrap());
        let graph = Graph::new(store, catalog);
        (graph, dir)
    }

    #[test]
    fn test_validation_result_creation() {
        let result = ValidationResult {
            is_valid: true,
            errors: vec![],
            warnings: vec![],
            stats: ValidationStats {
                nodes_checked: 0,
                edges_checked: 0,
                properties_checked: 0,
                labels_checked: 0,
                relationship_types_checked: 0,
                validation_time_ms: 0,
            },
        };

        assert!(result.is_valid);
        assert!(result.errors.is_empty());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_validation_error_creation() {
        let error = ValidationError {
            error_type: ValidationErrorType::NodeNotFound,
            message: "Node not found".to_string(),
            entity_id: Some("node:123".to_string()),
            severity: ValidationSeverity::High,
        };

        assert_eq!(error.error_type, ValidationErrorType::NodeNotFound);
        assert_eq!(error.severity, ValidationSeverity::High);
        assert!(error.entity_id.is_some());
    }

    #[test]
    fn test_validation_warning_creation() {
        let warning = ValidationWarning {
            warning_type: ValidationWarningType::EmptyNode,
            message: "Node is empty".to_string(),
            entity_id: Some("node:456".to_string()),
        };

        assert_eq!(warning.warning_type, ValidationWarningType::EmptyNode);
        assert!(warning.entity_id.is_some());
    }

    #[test]
    fn test_validation_config_default() {
        let config = ValidationConfig::default();
        assert!(config.validate_nodes);
        assert!(config.validate_edges);
        assert!(config.validate_consistency);
        assert!(!config.validate_storage);
        assert_eq!(config.max_errors, 1000);
        assert_eq!(config.max_warnings, 500);
    }

    #[test]
    fn test_graph_validator_creation() {
        let validator = GraphValidator::new();
        assert!(validator.config.validate_nodes);
        assert!(validator.config.validate_edges);
    }

    #[test]
    fn test_graph_validator_with_config() {
        let config = ValidationConfig {
            validate_nodes: false,
            validate_edges: true,
            validate_consistency: false,
            validate_storage: false,
            validate_catalog: false,
            validate_indexes: false,
            max_errors: 100,
            max_warnings: 50,
            stop_on_critical: true,
            validate_properties: false,
            check_orphaned: false,
            check_duplicates: false,
        };

        let validator = GraphValidator::with_config(config);
        assert!(!validator.config.validate_nodes);
        assert!(validator.config.validate_edges);
        assert!(!validator.config.validate_consistency);
        assert_eq!(validator.config.max_errors, 100);
    }

    #[test]
    #[ignore] // TODO: Fix temp dir race condition
    fn test_validate_empty_graph() {
        let (graph, _dir) = create_test_graph();
        let validator = GraphValidator::new();
        let result = validator.validate_graph(&graph).unwrap();

        assert!(result.is_valid);
        assert!(result.errors.is_empty());
        assert_eq!(result.stats.nodes_checked, 0);
        assert_eq!(result.stats.edges_checked, 0);
    }

    #[test]
    fn test_validate_graph_with_nodes() {
        let (graph, _dir) = create_test_graph();

        // Create some nodes
        let _node1 = graph.create_node(vec!["Person".to_string()]).unwrap();
        let _node2 = graph.create_node(vec!["Company".to_string()]).unwrap();

        let validator = GraphValidator::new();
        let result = validator.validate_graph(&graph).unwrap();

        assert!(result.is_valid);
        assert_eq!(result.stats.nodes_checked, 2);
        assert_eq!(result.stats.labels_checked, 2);
    }

    #[test]
    fn test_validate_node_with_duplicate_labels() {
        let (graph, _dir) = create_test_graph();

        // Create a node with duplicate labels directly (bypassing add_label check)
        let node_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let mut node = graph.get_node(node_id).unwrap().unwrap();
        node.labels.push("Person".to_string()); // Add duplicate label directly
        graph.update_node(node).unwrap();

        let validator = GraphValidator::new();
        let result = validator.validate_graph(&graph).unwrap();

        // The validation should catch the duplicate label
        assert!(!result.errors.is_empty());
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.error_type == ValidationErrorType::NodeHasDuplicateLabels)
        );
    }

    #[test]
    fn test_validate_node_properties() {
        let (graph, _dir) = create_test_graph();

        let node_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let mut node = graph.get_node(node_id).unwrap().unwrap();

        // Add some properties
        node.set_property(
            "name".to_string(),
            PropertyValue::String("test".to_string()),
        );
        node.set_property("age".to_string(), PropertyValue::Int64(30));
        node.set_property("height".to_string(), PropertyValue::Float64(1.75));
        graph.update_node(node).unwrap();

        let validator = GraphValidator::new();
        let result = validator.validate_graph(&graph).unwrap();

        assert!(result.is_valid);
        assert_eq!(result.stats.properties_checked, 3);
    }

    #[test]
    fn test_validate_invalid_property_values() {
        let (graph, _dir) = create_test_graph();

        let node_id = graph.create_node(vec!["Person".to_string()]).unwrap();
        let mut node = graph.get_node(node_id).unwrap().unwrap();

        // Add invalid property values
        node.set_property(
            "invalid_float".to_string(),
            PropertyValue::Float64(f64::NAN),
        );
        node.set_property(
            "infinite_float".to_string(),
            PropertyValue::Float64(f64::INFINITY),
        );

        // Test validation directly on the node (since properties aren't persisted yet)
        let validator = GraphValidator::new();
        let mut result = ValidationResult {
            is_valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
            stats: ValidationStats {
                nodes_checked: 0,
                edges_checked: 0,
                properties_checked: 0,
                labels_checked: 0,
                relationship_types_checked: 0,
                validation_time_ms: 0,
            },
        };

        validator
            .validate_node_properties(&node, &mut result)
            .unwrap();

        assert!(!result.errors.is_empty());
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.message.contains("invalid float value"))
        );
    }

    #[test]
    fn test_validate_edges() {
        let (graph, _dir) = create_test_graph();

        // Create nodes
        let node1 = graph.create_node(vec!["Person".to_string()]).unwrap();
        let node2 = graph.create_node(vec!["Person".to_string()]).unwrap();

        // Create edge
        let _edge = graph
            .create_edge(node1, node2, "KNOWS".to_string())
            .unwrap();

        let validator = GraphValidator::new();
        let result = validator.validate_graph(&graph).unwrap();

        assert!(result.is_valid);
        assert_eq!(result.stats.edges_checked, 1);
        assert_eq!(result.stats.relationship_types_checked, 1);
    }

    #[test]
    fn test_validate_self_loop() {
        let (graph, _dir) = create_test_graph();

        let node = graph.create_node(vec!["Person".to_string()]).unwrap();

        // Create self-loop edge
        let _edge = graph.create_edge(node, node, "KNOWS".to_string()).unwrap();

        let validator = GraphValidator::new();
        let result = validator.validate_graph(&graph).unwrap();

        assert!(result.is_valid);
        assert!(!result.warnings.is_empty());
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.warning_type == ValidationWarningType::DenseSubgraph)
        );
    }

    #[test]
    #[ignore] // TODO: Fix temp dir race condition
    fn test_validate_isolated_node() {
        let (graph, _dir) = create_test_graph();

        // Create isolated node
        let _node = graph.create_node(vec!["Person".to_string()]).unwrap();

        let validator = GraphValidator::new();
        let result = validator.validate_graph(&graph).unwrap();

        assert!(result.is_valid);
        assert!(!result.warnings.is_empty());
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.warning_type == ValidationWarningType::IsolatedNode)
        );
    }

    #[test]
    fn test_validation_severity_ordering() {
        assert!(ValidationSeverity::Low < ValidationSeverity::Medium);
        assert!(ValidationSeverity::Medium < ValidationSeverity::High);
        assert!(ValidationSeverity::High < ValidationSeverity::Critical);
    }

    #[test]
    fn test_validation_stats() {
        let stats = ValidationStats {
            nodes_checked: 10,
            edges_checked: 15,
            properties_checked: 25,
            labels_checked: 5,
            relationship_types_checked: 3,
            validation_time_ms: 100,
        };

        assert_eq!(stats.nodes_checked, 10);
        assert_eq!(stats.edges_checked, 15);
        assert_eq!(stats.properties_checked, 25);
        assert_eq!(stats.labels_checked, 5);
        assert_eq!(stats.relationship_types_checked, 3);
        assert_eq!(stats.validation_time_ms, 100);
    }
}
