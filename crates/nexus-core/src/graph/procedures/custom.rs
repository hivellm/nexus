//! Custom (user-defined) procedure support

use super::types::{GraphProcedure, ProcedureParameter, ProcedureResult};
use crate::Result;
use crate::graph::algorithms::Graph;
use serde_json::Value;
use std::collections::HashMap;

/// Custom procedure function type
pub type CustomProcedureFn =
    Box<dyn Fn(&Graph, &HashMap<String, Value>) -> Result<ProcedureResult> + Send + Sync>;

/// Wrapper for custom procedures
pub struct CustomProcedure {
    name: String,
    signature: Vec<ProcedureParameter>,
    function: CustomProcedureFn,
}

impl CustomProcedure {
    /// Create a new custom procedure
    pub fn new<F>(name: String, signature: Vec<ProcedureParameter>, function: F) -> Self
    where
        F: Fn(&Graph, &HashMap<String, Value>) -> Result<ProcedureResult> + Send + Sync + 'static,
    {
        Self {
            name,
            signature,
            function: Box::new(function),
        }
    }
}

impl GraphProcedure for CustomProcedure {
    fn name(&self) -> &str {
        &self.name
    }

    fn signature(&self) -> Vec<ProcedureParameter> {
        self.signature.clone()
    }

    fn execute(&self, graph: &Graph, args: &HashMap<String, Value>) -> Result<ProcedureResult> {
        (self.function)(graph, args)
    }

    // Custom procedures can optionally support streaming by overriding supports_streaming
    // and execute_streaming methods
}
