//! Engine-level read-only operations: KNN search, export, graph
//! statistics, integrity validation, and health checks.
//!
//! These methods form the "observability / maintenance" surface of
//! `Engine` — they do not mutate graph state (`clear_all_data` is the
//! single intentional exception, grouped here because it is the
//! lifecycle counterpart of `validate_graph`). Extracted from
//! `engine/mod.rs` during the split — public API is unchanged;
//! methods are still `Engine`'s via a second `impl Engine { ... }`
//! block here.

use super::Engine;
use crate::{Graph, Result, ValidationResult, catalog, storage};
use std::sync::Arc;

use super::config::GraphStatistics;
use super::stats::{HealthState, HealthStatus};

impl Engine {
    /// Perform KNN search over the vector index registered for `label`.
    pub fn knn_search(&self, label: &str, vector: &[f32], k: usize) -> Result<Vec<(u64, f32)>> {
        self.indexes.knn_search(label, vector, k)
    }

    /// Export graph data to JSON format (nodes + relationships with
    /// labels, types, and properties).
    pub fn export_to_json(&mut self) -> Result<serde_json::Value> {
        let mut export_data = serde_json::Map::new();

        let mut nodes = Vec::new();
        for node_id in 0..self.storage.node_count() {
            if let Ok(Some(node_record)) = self.get_node(node_id) {
                let labels = self
                    .catalog
                    .get_labels_from_bitmap(node_record.label_bits)?;
                let properties = self
                    .storage
                    .load_node_properties(node_id)
                    .unwrap_or(None)
                    .unwrap_or_else(|| serde_json::json!({}));

                nodes.push(serde_json::json!({
                    "id": node_id,
                    "labels": labels,
                    "properties": properties,
                }));
            }
        }
        export_data.insert("nodes".to_string(), serde_json::Value::Array(nodes));

        let mut relationships = Vec::new();
        for rel_id in 0..self.storage.relationship_count() {
            if let Ok(Some(rel_record)) = self.get_relationship(rel_id) {
                let rel_type = self
                    .catalog
                    .get_type_name(rel_record.type_id)
                    .unwrap_or_else(|_| Some("UNKNOWN".to_string()))
                    .unwrap_or_else(|| "UNKNOWN".to_string());

                // Copy values out of the #[repr(packed)] record to
                // dodge alignment warnings.
                let src_id = rel_record.src_id;
                let dst_id = rel_record.dst_id;

                let properties = self
                    .storage
                    .load_relationship_properties(rel_id)
                    .unwrap_or(None)
                    .unwrap_or_else(|| serde_json::json!({}));

                relationships.push(serde_json::json!({
                    "id": rel_id,
                    "source": src_id,
                    "target": dst_id,
                    "type": rel_type,
                    "properties": properties,
                }));
            }
        }
        export_data.insert(
            "relationships".to_string(),
            serde_json::Value::Array(relationships),
        );

        Ok(serde_json::Value::Object(export_data))
    }

    /// Walk every node and relationship record and return a summary
    /// with per-label and per-type counts.
    pub fn get_graph_statistics(&mut self) -> Result<GraphStatistics> {
        let mut stats = GraphStatistics::default();

        for node_id in 0..self.storage.node_count() {
            if let Ok(Some(node_record)) = self.get_node(node_id) {
                if !node_record.is_deleted() {
                    stats.node_count += 1;

                    let labels = self
                        .catalog
                        .get_labels_from_bitmap(node_record.label_bits)?;
                    for label in labels {
                        *stats.label_counts.entry(label).or_insert(0) += 1;
                    }
                }
            }
        }

        for rel_id in 0..self.storage.relationship_count() {
            if let Ok(Some(rel_record)) = self.get_relationship(rel_id) {
                if !rel_record.is_deleted() {
                    stats.relationship_count += 1;

                    let rel_type = self
                        .catalog
                        .get_type_name(rel_record.type_id)
                        .unwrap_or_else(|_| Some("UNKNOWN".to_string()))
                        .unwrap_or_else(|| "UNKNOWN".to_string());
                    *stats.relationship_type_counts.entry(rel_type).or_insert(0) += 1;
                }
            }
        }

        Ok(stats)
    }

    /// Clear all node / relationship records from storage and reset
    /// catalog counters. Used by `drop-database` style admin flows.
    pub fn clear_all_data(&mut self) -> Result<()> {
        self.storage.clear_all()?;

        let mut stats = self.catalog.get_statistics()?;
        stats.node_counts.clear();
        stats.rel_counts.clear();
        self.catalog.update_statistics(&stats)?;

        Ok(())
    }

    /// Validate the entire graph for integrity and consistency.
    ///
    /// Builds an isolated temporary copy of the storage + catalog so
    /// the validation pass does not mutate the live engine state.
    pub fn validate_graph(&self) -> Result<ValidationResult> {
        let temp_dir = tempfile::tempdir()?;
        let store = storage::RecordStore::new(temp_dir.path())?;
        let catalog = catalog::Catalog::new(temp_dir.path().join("catalog"))?;
        let graph = Graph::new(store, Arc::new(catalog));
        graph.validate()
    }

    /// Boolean shorthand over `validate_graph` — true when every
    /// integrity invariant holds.
    pub fn graph_health_check(&self) -> Result<bool> {
        self.validate_graph().map(|result| result.is_valid)
    }

    /// Per-subsystem health report.
    ///
    /// Queries `health_check()` on each owned subsystem
    /// (catalog, storage, page cache, WAL, index manager) and rolls
    /// an aggregate `overall` state up from the individual outcomes.
    /// Any `Err` from a subsystem demotes `overall` to `Unhealthy`.
    pub fn health_check(&self) -> Result<HealthStatus> {
        let mut status = HealthStatus {
            overall: HealthState::Healthy,
            components: std::collections::HashMap::new(),
        };

        for (name, result) in [
            ("catalog", self.catalog.health_check().map(|_| ())),
            ("storage", self.storage.health_check().map(|_| ())),
            ("page_cache", self.page_cache.health_check().map(|_| ())),
            ("wal", self.wal.health_check().map(|_| ())),
            ("indexes", self.indexes.health_check().map(|_| ())),
        ] {
            match result {
                Ok(()) => {
                    status
                        .components
                        .insert(name.to_string(), HealthState::Healthy);
                }
                Err(_) => {
                    status
                        .components
                        .insert(name.to_string(), HealthState::Unhealthy);
                    status.overall = HealthState::Unhealthy;
                }
            }
        }

        Ok(status)
    }
}
