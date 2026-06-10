//! Core data-flow analyser: scans source files, tracks variables and
//! transformations, builds the enhanced data-flow graph.

use std::collections::HashMap;

use crate::Result;
use crate::graph::correlation::{CorrelationGraph, EdgeType, GraphEdge, GraphNode, NodeType};

use super::{
    DataFlowEdge, DataTransformation, TransformationType, TypePropagator, UsageType,
    VariableTracker,
};

/// Data flow analyzer
pub struct DataFlowAnalyzer {
    /// Variable tracker
    tracker: VariableTracker,
    /// Data transformations
    transformations: Vec<DataTransformation>,
    /// Type propagator (Task 11.5)
    type_propagator: TypePropagator,
}

impl DataFlowAnalyzer {
    /// Create a new data flow analyzer
    pub fn new() -> Self {
        Self {
            tracker: VariableTracker::new(),
            transformations: Vec::new(),
            type_propagator: TypePropagator::new(),
        }
    }

    /// Analyze source code for data flow patterns
    pub fn analyze_source_code(&mut self, files: &HashMap<String, String>) -> Result<()> {
        // Simple heuristic-based analysis
        // In a full implementation, this would use AST parsing
        for (file_path, content) in files {
            self.analyze_file(file_path, content)?;
        }
        Ok(())
    }

    /// Analyze a single file for data flow
    fn analyze_file(&mut self, file_path: &str, content: &str) -> Result<()> {
        let lines: Vec<&str> = content.lines().collect();

        for (line_num, line) in lines.iter().enumerate() {
            let line = line.trim();

            // Simple pattern matching for variable definitions (heuristic; full
            // implementation would use a proper language-aware parser).
            if line.contains("let ") || line.contains("const ") || line.contains("var ") {
                if let Some(var_name) = self.extract_variable_name(line) {
                    let is_mutable = line.contains("let ") && !line.contains("const");
                    let type_hint = self.extract_type_hint(line);

                    // Infer type using type propagator (Task 11.5)
                    if type_hint.is_none() {
                        if let Some(_inferred_type) = self
                            .type_propagator
                            .infer_type_from_definition(&var_name, line)
                        {
                            // Type inferred, will be used
                        }
                    }

                    self.tracker.track_definition(
                        var_name.clone(),
                        file_path.to_string(),
                        line_num + 1,
                        type_hint.or_else(|| self.type_propagator.get_type(&var_name).cloned()),
                        is_mutable,
                        false, // Would need function context to determine
                    );
                }
            }

            // Track variable usages
            let var_names: Vec<String> = self.tracker.variables.keys().cloned().collect();
            for var_name in var_names {
                if line.contains(&var_name) {
                    let usage_type = if line.contains("=") && line.contains(&var_name) {
                        UsageType::ReadWrite
                    } else if line.contains("=") {
                        UsageType::Write
                    } else {
                        UsageType::Read
                    };

                    self.tracker.track_usage(
                        &var_name,
                        file_path.to_string(),
                        None, // Would need function context
                        line_num + 1,
                        usage_type,
                    );
                }
            }

            // Detect data transformations (Task 11.3)
            self.detect_transformations(line, file_path, line_num + 1);
        }

        // Propagate types through transformations (Task 11.5)
        self.type_propagator
            .analyze_and_propagate(&mut self.transformations);

        Ok(())
    }

    /// Detect data transformations in a line of code
    fn detect_transformations(&mut self, line: &str, _file_path: &str, _line_num: usize) {
        let line = line.trim();

        // Detect assignments (direct transformations)
        if let Some(assignment) = self.detect_assignment(line) {
            self.add_transformation(assignment);
        }

        // Detect function call transformations
        if let Some(func_call) = self.detect_function_call_transformation(line) {
            self.add_transformation(func_call);
        }

        // Detect type conversions
        if let Some(type_conv) = self.detect_type_conversion(line) {
            self.add_transformation(type_conv);
        }

        // Detect aggregation operations
        if let Some(agg) = self.detect_aggregation(line) {
            self.add_transformation(agg);
        }

        // Detect filter operations
        if let Some(filter) = self.detect_filter_operation(line) {
            self.add_transformation(filter);
        }

        // Detect map operations
        if let Some(map_op) = self.detect_map_operation(line) {
            self.add_transformation(map_op);
        }

        // Detect reduce operations
        if let Some(reduce_op) = self.detect_reduce_operation(line) {
            self.add_transformation(reduce_op);
        }
    }

