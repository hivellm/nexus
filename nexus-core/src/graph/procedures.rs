//! Graph algorithm procedures for CALL statements
//!
//! This module provides procedure wrappers for graph algorithms that can be called
//! via Cypher CALL statements once procedure support is fully implemented.
//!
//! Example usage (when CALL procedures are supported):
//! ```cypher
//! CALL gds.shortestPath.dijkstra(sourceNode, targetNode, {weightProperty: 'weight'})
//! YIELD path, cost
//! RETURN path, cost
//! ```

use crate::graph::algorithms::Graph;
use crate::{Error, Result};
use parking_lot::RwLock;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

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
    fn execute_streaming(
        &self,
        graph: &Graph,
        args: &HashMap<String, Value>,
        #[allow(clippy::type_complexity)]
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

/// Procedure registry (thread-safe)
#[derive(Clone)]
pub struct ProcedureRegistry {
    procedures: Arc<RwLock<HashMap<String, Arc<dyn GraphProcedure>>>>,
    catalog: Option<Arc<crate::catalog::Catalog>>,
}

impl ProcedureRegistry {
    /// Create a new procedure registry with all graph algorithm procedures
    pub fn new() -> Self {
        let registry = Self {
            procedures: Arc::new(RwLock::new(HashMap::new())),
            catalog: None,
        };

        // Register all built-in procedures
        registry.register_builtin(Arc::new(DijkstraProcedure) as Arc<dyn GraphProcedure>);
        registry.register_builtin(Arc::new(AStarProcedure) as Arc<dyn GraphProcedure>);
        registry.register_builtin(Arc::new(BellmanFordProcedure) as Arc<dyn GraphProcedure>);
        registry.register_builtin(Arc::new(PageRankProcedure) as Arc<dyn GraphProcedure>);
        registry
            .register_builtin(Arc::new(BetweennessCentralityProcedure) as Arc<dyn GraphProcedure>);
        registry
            .register_builtin(Arc::new(ClosenessCentralityProcedure) as Arc<dyn GraphProcedure>);
        registry.register_builtin(Arc::new(DegreeCentralityProcedure) as Arc<dyn GraphProcedure>);
        registry.register_builtin(Arc::new(LouvainProcedure) as Arc<dyn GraphProcedure>);
        registry.register_builtin(Arc::new(LabelPropagationProcedure) as Arc<dyn GraphProcedure>);
        registry.register_builtin(
            Arc::new(StronglyConnectedComponentsProcedure) as Arc<dyn GraphProcedure>
        );
        registry.register_builtin(
            Arc::new(WeaklyConnectedComponentsProcedure) as Arc<dyn GraphProcedure>
        );
        registry.register_builtin(Arc::new(JaccardSimilarityProcedure) as Arc<dyn GraphProcedure>);
        registry.register_builtin(Arc::new(CosineSimilarityProcedure) as Arc<dyn GraphProcedure>);

        // Register geospatial procedures
        registry
            .register_builtin(Arc::new(crate::geospatial::procedures::WithinBBoxProcedure)
                as Arc<dyn GraphProcedure>);
        registry.register_builtin(
            Arc::new(crate::geospatial::procedures::WithinDistanceProcedure)
                as Arc<dyn GraphProcedure>,
        );

        registry
    }

    /// Create a new procedure registry with catalog persistence
    pub fn with_catalog(catalog: Arc<crate::catalog::Catalog>) -> Self {
        let registry = Self {
            procedures: Arc::new(RwLock::new(HashMap::new())),
            catalog: Some(catalog.clone()),
        };

        // Register all built-in procedures
        registry.register_builtin(Arc::new(DijkstraProcedure) as Arc<dyn GraphProcedure>);
        registry.register_builtin(Arc::new(AStarProcedure) as Arc<dyn GraphProcedure>);
        registry.register_builtin(Arc::new(BellmanFordProcedure) as Arc<dyn GraphProcedure>);
        registry.register_builtin(Arc::new(PageRankProcedure) as Arc<dyn GraphProcedure>);
        registry
            .register_builtin(Arc::new(BetweennessCentralityProcedure) as Arc<dyn GraphProcedure>);
        registry
            .register_builtin(Arc::new(ClosenessCentralityProcedure) as Arc<dyn GraphProcedure>);
        registry.register_builtin(Arc::new(DegreeCentralityProcedure) as Arc<dyn GraphProcedure>);
        registry.register_builtin(Arc::new(LouvainProcedure) as Arc<dyn GraphProcedure>);
        registry.register_builtin(Arc::new(LabelPropagationProcedure) as Arc<dyn GraphProcedure>);
        registry.register_builtin(
            Arc::new(StronglyConnectedComponentsProcedure) as Arc<dyn GraphProcedure>
        );
        registry.register_builtin(
            Arc::new(WeaklyConnectedComponentsProcedure) as Arc<dyn GraphProcedure>
        );
        registry.register_builtin(Arc::new(JaccardSimilarityProcedure) as Arc<dyn GraphProcedure>);
        registry.register_builtin(Arc::new(CosineSimilarityProcedure) as Arc<dyn GraphProcedure>);

        // Register geospatial procedures
        registry
            .register_builtin(Arc::new(crate::geospatial::procedures::WithinBBoxProcedure)
                as Arc<dyn GraphProcedure>);
        registry.register_builtin(
            Arc::new(crate::geospatial::procedures::WithinDistanceProcedure)
                as Arc<dyn GraphProcedure>,
        );

        // Load custom procedures from catalog
        if let Ok(_procedure_names) = catalog.list_procedures() {
            // Signatures are loaded but function implementations need to be provided
            // This is expected - custom procedures need to be re-registered
        }

        registry
    }

    /// Register a built-in procedure (internal use)
    fn register_builtin(&self, procedure: Arc<dyn GraphProcedure>) {
        let name = procedure.name().to_string();
        let mut procedures = self.procedures.write();
        procedures.insert(name.clone(), procedure);
    }

    /// Register a custom procedure
    pub fn register_custom(&self, procedure: CustomProcedure) -> Result<()> {
        let name = procedure.name().to_string();
        let signature_params = procedure.signature();

        // Check if already registered
        {
            let procedures = self.procedures.read();
            if procedures.contains_key(&name) {
                return Err(Error::CypherSyntax(format!(
                    "Procedure '{}' already registered",
                    name
                )));
            }
        }

        // Persist signature to catalog if available
        if let Some(ref catalog) = self.catalog {
            // Get output columns from a test execution (or use default)
            // For now, we'll use empty columns - in practice, procedures should specify their outputs
            let proc_sig = ProcedureSignature {
                name: name.clone(),
                parameters: signature_params.clone(),
                output_columns: Vec::new(), // Will be populated when procedure is executed
                description: None,
            };
            catalog.store_procedure(&proc_sig)?;
        }

        // Register in memory
        let mut procedures = self.procedures.write();
        procedures.insert(name, Arc::new(procedure));
        Ok(())
    }

    /// Register a custom procedure from a function
    pub fn register_custom_fn<F>(
        &self,
        name: String,
        signature: Vec<ProcedureParameter>,
        function: F,
    ) -> Result<()>
    where
        F: Fn(&Graph, &HashMap<String, Value>) -> Result<ProcedureResult> + Send + Sync + 'static,
    {
        let procedure = CustomProcedure::new(name.clone(), signature, function);
        self.register_custom(procedure)
    }

    /// Unregister a procedure (only custom procedures can be unregistered)
    pub fn unregister(&self, name: &str) -> Result<()> {
        // Check if it's a built-in procedure (prefixed with "gds.")
        if name.starts_with("gds.") {
            return Err(Error::CypherSyntax(format!(
                "Cannot unregister built-in procedure '{}'",
                name
            )));
        }

        // Remove from memory
        {
            let mut procedures = self.procedures.write();
            procedures
                .remove(name)
                .ok_or_else(|| Error::CypherSyntax(format!("Procedure '{}' not found", name)))?;
        }

        // Remove from catalog if available
        if let Some(ref catalog) = self.catalog {
            catalog.remove_procedure(name)?;
        }

        Ok(())
    }

    /// Get a procedure by name
    pub fn get(&self, name: &str) -> Option<Arc<dyn GraphProcedure>> {
        let procedures = self.procedures.read();
        procedures.get(name).cloned()
    }

    /// List all registered procedure names
    pub fn list(&self) -> Vec<String> {
        let procedures = self.procedures.read();
        procedures.keys().cloned().collect()
    }

    /// Check if a procedure is registered
    pub fn contains(&self, name: &str) -> bool {
        let procedures = self.procedures.read();
        procedures.contains_key(name)
    }
}

impl Default for ProcedureRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_procedure_registry() {
        let registry = ProcedureRegistry::new();
        assert!(registry.get("gds.shortestPath.dijkstra").is_some());
        assert!(registry.get("gds.centrality.pagerank").is_some());
        assert!(registry.get("gds.community.louvain").is_some());
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_custom_procedure_registration() {
        let registry = ProcedureRegistry::new();

        // Register a custom procedure
        let procedure = CustomProcedure::new("custom.test".to_string(), vec![], |_graph, _args| {
            Ok(ProcedureResult {
                columns: vec!["result".to_string()],
                rows: vec![vec![Value::String("test".to_string())]],
            })
        });

        registry.register_custom(procedure).unwrap();
        assert!(registry.contains("custom.test"));
        assert_eq!(registry.list().len(), 16); // 15 built-in + 1 custom

        // Test execution
        let proc = registry.get("custom.test").unwrap();
        let result = proc.execute(&Graph::new(), &HashMap::new()).unwrap();
        assert_eq!(result.columns, vec!["result"]);
        assert_eq!(result.rows.len(), 1);

        // Unregister
        registry.unregister("custom.test").unwrap();
        assert!(!registry.contains("custom.test"));
    }

    #[test]
    fn test_custom_procedure_registration_fn() {
        let registry = ProcedureRegistry::new();

        // Register using register_custom_fn
        registry
            .register_custom_fn(
                "custom.add".to_string(),
                vec![
                    ProcedureParameter {
                        name: "a".to_string(),
                        param_type: ParameterType::Integer,
                        required: true,
                        default: None,
                    },
                    ProcedureParameter {
                        name: "b".to_string(),
                        param_type: ParameterType::Integer,
                        required: true,
                        default: None,
                    },
                ],
                |_graph, args| {
                    let a = args.get("a").and_then(|v| v.as_i64()).unwrap_or(0);
                    let b = args.get("b").and_then(|v| v.as_i64()).unwrap_or(0);
                    Ok(ProcedureResult {
                        columns: vec!["sum".to_string()],
                        rows: vec![vec![Value::Number((a + b).into())]],
                    })
                },
            )
            .unwrap();

        let proc = registry.get("custom.add").unwrap();
        let mut args = HashMap::new();
        args.insert("a".to_string(), Value::Number(10.into()));
        args.insert("b".to_string(), Value::Number(20.into()));
        let result = proc.execute(&Graph::new(), &args).unwrap();
        assert_eq!(result.rows[0][0], Value::Number(30.into()));
    }

    #[test]
    fn test_cannot_unregister_builtin() {
        let registry = ProcedureRegistry::new();

        // Try to unregister a built-in procedure
        let result = registry.unregister("gds.shortestPath.dijkstra");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Cannot unregister built-in")
        );
    }

    #[test]
    fn test_dijkstra_procedure() {
        let mut graph = Graph::new();
        graph.add_node(1, vec![]);
        graph.add_node(2, vec![]);
        graph.add_node(3, vec![]);
        graph.add_edge(1, 2, 1.0, vec![]);
        graph.add_edge(2, 3, 2.0, vec![]);

        let procedure = DijkstraProcedure;
        let mut args = HashMap::new();
        args.insert("sourceNode".to_string(), Value::Number(1.into()));
        args.insert("targetNode".to_string(), Value::Number(3.into()));

        let result = procedure.execute(&graph, &args).unwrap();
        assert_eq!(result.columns.len(), 2);
        assert!(!result.rows.is_empty());
    }

    #[test]
    fn test_procedure_with_catalog() {
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        let catalog = Arc::new(crate::catalog::Catalog::new(dir.path()).unwrap());
        let registry = ProcedureRegistry::with_catalog(catalog.clone());

        // Register a custom procedure
        let procedure = CustomProcedure::new(
            "custom.catalog_test".to_string(),
            vec![],
            |_graph, _args| {
                Ok(ProcedureResult {
                    columns: vec!["result".to_string()],
                    rows: vec![vec![Value::String("test".to_string())]],
                })
            },
        );

        registry.register_custom(procedure).unwrap();

        // Verify it's persisted in catalog
        let catalog_proc = catalog.get_procedure("custom.catalog_test").unwrap();
        assert!(catalog_proc.is_some());
        assert_eq!(catalog_proc.unwrap().name, "custom.catalog_test");

        // Verify it's in registry
        assert!(registry.contains("custom.catalog_test"));

        // Unregister and verify removal from catalog
        registry.unregister("custom.catalog_test").unwrap();
        let catalog_proc_after = catalog.get_procedure("custom.catalog_test").unwrap();
        assert!(catalog_proc_after.is_none());
    }

    #[test]
    fn test_procedure_unregister_nonexistent() {
        let registry = ProcedureRegistry::new();
        let result = registry.unregister("nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_procedure_get_nonexistent() {
        let registry = ProcedureRegistry::new();
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_procedure_list() {
        let registry = ProcedureRegistry::new();
        let procedures = registry.list();
        assert!(procedures.len() >= 13); // At least 13 built-in procedures
        assert!(procedures.contains(&"gds.shortestPath.dijkstra".to_string()));
    }

    #[test]
    fn test_procedure_duplicate_registration() {
        let registry = ProcedureRegistry::new();

        let procedure1 =
            CustomProcedure::new("custom.duplicate".to_string(), vec![], |_graph, _args| {
                Ok(ProcedureResult {
                    columns: vec!["result".to_string()],
                    rows: vec![vec![Value::String("test1".to_string())]],
                })
            });
        registry.register_custom(procedure1).unwrap();

        let procedure2 =
            CustomProcedure::new("custom.duplicate".to_string(), vec![], |_graph, _args| {
                Ok(ProcedureResult {
                    columns: vec!["result".to_string()],
                    rows: vec![vec![Value::String("test2".to_string())]],
                })
            });
        let result = registry.register_custom(procedure2);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("already registered")
        );
    }

    #[test]
    fn test_procedure_streaming_support() {
        // Create a custom procedure that supports streaming
        struct StreamingProcedure;

        impl GraphProcedure for StreamingProcedure {
            fn name(&self) -> &str {
                "custom.streaming"
            }

            fn signature(&self) -> Vec<ProcedureParameter> {
                vec![]
            }

            fn execute(
                &self,
                _graph: &Graph,
                _args: &HashMap<String, Value>,
            ) -> Result<ProcedureResult> {
                Ok(ProcedureResult {
                    columns: vec!["value".to_string()],
                    rows: vec![vec![Value::Number(1.into())]],
                })
            }

            fn supports_streaming(&self) -> bool {
                true
            }

            fn execute_streaming(
                &self,
                _graph: &Graph,
                _args: &HashMap<String, Value>,
                mut callback: Box<dyn FnMut(&[String], &[Value]) -> Result<()> + Send>,
            ) -> Result<()> {
                // Stream multiple rows
                callback(&["value".to_string()], &[Value::Number(1.into())])?;
                callback(&["value".to_string()], &[Value::Number(2.into())])?;
                callback(&["value".to_string()], &[Value::Number(3.into())])?;
                Ok(())
            }
        }

        let procedure = StreamingProcedure;
        assert!(procedure.supports_streaming());

        use std::sync::{Arc, Mutex};
        let collected_rows = Arc::new(Mutex::new(Vec::new()));
        let collected_columns = Arc::new(Mutex::new(None::<Vec<String>>));

        let rows_clone = collected_rows.clone();
        let cols_clone = collected_columns.clone();

        procedure
            .execute_streaming(
                &Graph::new(),
                &HashMap::new(),
                Box::new(move |cols, row| {
                    {
                        let mut cols_ref = cols_clone.lock().unwrap();
                        if cols_ref.is_none() {
                            *cols_ref = Some(cols.to_vec());
                        }
                    }
                    rows_clone.lock().unwrap().push(row.to_vec());
                    Ok(())
                }),
            )
            .unwrap();

        let collected_rows = collected_rows.lock().unwrap();
        let collected_columns = collected_columns.lock().unwrap();
        assert_eq!(collected_rows.len(), 3);
        assert_eq!(
            collected_columns.as_ref().unwrap(),
            &vec!["value".to_string()]
        );
        assert_eq!(collected_rows[0][0], Value::Number(1.into()));
        assert_eq!(collected_rows[1][0], Value::Number(2.into()));
        assert_eq!(collected_rows[2][0], Value::Number(3.into()));
    }

    #[test]
    fn test_procedure_default_streaming_implementation() {
        // Test that default streaming implementation works
        let procedure = DijkstraProcedure;
        assert!(!procedure.supports_streaming());

        let mut graph = Graph::new();
        graph.add_node(1, vec![]);
        graph.add_node(2, vec![]);
        graph.add_edge(1, 2, 1.0, vec![]);

        let mut args = HashMap::new();
        args.insert("sourceNode".to_string(), Value::Number(1.into()));
        args.insert("targetNode".to_string(), Value::Number(2.into()));

        use std::sync::{Arc, Mutex};
        let collected_rows = Arc::new(Mutex::new(Vec::new()));
        let collected_columns = Arc::new(Mutex::new(None::<Vec<String>>));

        let rows_clone = collected_rows.clone();
        let cols_clone = collected_columns.clone();

        // Default implementation should collect all results and stream them
        procedure
            .execute_streaming(
                &graph,
                &args,
                Box::new(move |cols, row| {
                    {
                        let mut cols_ref = cols_clone.lock().unwrap();
                        if cols_ref.is_none() {
                            *cols_ref = Some(cols.to_vec());
                        }
                    }
                    rows_clone.lock().unwrap().push(row.to_vec());
                    Ok(())
                }),
            )
            .unwrap();

        let collected_rows = collected_rows.lock().unwrap();
        let collected_columns = collected_columns.lock().unwrap();
        assert!(!collected_rows.is_empty());
        assert!(collected_columns.is_some());
    }
}
