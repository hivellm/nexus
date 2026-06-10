//! Centrality algorithm procedures: PageRank, Betweenness, Closeness, Degree, Eigenvector

use super::types::{GraphProcedure, ParameterType, ProcedureParameter, ProcedureResult};
use crate::Result;
use crate::graph::algorithms::Graph;
use serde_json::Value;
use std::collections::HashMap;

/// PageRank procedure
pub struct PageRankProcedure;

impl GraphProcedure for PageRankProcedure {
    fn name(&self) -> &str {
        "gds.centrality.pagerank"
    }

    fn signature(&self) -> Vec<ProcedureParameter> {
        vec![
            ProcedureParameter {
                name: "dampingFactor".to_string(),
                param_type: ParameterType::Float,
                required: false,
                default: Some(Value::Number(
                    serde_json::Number::from_f64(0.85)
                        .unwrap_or_else(|| serde_json::Number::from(0)),
                )),
            },
            ProcedureParameter {
                name: "maxIterations".to_string(),
                param_type: ParameterType::Integer,
                required: false,
                default: Some(Value::Number(100.into())),
            },
            ProcedureParameter {
                name: "tolerance".to_string(),
                param_type: ParameterType::Float,
                required: false,
                default: Some(Value::Number(
                    serde_json::Number::from_f64(0.0001)
                        .unwrap_or_else(|| serde_json::Number::from(0)),
                )),
            },
        ]
    }

    fn execute(&self, graph: &Graph, args: &HashMap<String, Value>) -> Result<ProcedureResult> {
        let damping_factor = args
            .get("dampingFactor")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.85);
        let max_iterations = args
            .get("maxIterations")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(100);
        let tolerance = args
            .get("tolerance")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0001);

        let ranks = graph.pagerank(damping_factor, max_iterations, tolerance);

        let mut rows = Vec::new();
        for (node, rank) in &ranks {
            rows.push(vec![
                Value::Number((*node).into()),
                Value::Number(
                    serde_json::Number::from_f64(*rank)
                        .unwrap_or_else(|| serde_json::Number::from(0)),
                ),
            ]);
        }

        Ok(ProcedureResult {
            columns: vec!["node".to_string(), "score".to_string()],
            rows,
        })
    }
}

/// Weighted PageRank procedure - uses edge weights for contribution distribution
pub struct WeightedPageRankProcedure;

impl GraphProcedure for WeightedPageRankProcedure {
    fn name(&self) -> &str {
        "gds.centrality.pagerank.weighted"
    }

    fn signature(&self) -> Vec<ProcedureParameter> {
        vec![
            ProcedureParameter {
                name: "dampingFactor".to_string(),
                param_type: ParameterType::Float,
                required: false,
                default: Some(Value::Number(
                    serde_json::Number::from_f64(0.85)
                        .unwrap_or_else(|| serde_json::Number::from(0)),
                )),
            },
            ProcedureParameter {
                name: "maxIterations".to_string(),
                param_type: ParameterType::Integer,
                required: false,
                default: Some(Value::Number(100.into())),
            },
            ProcedureParameter {
                name: "tolerance".to_string(),
                param_type: ParameterType::Float,
                required: false,
                default: Some(Value::Number(
                    serde_json::Number::from_f64(0.0001)
                        .unwrap_or_else(|| serde_json::Number::from(0)),
                )),
            },
        ]
    }

    fn execute(&self, graph: &Graph, args: &HashMap<String, Value>) -> Result<ProcedureResult> {
        let damping_factor = args
            .get("dampingFactor")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.85);
        let max_iterations = args
            .get("maxIterations")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(100);
        let tolerance = args
            .get("tolerance")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0001);

        let ranks = graph.weighted_pagerank(damping_factor, max_iterations, tolerance);

        let mut rows = Vec::new();
        for (node, rank) in &ranks {
            rows.push(vec![
                Value::Number((*node).into()),
                Value::Number(
                    serde_json::Number::from_f64(*rank)
                        .unwrap_or_else(|| serde_json::Number::from(0)),
                ),
            ]);
        }

        Ok(ProcedureResult {
            columns: vec!["node".to_string(), "score".to_string()],
            rows,
        })
    }
}

/// Betweenness Centrality procedure
pub struct BetweennessCentralityProcedure;

