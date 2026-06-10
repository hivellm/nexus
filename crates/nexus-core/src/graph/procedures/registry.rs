//! Procedure registry: thread-safe registry for built-in and custom procedures

use super::centrality::{
    BetweennessCentralityProcedure, ClosenessCentralityProcedure, DegreeCentralityProcedure,
    EigenvectorCentralityProcedure, PageRankProcedure, WeightedPageRankProcedure,
};
use super::community::{
    LabelPropagationProcedure, LouvainProcedure, StronglyConnectedComponentsProcedure,
    WeaklyConnectedComponentsProcedure,
};
use super::custom::CustomProcedure;
use super::shortest_path::{
    AStarProcedure, BellmanFordProcedure, DijkstraProcedure, KShortestPathsProcedure,
};
use super::similarity::{CosineSimilarityProcedure, JaccardSimilarityProcedure};
use super::topology::{
    GlobalClusteringCoefficientProcedure, LocalClusteringCoefficientProcedure,
    TriangleCountProcedure,
};
use super::types::{GraphProcedure, ProcedureParameter, ProcedureSignature};
use crate::{Error, Result};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

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
        registry.register_builtin(Arc::new(WeightedPageRankProcedure) as Arc<dyn GraphProcedure>);
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
        registry
            .register_builtin(Arc::new(EigenvectorCentralityProcedure) as Arc<dyn GraphProcedure>);
        registry.register_builtin(Arc::new(KShortestPathsProcedure) as Arc<dyn GraphProcedure>);
        registry.register_builtin(Arc::new(TriangleCountProcedure) as Arc<dyn GraphProcedure>);
        registry.register_builtin(
            Arc::new(LocalClusteringCoefficientProcedure) as Arc<dyn GraphProcedure>
        );
        registry.register_builtin(
            Arc::new(GlobalClusteringCoefficientProcedure) as Arc<dyn GraphProcedure>
        );

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
        registry.register_builtin(Arc::new(WeightedPageRankProcedure) as Arc<dyn GraphProcedure>);
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
        registry
            .register_builtin(Arc::new(EigenvectorCentralityProcedure) as Arc<dyn GraphProcedure>);
        registry.register_builtin(Arc::new(KShortestPathsProcedure) as Arc<dyn GraphProcedure>);
        registry.register_builtin(Arc::new(TriangleCountProcedure) as Arc<dyn GraphProcedure>);
        registry.register_builtin(
            Arc::new(LocalClusteringCoefficientProcedure) as Arc<dyn GraphProcedure>
        );
        registry.register_builtin(
            Arc::new(GlobalClusteringCoefficientProcedure) as Arc<dyn GraphProcedure>
        );

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
        F: Fn(
                &crate::graph::algorithms::Graph,
                &HashMap<String, serde_json::Value>,
            ) -> Result<super::types::ProcedureResult>
            + Send
            + Sync
            + 'static,
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
