//! Shortest path algorithm procedures: Dijkstra, A*, Bellman-Ford, Yen's k-shortest

use super::types::{GraphProcedure, ParameterType, ProcedureParameter, ProcedureResult};
use crate::graph::algorithms::Graph;
use crate::{Error, Result};
use serde_json::Value;
use std::collections::HashMap;

/// Dijkstra shortest path procedure
pub struct DijkstraProcedure;

impl GraphProcedure for DijkstraProcedure {
    fn name(&self) -> &str {
        "gds.shortestPath.dijkstra"
    }

    fn signature(&self) -> Vec<ProcedureParameter> {
        vec![
            ProcedureParameter {
                name: "sourceNode".to_string(),
                param_type: ParameterType::Integer,
                required: true,
                default: None,
            },
            ProcedureParameter {
                name: "targetNode".to_string(),
                param_type: ParameterType::Integer,
                required: false,
                default: None,
            },
            ProcedureParameter {
                name: "weightProperty".to_string(),
                param_type: ParameterType::String,
                required: false,
                default: Some(Value::String("weight".to_string())),
            },
        ]
    }

    fn execute(&self, graph: &Graph, args: &HashMap<String, Value>) -> Result<ProcedureResult> {
        let source = args
            .get("sourceNode")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| Error::InvalidInput("sourceNode parameter required".to_string()))?;

        let target = args.get("targetNode").and_then(|v| v.as_u64());

        let result = graph.dijkstra(source, target)?;

        let mut rows = Vec::new();
        if let Some(path) = &result.path {
            rows.push(vec![
                Value::Array(path.iter().map(|&n| Value::Number(n.into())).collect()),
                Value::Number(
                    serde_json::Number::from_f64(
                        result
                            .distances
                            .get(&path[path.len() - 1])
                            .copied()
                            .unwrap_or(0.0),
                    )
                    .unwrap_or_else(|| serde_json::Number::from(0)),
                ),
            ]);
        } else if target.is_none() {
            // Return all distances
            for (node, distance) in &result.distances {
                rows.push(vec![
                    Value::Number((*node).into()),
                    Value::Number(
                        serde_json::Number::from_f64(*distance)
                            .unwrap_or_else(|| serde_json::Number::from(0)),
                    ),
                ]);
            }
        }

        Ok(ProcedureResult {
            columns: vec!["path".to_string(), "cost".to_string()],
            rows,
        })
    }
}

/// A* shortest path procedure
pub struct AStarProcedure;

impl GraphProcedure for AStarProcedure {
    fn name(&self) -> &str {
        "gds.shortestPath.astar"
    }

    fn signature(&self) -> Vec<ProcedureParameter> {
        vec![
            ProcedureParameter {
                name: "sourceNode".to_string(),
                param_type: ParameterType::Integer,
                required: true,
                default: None,
            },
            ProcedureParameter {
                name: "targetNode".to_string(),
                param_type: ParameterType::Integer,
                required: true,
                default: None,
            },
            ProcedureParameter {
                name: "weightProperty".to_string(),
                param_type: ParameterType::String,
                required: false,
                default: Some(Value::String("weight".to_string())),
            },
        ]
    }

    fn execute(&self, graph: &Graph, args: &HashMap<String, Value>) -> Result<ProcedureResult> {
        let source = args
            .get("sourceNode")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| Error::InvalidInput("sourceNode parameter required".to_string()))?;

        let target = args
            .get("targetNode")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| Error::InvalidInput("targetNode parameter required".to_string()))?;

        // Simple heuristic: Euclidean distance (in real implementation, use node coordinates)
        let heuristic = |_n1: u64, _n2: u64| 0.0;
        let result = graph.astar(source, target, heuristic)?;

        let mut rows = Vec::new();
        if let Some(path) = &result.path {
            rows.push(vec![
                Value::Array(path.iter().map(|&n| Value::Number(n.into())).collect()),
                Value::Number(
                    serde_json::Number::from_f64(
                        result
                            .distances
                            .get(&path[path.len() - 1])
                            .copied()
                            .unwrap_or(0.0),
                    )
                    .unwrap_or_else(|| serde_json::Number::from(0)),
                ),
            ]);
        }

        Ok(ProcedureResult {
            columns: vec!["path".to_string(), "cost".to_string()],
            rows,
        })
    }
}

/// Bellman-Ford shortest path procedure
pub struct BellmanFordProcedure;

impl GraphProcedure for BellmanFordProcedure {
    fn name(&self) -> &str {
        "gds.shortestPath.bellmanFord"
    }

    fn signature(&self) -> Vec<ProcedureParameter> {
        vec![ProcedureParameter {
            name: "sourceNode".to_string(),
            param_type: ParameterType::Integer,
            required: true,
            default: None,
        }]
    }

    fn execute(&self, graph: &Graph, args: &HashMap<String, Value>) -> Result<ProcedureResult> {
        let source = args
            .get("sourceNode")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| Error::InvalidInput("sourceNode parameter required".to_string()))?;

        let (result, has_negative_cycle) = graph.bellman_ford(source)?;

        let mut rows = Vec::new();
        for (node, distance) in &result.distances {
            rows.push(vec![
                Value::Number((*node).into()),
                Value::Number(
                    serde_json::Number::from_f64(*distance)
                        .unwrap_or_else(|| serde_json::Number::from(0)),
                ),
                Value::Bool(has_negative_cycle),
            ]);
        }

        Ok(ProcedureResult {
            columns: vec![
                "node".to_string(),
                "distance".to_string(),
                "hasNegativeCycle".to_string(),
            ],
            rows,
        })
    }
}

/// K Shortest Paths procedure (Yen's algorithm)
pub struct KShortestPathsProcedure;

impl GraphProcedure for KShortestPathsProcedure {
    fn name(&self) -> &str {
        "gds.shortestPath.yens"
    }

    fn signature(&self) -> Vec<ProcedureParameter> {
        vec![
            ProcedureParameter {
                name: "sourceNode".to_string(),
                param_type: ParameterType::Integer,
                required: true,
                default: None,
            },
            ProcedureParameter {
                name: "targetNode".to_string(),
                param_type: ParameterType::Integer,
                required: true,
                default: None,
            },
            ProcedureParameter {
                name: "k".to_string(),
                param_type: ParameterType::Integer,
                required: false,
                default: Some(Value::Number(3.into())),
            },
        ]
    }

    fn execute(&self, graph: &Graph, args: &HashMap<String, Value>) -> Result<ProcedureResult> {
        let source = args
            .get("sourceNode")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| Error::InvalidInput("sourceNode parameter required".to_string()))?;

        let target = args
            .get("targetNode")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| Error::InvalidInput("targetNode parameter required".to_string()))?;

        let k = args
            .get("k")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(3);

        let paths = graph.k_shortest_paths(source, target, k)?;

        let mut rows = Vec::new();
        for (index, path_result) in paths.iter().enumerate() {
            rows.push(vec![
                Value::Number((index + 1).into()),
                Value::Array(
                    path_result
                        .path
                        .iter()
                        .map(|&n| Value::Number(n.into()))
                        .collect(),
                ),
                Value::Number(
                    serde_json::Number::from_f64(path_result.length)
                        .unwrap_or_else(|| serde_json::Number::from(0)),
                ),
            ]);
        }

        Ok(ProcedureResult {
            columns: vec!["index".to_string(), "path".to_string(), "cost".to_string()],
            rows,
        })
    }
}