    /// Detect assignment transformations (e.g., y = x + 1)
    pub fn detect_assignment(&self, line: &str) -> Option<DataTransformation> {
        if !line.contains("=") {
            return None;
        }

        // Simple pattern: target = source expression
        if let Some(equals_pos) = line.find('=') {
            let target = line[..equals_pos].trim();
            let source_expr = line[equals_pos + 1..].trim();

            // Extract source variables from expression
            let source_vars: Vec<String> = self
                .tracker
                .variables
                .keys()
                .filter(|var| source_expr.contains(*var))
                .cloned()
                .collect();

            if !source_vars.is_empty() && !target.is_empty() {
                let source = source_vars.join(", ");
                return Some(DataTransformation {
                    source,
                    target: target.to_string(),
                    transformation_type: TransformationType::Assignment,
                    input_types: vec![],
                    output_types: vec![],
                });
            }
        }

        None
    }

    /// Detect function call transformations (e.g., y = process(x))
    pub fn detect_function_call_transformation(&self, line: &str) -> Option<DataTransformation> {
        // Pattern: target = function_name(...)
        if let Some(equals_pos) = line.find('=') {
            let target = line[..equals_pos].trim();
            let expr = line[equals_pos + 1..].trim();

            // Look for function call pattern: name(...)
            if let Some(open_paren) = expr.find('(') {
                let func_name = expr[..open_paren].trim();
                let args = &expr[open_paren + 1..];

                // Extract argument variables
                let arg_vars: Vec<String> = self
                    .tracker
                    .variables
                    .keys()
                    .filter(|var| args.contains(*var))
                    .cloned()
                    .collect();

                if !arg_vars.is_empty() && !target.is_empty() && !func_name.is_empty() {
                    return Some(DataTransformation {
                        source: format!("{}:{}", func_name, arg_vars.join(", ")),
                        target: target.to_string(),
                        transformation_type: TransformationType::FunctionCall,
                        input_types: vec![],
                        output_types: vec![],
                    });
                }
            }
        }

        None
    }

    /// Detect type conversions (e.g., y = x as String, y = String::from(x))
    pub fn detect_type_conversion(&self, line: &str) -> Option<DataTransformation> {
        let type_conversion_patterns = [
            " as ",
            "::from(",
            ".parse()",
            ".into()",
            ".to_string()",
            ".to_vec()",
        ];

        for pattern in &type_conversion_patterns {
            if line.contains(pattern) {
                if let Some(equals_pos) = line.find('=') {
                    let target = line[..equals_pos].trim();
                    let source_vars: Vec<String> = self
                        .tracker
                        .variables
                        .keys()
                        .filter(|var| line.contains(*var))
                        .cloned()
                        .collect();

                    if !source_vars.is_empty() && !target.is_empty() {
                        return Some(DataTransformation {
                            source: source_vars.join(", "),
                            target: target.to_string(),
                            transformation_type: TransformationType::TypeConversion,
                            input_types: vec![],
                            output_types: vec![],
                        });
                    }
                }
            }
        }

        None
    }

    /// Detect aggregation operations (e.g., sum, count, avg)
    pub fn detect_aggregation(&self, line: &str) -> Option<DataTransformation> {
        let aggregation_patterns = [
            ".sum()",
            ".count()",
            ".avg()",
            ".average()",
            ".max()",
            ".min()",
            ".reduce(",
        ];

        for pattern in &aggregation_patterns {
            if line.contains(pattern) {
                if let Some(equals_pos) = line.find('=') {
                    let target = line[..equals_pos].trim();
                    let source_vars: Vec<String> = self
                        .tracker
                        .variables
                        .keys()
                        .filter(|var| line.contains(*var))
                        .cloned()
                        .collect();

                    if !source_vars.is_empty() && !target.is_empty() {
                        return Some(DataTransformation {
                            source: source_vars.join(", "),
                            target: target.to_string(),
                            transformation_type: TransformationType::Aggregation,
                            input_types: vec![],
                            output_types: vec![],
                        });
                    }
                }
            }
        }

        None
    }

    /// Detect filter operations (e.g., .filter(), .where())
    pub fn detect_filter_operation(&self, line: &str) -> Option<DataTransformation> {
        let filter_patterns = [".filter(", ".where(", ".select(", ".find("];

        for pattern in &filter_patterns {
            if line.contains(pattern) {
                if let Some(equals_pos) = line.find('=') {
                    let target = line[..equals_pos].trim();
                    let source_vars: Vec<String> = self
                        .tracker
                        .variables
                        .keys()
                        .filter(|var| line.contains(*var))
                        .cloned()
                        .collect();

                    if !source_vars.is_empty() && !target.is_empty() {
                        return Some(DataTransformation {
                            source: source_vars.join(", "),
                            target: target.to_string(),
                            transformation_type: TransformationType::Filter,
                            input_types: vec![],
                            output_types: vec![],
                        });
                    }
                }
            }
        }

        None
    }

