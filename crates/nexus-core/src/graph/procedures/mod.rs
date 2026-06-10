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

mod centrality;
mod community;
mod custom;
mod registry;
mod shortest_path;
mod similarity;
mod topology;
mod types;

// Re-export shared types
pub use types::{
    GraphProcedure, ParameterType, ProcedureParameter, ProcedureResult, ProcedureSignature,
};

// Re-export shortest path procedures
pub use shortest_path::{
    AStarProcedure, BellmanFordProcedure, DijkstraProcedure, KShortestPathsProcedure,
};

// Re-export centrality procedures
pub use centrality::{
    BetweennessCentralityProcedure, ClosenessCentralityProcedure, DegreeCentralityProcedure,
    EigenvectorCentralityProcedure, PageRankProcedure, WeightedPageRankProcedure,
};

// Re-export community procedures
pub use community::{
    LabelPropagationProcedure, LouvainProcedure, StronglyConnectedComponentsProcedure,
    WeaklyConnectedComponentsProcedure,
};

// Re-export similarity procedures
pub use similarity::{CosineSimilarityProcedure, JaccardSimilarityProcedure};

// Re-export topology procedures
pub use topology::{
    GlobalClusteringCoefficientProcedure, LocalClusteringCoefficientProcedure,
    TriangleCountProcedure,
};

// Re-export custom procedure support
pub use custom::{CustomProcedure, CustomProcedureFn};

// Re-export registry
pub use registry::ProcedureRegistry;

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
                rows: vec![vec![serde_json::Value::String("test".to_string())]],
            })
        });

        registry.register_custom(procedure).unwrap();
        assert!(registry.contains("custom.test"));
        assert_eq!(registry.list().len(), 22); // 21 built-in + 1 custom

        // Test execution
        let proc = registry.get("custom.test").unwrap();
        let result = proc
            .execute(
                &crate::graph::algorithms::Graph::new(),
                &std::collections::HashMap::new(),
            )
            .unwrap();
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
                        rows: vec![vec![serde_json::Value::Number((a + b).into())]],
                    })
                },
            )
            .unwrap();

        let proc = registry.get("custom.add").unwrap();
        let mut args = std::collections::HashMap::new();
        args.insert("a".to_string(), serde_json::Value::Number(10.into()));
        args.insert("b".to_string(), serde_json::Value::Number(20.into()));
        let result = proc
            .execute(&crate::graph::algorithms::Graph::new(), &args)
            .unwrap();
        assert_eq!(result.rows[0][0], serde_json::Value::Number(30.into()));
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
        let mut graph = crate::graph::algorithms::Graph::new();
        graph.add_node(1, vec![]);
        graph.add_node(2, vec![]);
        graph.add_node(3, vec![]);
        graph.add_edge(1, 2, 1.0, vec![]);
        graph.add_edge(2, 3, 2.0, vec![]);

        let procedure = DijkstraProcedure;
        let mut args = std::collections::HashMap::new();
        args.insert(
            "sourceNode".to_string(),
            serde_json::Value::Number(1.into()),
        );
        args.insert(
            "targetNode".to_string(),
            serde_json::Value::Number(3.into()),
        );

        let result = procedure.execute(&graph, &args).unwrap();
        assert_eq!(result.columns.len(), 2);
        assert!(!result.rows.is_empty());
    }

    #[test]
    fn test_procedure_with_catalog() {
        use crate::testing::TestContext;
        let ctx = TestContext::new();
        let catalog = std::sync::Arc::new(crate::catalog::Catalog::new(ctx.path()).unwrap());
        let registry = ProcedureRegistry::with_catalog(catalog.clone());

        // Register a custom procedure
        let procedure = CustomProcedure::new(
            "custom.catalog_test".to_string(),
            vec![],
            |_graph, _args| {
                Ok(ProcedureResult {
                    columns: vec!["result".to_string()],
                    rows: vec![vec![serde_json::Value::String("test".to_string())]],
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
                    rows: vec![vec![serde_json::Value::String("test1".to_string())]],
                })
            });
        registry.register_custom(procedure1).unwrap();

        let procedure2 =
            CustomProcedure::new("custom.duplicate".to_string(), vec![], |_graph, _args| {
                Ok(ProcedureResult {
                    columns: vec!["result".to_string()],
                    rows: vec![vec![serde_json::Value::String("test2".to_string())]],
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
                _graph: &crate::graph::algorithms::Graph,
                _args: &std::collections::HashMap<String, serde_json::Value>,
            ) -> crate::Result<ProcedureResult> {
                Ok(ProcedureResult {
                    columns: vec!["value".to_string()],
                    rows: vec![vec![serde_json::Value::Number(1.into())]],
                })
            }

            fn supports_streaming(&self) -> bool {
                true
            }

            #[allow(clippy::type_complexity)]
            fn execute_streaming(
                &self,
                _graph: &crate::graph::algorithms::Graph,
                _args: &std::collections::HashMap<String, serde_json::Value>,
                mut callback: Box<
                    dyn FnMut(&[String], &[serde_json::Value]) -> crate::Result<()> + Send,
                >,
            ) -> crate::Result<()> {
                // Stream multiple rows
                callback(
                    &["value".to_string()],
                    &[serde_json::Value::Number(1.into())],
                )?;
                callback(
                    &["value".to_string()],
                    &[serde_json::Value::Number(2.into())],
                )?;
                callback(
                    &["value".to_string()],
                    &[serde_json::Value::Number(3.into())],
                )?;
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
                &crate::graph::algorithms::Graph::new(),
                &std::collections::HashMap::new(),
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
        assert_eq!(collected_rows[0][0], serde_json::Value::Number(1.into()));
        assert_eq!(collected_rows[1][0], serde_json::Value::Number(2.into()));
        assert_eq!(collected_rows[2][0], serde_json::Value::Number(3.into()));
    }

    #[test]
    fn test_procedure_default_streaming_implementation() {
        // Test that default streaming implementation works
        let procedure = DijkstraProcedure;
        assert!(!procedure.supports_streaming());

        let mut graph = crate::graph::algorithms::Graph::new();
        graph.add_node(1, vec![]);
        graph.add_node(2, vec![]);
        graph.add_edge(1, 2, 1.0, vec![]);

        let mut args = std::collections::HashMap::new();
        args.insert(
            "sourceNode".to_string(),
            serde_json::Value::Number(1.into()),
        );
        args.insert(
            "targetNode".to_string(),
            serde_json::Value::Number(2.into()),
        );

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
