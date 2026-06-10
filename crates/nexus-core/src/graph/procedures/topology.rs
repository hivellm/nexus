//! Topology analysis procedures: TriangleCount, Local/Global Clustering Coefficient

use super::types::{GraphProcedure, ProcedureParameter, ProcedureResult};
use crate::Result;
use crate::graph::algorithms::Graph;
use serde_json::Value;
use std::collections::HashMap;

/// Triangle Count procedure
pub struct TriangleCountProcedure;

impl GraphProcedure for TriangleCountProcedure {
    fn name(&self) -> &str {
        "gds.triangleCount"
    }

    fn signature(&self) -> Vec<ProcedureParameter> {
        vec![]
    }

    fn execute(&self, graph: &Graph, _args: &HashMap<String, Value>) -> Result<ProcedureResult> {
        let count = graph.triangle_count();

        Ok(ProcedureResult {
            columns: vec!["triangleCount".to_string()],
            rows: vec![vec![Value::Number((count as u64).into())]],
        })
    }
}

/// Local Clustering Coefficient procedure
pub struct LocalClusteringCoefficientProcedure;

impl GraphProcedure for LocalClusteringCoefficientProcedure {
    fn name(&self) -> &str {
        "gds.localClusteringCoefficient"
    }

    fn signature(&self) -> Vec<ProcedureParameter> {
        vec![]
    }

    fn execute(&self, graph: &Graph, _args: &HashMap<String, Value>) -> Result<ProcedureResult> {
        let coefficients = graph.clustering_coefficient();

        let mut rows = Vec::new();
        for (node, coefficient) in &coefficients {
            rows.push(vec![
                Value::Number((*node).into()),
                Value::Number(
                    serde_json::Number::from_f64(*coefficient)
                        .unwrap_or_else(|| serde_json::Number::from(0)),
                ),
            ]);
        }

        Ok(ProcedureResult {
            columns: vec!["node".to_string(), "coefficient".to_string()],
            rows,
        })
    }
}

/// Global Clustering Coefficient procedure
pub struct GlobalClusteringCoefficientProcedure;

impl GraphProcedure for GlobalClusteringCoefficientProcedure {
    fn name(&self) -> &str {
        "gds.globalClusteringCoefficient"
    }

    fn signature(&self) -> Vec<ProcedureParameter> {
        vec![]
    }

    fn execute(&self, graph: &Graph, _args: &HashMap<String, Value>) -> Result<ProcedureResult> {
        let coefficient = graph.global_clustering_coefficient();

        Ok(ProcedureResult {
            columns: vec!["coefficient".to_string()],
            rows: vec![vec![Value::Number(
                serde_json::Number::from_f64(coefficient)
                    .unwrap_or_else(|| serde_json::Number::from(0)),
            )]],
        })
    }
}