impl GraphProcedure for BetweennessCentralityProcedure {
    fn name(&self) -> &str {
        "gds.centrality.betweenness"
    }

    fn signature(&self) -> Vec<ProcedureParameter> {
        vec![]
    }

    fn execute(&self, graph: &Graph, _args: &HashMap<String, Value>) -> Result<ProcedureResult> {
        let centrality = graph.betweenness_centrality();

        let mut rows = Vec::new();
        for (node, score) in &centrality {
            rows.push(vec![
                Value::Number((*node).into()),
                Value::Number(
                    serde_json::Number::from_f64(*score)
                        .unwrap_or_else(|| serde_json::Number::from(0)),
                ),
            ]);
        }

        Ok(ProcedureResult {
            columns: vec!["node".to_string(), "score".to_string()],
            rows,
        })
    }
}

/// Closeness Centrality procedure
pub struct ClosenessCentralityProcedure;

impl GraphProcedure for ClosenessCentralityProcedure {
    fn name(&self) -> &str {
        "gds.centrality.closeness"
    }

    fn signature(&self) -> Vec<ProcedureParameter> {
        vec![]
    }

    fn execute(&self, graph: &Graph, _args: &HashMap<String, Value>) -> Result<ProcedureResult> {
        let centrality = graph.closeness_centrality();

        let mut rows = Vec::new();
        for (node, score) in &centrality {
            rows.push(vec![
                Value::Number((*node).into()),
                Value::Number(
                    serde_json::Number::from_f64(*score)
                        .unwrap_or_else(|| serde_json::Number::from(0)),
                ),
            ]);
        }

        Ok(ProcedureResult {
            columns: vec!["node".to_string(), "score".to_string()],
            rows,
        })
    }
}

/// Degree Centrality procedure
pub struct DegreeCentralityProcedure;

impl GraphProcedure for DegreeCentralityProcedure {
    fn name(&self) -> &str {
        "gds.centrality.degree"
    }

    fn signature(&self) -> Vec<ProcedureParameter> {
        vec![]
    }

    fn execute(&self, graph: &Graph, _args: &HashMap<String, Value>) -> Result<ProcedureResult> {
        let centrality = graph.degree_centrality();

        let mut rows = Vec::new();
        for (node, score) in &centrality {
            rows.push(vec![
                Value::Number((*node).into()),
                Value::Number(
                    serde_json::Number::from_f64(*score)
                        .unwrap_or_else(|| serde_json::Number::from(0)),
                ),
            ]);
        }

        Ok(ProcedureResult {
            columns: vec!["node".to_string(), "score".to_string()],
            rows,
        })
    }
}

/// Eigenvector Centrality procedure
pub struct EigenvectorCentralityProcedure;

impl GraphProcedure for EigenvectorCentralityProcedure {
    fn name(&self) -> &str {
        "gds.centrality.eigenvector"
    }

    fn signature(&self) -> Vec<ProcedureParameter> {
        vec![
            ProcedureParameter {
                name: "maxIterations".to_string(),
                param_type: ParameterType::Integer,
                required: false,
                default: Some(Value::Number(100.into())),
            },
            ProcedureParameter {
                name: "tolerance".to_string(),
                param_type: ParameterType::Float,
                required: false,
                default: Some(Value::Number(
                    serde_json::Number::from_f64(1e-6)
                        .unwrap_or_else(|| serde_json::Number::from(0)),
                )),
            },
        ]
    }

    fn execute(&self, graph: &Graph, args: &HashMap<String, Value>) -> Result<ProcedureResult> {
        let max_iterations = args
            .get("maxIterations")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(100);
        let tolerance = args
            .get("tolerance")
            .and_then(|v| v.as_f64())
            .unwrap_or(1e-6);

        let centrality = graph.eigenvector_centrality_with_params(max_iterations, tolerance);

        let mut rows = Vec::new();
        for (node, score) in &centrality {
            rows.push(vec![
                Value::Number((*node).into()),
                Value::Number(
                    serde_json::Number::from_f64(*score)
                        .unwrap_or_else(|| serde_json::Number::from(0)),
                ),
            ]);
        }

        Ok(ProcedureResult {
            columns: vec!["node".to_string(), "score".to_string()],
            rows,
        })
    }
}
