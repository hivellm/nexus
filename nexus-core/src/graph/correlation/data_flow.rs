//! Data Flow Analysis Module
//!
//! Provides advanced data flow analysis capabilities:
//! - Variable usage tracking
//! - Data transformation analysis
//! - Data type propagation
//! - Flow-based layout algorithms
//! - Flow optimization suggestions
//! - Data flow statistics

use crate::graph::correlation::visualization::VisualizationConfig;

use crate::Result;
use crate::graph::correlation::{CorrelationGraph, EdgeType, GraphEdge, GraphNode, NodeType};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Variable usage information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariableUsage {
    /// Variable name
    pub name: String,
    /// File where variable is defined
    pub file: String,
    /// Line number where variable is defined
    pub line: usize,
    /// Variable type (if known)
    pub var_type: Option<String>,
    /// Files/functions where variable is used
    pub usages: Vec<VariableUsageSite>,
    /// Whether variable is modified
    pub is_mutable: bool,
    /// Whether variable is passed as parameter
    pub is_parameter: bool,
}

/// Variable usage site
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariableUsageSite {
    /// File where variable is used
    pub file: String,
    /// Function where variable is used
    pub function: Option<String>,
    /// Line number
    pub line: usize,
    /// Usage type (read, write, read_write)
    pub usage_type: UsageType,
}

/// Variable usage type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UsageType {
    /// Variable is read
    Read,
    /// Variable is written
    Write,
    /// Variable is both read and written
    ReadWrite,
}

/// Variable usage tracker
pub struct VariableTracker {
    /// Map of variable name to usage information
    variables: HashMap<String, VariableUsage>,
    /// Map of file to variables defined in that file
    file_variables: HashMap<String, Vec<String>>,
}

impl VariableTracker {
    /// Create a new variable tracker
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            file_variables: HashMap::new(),
        }
    }

    /// Track a variable definition
    pub fn track_definition(
        &mut self,
        name: String,
        file: String,
        line: usize,
        var_type: Option<String>,
        is_mutable: bool,
        is_parameter: bool,
    ) {
        let usage = VariableUsage {
            name: name.clone(),
            file: file.clone(),
            line,
            var_type,
            usages: Vec::new(),
            is_mutable,
            is_parameter,
        };
        self.variables.insert(name.clone(), usage);
        self.file_variables.entry(file).or_default().push(name);
    }

    /// Track a variable usage
    pub fn track_usage(
        &mut self,
        name: &str,
        file: String,
        function: Option<String>,
        line: usize,
        usage_type: UsageType,
    ) {
        if let Some(var) = self.variables.get_mut(name) {
            var.usages.push(VariableUsageSite {
                file,
                function,
                line,
                usage_type,
            });
        }
    }

    /// Get variable usage information
    pub fn get_variable(&self, name: &str) -> Option<&VariableUsage> {
        self.variables.get(name)
    }

    /// Get all variables defined in a file
    pub fn get_file_variables(&self, file: &str) -> Vec<&VariableUsage> {
        self.file_variables
            .get(file)
            .map(|names| {
                names
                    .iter()
                    .filter_map(|name| self.variables.get(name))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all tracked variables
    pub fn all_variables(&self) -> Vec<&VariableUsage> {
        self.variables.values().collect()
    }

    /// Build data flow edges from variable usage
    pub fn build_data_flow_edges(&self) -> Vec<DataFlowEdge> {
        let mut edges = Vec::new();

        for variable in self.variables.values() {
            // Create edges from definition to each usage
            let def_node_id = format!("var:{}:{}:{}", variable.file, variable.name, variable.line);

            for usage in &variable.usages {
                let usage_node_id = if let Some(ref func) = usage.function {
                    format!("usage:{}:{}:{}", usage.file, func, usage.line)
                } else {
                    format!("usage:{}:{}", usage.file, usage.line)
                };

                edges.push(DataFlowEdge {
                    source: def_node_id.clone(),
                    target: usage_node_id,
                    variable: variable.name.clone(),
                    flow_type: match usage.usage_type {
                        UsageType::Read => FlowType::Read,
                        UsageType::Write => FlowType::Write,
                        UsageType::ReadWrite => FlowType::ReadWrite,
                    },
                });
            }
        }

        edges
    }
}

impl Default for VariableTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Data flow edge information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataFlowEdge {
    /// Source node ID
    pub source: String,
    /// Target node ID
    pub target: String,
    /// Variable name flowing through this edge
    pub variable: String,
    /// Flow type
    pub flow_type: FlowType,
}

/// Data flow type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlowType {
    /// Data is read
    Read,
    /// Data is written
    Write,
    /// Data is both read and written
    ReadWrite,
    /// Data is transformed
    Transform,
}

/// Data transformation information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataTransformation {
    /// Source variable/function
    pub source: String,
    /// Target variable/function
    pub target: String,
    /// Transformation type
    pub transformation_type: TransformationType,
    /// Input data types
    pub input_types: Vec<String>,
    /// Output data types
    pub output_types: Vec<String>,
}

/// Data type propagation analyzer
///
/// Tracks how data types flow through transformations and propagates
/// type information through the data flow graph.
pub struct TypePropagator {
    /// Map of variable/function to its inferred type
    type_map: HashMap<String, String>,
    /// Map of transformation to input/output types
    transformation_types: HashMap<String, (Vec<String>, Vec<String>)>,
}

impl TypePropagator {
    /// Create a new type propagator
    pub fn new() -> Self {
        Self {
            type_map: HashMap::new(),
            transformation_types: HashMap::new(),
        }
    }

    /// Infer type from variable definition
    pub fn infer_type_from_definition(
        &mut self,
        var_name: &str,
        definition: &str,
    ) -> Option<String> {
        // Extract type hints from definition
        if let Some(colon_pos) = definition.find(':') {
            let after_colon = &definition[colon_pos + 1..];
            if let Some(end) = after_colon.find([' ', '=']) {
                let type_hint = after_colon[..end].trim();
                if !type_hint.is_empty() {
                    self.type_map
                        .insert(var_name.to_string(), type_hint.to_string());
                    return Some(type_hint.to_string());
                }
            }
        }

        // Infer from patterns
        if definition.contains("String::from") || definition.contains(".to_string()") {
            self.type_map
                .insert(var_name.to_string(), "String".to_string());
            return Some("String".to_string());
        }
        if definition.contains("Vec::new")
            || definition.contains("vec!")
            || definition.contains(".to_vec()")
        {
            self.type_map
                .insert(var_name.to_string(), "Vec<T>".to_string());
            return Some("Vec<T>".to_string());
        }
        if definition.contains("HashMap::new") || definition.contains("BTreeMap::new") {
            self.type_map
                .insert(var_name.to_string(), "HashMap<K, V>".to_string());
            return Some("HashMap<K, V>".to_string());
        }
        if definition.matches(|c: char| c.is_ascii_digit()).count() > 0 && !definition.contains(".")
        {
            self.type_map
                .insert(var_name.to_string(), "i32".to_string());
            return Some("i32".to_string());
        }
        if definition.contains("true") || definition.contains("false") {
            self.type_map
                .insert(var_name.to_string(), "bool".to_string());
            return Some("bool".to_string());
        }

        None
    }

