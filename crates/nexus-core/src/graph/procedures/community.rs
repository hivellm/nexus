//! Community detection procedures: Louvain, LabelPropagation, SCC, WCC

use super::types::{GraphProcedure, ParameterType, ProcedureParameter, ProcedureResult};
use crate::Result;
use crate::graph::algorithms::Graph;
use serde_json::Value;
use std::collections::HashMap;

/// Louvain community detection procedure
pub struct LouvainProcedure;

impl GraphProcedure for LouvainProcedure {
    fn name(&self) -> &str {
        "gds.community.louvain"
    }

    fn signature(&self) -> Vec<ProcedureParameter> {
        vec![ProcedureParameter {
            name: "maxIterations".to_string(),
            param_type: ParameterType::Integer,
            required: false,
            default: Some(Value::Number(10.into())),
        }]
    }

    fn execute(&self, graph: &Graph, args: &HashMap<String, Value>) -> Result<ProcedureResult> {
        let max_iterations = args
            .get("maxIterations")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(10);

        let result = graph.louvain(max_iterations);

        let mut rows = Vec::new();
        for (node, component_id) in &result.components {
            rows.push(vec![
                Value::Number((*node).into()),
                Value::Number((*component_id as u64).into()),
            ]);
        }

        Ok(ProcedureResult {
            columns: vec!["node".to_string(), "community".to_string()],
            rows,
        })
    }
}

/// Label Propagation procedure
pub struct LabelPropagationProcedure;

impl GraphProcedure for LabelPropagationProcedure {
    fn name(&self) -> &str {
        "gds.community.labelPropagation"
    }

    fn signature(&self) -> Vec<ProcedureParameter> {
        vec![ProcedureParameter {
            name: "maxIterations".to_string(),
            param_type: ParameterType::Integer,
            required: false,
            default: Some(Value::Number(10.into())),
        }]
    }

    fn execute(&self, graph: &Graph, args: &HashMap<String, Value>) -> Result<ProcedureResult> {
        let max_iterations = args
            .get("maxIterations")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(10);

        let result = graph.label_propagation(max_iterations);

        let mut rows = Vec::new();
        for (node, component_id) in &result.components {
            rows.push(vec![
                Value::Number((*node).into()),
                Value::Number((*component_id as u64).into()),
            ]);
        }

        Ok(ProcedureResult {
            columns: vec!["node".to_string(), "community".to_string()],
            rows,
        })
    }
}

/// Strongly Connected Components procedure
pub struct StronglyConnectedComponentsProcedure;

impl GraphProcedure for StronglyConnectedComponentsProcedure {
    fn name(&self) -> &str {
        "gds.community.stronglyConnectedComponents"
    }

    fn signature(&self) -> Vec<ProcedureParameter> {
        vec![]
    }

    fn execute(&self, graph: &Graph, _args: &HashMap<String, Value>) -> Result<ProcedureResult> {
        let result = graph.strongly_connected_components();

        let mut rows = Vec::new();
        for (node, component_id) in &result.components {
            rows.push(vec![
                Value::Number((*node).into()),
                Value::Number((*component_id as u64).into()),
            ]);
        }

        Ok(ProcedureResult {
            columns: vec!["node".to_string(), "component".to_string()],
            rows,
        })
    }
}

/// Weakly Connected Components procedure
pub struct WeaklyConnectedComponentsProcedure;

impl GraphProcedure for WeaklyConnectedComponentsProcedure {
    fn name(&self) -> &str {
        "gds.community.weaklyConnectedComponents"
    }

    fn signature(&self) -> Vec<ProcedureParameter> {
        vec![]
    }

    fn execute(&self, graph: &Graph, _args: &HashMap<String, Value>) -> Result<ProcedureResult> {
        let result = graph.connected_components();

        let mut rows = Vec::new();
        for (node, component_id) in &result.components {
            rows.push(vec![
                Value::Number((*node).into()),
                Value::Number((*component_id as u64).into()),
            ]);
        }

        Ok(ProcedureResult {
            columns: vec!["node".to_string(), "component".to_string()],
            rows,
        })
    }
}
