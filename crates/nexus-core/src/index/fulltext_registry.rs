//! Named full-text index registry (phase6_opencypher-fulltext-search).
//!
//! Sits on top of the pre-existing `crate::index::fulltext::FullTextIndex`
//! (one-directory Tantivy wrapper) and adds:
//!
//! - **Named indexes** keyed by user-supplied string, matching
//!   Neo4j's `db.index.fulltext.*` shape.
//! - **Per-index metadata** (labels / properties / analyzer /
//!   refresh_ms / entity type) so `db.indexes()` can report the
//!   full constraint-catalogue row without probing Tantivy.
//! - **Thread-safe** registry (`Arc<RwLock<...>>`) suitable for
//!   calling from the executor's procedure dispatch path.
//!
//! The registry does not own WAL integration — that's a separate
//! subsystem, tracked as a follow-up. On rebuild the caller is
//! expected to re-enqueue the dataset through `add_node_document`.

use super::fulltext::{DocumentParams, FullTextIndex, SearchOptions};
use crate::{Error, Result};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Entity scope for a full-text index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FullTextEntity {
    Node,
    Relationship,
}

/// Metadata record for a named full-text index. Mirrors the shape
/// `db.indexes()` emits for FULLTEXT rows.
#[derive(Debug, Clone)]
pub struct FullTextIndexMeta {
    pub name: String,
    pub entity: FullTextEntity,
    pub labels_or_types: Vec<String>,
    pub properties: Vec<String>,
    pub analyzer: String,
    pub refresh_ms: u64,
    pub top_k: usize,
    pub path: PathBuf,
}

impl Default for FullTextIndexMeta {
    fn default() -> Self {
        Self {
            name: String::new(),
            entity: FullTextEntity::Node,
            labels_or_types: Vec::new(),
            properties: Vec::new(),
            analyzer: "standard".to_string(),
            refresh_ms: 1000,
            top_k: 100,
            path: PathBuf::new(),
        }
    }
}

/// A registered named full-text index.
pub struct NamedFullTextIndex {
    pub meta: FullTextIndexMeta,
    pub index: Arc<FullTextIndex>,
}

/// Thread-safe registry of named full-text indexes.
#[derive(Clone, Default)]
pub struct FullTextRegistry {
    inner: Arc<RwLock<HashMap<String, Arc<NamedFullTextIndex>>>>,
    base_dir: Arc<RwLock<Option<PathBuf>>>,
}