    /// Propagate types through a transformation
    pub fn propagate_through_transformation(
        &mut self,
        transformation: &DataTransformation,
    ) -> Vec<String> {
        // Determine output types based on transformation type
        let output_types = match transformation.transformation_type {
            TransformationType::Assignment => {
                // Output type same as input type
                transformation.input_types.clone()
            }
            TransformationType::TypeConversion => {
                // Infer from conversion pattern
                self.infer_output_type_from_conversion(&transformation.target)
            }
            TransformationType::FunctionCall => {
                // Infer from function name patterns
                self.infer_output_type_from_function(&transformation.source)
            }
            TransformationType::Aggregation => {
                // Aggregations typically return numeric types
                vec!["i64".to_string(), "f64".to_string()]
            }
            TransformationType::Filter => {
                // Filter returns same type as input (collection)
                transformation.input_types.clone()
            }
            TransformationType::Map => {
                // Map transforms element type, keep collection type
                if let Some(input_type) = transformation.input_types.first() {
                    if input_type.starts_with("Vec<") {
                        vec![input_type.clone()]
                    } else {
                        vec![format!("Vec<{}>", input_type)]
                    }
                } else {
                    vec!["Vec<T>".to_string()]
                }
            }
            TransformationType::Reduce => {
                // Reduce returns single value (often same as element type)
                if let Some(input_type) = transformation.input_types.first() {
                    if input_type.starts_with("Vec<") {
                        // Extract element type
                        let element_type = input_type
                            .strip_prefix("Vec<")
                            .and_then(|s| s.strip_suffix(">"))
                            .unwrap_or("T");
                        vec![element_type.to_string()]
                    } else {
                        vec![input_type.clone()]
                    }
                } else {
                    vec!["T".to_string()]
                }
            }
        };

        // Store transformation types
        let key = format!("{}:{}", transformation.source, transformation.target);
        self.transformation_types.insert(
            key,
            (transformation.input_types.clone(), output_types.clone()),
        );

        // Update type map for target
        if !output_types.is_empty() {
            self.type_map
                .insert(transformation.target.clone(), output_types[0].clone());
        }

        output_types
    }

    /// Infer output type from conversion pattern
    fn infer_output_type_from_conversion(&self, target: &str) -> Vec<String> {
        if target.contains("to_string") || target.contains("String::from") {
            vec!["String".to_string()]
        } else if target.contains("to_vec") || target.contains("Vec::from") {
            vec!["Vec<T>".to_string()]
        } else if target.contains("parse") {
            vec!["i32".to_string(), "f64".to_string()]
        } else {
            vec!["T".to_string()]
        }
    }

    /// Infer output type from function name patterns
    fn infer_output_type_from_function(&self, source: &str) -> Vec<String> {
        let func_name = source.split(':').next().unwrap_or(source);

        if func_name.contains("sum") || func_name.contains("total") || func_name.contains("count") {
            vec!["i64".to_string(), "f64".to_string()]
        } else if func_name.contains("string") || func_name.contains("str") {
            vec!["String".to_string()]
        } else if func_name.contains("vec")
            || func_name.contains("array")
            || func_name.contains("list")
        {
            vec!["Vec<T>".to_string()]
        } else if func_name.contains("bool")
            || func_name.contains("is_")
            || func_name.contains("has_")
        {
            vec!["bool".to_string()]
        } else {
            vec!["T".to_string()]
        }
    }

    /// Get inferred type for a variable
    pub fn get_type(&self, var_name: &str) -> Option<&String> {
        self.type_map.get(var_name)
    }

    /// Get all known types
    pub fn all_types(&self) -> &HashMap<String, String> {
        &self.type_map
    }

    /// Get transformation types
    pub fn get_transformation_types(
        &self,
        source: &str,
        target: &str,
    ) -> Option<&(Vec<String>, Vec<String>)> {
        let key = format!("{}:{}", source, target);
        self.transformation_types.get(&key)
    }

    /// Analyze and propagate types through transformations
    pub fn analyze_and_propagate(&mut self, transformations: &mut [DataTransformation]) {
        // First pass: infer types from definitions
        for transformation in transformations.iter_mut() {
            // Try to infer input types from source
            if transformation.input_types.is_empty() {
                if let Some(source_type) = self.type_map.get(&transformation.source) {
                    transformation.input_types.push(source_type.clone());
                }
            }
        }

        // Second pass: propagate through transformations and update output types
        for transformation in transformations.iter_mut() {
            let output_types = self.propagate_through_transformation(transformation);
            if transformation.output_types.is_empty() {
                transformation.output_types = output_types;
            }
        }
    }
}

impl Default for TypePropagator {
    fn default() -> Self {
        Self::new()
    }
}

