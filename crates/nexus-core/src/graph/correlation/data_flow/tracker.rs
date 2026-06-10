//! Variable usage tracking for data-flow analysis.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::{DataFlowEdge, FlowType};

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
    pub(super) variables: HashMap<String, VariableUsage>,
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