impl FullTextRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the base directory under which each index lives in its
    /// own `<name>` subdirectory. Must be called before any create.
    pub fn set_base_dir(&self, dir: PathBuf) {
        *self.base_dir.write() = Some(dir);
    }

    fn resolve_base(&self) -> Result<PathBuf> {
        self.base_dir
            .read()
            .clone()
            .ok_or_else(|| Error::storage("FTS base_dir not configured".to_string()))
    }

    /// Register a new node-scoped FTS index. Reuses existing
    /// directory state if the name was registered before.
    pub fn create_node_index(
        &self,
        name: &str,
        labels: &[&str],
        properties: &[&str],
        analyzer: Option<&str>,
    ) -> Result<()> {
        self.create_index_inner(name, FullTextEntity::Node, labels, properties, analyzer)
    }

    /// Relationship-scoped variant.
    pub fn create_relationship_index(
        &self,
        name: &str,
        types: &[&str],
        properties: &[&str],
        analyzer: Option<&str>,
    ) -> Result<()> {
        self.create_index_inner(
            name,
            FullTextEntity::Relationship,
            types,
            properties,
            analyzer,
        )
    }

    fn create_index_inner(
        &self,
        name: &str,
        entity: FullTextEntity,
        labels_or_types: &[&str],
        properties: &[&str],
        analyzer: Option<&str>,
    ) -> Result<()> {
        // Name uniqueness — cross-kind within the registry.
        if self.inner.read().contains_key(name) {
            return Err(Error::storage(format!(
                "ERR_FTS_INDEX_EXISTS: index {name:?} already registered",
            )));
        }
        if labels_or_types.is_empty() {
            return Err(Error::storage(
                "ERR_FTS_INDEX_INVALID: at least one label/type required".to_string(),
            ));
        }
        if properties.is_empty() {
            return Err(Error::storage(
                "ERR_FTS_INDEX_INVALID: at least one property required".to_string(),
            ));
        }

        let base = self.resolve_base()?;
        let dir = base.join(name);
        let index = Arc::new(FullTextIndex::new(&dir)?);

        let meta = FullTextIndexMeta {
            name: name.to_string(),
            entity,
            labels_or_types: labels_or_types.iter().map(|s| s.to_string()).collect(),
            properties: properties.iter().map(|s| s.to_string()).collect(),
            analyzer: analyzer.unwrap_or("standard").to_string(),
            refresh_ms: 1000,
            top_k: 100,
            path: dir,
        };
        self.inner.write().insert(
            name.to_string(),
            Arc::new(NamedFullTextIndex { meta, index }),
        );
        Ok(())
    }

    /// Drop an index: remove from registry + best-effort filesystem
    /// cleanup. Returns `Ok(false)` if the name didn't exist.
    pub fn drop_index(&self, name: &str) -> Result<bool> {
        let removed = self.inner.write().remove(name);
        let Some(entry) = removed else {
            return Ok(false);
        };
        // Best-effort delete — ignore errors (test-isolation paths
        // may already have the directory gone).
        let _ = std::fs::remove_dir_all(&entry.meta.path);
        Ok(true)
    }

    /// Borrow a registered index.
    pub fn get(&self, name: &str) -> Option<Arc<NamedFullTextIndex>> {
        self.inner.read().get(name).cloned()
    }

    /// Enumerate every registered index for reporting through
    /// `db.indexes()`.
    pub fn list(&self) -> Vec<FullTextIndexMeta> {
        self.inner.read().values().map(|e| e.meta.clone()).collect()
    }

    /// Run a BM25 search against the named index.
    pub fn query(
        &self,
        name: &str,
        query: &str,
        limit: Option<usize>,
    ) -> Result<Vec<super::fulltext::SearchResult>> {
        let entry = self
            .get(name)
            .ok_or_else(|| Error::storage(format!("ERR_FTS_INDEX_NOT_FOUND: {name:?}")))?;
        let options = SearchOptions {
            limit: Some(limit.unwrap_or(entry.meta.top_k)),
            ..Default::default()
        };
        entry.index.search(query, options)
    }

    /// Add a node document to the named index. Property values are
    /// concatenated into the single `content` field that the
    /// underlying `FullTextIndex` already maintains.
    pub fn add_node_document(
        &self,
        name: &str,
        node_id: u64,
        label_id: u32,
        key_id: u32,
        text: &str,
    ) -> Result<()> {
        let entry = self
            .get(name)
            .ok_or_else(|| Error::storage(format!("ERR_FTS_INDEX_NOT_FOUND: {name:?}")))?;
        entry.index.add_document(DocumentParams {
            node_id,
            label_id,
            key_id,
            content: text.to_string(),
            value: text.to_string(),
            language: None,
            boost: None,
        })?;
        Ok(())
    }

    /// Names known to the registry — used by duplicate-detection at
    /// creation time and by the `db.indexes()` procedure.
    pub fn names(&self) -> Vec<String> {
        self.inner.read().keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn fresh_registry() -> (FullTextRegistry, TempDir) {
        let reg = FullTextRegistry::new();
        let dir = TempDir::new().unwrap();
        reg.set_base_dir(dir.path().to_path_buf());
        (reg, dir)
    }

    #[test]
    fn create_registers_metadata() {
        let (reg, _dir) = fresh_registry();
        reg.create_node_index("movies", &["Movie"], &["title", "overview"], None)
            .unwrap();
        let metas = reg.list();
        assert_eq!(metas.len(), 1);
        assert_eq!(metas[0].name, "movies");
        assert_eq!(metas[0].entity, FullTextEntity::Node);
        assert_eq!(metas[0].labels_or_types, vec!["Movie".to_string()]);
        assert_eq!(
            metas[0].properties,
            vec!["title".to_string(), "overview".to_string()]
        );
        assert_eq!(metas[0].analyzer, "standard");
    }

    #[test]
    fn duplicate_name_rejected() {
        let (reg, _dir) = fresh_registry();
        reg.create_node_index("x", &["A"], &["p"], None).unwrap();
        let err = reg
            .create_node_index("x", &["B"], &["q"], None)
            .unwrap_err();
        assert!(err.to_string().contains("ERR_FTS_INDEX_EXISTS"));
    }

    #[test]
    fn drop_removes_from_registry() {
        let (reg, _dir) = fresh_registry();
        reg.create_node_index("gone", &["X"], &["p"], None).unwrap();
        assert!(reg.drop_index("gone").unwrap());
        assert!(reg.list().is_empty());
        // dropping again is a no-op
        assert!(!reg.drop_index("gone").unwrap());
    }

    #[test]
    fn empty_label_or_property_list_rejected() {
        let (reg, _dir) = fresh_registry();
        assert!(reg.create_node_index("bad", &[], &["p"], None).is_err());
        assert!(reg.create_node_index("bad2", &["L"], &[], None).is_err());
    }

    #[test]
    fn query_missing_index_errors() {
        let (reg, _dir) = fresh_registry();
        let err = reg.query("ghost", "anything", None).unwrap_err();
        assert!(err.to_string().contains("ERR_FTS_INDEX_NOT_FOUND"));
    }

    #[test]
    fn add_then_query_roundtrip() {
        let (reg, _dir) = fresh_registry();
        reg.create_node_index("docs", &["Doc"], &["body"], None)
            .unwrap();
        reg.add_node_document("docs", 1, 0, 0, "the quick brown fox")
            .unwrap();
        reg.add_node_document("docs", 2, 0, 0, "sleepy cat on a mat")
            .unwrap();
        let results = reg.query("docs", "fox", None).unwrap();
        assert!(
            results.iter().any(|r| r.node_id == 1),
            "expected node 1 in results, got {results:?}"
        );
    }
}