    /// Detect map operations (e.g., .map(), .transform())
    pub fn detect_map_operation(&self, line: &str) -> Option<DataTransformation> {
        let map_patterns = [".map(", ".transform(", ".flat_map("];

        for pattern in &map_patterns {
            if line.contains(pattern) {
                if let Some(equals_pos) = line.find('=') {
                    let target = line[..equals_pos].trim();
                    let source_vars: Vec<String> = self
                        .tracker
                        .variables
                        .keys()
                        .filter(|var| line.contains(*var))
                        .cloned()
                        .collect();

                    if !source_vars.is_empty() && !target.is_empty() {
                        return Some(DataTransformation {
                            source: source_vars.join(", "),
                            target: target.to_string(),
                            transformation_type: TransformationType::Map,
                            input_types: vec![],
                            output_types: vec![],
                        });
                    }
                }
            }
        }

        None
    }

    /// Detect reduce operations (e.g., .reduce(), .fold())
    pub fn detect_reduce_operation(&self, line: &str) -> Option<DataTransformation> {
        let reduce_patterns = [".reduce(", ".fold(", ".aggregate("];

        for pattern in &reduce_patterns {
            if line.contains(pattern) {
                if let Some(equals_pos) = line.find('=') {
                    let target = line[..equals_pos].trim();
                    let source_vars: Vec<String> = self
                        .tracker
                        .variables
                        .keys()
                        .filter(|var| line.contains(*var))
                        .cloned()
                        .collect();

                    if !source_vars.is_empty() && !target.is_empty() {
                        return Some(DataTransformation {
                            source: source_vars.join(", "),
                            target: target.to_string(),
                            transformation_type: TransformationType::Reduce,
                            input_types: vec![],
                            output_types: vec![],
                        });
                    }
                }
            }
        }

        None
    }

    /// Extract variable name from a line (simple heuristic)
    fn extract_variable_name(&self, line: &str) -> Option<String> {
        // Very simple extraction - would need proper parsing in production
        if let Some(start) = line.find("let ") {
            let after_let = &line[start + 4..];
            if let Some(end) = after_let.find([' ', '=', ':']) {
                Some(after_let[..end].trim().to_string())
            } else {
                Some(after_let.trim().to_string())
            }
        } else if let Some(start) = line.find("const ") {
            let after_const = &line[start + 6..];
            if let Some(end) = after_const.find([' ', '=', ':']) {
                Some(after_const[..end].trim().to_string())
            } else {
                Some(after_const.trim().to_string())
            }
        } else {
            None
        }
    }

    /// Extract type hint from a line (simple heuristic)
    fn extract_type_hint(&self, line: &str) -> Option<String> {
        if let Some(colon_pos) = line.find(':') {
            let after_colon = &line[colon_pos + 1..];
            if let Some(end) = after_colon.find([' ', '=']) {
                Some(after_colon[..end].trim().to_string())
            } else {
                Some(after_colon.trim().to_string())
            }
        } else {
            None
        }
    }

    /// Get variable tracker
    pub fn tracker(&self) -> &VariableTracker {
        &self.tracker
    }

    /// Get variable tracker mutably
    pub fn tracker_mut(&mut self) -> &mut VariableTracker {
        &mut self.tracker
    }

    /// Add a data transformation
    pub fn add_transformation(&mut self, transformation: DataTransformation) {
        self.transformations.push(transformation);
    }

    /// Get all transformations
    pub fn transformations(&self) -> &[DataTransformation] {
        &self.transformations
    }

    /// Get type propagator
    pub fn type_propagator(&self) -> &TypePropagator {
        &self.type_propagator
    }

    /// Get type propagator mutably
    pub fn type_propagator_mut(&mut self) -> &mut TypePropagator {
        &mut self.type_propagator
    }

