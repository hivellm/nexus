//! Core data-flow types: edges, transformations, and type propagation.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

/// Data type propagation analyzer
///
/// Tracks how data types flow through transformations and propagates
/// type information through the data flow graph.
pub struct TypePropagator {
    /// Map of variable/function to its inferred type
    pub(super) type_map: HashMap<String, String>,
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
