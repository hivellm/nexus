//! Similarity procedures: Jaccard, Cosine

use super::types::{GraphProcedure, ParameterType, ProcedureParameter, ProcedureResult};
use crate::graph::algorithms::Graph;
use crate::{Error, Result};
use serde_json::Value;
use std::collections::HashMap;

/// Jaccard Similarity procedure
pub struct JaccardSimilarityProcedure;

impl GraphProcedure for JaccardSimilarityProcedure {
    fn name(&self) -> &str {
        "gds.similarity.jaccard"
    }

    fn signature(&self) -> Vec<ProcedureParameter> {
        vec![
            ProcedureParameter {
                name: "node1".to_string(),
                param_type: ParameterType::Integer,
                required: true,
                default: None,
            },
            ProcedureParameter {
                name: "node2".to_string(),
                param_type: ParameterType::Integer,
                required: true,
                default: None,
            },
        ]
    }

    fn execute(&self, graph: &Graph, args: &HashMap<String, Value>) -> Result<ProcedureResult> {
        let node1 = args
            .get("node1")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| Error::InvalidInput("node1 parameter required".to_string()))?;

        let node2 = args
            .get("node2")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| Error::InvalidInput("node2 parameter required".to_string()))?;

        let similarity = graph.jaccard_similarity(node1, node2);

        Ok(ProcedureResult {
            columns: vec!["similarity".to_string()],
            rows: vec![vec![Value::Number(
                serde_json::Number::from_f64(similarity)
                    .unwrap_or_else(|| serde_json::Number::from(0)),
            )]],
        })
    }
}

/// Cosine Similarity procedure
pub struct CosineSimilarityProcedure;

impl GraphProcedure for CosineSimilarityProcedure {
    fn name(&self) -> &str {
        "gds.similarity.cosine"
    }

    fn signature(&self) -> Vec<ProcedureParameter> {
        vec![
            ProcedureParameter {
                name: "node1".to_string(),
                param_type: ParameterType::Integer,
                required: true,
                default: None,
            },
            ProcedureParameter {
                name: "node2".to_string(),
                param_type: ParameterType::Integer,
                required: true,
                default: None,
            },
        ]
    }

    fn execute(&self, graph: &Graph, args: &HashMap<String, Value>) -> Result<ProcedureResult> {
        let node1 = args
            .get("node1")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| Error::InvalidInput("node1 parameter required".to_string()))?;

        let node2 = args
            .get("node2")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| Error::InvalidInput("node2 parameter required".to_string()))?;

        let similarity = graph.cosine_similarity(node1, node2);

        Ok(ProcedureResult {
            columns: vec!["similarity".to_string()],
            rows: vec![vec![Value::Number(
                serde_json::Number::from_f64(similarity)
                    .unwrap_or_else(|| serde_json::Number::from(0)),
            )]],
        })
    }
}