    /// Build enhanced data flow graph with variable tracking
    pub fn build_enhanced_data_flow_graph(
        &self,
        base_graph: &CorrelationGraph,
    ) -> Result<CorrelationGraph> {
        let mut graph = base_graph.clone();

        // Add variable nodes
        for variable in self.tracker.all_variables() {
            let node_id = format!("var:{}:{}:{}", variable.file, variable.name, variable.line);

            // Check if node already exists
            if graph.nodes.iter().any(|n| n.id == node_id) {
                continue;
            }

            let mut metadata = HashMap::new();
            metadata.insert(
                "variable_name".to_string(),
                serde_json::Value::String(variable.name.clone()),
            );
            metadata.insert(
                "file".to_string(),
                serde_json::Value::String(variable.file.clone()),
            );
            metadata.insert(
                "line".to_string(),
                serde_json::Value::Number(variable.line.into()),
            );
            if let Some(ref var_type) = variable.var_type {
                metadata.insert(
                    "type".to_string(),
                    serde_json::Value::String(var_type.clone()),
                );
            }
            metadata.insert(
                "is_mutable".to_string(),
                serde_json::Value::Bool(variable.is_mutable),
            );
            metadata.insert(
                "is_parameter".to_string(),
                serde_json::Value::Bool(variable.is_parameter),
            );
            metadata.insert(
                "usage_count".to_string(),
                serde_json::Value::Number(variable.usages.len().into()),
            );

            let node = GraphNode {
                id: node_id.clone(),
                node_type: NodeType::Variable,
                label: format!("{}:{}", variable.name, variable.file),
                metadata,
                position: None,
                size: Some(8.0),
                color: Some(if variable.is_mutable {
                    "#e74c3c".to_string()
                } else {
                    "#3498db".to_string()
                }),
            };

            graph.add_node(node)?;
        }

        // Add transformation edges (Task 11.3)
        for transformation in &self.transformations {
            let source_node_id = format!("trans:{}", transformation.source);
            let target_node_id = format!("trans:{}", transformation.target);

            // Ensure transformation nodes exist
            if !graph.nodes.iter().any(|n| n.id == source_node_id) {
                let mut metadata = HashMap::new();
                metadata.insert(
                    "transformation_source".to_string(),
                    serde_json::Value::String(transformation.source.clone()),
                );
                metadata.insert(
                    "transformation_type".to_string(),
                    serde_json::Value::String(format!("{:?}", transformation.transformation_type)),
                );

                let node = GraphNode {
                    id: source_node_id.clone(),
                    node_type: NodeType::Function,
                    label: format!("Source: {}", transformation.source),
                    metadata,
                    position: None,
                    size: Some(8.0),
                    color: Some("#9b59b6".to_string()),
                };
                let _ = graph.add_node(node);
            }

            if !graph.nodes.iter().any(|n| n.id == target_node_id) {
                let mut metadata = HashMap::new();
                metadata.insert(
                    "transformation_target".to_string(),
                    serde_json::Value::String(transformation.target.clone()),
                );
                metadata.insert(
                    "transformation_type".to_string(),
                    serde_json::Value::String(format!("{:?}", transformation.transformation_type)),
                );

                let node = GraphNode {
                    id: target_node_id.clone(),
                    node_type: NodeType::Function,
                    label: format!("Target: {}", transformation.target),
                    metadata,
                    position: None,
                    size: Some(8.0),
                    color: Some("#e67e22".to_string()),
                };
                let _ = graph.add_node(node);
            }

            // Add transformation edge
            let edge_id = format!("trans:{}:{}", transformation.source, transformation.target);
            let mut metadata = HashMap::new();
            metadata.insert(
                "transformation_type".to_string(),
                serde_json::Value::String(format!("{:?}", transformation.transformation_type)),
            );
            if !transformation.input_types.is_empty() {
                metadata.insert(
                    "input_types".to_string(),
                    serde_json::Value::Array(
                        transformation
                            .input_types
                            .iter()
                            .map(|t| serde_json::Value::String(t.clone()))
                            .collect(),
                    ),
                );
            }
            if !transformation.output_types.is_empty() {
                metadata.insert(
                    "output_types".to_string(),
                    serde_json::Value::Array(
                        transformation
                            .output_types
                            .iter()
                            .map(|t| serde_json::Value::String(t.clone()))
                            .collect(),
                    ),
                );
            }

            let edge = GraphEdge {
                id: edge_id,
                source: source_node_id.clone(),
                target: target_node_id.clone(),
                edge_type: EdgeType::Transforms,
                weight: 2.0, // Transformations have higher weight
                metadata,
                label: Some(format!("{:?}", transformation.transformation_type)),
            };
            let _ = graph.add_edge(edge);
        }

        // Add data flow edges
        for flow_edge in self.tracker.build_data_flow_edges() {
            // Ensure source and target nodes exist
            let source_exists = graph.nodes.iter().any(|n| n.id == flow_edge.source);
            let target_exists = graph.nodes.iter().any(|n| n.id == flow_edge.target);

            if source_exists && target_exists {
                let edge_id = format!("flow:{}:{}", flow_edge.source, flow_edge.target);

                let mut metadata = HashMap::new();
                metadata.insert(
                    "variable".to_string(),
                    serde_json::Value::String(flow_edge.variable.clone()),
                );
                metadata.insert(
                    "flow_type".to_string(),
                    serde_json::Value::String(format!("{:?}", flow_edge.flow_type)),
                );

                let edge = GraphEdge {
                    id: edge_id,
                    source: flow_edge.source,
                    target: flow_edge.target,
                    edge_type: EdgeType::Transforms, // Data flow transformation
                    weight: 1.0,
                    metadata,
                    label: Some(flow_edge.variable),
                };

                graph.add_edge(edge)?;
            }
        }

        Ok(graph)
    }
}

impl Default for DataFlowAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}
