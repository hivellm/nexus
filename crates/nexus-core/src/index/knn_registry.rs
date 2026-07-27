//! Vector (HNSW/KNN) index registry — single-active-index owner
//! (phase20_knn-write-path-wiring §1.2).
//!
//! Mirrors the concurrency shape of [`crate::index::rtree::RTreeRegistry`]:
//! a `parking_lot::RwLock` guarding the registered state, `Error::index`
//! for conflicts, and a `definitions()` / `contains()` / `drop_index()`
//! surface the write-path hooks can call without re-parsing anything.
//!
//! ## Design constraint — one active index
//!
//! Unlike the R-tree or full-text registries, which happily hold many
//! independent indexes, the KNN engine backs every registered vector
//! index with a single global [`crate::index::knn_index::KnnIndex`]
//! graph. Registering a second, *different* `(name, label, property)`
//! definition without `replace = true` is therefore a conflict, not a
//! new independent index. Re-registering the exact same definition is
//! idempotent so callers (e.g. `CREATE VECTOR INDEX ... IF NOT EXISTS`)
//! don't need to special-case "did I already do this".
//!
//! ## Concurrency
//!
//! `VectorIndexRegistry` is `Send + Sync`. The single `RwLock<Option<..>>`
//! slot is cheap to read (`contains` / `definitions` / `indexes_containing`
//! all take the read lock) and mutations (`register` / `drop_index`) take
//! the write lock briefly, matching `RTreeRegistry`'s locking granularity.

use parking_lot::RwLock;

use crate::{Error, Result};

/// A single registered vector index definition: the index name plus the
/// `(label, property)` pair whose values feed the global HNSW graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VectorIndexDefinition {
    /// The index name (e.g. `"embedding_idx"`).
    pub name: String,
    /// The label this index covers (e.g. `"Document"`).
    pub label: String,
    /// The property key this index covers (e.g. `"embedding"`).
    pub property: String,
}

/// Registry for the single active vector index. See the module docs for
/// why only one definition can be registered at a time.
#[derive(Default, Debug)]
pub struct VectorIndexRegistry {
    inner: RwLock<Option<VectorIndexDefinition>>,
}

impl VectorIndexRegistry {
    /// Empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `name` over `(label, property)`.
    ///
    /// * No index registered yet — always succeeds.
    /// * Same `(name, label, property)` already registered — idempotent
    ///   no-op success.
    /// * A *different* index already registered and `replace == false` —
    ///   returns `Err(Error::index(..))`; only one active vector index
    ///   is allowed at a time (single global HNSW graph).
    /// * A *different* index already registered and `replace == true` —
    ///   the old definition is discarded and `name` becomes the new
    ///   active index.
    pub fn register(&self, name: &str, label: &str, property: &str, replace: bool) -> Result<()> {
        let mut guard = self.inner.write();
        if let Some(existing) = guard.as_ref() {
            let same =
                existing.name == name && existing.label == label && existing.property == property;
            if same {
                return Ok(());
            }
            if !replace {
                return Err(Error::index(format!(
                    "ERR_VECTOR_INDEX_EXISTS: vector index {:?} already registered on {}.{} \
                     (only one active vector index is allowed at a time; pass replace to swap it)",
                    existing.name, existing.label, existing.property
                )));
            }
        }
        *guard = Some(VectorIndexDefinition {
            name: name.to_string(),
            label: label.to_string(),
            property: property.to_string(),
        });
        Ok(())
    }

    /// Snapshot the registered `(name, label, property)` tuples. At most
    /// one entry, since only one vector index can be active.
    pub fn definitions(&self) -> Vec<(String, String, String)> {
        self.inner
            .read()
            .as_ref()
            .map(|def| vec![(def.name.clone(), def.label.clone(), def.property.clone())])
            .unwrap_or_default()
    }

    /// `true` iff `name` is the currently registered active index.
    pub fn contains(&self, name: &str) -> bool {
        self.inner
            .read()
            .as_ref()
            .is_some_and(|def| def.name == name)
    }

    /// Drop the active index if it is named `name`. Returns `true` when
    /// an index existed under that name and was removed.
    pub fn drop_index(&self, name: &str) -> bool {
        let mut guard = self.inner.write();
        let matches = guard.as_ref().is_some_and(|def| def.name == name);
        if matches {
            *guard = None;
        }
        matches
    }