/// Data transformation type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransformationType {
    /// Direct assignment
    Assignment,
    /// Function call transformation
    FunctionCall,
    /// Type conversion
    TypeConversion,
    /// Aggregation (sum, count, etc.)
    Aggregation,
    /// Filter operation
    Filter,
    /// Map operation
    Map,
    /// Reduce operation
    Reduce,
}

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

            // Simple pattern matching for variable definitions
            // This is a placeholder - full implementation would use proper parsing
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
    fn detect_assignment(&self, line: &str) -> Option<DataTransformation> {
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
    fn detect_function_call_transformation(&self, line: &str) -> Option<DataTransformation> {
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
    fn detect_type_conversion(&self, line: &str) -> Option<DataTransformation> {
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
    fn detect_aggregation(&self, line: &str) -> Option<DataTransformation> {
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
    fn detect_filter_operation(&self, line: &str) -> Option<DataTransformation> {
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
    fn detect_map_operation(&self, line: &str) -> Option<DataTransformation> {
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
    fn detect_reduce_operation(&self, line: &str) -> Option<DataTransformation> {
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

// ============================================================================
// Flow Optimization Suggestions (Task 11.7)
// ============================================================================

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
    fn calculate_chain_length(node_id: &str, outgoing: &HashMap<String, Vec<String>>) -> usize {
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

// ============================================================================
// Data Flow Statistics (Task 11.8)
// ============================================================================

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_variable_tracker() {
        let mut tracker = VariableTracker::new();

        tracker.track_definition(
            "x".to_string(),
            "test.rs".to_string(),
            1,
            Some("i32".to_string()),
            true,
            false,
        );

        tracker.track_usage(
            "x",
            "test.rs".to_string(),
            Some("main".to_string()),
            5,
            UsageType::Read,
        );

        let var = tracker.get_variable("x").unwrap();
        assert_eq!(var.name, "x");
        assert_eq!(var.usages.len(), 1);
    }

    #[test]
    fn test_data_flow_analyzer() {
        let mut analyzer = DataFlowAnalyzer::new();
        let mut files = HashMap::new();
        files.insert(
            "test.rs".to_string(),
            "let x = 5;\nlet _y = x + 1;".to_string(),
        );

        let result = analyzer.analyze_source_code(&files);
        assert!(result.is_ok());

        // Should have tracked at least some variables
        assert!(!analyzer.tracker().all_variables().is_empty());
    }

    #[test]
    fn test_detect_assignment_transformation() {
        let mut analyzer = DataFlowAnalyzer::new();

        // Track a variable first
        analyzer.tracker_mut().track_definition(
            "x".to_string(),
            "test.rs".to_string(),
            1,
            Some("i32".to_string()),
            true,
            false,
        );

        // Detect assignment transformation
        let transformation = analyzer.detect_assignment("y = x + 1");
        assert!(transformation.is_some());
        let trans = transformation.unwrap();
        assert_eq!(trans.transformation_type, TransformationType::Assignment);
        assert_eq!(trans.target.trim(), "y");
    }

    #[test]
    fn test_detect_function_call_transformation() {
        let mut analyzer = DataFlowAnalyzer::new();

        analyzer.tracker_mut().track_definition(
            "data".to_string(),
            "test.rs".to_string(),
            1,
            None,
            true,
            false,
        );

        let transformation =
            analyzer.detect_function_call_transformation("let result = process(data)");
        assert!(transformation.is_some());
        let trans = transformation.unwrap();
        assert_eq!(trans.transformation_type, TransformationType::FunctionCall);
        assert!(trans.source.contains("process"));
    }

    #[test]
    fn test_detect_type_conversion() {
        let mut analyzer = DataFlowAnalyzer::new();

        analyzer.tracker_mut().track_definition(
            "num".to_string(),
            "test.rs".to_string(),
            1,
            Some("i32".to_string()),
            true,
            false,
        );

        let transformation = analyzer.detect_type_conversion("let str = num.to_string()");
        assert!(transformation.is_some());
        let trans = transformation.unwrap();
        assert_eq!(
            trans.transformation_type,
            TransformationType::TypeConversion
        );
    }

    #[test]
    fn test_detect_aggregation() {
        let mut analyzer = DataFlowAnalyzer::new();

        analyzer.tracker_mut().track_definition(
            "numbers".to_string(),
            "test.rs".to_string(),
            1,
            None,
            true,
            false,
        );

        let transformation = analyzer.detect_aggregation("let total = numbers.sum()");
        assert!(transformation.is_some());
        let trans = transformation.unwrap();
        assert_eq!(trans.transformation_type, TransformationType::Aggregation);
    }

    #[test]
    fn test_detect_filter_operation() {
        let mut analyzer = DataFlowAnalyzer::new();

        analyzer.tracker_mut().track_definition(
            "items".to_string(),
            "test.rs".to_string(),
            1,
            None,
            true,
            false,
        );

        let transformation =
            analyzer.detect_filter_operation("let filtered = items.filter(|x| x > 0)");
        assert!(transformation.is_some());
        let trans = transformation.unwrap();
        assert_eq!(trans.transformation_type, TransformationType::Filter);
    }

    #[test]
    fn test_detect_map_operation() {
        let mut analyzer = DataFlowAnalyzer::new();

        analyzer.tracker_mut().track_definition(
            "values".to_string(),
            "test.rs".to_string(),
            1,
            None,
            true,
            false,
        );

        let transformation = analyzer.detect_map_operation("let doubled = values.map(|x| x * 2)");
        assert!(transformation.is_some());
        let trans = transformation.unwrap();
        assert_eq!(trans.transformation_type, TransformationType::Map);
    }

    #[test]
    fn test_detect_reduce_operation() {
        let mut analyzer = DataFlowAnalyzer::new();

        analyzer.tracker_mut().track_definition(
            "numbers".to_string(),
            "test.rs".to_string(),
            1,
            None,
            true,
            false,
        );

        let transformation =
            analyzer.detect_reduce_operation("let sum = numbers.reduce(|a, b| a + b)");
        assert!(transformation.is_some());
        let trans = transformation.unwrap();
        assert_eq!(trans.transformation_type, TransformationType::Reduce);
    }

    #[test]
    fn test_transformation_integration() {
        let mut analyzer = DataFlowAnalyzer::new();
        let mut files = HashMap::new();
        files.insert(
            "test.rs".to_string(),
            "let x = 5;\nlet y = x + 1;\nlet z = y.to_string();\nlet sum = [1, 2, 3].sum();"
                .to_string(),
        );

        let result = analyzer.analyze_source_code(&files);
        assert!(result.is_ok());

        // Should have detected transformations
        assert!(!analyzer.transformations().is_empty());
    }

    #[test]
    fn test_type_propagator_inference() {
        let mut propagator = TypePropagator::new();

        // Test type inference from definition
        let inferred = propagator.infer_type_from_definition("x", "let x: i32 = 5");
        assert_eq!(inferred, Some("i32".to_string()));

        let inferred =
            propagator.infer_type_from_definition("str", "let str = String::from(\"hello\")");
        assert_eq!(inferred, Some("String".to_string()));

        let inferred = propagator.infer_type_from_definition("vec", "let vec = Vec::new()");
        assert_eq!(inferred, Some("Vec<T>".to_string()));

        let inferred = propagator.infer_type_from_definition("flag", "let flag = true");
        assert_eq!(inferred, Some("bool".to_string()));
    }

    #[test]
    fn test_type_propagation_through_transformation() {
        let mut propagator = TypePropagator::new();
        propagator
            .type_map
            .insert("x".to_string(), "i32".to_string());

        let transformation = DataTransformation {
            source: "x".to_string(),
            target: "y".to_string(),
            transformation_type: TransformationType::Assignment,
            input_types: vec!["i32".to_string()],
            output_types: vec![],
        };

        let output_types = propagator.propagate_through_transformation(&transformation);
        assert_eq!(output_types, vec!["i32".to_string()]);

        // Test type conversion - use a target that contains conversion pattern
        let conversion = DataTransformation {
            source: "num".to_string(),
            target: "str = num.to_string()".to_string(),
            transformation_type: TransformationType::TypeConversion,
            input_types: vec!["i32".to_string()],
            output_types: vec![],
        };

        let output_types = propagator.propagate_through_transformation(&conversion);
        assert_eq!(output_types, vec!["String".to_string()]);
    }

    #[test]
    fn test_type_propagation_aggregation() {
        let mut propagator = TypePropagator::new();

        let transformation = DataTransformation {
            source: "numbers".to_string(),
            target: "sum".to_string(),
            transformation_type: TransformationType::Aggregation,
            input_types: vec!["Vec<i32>".to_string()],
            output_types: vec![],
        };

        let output_types = propagator.propagate_through_transformation(&transformation);
        assert!(
            output_types.contains(&"i64".to_string()) || output_types.contains(&"f64".to_string())
        );
    }

    #[test]
    fn test_type_propagation_map() {
        let mut propagator = TypePropagator::new();

        let transformation = DataTransformation {
            source: "numbers".to_string(),
            target: "doubled".to_string(),
            transformation_type: TransformationType::Map,
            input_types: vec!["Vec<i32>".to_string()],
            output_types: vec![],
        };

        let output_types = propagator.propagate_through_transformation(&transformation);
        assert_eq!(output_types, vec!["Vec<i32>".to_string()]);
    }

    #[test]
    fn test_type_propagation_reduce() {
        let mut propagator = TypePropagator::new();

        let transformation = DataTransformation {
            source: "numbers".to_string(),
            target: "sum".to_string(),
            transformation_type: TransformationType::Reduce,
            input_types: vec!["Vec<i32>".to_string()],
            output_types: vec![],
        };

        let output_types = propagator.propagate_through_transformation(&transformation);
        assert_eq!(output_types, vec!["i32".to_string()]);
    }
}

/// Flow-based layout algorithm for data flow graphs
///
/// Organizes nodes in layers based on data flow:
/// - Input nodes (sources) on the left
/// - Transformation nodes in the middle layers
/// - Output nodes (sinks) on the right
pub struct FlowBasedLayout;

impl FlowBasedLayout {
    /// Apply flow-based layout to a data flow graph
    pub fn apply_layout(
        graph: &mut crate::graph::correlation::CorrelationGraph,
        config: &VisualizationConfig,
    ) -> Result<()> {
        use std::collections::{HashMap, HashSet};

        if graph.nodes.is_empty() {
            return Ok(());
        }

        // Build adjacency lists (forward and backward)
        let mut outgoing: HashMap<String, Vec<String>> = HashMap::new();
        let mut incoming: HashMap<String, Vec<String>> = HashMap::new();
        let mut node_ids: HashSet<String> = HashSet::new();

        for node in &graph.nodes {
            node_ids.insert(node.id.clone());
            outgoing.insert(node.id.clone(), Vec::new());
            incoming.insert(node.id.clone(), Vec::new());
        }

        for edge in &graph.edges {
            outgoing
                .entry(edge.source.clone())
                .or_default()
                .push(edge.target.clone());
            incoming
                .entry(edge.target.clone())
                .or_default()
                .push(edge.source.clone());
        }

        // Calculate layers using topological sort (BFS-based)
        let mut layers: Vec<Vec<String>> = Vec::new();
        let mut assigned = HashSet::new();
        let mut in_degree: HashMap<String, usize> = HashMap::new();

        // Initialize in-degrees
        for node_id in &node_ids {
            in_degree.insert(
                node_id.clone(),
                incoming.get(node_id).map(|v| v.len()).unwrap_or(0),
            );
        }

        // Find source nodes (nodes with no incoming edges)
        let mut current_layer: Vec<String> = node_ids
            .iter()
            .filter(|id| in_degree.get(*id).copied().unwrap_or(0) == 0)
            .cloned()
            .collect();

        // If no source nodes, start with all nodes (for disconnected components)
        if current_layer.is_empty() {
            current_layer = node_ids.iter().cloned().collect();
        }

        // Build layers using topological ordering
        while !current_layer.is_empty() {
            layers.push(current_layer.clone());

            for node_id in &current_layer {
                assigned.insert(node_id.clone());
            }

            // Find next layer (nodes whose dependencies are all assigned)
            let mut next_layer = Vec::new();
            for node_id in &node_ids {
                if assigned.contains(node_id) {
                    continue;
                }

                let deps_assigned = incoming
                    .get(node_id)
                    .map(|deps| deps.iter().all(|dep| assigned.contains(dep)))
                    .unwrap_or(true);

                if deps_assigned {
                    next_layer.push(node_id.clone());
                }
            }

            current_layer = next_layer;
        }

        // Handle any remaining unassigned nodes (disconnected components)
        for node_id in &node_ids {
            if !assigned.contains(node_id) {
                if layers.is_empty() {
                    layers.push(vec![node_id.clone()]);
                } else {
                    layers.last_mut().unwrap().push(node_id.clone());
                }
            }
        }

        // Calculate positions for each layer
        let layer_count = layers.len();
        let layer_width = if layer_count > 1 {
            (config.width - 2.0 * config.padding) / (layer_count - 1) as f32
        } else {
            0.0
        };

        for (layer_idx, layer_nodes) in layers.iter().enumerate() {
            let x = if layer_count > 1 {
                config.padding + layer_idx as f32 * layer_width
            } else {
                config.width / 2.0
            };

            let node_count = layer_nodes.len();
            let node_spacing = if node_count > 1 {
                (config.height - 2.0 * config.padding) / (node_count - 1) as f32
            } else {
                0.0
            };

            for (node_idx, node_id) in layer_nodes.iter().enumerate() {
                let y = if node_count > 1 {
                    config.padding + node_idx as f32 * node_spacing
                } else {
                    config.height / 2.0
                };

                // Update node position
                if let Some(node) = graph.nodes.iter_mut().find(|n| n.id == *node_id) {
                    node.position = Some((x, y));
                }
            }
        }

        Ok(())
    }
}

/// Apply flow-based layout to a data flow graph
pub fn apply_flow_layout(
    graph: &mut crate::graph::correlation::CorrelationGraph,
    config: &VisualizationConfig,
) -> Result<()> {
    FlowBasedLayout::apply_layout(graph, config)
}

/// Data flow visualization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataFlowVisualizationConfig {
    /// Color for variable nodes
    pub variable_color: String,
    /// Color for transformation nodes
    pub transformation_color: String,
    /// Color for input/source nodes
    pub source_color: String,
    /// Color for output/sink nodes
    pub sink_color: String,
    /// Show type information on nodes
    pub show_types: bool,
    /// Show variable names on edges
    pub show_edge_labels: bool,
    /// Highlight critical paths
    pub highlight_critical_paths: bool,
    /// Color map for different data types
    pub type_colors: HashMap<String, String>,
}

impl Default for DataFlowVisualizationConfig {
    fn default() -> Self {
        let mut type_colors = HashMap::new();
        type_colors.insert("String".to_string(), "#e74c3c".to_string());
        type_colors.insert("i32".to_string(), "#3498db".to_string());
        type_colors.insert("i64".to_string(), "#3498db".to_string());
        type_colors.insert("f64".to_string(), "#9b59b6".to_string());
        type_colors.insert("bool".to_string(), "#f39c12".to_string());
        type_colors.insert("Vec<T>".to_string(), "#1abc9c".to_string());
        type_colors.insert("HashMap<K, V>".to_string(), "#e67e22".to_string());

        Self {
            variable_color: "#3498db".to_string(),
            transformation_color: "#9b59b6".to_string(),
            source_color: "#2ecc71".to_string(),
            sink_color: "#e74c3c".to_string(),
            show_types: true,
            show_edge_labels: true,
            highlight_critical_paths: true,
            type_colors,
        }
    }
}

/// Apply data flow visualization styling to a graph
pub fn apply_data_flow_visualization(
    graph: &mut crate::graph::correlation::CorrelationGraph,
    config: &DataFlowVisualizationConfig,
    analyzer: &DataFlowAnalyzer,
) -> Result<()> {
    // EdgeType, GraphEdge, GraphNode, NodeType are used via CorrelationGraph

    // Build node type map from analyzer
    let type_map = analyzer.type_propagator().all_types();

    // Identify source and sink nodes
    let mut incoming_count: HashMap<String, usize> = HashMap::new();
    let mut outgoing_count: HashMap<String, usize> = HashMap::new();

    for edge in &graph.edges {
        *incoming_count.entry(edge.target.clone()).or_insert(0) += 1;
        *outgoing_count.entry(edge.source.clone()).or_insert(0) += 1;
    }

    // Apply styling to nodes
    for node in &mut graph.nodes {
        // Determine if node is source or sink
        let is_source = incoming_count.get(&node.id).copied().unwrap_or(0) == 0;
        let is_sink = outgoing_count.get(&node.id).copied().unwrap_or(0) == 0;

        // Set color based on node type and role
        if is_source {
            node.color = Some(config.source_color.clone());
        } else if is_sink {
            node.color = Some(config.sink_color.clone());
        } else if node.id.starts_with("trans:") {
            node.color = Some(config.transformation_color.clone());
        } else if node.id.starts_with("var:") {
            // Use type-based color if available
            if let Some(var_name) = node.metadata.get("variable_name").and_then(|v| v.as_str()) {
                if let Some(type_name) = type_map.get(var_name) {
                    if let Some(type_color) = config.type_colors.get(type_name) {
                        node.color = Some(type_color.clone());
                    } else {
                        node.color = Some(config.variable_color.clone());
                    }
                } else {
                    node.color = Some(config.variable_color.clone());
                }
            } else {
                node.color = Some(config.variable_color.clone());
            }
        } else {
            node.color = Some(config.variable_color.clone());
        }

        // Add type information to label if enabled
        if config.show_types {
            if let Some(var_name) = node.metadata.get("variable_name").and_then(|v| v.as_str()) {
                if let Some(type_name) = type_map.get(var_name) {
                    node.label = format!("{}: {}", node.label, type_name);
                }
            }
        }

        // Set node size based on importance
        if is_source || is_sink {
            node.size = Some(12.0); // Larger for sources/sinks
        } else {
            node.size = Some(8.0);
        }
    }

    // Apply styling to edges
    for edge in &mut graph.edges {
        // Add variable name to edge label if enabled
        if config.show_edge_labels {
            if let Some(var_name) = edge.metadata.get("variable").and_then(|v| v.as_str()) {
                edge.label = Some(var_name.to_string());
            }
        }

        // Set edge width based on flow type
        if edge.edge_type == EdgeType::Transforms {
            edge.weight = 2.0; // Thicker for transformations
        }
    }

    Ok(())
}

/// Generate data flow visualization with enhanced styling
pub fn visualize_data_flow(
    graph: &mut crate::graph::correlation::CorrelationGraph,
    visualization_config: &VisualizationConfig,
    data_flow_config: &DataFlowVisualizationConfig,
    analyzer: &DataFlowAnalyzer,
) -> Result<String> {
    use crate::graph::correlation::visualization::render_graph_to_svg;

    // Apply flow-based layout
    apply_flow_layout(graph, visualization_config)?;

    // Apply data flow visualization styling
    apply_data_flow_visualization(graph, data_flow_config, analyzer)?;

    // Render to SVG
    render_graph_to_svg(graph, visualization_config)
}

#[cfg(test)]
mod layout_tests {
    use super::*;
    use crate::graph::correlation::{
        CorrelationGraph, EdgeType, GraphEdge, GraphNode, GraphType, NodeType,
    };
    use std::collections::HashMap;

    #[test]
    fn test_flow_layout_simple_chain() {
        let mut graph = CorrelationGraph::new(GraphType::DataFlow, "Test Flow".to_string());

        // Create a simple chain: A -> B -> C
        let node_a = GraphNode {
            id: "A".to_string(),
            node_type: NodeType::Variable,
            label: "A".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };
        let node_b = GraphNode {
            id: "B".to_string(),
            node_type: NodeType::Function,
            label: "B".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };
        let node_c = GraphNode {
            id: "C".to_string(),
            node_type: NodeType::Variable,
            label: "C".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };

        graph.add_node(node_a).unwrap();
        graph.add_node(node_b).unwrap();
        graph.add_node(node_c).unwrap();

        graph
            .add_edge(GraphEdge {
                id: "e1".to_string(),
                source: "A".to_string(),
                target: "B".to_string(),
                edge_type: EdgeType::Transforms,
                weight: 1.0,
                metadata: HashMap::new(),
                label: None,
            })
            .unwrap();

        graph
            .add_edge(GraphEdge {
                id: "e2".to_string(),
                source: "B".to_string(),
                target: "C".to_string(),
                edge_type: EdgeType::Transforms,
                weight: 1.0,
                metadata: HashMap::new(),
                label: None,
            })
            .unwrap();

        let config = VisualizationConfig {
            width: 1000.0,
            height: 800.0,
            ..Default::default()
        };

        let result = FlowBasedLayout::apply_layout(&mut graph, &config);
        assert!(result.is_ok());

        // Verify all nodes have positions
        for node in &graph.nodes {
            assert!(
                node.position.is_some(),
                "Node {} should have a position",
                node.id
            );
        }

        // Verify nodes are in layers (A should be leftmost, C rightmost)
        let pos_a = graph
            .nodes
            .iter()
            .find(|n| n.id == "A")
            .unwrap()
            .position
            .unwrap();
        let pos_b = graph
            .nodes
            .iter()
            .find(|n| n.id == "B")
            .unwrap()
            .position
            .unwrap();
        let pos_c = graph
            .nodes
            .iter()
            .find(|n| n.id == "C")
            .unwrap()
            .position
            .unwrap();

        assert!(pos_a.0 < pos_b.0, "A should be to the left of B");
        assert!(pos_b.0 < pos_c.0, "B should be to the left of C");
    }

    #[test]
    fn test_flow_layout_empty_graph() {
        let mut graph = CorrelationGraph::new(GraphType::DataFlow, "Empty".to_string());
        let config = VisualizationConfig::default();

        let result = FlowBasedLayout::apply_layout(&mut graph, &config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_flow_layout_single_node() {
        let mut graph = CorrelationGraph::new(GraphType::DataFlow, "Single".to_string());

        let node = GraphNode {
            id: "single".to_string(),
            node_type: NodeType::Variable,
            label: "Single".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };
        graph.add_node(node).unwrap();

        let config = VisualizationConfig {
            width: 1000.0,
            height: 800.0,
            ..Default::default()
        };

        let result = FlowBasedLayout::apply_layout(&mut graph, &config);
        assert!(result.is_ok());

        let pos = graph.nodes[0].position.unwrap();
        // Single node should be centered
        assert!((pos.0 - config.width / 2.0).abs() < 1.0);
        assert!((pos.1 - config.height / 2.0).abs() < 1.0);
    }

    #[test]
    fn test_flow_layout_disconnected_components() {
        let mut graph = CorrelationGraph::new(GraphType::DataFlow, "Disconnected".to_string());

        // Create two disconnected components
        let node_a = GraphNode {
            id: "A".to_string(),
            node_type: NodeType::Variable,
            label: "A".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };
        let node_b = GraphNode {
            id: "B".to_string(),
            node_type: NodeType::Variable,
            label: "B".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };

        graph.add_node(node_a).unwrap();
        graph.add_node(node_b).unwrap();

        let config = VisualizationConfig {
            width: 1000.0,
            height: 800.0,
            ..Default::default()
        };

        let result = FlowBasedLayout::apply_layout(&mut graph, &config);
        assert!(result.is_ok());

        // Both nodes should have positions
        for node in &graph.nodes {
            assert!(node.position.is_some());
        }
    }
}

#[cfg(test)]
mod visualization_tests {
    use super::*;
    use crate::graph::correlation::{
        CorrelationGraph, EdgeType, GraphEdge, GraphNode, GraphType, NodeType,
    };
    use std::collections::HashMap;

    #[test]
    fn test_data_flow_visualization_config_default() {
        let config = DataFlowVisualizationConfig::default();
        assert_eq!(config.variable_color, "#3498db");
        assert_eq!(config.transformation_color, "#9b59b6");
        assert_eq!(config.source_color, "#2ecc71");
        assert_eq!(config.sink_color, "#e74c3c");
        assert!(config.show_types);
        assert!(config.show_edge_labels);
        assert!(!config.type_colors.is_empty());
    }

    #[test]
    fn test_apply_data_flow_visualization() {
        let mut graph = CorrelationGraph::new(GraphType::DataFlow, "Test".to_string());

        let node_a = GraphNode {
            id: "A".to_string(),
            node_type: NodeType::Variable,
            label: "A".to_string(),
            metadata: {
                let mut m = HashMap::new();
                m.insert(
                    "variable_name".to_string(),
                    serde_json::Value::String("x".to_string()),
                );
                m
            },
            position: None,
            size: None,
            color: None,
        };
        let node_b = GraphNode {
            id: "B".to_string(),
            node_type: NodeType::Function,
            label: "B".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };

        graph.add_node(node_a).unwrap();
        graph.add_node(node_b).unwrap();

        graph
            .add_edge(GraphEdge {
                id: "e1".to_string(),
                source: "A".to_string(),
                target: "B".to_string(),
                edge_type: EdgeType::Transforms,
                weight: 1.0,
                metadata: {
                    let mut m = HashMap::new();
                    m.insert(
                        "variable".to_string(),
                        serde_json::Value::String("x".to_string()),
                    );
                    m
                },
                label: None,
            })
            .unwrap();

        let mut analyzer = DataFlowAnalyzer::new();
        analyzer
            .type_propagator_mut()
            .type_map
            .insert("x".to_string(), "i32".to_string());

        let config = DataFlowVisualizationConfig::default();
        let result = apply_data_flow_visualization(&mut graph, &config, &analyzer);
        assert!(result.is_ok());

        // Verify nodes have colors
        let node_a = graph.nodes.iter().find(|n| n.id == "A").unwrap();
        assert!(node_a.color.is_some());

        // Verify source node (A) has source color
        assert_eq!(node_a.color.as_ref().unwrap(), &config.source_color);

        // Verify edge has label
        let edge = graph.edges.iter().find(|e| e.id == "e1").unwrap();
        assert!(edge.label.is_some());
    }

    #[test]
    fn test_visualize_data_flow() {
        let mut graph = CorrelationGraph::new(GraphType::DataFlow, "Test".to_string());

        let node = GraphNode {
            id: "var1".to_string(),
            node_type: NodeType::Variable,
            label: "x".to_string(),
            metadata: HashMap::new(),
            position: None,
            size: None,
            color: None,
        };
        graph.add_node(node).unwrap();

        use crate::graph::correlation::visualization::LayoutAlgorithm;
        let vis_config = VisualizationConfig {
            width: 800.0,
            height: 600.0,
            layout_algorithm: LayoutAlgorithm::FlowBased,
            ..Default::default()
        };
        let data_flow_config = DataFlowVisualizationConfig::default();
        let analyzer = DataFlowAnalyzer::new();

        let result = visualize_data_flow(&mut graph, &vis_config, &data_flow_config, &analyzer);
        assert!(result.is_ok());

        let svg = result.unwrap();
        assert!(svg.contains("<svg"));
    }
}

#[cfg(test)]
mod optimization_tests {
    use super::*;
    use crate::graph::correlation::{
        CorrelationGraph, EdgeType, GraphEdge, GraphNode, GraphType, NodeType,
    };
    use std::collections::HashMap;

    #[test]
    fn test_flow_optimization_analyzer_detect_unused_variables() {
        let mut analyzer = DataFlowAnalyzer::new();
        analyzer.tracker_mut().track_definition(
            "unused_var".to_string(),
            "test.rs".to_string(),
            1,
            Some("i32".to_string()),
            true,
            false,
        );
        analyzer.tracker_mut().track_definition(
            "used_var".to_string(),
            "test.rs".to_string(),
            2,
            Some("i32".to_string()),
            true,
            false,
        );
        analyzer.tracker_mut().track_usage(
            "used_var",
            "test.rs".to_string(),
            None,
            5,
            UsageType::Read,
        );

        let graph = CorrelationGraph::new(GraphType::DataFlow, "Test".to_string());
        let suggestions = FlowOptimizationAnalyzer::analyze(&graph, &analyzer);

        // Should detect unused variable
        assert!(!suggestions.is_empty());
        let unused_suggestion = suggestions
            .iter()
            .find(|s| s.category == "Unused Variables");
        assert!(unused_suggestion.is_some());
    }

    #[test]
    fn test_flow_optimization_analyzer_detect_redundant_conversions() {
        let mut analyzer = DataFlowAnalyzer::new();

        // Add multiple type conversions for same source
        analyzer.add_transformation(DataTransformation {
            source: "x".to_string(),
            target: "y".to_string(),
            transformation_type: TransformationType::TypeConversion,
            input_types: vec!["i32".to_string()],
            output_types: vec!["String".to_string()],
        });
        analyzer.add_transformation(DataTransformation {
            source: "x".to_string(),
            target: "z".to_string(),
            transformation_type: TransformationType::TypeConversion,
            input_types: vec!["i32".to_string()],
            output_types: vec!["String".to_string()],
        });
        analyzer.add_transformation(DataTransformation {
            source: "x".to_string(),
            target: "w".to_string(),
            transformation_type: TransformationType::TypeConversion,
            input_types: vec!["i32".to_string()],
            output_types: vec!["String".to_string()],
        });

        let graph = CorrelationGraph::new(GraphType::DataFlow, "Test".to_string());
        let suggestions = FlowOptimizationAnalyzer::analyze(&graph, &analyzer);

        // Should detect redundant conversions
        let redundant = suggestions
            .iter()
            .find(|s| s.category == "Redundant Conversions");
        assert!(redundant.is_some());
    }

    #[test]
    fn test_flow_optimization_analyzer_detect_long_chains() {
        let mut graph = CorrelationGraph::new(GraphType::DataFlow, "Test".to_string());

        // Create a long chain: A -> B -> C -> D -> E -> F -> G
        let nodes = vec!["A", "B", "C", "D", "E", "F", "G"];
        for node_id in &nodes {
            graph
                .add_node(GraphNode {
                    id: node_id.to_string(),
                    node_type: NodeType::Variable,
                    label: node_id.to_string(),
                    metadata: HashMap::new(),
                    position: None,
                    size: None,
                    color: None,
                })
                .unwrap();
        }

        for i in 0..nodes.len() - 1 {
            graph
                .add_edge(GraphEdge {
                    id: format!("e{}", i),
                    source: nodes[i].to_string(),
                    target: nodes[i + 1].to_string(),
                    edge_type: EdgeType::Transforms,
                    weight: 1.0,
                    metadata: HashMap::new(),
                    label: None,
                })
                .unwrap();
        }

        let analyzer = DataFlowAnalyzer::new();
        let suggestions = FlowOptimizationAnalyzer::analyze(&graph, &analyzer);

        // Should detect long chain
        let long_chain = suggestions
            .iter()
            .find(|s| s.category == "Long Transformation Chain");
        assert!(long_chain.is_some());
    }

    #[test]
    fn test_flow_optimization_analyzer_detect_parallelization() {
        let mut graph = CorrelationGraph::new(GraphType::DataFlow, "Test".to_string());

        // Create multiple independent chains (source nodes)
        for i in 0..5 {
            graph
                .add_node(GraphNode {
                    id: format!("source{}", i),
                    node_type: NodeType::Variable,
                    label: format!("Source{}", i),
                    metadata: HashMap::new(),
                    position: None,
                    size: None,
                    color: None,
                })
                .unwrap();
        }

        let analyzer = DataFlowAnalyzer::new();
        let suggestions = FlowOptimizationAnalyzer::analyze(&graph, &analyzer);

        // Should detect parallelization opportunity
        let parallel = suggestions.iter().find(|s| s.category == "Parallelization");
        assert!(parallel.is_some());
    }

    #[test]
    fn test_data_flow_statistics_calculation() {
        let mut analyzer = DataFlowAnalyzer::new();

        // Add some variables
        analyzer.tracker_mut().track_definition(
            "x".to_string(),
            "test.rs".to_string(),
            1,
            Some("i32".to_string()),
            true,
            false,
        );
        analyzer.tracker_mut().track_definition(
            "y".to_string(),
            "test.rs".to_string(),
            2,
            None,
            true,
            false,
        );
        analyzer
            .tracker_mut()
            .track_usage("x", "test.rs".to_string(), None, 5, UsageType::Read);
        analyzer
            .tracker_mut()
            .track_usage("x", "test.rs".to_string(), None, 6, UsageType::Read);

        // Add transformations
        analyzer.add_transformation(DataTransformation {
            source: "x".to_string(),
            target: "y".to_string(),
            transformation_type: TransformationType::Assignment,
            input_types: vec!["i32".to_string()],
            output_types: vec!["i32".to_string()],
        });
        analyzer.add_transformation(DataTransformation {
            source: "y".to_string(),
            target: "z".to_string(),
            transformation_type: TransformationType::TypeConversion,
            input_types: vec!["i32".to_string()],
            output_types: vec!["String".to_string()],
        });

        let mut graph = CorrelationGraph::new(GraphType::DataFlow, "Test".to_string());
        graph
            .add_node(GraphNode {
                id: "x".to_string(),
                node_type: NodeType::Variable,
                label: "x".to_string(),
                metadata: HashMap::new(),
                position: None,
                size: None,
                color: None,
            })
            .unwrap();

        let stats = analyzer.calculate_statistics(&graph);

        assert_eq!(stats.total_variables, 2);
        assert_eq!(stats.typed_variables, 1);
        assert_eq!(stats.total_transformations, 2);
        assert_eq!(stats.type_conversions, 1);
        assert_eq!(stats.unused_variables, 1); // y is unused
        assert_eq!(stats.multi_usage_variables, 1); // x has 2 usages
        assert!(stats.average_usages_per_variable > 0.0);
    }

    #[test]
    fn test_data_flow_statistics_empty() {
        let analyzer = DataFlowAnalyzer::new();
        let graph = CorrelationGraph::new(GraphType::DataFlow, "Empty".to_string());

        let stats = analyzer.calculate_statistics(&graph);

        assert_eq!(stats.total_variables, 0);
        assert_eq!(stats.total_transformations, 0);
        assert_eq!(stats.average_chain_length, 0.0);
        assert_eq!(stats.max_chain_length, 0);
    }

    #[test]
    fn test_optimization_suggestions_priority_sorting() {
        let mut analyzer = DataFlowAnalyzer::new();

        // Add unused variable (low priority)
        analyzer.tracker_mut().track_definition(
            "unused".to_string(),
            "test.rs".to_string(),
            1,
            None,
            true,
            false,
        );

        // Add multiple conversions (medium priority)
        for i in 0..4 {
            analyzer.add_transformation(DataTransformation {
                source: "x".to_string(),
                target: format!("y{}", i),
                transformation_type: TransformationType::TypeConversion,
                input_types: vec![],
                output_types: vec![],
            });
        }

        let graph = CorrelationGraph::new(GraphType::DataFlow, "Test".to_string());
        let suggestions = analyzer.get_optimization_suggestions(&graph);

        // Should be sorted by priority (higher first)
        if suggestions.len() > 1 {
            for i in 0..suggestions.len() - 1 {
                let current_priority = match suggestions[i].priority {
                    OptimizationPriority::Critical => 4,
                    OptimizationPriority::High => 3,
                    OptimizationPriority::Medium => 2,
                    OptimizationPriority::Low => 1,
                };
                let next_priority = match suggestions[i + 1].priority {
                    OptimizationPriority::Critical => 4,
                    OptimizationPriority::High => 3,
                    OptimizationPriority::Medium => 2,
                    OptimizationPriority::Low => 1,
                };
                assert!(current_priority >= next_priority);
            }
        }
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::graph::correlation::{CorrelationGraph, GraphNode, GraphType, NodeType};
    use std::collections::HashMap;

    #[test]
    fn test_data_pipeline_analysis() {
        // Simulate a data pipeline: input -> filter -> map -> reduce -> output
        let mut analyzer = DataFlowAnalyzer::new();
        let mut files = HashMap::new();

        files.insert(
            "pipeline.rs".to_string(),
            "let data = vec![1, 2, 3, 4, 5];\n\
             let filtered = data.filter(|x| x > 2);\n\
             let mapped = filtered.map(|x| x * 2);\n\
             let result = mapped.reduce(|a, b| a + b);"
                .to_string(),
        );

        let result = analyzer.analyze_source_code(&files);
        assert!(result.is_ok());

        // Should have detected transformations
        assert!(!analyzer.transformations().is_empty());

        // Should have tracked variables
        assert!(!analyzer.tracker().all_variables().is_empty());
    }

    #[test]
    fn test_complete_data_flow_workflow() {
        // Test complete workflow: analyze -> build graph -> get suggestions -> calculate stats
        let mut analyzer = DataFlowAnalyzer::new();
        let mut files = HashMap::new();

        files.insert(
            "workflow.rs".to_string(),
            "let x: i32 = 5;\n\
             let y = x + 1;\n\
             let z = y.to_string();\n\
             let sum = [1, 2, 3].sum();"
                .to_string(),
        );

        analyzer.analyze_source_code(&files).unwrap();

        let base_graph = CorrelationGraph::new(GraphType::DataFlow, "Workflow".to_string());
        let graph = analyzer
            .build_enhanced_data_flow_graph(&base_graph)
            .unwrap();

        // Get optimization suggestions
        let suggestions = analyzer.get_optimization_suggestions(&graph);
        assert!(!suggestions.is_empty());

        // Calculate statistics
        let stats = analyzer.calculate_statistics(&graph);
        assert!(stats.total_variables > 0);
        assert!(stats.total_transformations > 0);
    }

    #[test]
    fn test_data_flow_with_type_propagation() {
        let mut analyzer = DataFlowAnalyzer::new();

        // Track variable with type
        analyzer.tracker_mut().track_definition(
            "numbers".to_string(),
            "test.rs".to_string(),
            1,
            Some("Vec<i32>".to_string()),
            true,
            false,
        );

        // Add transformations that should propagate types
        analyzer.add_transformation(DataTransformation {
            source: "numbers".to_string(),
            target: "filtered".to_string(),
            transformation_type: TransformationType::Filter,
            input_types: vec!["Vec<i32>".to_string()],
            output_types: vec![],
        });

        analyzer.add_transformation(DataTransformation {
            source: "filtered".to_string(),
            target: "sum".to_string(),
            transformation_type: TransformationType::Aggregation,
            input_types: vec![],
            output_types: vec![],
        });

        // Propagate types
        let mut transformations: Vec<DataTransformation> = analyzer.transformations().to_vec();
        analyzer
            .type_propagator_mut()
            .analyze_and_propagate(&mut transformations);

        // Verify type propagation worked
        let stats = analyzer.calculate_statistics(&CorrelationGraph::new(
            GraphType::DataFlow,
            "Test".to_string(),
        ));
        assert!(stats.typed_variables > 0);
    }

    #[test]
    fn test_optimization_suggestions_for_complex_pipeline() {
        let mut graph = CorrelationGraph::new(GraphType::DataFlow, "Complex Pipeline".to_string());
        let mut analyzer = DataFlowAnalyzer::new();

        // Create a complex pipeline with multiple independent chains
        for i in 0..4 {
            let source_id = format!("source{}", i);
            graph
                .add_node(GraphNode {
                    id: source_id.clone(),
                    node_type: NodeType::Variable,
                    label: source_id.clone(),
                    metadata: HashMap::new(),
                    position: None,
                    size: None,
                    color: None,
                })
                .unwrap();

            analyzer.tracker_mut().track_definition(
                format!("var{}", i),
                "pipeline.rs".to_string(),
                i + 1,
                Some("i32".to_string()),
                true,
                false,
            );

            // Add multiple transformations per source
            for j in 0..3 {
                analyzer.add_transformation(DataTransformation {
                    source: format!("var{}", i),
                    target: format!("trans{}_{}", i, j),
                    transformation_type: TransformationType::TypeConversion,
                    input_types: vec![],
                    output_types: vec![],
                });
            }
        }

        let suggestions = analyzer.get_optimization_suggestions(&graph);

        // Should detect parallelization opportunities
        let parallel_suggestion = suggestions.iter().find(|s| s.category == "Parallelization");
        assert!(parallel_suggestion.is_some());

        // Should detect redundant conversions
        let redundant_suggestion = suggestions
            .iter()
            .find(|s| s.category == "Redundant Conversions");
        assert!(redundant_suggestion.is_some());
    }
}
