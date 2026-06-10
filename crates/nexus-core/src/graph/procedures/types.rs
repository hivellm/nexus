//! Shared types for graph procedure infrastructure

use crate::Result;
use crate::graph::algorithms::Graph;
use serde_json::Value;
use std::collections::HashMap;

/// Procedure result structure
#[derive(Debug, Clone)]
pub struct ProcedureResult {
    /// Column names
    pub columns: Vec<String>,
    /// Rows of data
    pub rows: Vec<Vec<Value>>,
}

/// Trait for graph algorithm procedures
pub trait GraphProcedure: Send + Sync {
    /// Get the procedure name (e.g., "gds.shortestPath.dijkstra")
    fn name(&self) -> &str;

    /// Get the procedure signature (input parameters)
    fn signature(&self) -> Vec<ProcedureParameter>;

    /// Execute the procedure with given arguments
    fn execute(&self, graph: &Graph, args: &HashMap<String, Value>) -> Result<ProcedureResult>;

    /// Check if this procedure supports streaming results
    ///
    /// If true, `execute_streaming` can be used for better memory efficiency
    /// with large result sets. Default implementation returns false.
    fn supports_streaming(&self) -> bool {
        false
    }

    /// Execute the procedure with streaming results
    ///
    /// This method is called when streaming is enabled. The callback is invoked
    /// for each row as it becomes available. This allows processing large result
    /// sets without loading everything into memory at once.
    ///
    /// Default implementation collects all results and calls the callback sequentially.
    /// Procedures that support true streaming should override this method.
    #[allow(clippy::type_complexity)]
    fn execute_streaming(
        &self,
        graph: &Graph,
        args: &HashMap<String, Value>,
        mut callback: Box<dyn FnMut(&[String], &[Value]) -> Result<()> + Send>,
    ) -> Result<()> {
        // Default implementation: collect all results and stream them
        let result = self.execute(graph, args)?;
        for row in &result.rows {
            callback(&result.columns, row)?;
        }
        Ok(())
    }
}

/// Procedure parameter definition
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProcedureParameter {
    /// Parameter name
    pub name: String,
    /// Parameter type
    pub param_type: ParameterType,
    /// Whether parameter is required
    pub required: bool,
    /// Default value (if optional)
    pub default: Option<Value>,
}

/// Parameter types for procedures
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ParameterType {
    Integer,
    Float,
    String,
    Boolean,
    Node,
    Map,
    List,
}

/// Procedure signature for storage
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProcedureSignature {
    /// Procedure name
    pub name: String,
    /// Procedure parameters
    pub parameters: Vec<ProcedureParameter>,
    /// Output columns
    pub output_columns: Vec<String>,
    /// Description (optional)
    pub description: Option<String>,
}