    /// Return every registered definition whose `label` is in `labels`
    /// and whose `property` is in `properties`. Used by the write-path
    /// hooks (autopopulate / refresh / evict) to decide whether a
    /// created or updated node's labels and touched property keys match
    /// the active vector index without re-reading the whole property
    /// list against every possible index.
    pub fn indexes_containing(
        &self,
        labels: &[&str],
        properties: &[&str],
    ) -> Vec<VectorIndexDefinition> {
        self.inner
            .read()
            .as_ref()
            .filter(|def| {
                labels.contains(&def.label.as_str()) && properties.contains(&def.property.as_str())
            })
            .cloned()
            .into_iter()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_registry_has_no_indexes() {
        let reg = VectorIndexRegistry::new();
        assert!(reg.definitions().is_empty());
        assert!(!reg.contains("embedding_idx"));
    }

    #[test]
    fn register_succeeds_on_empty_registry() {
        let reg = VectorIndexRegistry::new();
        reg.register("embedding_idx", "Document", "embedding", false)
            .unwrap();
        assert!(reg.contains("embedding_idx"));
        assert_eq!(
            reg.definitions(),
            vec![(
                "embedding_idx".to_string(),
                "Document".to_string(),
                "embedding".to_string()
            )]
        );
    }

    #[test]
    fn register_distinct_index_without_replace_errors() {
        let reg = VectorIndexRegistry::new();
        reg.register("embedding_idx", "Document", "embedding", false)
            .unwrap();
        let err = reg
            .register("other_idx", "Chunk", "vector", false)
            .unwrap_err();
        assert!(err.to_string().contains("ERR_VECTOR_INDEX_EXISTS"));
        // The original registration is untouched.
        assert!(reg.contains("embedding_idx"));
        assert!(!reg.contains("other_idx"));
    }

    #[test]
    fn register_same_definition_again_is_idempotent() {
        let reg = VectorIndexRegistry::new();
        reg.register("embedding_idx", "Document", "embedding", false)
            .unwrap();
        reg.register("embedding_idx", "Document", "embedding", false)
            .unwrap();
        assert_eq!(reg.definitions().len(), 1);
    }

    #[test]
    fn register_with_replace_swaps_active_index() {
        let reg = VectorIndexRegistry::new();
        reg.register("embedding_idx", "Document", "embedding", false)
            .unwrap();
        reg.register("other_idx", "Chunk", "vector", true).unwrap();
        assert!(!reg.contains("embedding_idx"));
        assert!(reg.contains("other_idx"));
        assert_eq!(
            reg.definitions(),
            vec![(
                "other_idx".to_string(),
                "Chunk".to_string(),
                "vector".to_string()
            )]
        );
    }

    #[test]
    fn drop_index_removes_active_index() {
        let reg = VectorIndexRegistry::new();
        reg.register("embedding_idx", "Document", "embedding", false)
            .unwrap();
        assert!(reg.drop_index("embedding_idx"));
        assert!(!reg.contains("embedding_idx"));
        assert!(reg.definitions().is_empty());
    }

    #[test]
    fn drop_index_unknown_name_returns_false() {
        let reg = VectorIndexRegistry::new();
        reg.register("embedding_idx", "Document", "embedding", false)
            .unwrap();
        assert!(!reg.drop_index("nonexistent"));
        // The active index is untouched.
        assert!(reg.contains("embedding_idx"));
    }

    #[test]
    fn indexes_containing_matches_registered_label_and_property() {
        let reg = VectorIndexRegistry::new();
        reg.register("embedding_idx", "Document", "embedding", false)
            .unwrap();

        let hits = reg.indexes_containing(&["Document", "Person"], &["embedding", "name"]);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "embedding_idx");

        // No match on an unrelated label.
        let no_hits = reg.indexes_containing(&["Chunk"], &["embedding"]);
        assert!(no_hits.is_empty());

        // No match on an unrelated property.
        let no_hits2 = reg.indexes_containing(&["Document"], &["vector"]);
        assert!(no_hits2.is_empty());
    }

    #[test]
    fn indexes_containing_empty_registry_returns_empty() {
        let reg = VectorIndexRegistry::new();
        assert!(
            reg.indexes_containing(&["Document"], &["embedding"])
                .is_empty()
        );
    }
}
