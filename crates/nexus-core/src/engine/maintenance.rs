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

/// Multiplier applied to the caller's `k` before querying the (single,
/// global) HNSW graph, so that post-filtering by label still has enough
/// raw candidates to return `k` label-matching hits.
///
/// The vector index backing `knn_search` is one global HNSW graph shared
/// across every label (phase20_knn-write-path-wiring §1.2 —
/// `VectorIndexRegistry` tracks only which `(label, property)` feeds it,
/// not a separate graph per label). Asking the HNSW for the top-`k`
/// nearest neighbours and then dropping hits that don't carry the
/// requested label can leave fewer than `k` results even when `k+`
/// label-matching nodes exist in the graph — the dropped hits "use up"
/// slots that a label-aware index would never have spent. Over-fetching
/// trades extra HNSW work (larger `ef`/candidate list) for correctness:
/// a 10x oversample keeps the common case (label is a sizeable fraction
/// of the graph) recall-correct without an unbounded raw scan. The
/// long-term fix is a per-label vector index; this is the v1 mitigation
/// documented in phase20_knn-write-path-wiring §2.6.
const KNN_LABEL_FILTER_OVERSAMPLE: usize = 10;

/// Hard ceiling on the oversampled `k` passed to the HNSW so a large
/// caller-supplied `k` can't force an unbounded raw candidate scan.
const KNN_LABEL_FILTER_MAX_RAW_K: usize = 10_000;

impl Engine {
    /// Perform KNN search over the vector index registered for `label`,
    /// post-filtered to nodes that carry `label`.
    ///
    /// The HNSW graph underneath `self.indexes.knn_search` is a single
    /// global index shared by every label (see
    /// [`KNN_LABEL_FILTER_OVERSAMPLE`] for why); this method resolves
    /// `label` to its catalog id, fetches the label's node-id bitmap, and
    /// filters the raw ranked HNSW hits down to that set — preserving
    /// rank order and scores. An unknown `label` (no catalog entry) or a
    /// label with no nodes yields an empty result, never an error.
    pub fn knn_search(&self, label: &str, vector: &[f32], k: usize) -> Result<Vec<(u64, f32)>> {
        let label_id = match self.catalog.get_label_id(label) {
            Ok(id) => id,
            Err(_) => return Ok(Vec::new()),
        };
        let label_nodes = self.indexes.label_index.get_nodes(label_id)?;
        if label_nodes.is_empty() {
            return Ok(Vec::new());
        }

        let raw_k = k
            .saturating_mul(KNN_LABEL_FILTER_OVERSAMPLE)
            .min(KNN_LABEL_FILTER_MAX_RAW_K)
            .max(k);
        let raw_hits = self.indexes.knn_search(label, vector, raw_k)?;

        let mut filtered: Vec<(u64, f32)> = raw_hits
            .into_iter()
            .filter(|(node_id, _)| label_nodes.contains(*node_id as u32))
            .collect();
        filtered.truncate(k);
        Ok(filtered)
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

        // Defense in depth (phase0_fix-store-size-per-clone-divergence §3.2):
        // clear_all shrinks the shared record-store mmap; refresh the cached
        // executor clone promptly so it re-clones the store with the reset
        // size instead of leaving a stale-large snapshot until the next
        // natural refresh_executor. The read path already bound-checks against
        // the live mmap length, so this is belt-and-suspenders, not the
        // primary fix.
        self.refresh_executor()?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::DEFAULT_VECTORIZER_DIMENSION;
    use crate::testing::TestContext;

    /// `knn_search` must post-filter the global HNSW graph's raw hits down
    /// to nodes carrying the requested label — a :Doc query must never
    /// surface an equally (or more) similar :Other node.
    #[test]
    fn knn_search_filters_out_other_labels() {
        let ctx = TestContext::new();
        let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

        let doc_id = engine
            .create_node(vec!["Doc".to_string()], serde_json::json!({}))
            .expect("create :Doc node");
        let other_id = engine
            .create_node(vec!["Other".to_string()], serde_json::json!({}))
            .expect("create :Other node");

        let query = vec![1.0_f32; DEFAULT_VECTORIZER_DIMENSION];
        // The :Other node gets the exact query vector (perfect match);
        // the :Doc node gets a merely close vector — without label
        // filtering the :Other node would rank first and dominate the
        // top-1 result.
        let mut doc_vector = vec![1.0_f32; DEFAULT_VECTORIZER_DIMENSION];
        doc_vector[0] = 0.9;
        engine
            .indexes
            .knn_index
            .add_vector(doc_id, doc_vector)
            .expect("add :Doc vector");
        engine
            .indexes
            .knn_index
            .add_vector(other_id, query.clone())
            .expect("add :Other vector");

        let results = engine
            .knn_search("Doc", &query, 5)
            .expect("knn_search must succeed");

        assert_eq!(
            results.len(),
            1,
            "only the single :Doc node must be returned, got {results:?}"
        );
        assert_eq!(
            results[0].0, doc_id,
            "the returned hit must be the :Doc node, not the :Other node"
        );
        assert!(
            results.iter().all(|(id, _)| *id != other_id),
            "the :Other node must never appear in a :Doc knn_search, got {results:?}"
        );
    }

    /// An unregistered / nonexistent label must yield an empty result,
    /// never an error — mirrors the "label doesn't exist" convention used
    /// elsewhere in the engine (e.g. `catalog.get_label_id` callers that
    /// treat `Err` as "no matches").
    #[test]
    fn knn_search_unknown_label_returns_empty() {
        let ctx = TestContext::new();
        let engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

        let query = vec![1.0_f32; DEFAULT_VECTORIZER_DIMENSION];
        let results = engine
            .knn_search("NoSuchLabel", &query, 5)
            .expect("unknown label must not error");
        assert!(
            results.is_empty(),
            "unknown label must yield an empty result, got {results:?}"
        );
    }
}
