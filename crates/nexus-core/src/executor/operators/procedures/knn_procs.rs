//! KNN (vector) index auto-populate hook: `knn_autopopulate_node`,
//! called from the CREATE paths.
//!
//! phase20_knn-write-path-wiring §2.4/§2.5 — mirrors
//! `spatial_autopopulate_node` / `fts_autopopulate_node`: routes
//! through the executor's shared `VectorIndexRegistry` /
//! `KnnIndex` (`ExecutorShared::knn_registry` /
//! `ExecutorShared::knn_index`, wired in §1.4) so any Cypher `CREATE`
//! that runs through the executor keeps the same global HNSW graph
//! the engine-side hook (`Engine::knn_autopopulate_node`, §2.2)
//! populates in lockstep.

use super::super::super::engine::Executor;

/// Extract an embedding for the KNN write-path hook from a raw
/// property value. Returns `None` (a silent skip, not an error)
/// unless `val` is a JSON array of finite numbers — mirrors
/// `Engine::knn_embedding_from_json` (the engine-side twin of this
/// hook) and how `Point::from_json_value` rejects a malformed Point
/// for `spatial_autopopulate_node` without aborting the containing
/// write. Dimension is deliberately NOT checked here: an
/// out-of-dimension vector is passed through to
/// `KnnIndex::add_vector`, whose own dimension check produces the
/// `Err` the caller logs and skips on.
fn knn_embedding_from_json(val: &serde_json::Value) -> Option<Vec<f32>> {
    let arr = val.as_array()?;
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let n = item.as_f64()?;
        if !n.is_finite() {
            return None;
        }
        out.push(n as f32);
    }
    Some(out)
}

impl Executor {
    /// §2.4/§2.5 — auto-populate the active vector index when the node
    /// just created carries its label AND a well-formed numeric-array
    /// embedding on its indexed property.
    ///
    /// Match rule (mirrors `spatial_autopopulate_node` /
    /// `fts_autopopulate_node`):
    /// - The node carries the index's label, AND
    /// - the indexed property holds a JSON array of finite numbers.
    ///
    /// A property that is missing, not an array, or holds a non-finite
    /// element is silently skipped (not an error) — the node simply
    /// isn't indexed by the active vector index. A dimension mismatch
    /// or any other `add_vector` failure is logged via
    /// `tracing::warn!` and swallowed — the KNN index is never the
    /// source of truth.
    ///
    /// No WAL is emitted here: replay-WAL journaling lives on the
    /// engine-side hook (`Engine::knn_autopopulate_node`); the executor
    /// hook only keeps the in-memory HNSW graph current. This matches
    /// the FTS / spatial auto-populate layering — the executor crate
    /// does not own the WAL handle.
    pub(in crate::executor) fn knn_autopopulate_node(
        &self,
        node_id: u64,
        label_ids: &[u32],
        properties: &serde_json::Value,
    ) {
        let Some(props_obj) = properties.as_object() else {
            return;
        };
        let registry = self.knn_registry();
        for (name, label_name, property_key) in registry.definitions() {
            let label_id = match self.catalog().get_label_id(&label_name) {
                Ok(id) => id,
                Err(_) => continue,
            };
            if !label_ids.contains(&label_id) {
                continue;
            }
            let Some(val) = props_obj.get(&property_key) else {
                continue;
            };
            let Some(embedding) = knn_embedding_from_json(val) else {
                continue;
            };
            if let Err(e) = self.knn_index().add_vector(node_id, embedding) {
                tracing::warn!("KNN: add_vector on index {name:?} for node {node_id} failed: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::engine::Executor;
    use crate::index::DEFAULT_VECTORIZER_DIMENSION;

    fn embedding_json(dim: usize, seed: f32) -> serde_json::Value {
        let values: Vec<serde_json::Value> = (0..dim)
            .map(|i| serde_json::json!(seed + i as f32 * 0.01))
            .collect();
        serde_json::Value::Array(values)
    }

    #[test]
    fn knn_autopopulate_node_adds_vector_when_index_and_property_match() {
        let executor = Executor::default();
        let doc_id = executor.catalog().get_or_create_label("Doc").unwrap();
        executor
            .knn_registry()
            .register("doc_embedding_idx", "Doc", "embedding", false)
            .unwrap();

        let props =
            serde_json::json!({ "embedding": embedding_json(DEFAULT_VECTORIZER_DIMENSION, 1.0) });
        executor.knn_autopopulate_node(7, &[doc_id], &props);

        assert!(executor.knn_index().has_vector(7));
    }

    #[test]
    fn knn_autopopulate_node_is_noop_without_registered_index() {
        let executor = Executor::default();
        let doc_id = executor.catalog().get_or_create_label("Doc").unwrap();

        let props =
            serde_json::json!({ "embedding": embedding_json(DEFAULT_VECTORIZER_DIMENSION, 1.0) });
        executor.knn_autopopulate_node(7, &[doc_id], &props);

        assert!(!executor.knn_index().has_vector(7));
    }

    #[test]
    fn knn_autopopulate_node_is_noop_when_property_missing_not_array_or_label_mismatch() {
        let executor = Executor::default();
        let doc_id = executor.catalog().get_or_create_label("Doc").unwrap();
        executor
            .knn_registry()
            .register("doc_embedding_idx", "Doc", "embedding", false)
            .unwrap();

        // Missing property.
        executor.knn_autopopulate_node(1, &[doc_id], &serde_json::json!({ "title": "x" }));
        assert!(!executor.knn_index().has_vector(1));

        // Property is not an array.
        executor.knn_autopopulate_node(
            2,
            &[doc_id],
            &serde_json::json!({ "embedding": "not-an-array" }),
        );
        assert!(!executor.knn_index().has_vector(2));

        // Label mismatch.
        executor.knn_autopopulate_node(
            3,
            &[999],
            &serde_json::json!({ "embedding": embedding_json(DEFAULT_VECTORIZER_DIMENSION, 1.0) }),
        );
        assert!(!executor.knn_index().has_vector(3));
    }
}
