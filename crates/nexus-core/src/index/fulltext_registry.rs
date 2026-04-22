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
use super::fulltext_analyzer::resolve as resolve_analyzer;
use super::fulltext_writer::{WriterCommand, WriterConfig, WriterHandle};
use crate::{Error, Result};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// On-disk sidecar persisted alongside each FTS index directory.
/// Written to `<index_dir>/_meta.json` on create and read back by
/// `FullTextRegistry::load_from_disk` at engine startup so the
/// catalogue survives restarts without requiring a WAL replay.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedMeta {
    name: String,
    entity: u8, // 0 = Node, 1 = Relationship — stable on-disk format
    labels_or_types: Vec<String>,
    properties: Vec<String>,
    analyzer: String,
    refresh_ms: u64,
    top_k: usize,
}

/// Recover `(min, max)` from a display string like `"ngram(3,5)"`.
/// Returns `None` when the shape does not match so the caller can
/// fall back to bare-name resolution.
fn parse_ngram_display(s: &str) -> Option<(usize, usize)> {
    let rest = s.strip_prefix("ngram(")?.strip_suffix(')')?;
    let (lo, hi) = rest.split_once(',')?;
    let lo = lo.trim().parse::<usize>().ok()?;
    let hi = hi.trim().parse::<usize>().ok()?;
    Some((lo, hi))
}

impl PersistedMeta {
    fn from_runtime(meta: &FullTextIndexMeta) -> Self {
        Self {
            name: meta.name.clone(),
            entity: match meta.entity {
                FullTextEntity::Node => 0,
                FullTextEntity::Relationship => 1,
            },
            labels_or_types: meta.labels_or_types.clone(),
            properties: meta.properties.clone(),
            analyzer: meta.analyzer.clone(),
            refresh_ms: meta.refresh_ms,
            top_k: meta.top_k,
        }
    }

    fn entity_runtime(&self) -> Result<FullTextEntity> {
        match self.entity {
            0 => Ok(FullTextEntity::Node),
            1 => Ok(FullTextEntity::Relationship),
            other => Err(Error::storage(format!(
                "ERR_FTS_META_CORRUPT: unknown entity discriminant {other}"
            ))),
        }
    }
}

/// Entity scope for a full-text index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FullTextEntity {
    Node,
    Relationship,
}

/// Config payload for picking (and parameterising) the analyzer of
/// a new full-text index. Produced by the procedure dispatcher from
/// the `config` map argument of `db.index.fulltext.createNodeIndex`.
#[derive(Debug, Clone)]
pub struct AnalyzerConfig {
    /// Catalogued analyzer name — e.g. `"standard"`, `"ngram"`.
    pub name: String,
    /// Lower bound for the `ngram` analyzer. Ignored otherwise.
    pub ngram_min: Option<usize>,
    /// Upper bound for the `ngram` analyzer. Ignored otherwise.
    pub ngram_max: Option<usize>,
}

impl AnalyzerConfig {
    /// Build from a bare analyzer name; defaults to `"standard"`.
    pub fn of_name(name: Option<&str>) -> Self {
        Self {
            name: name.unwrap_or("standard").to_string(),
            ngram_min: None,
            ngram_max: None,
        }
    }
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
    /// In-memory set of entity ids currently indexed. Tracked so
    /// the SET / REMOVE / DELETE refresh paths can enumerate every
    /// index a node belongs to without re-checking label
    /// membership — the engine and executor label indexes can drift
    /// across `refresh_executor` cycles, so FTS maintains its own
    /// authoritative view.
    pub members: Arc<RwLock<std::collections::HashSet<u64>>>,
    /// Optional async writer (phase6_fulltext-async-writer). When
    /// spawned, hot-path `add_node_document` / `add_node_documents_bulk`
    /// / `remove_entity` enqueue onto the writer's channel; the
    /// background thread batches + commits on `refresh_ms` cadence
    /// or at `max_batch_size`. When `None`, writes fall through to
    /// the synchronous Tantivy-commit path.
    pub writer: RwLock<Option<Arc<WriterHandle>>>,
}

impl NamedFullTextIndex {
    fn new(meta: FullTextIndexMeta, index: Arc<FullTextIndex>) -> Self {
        Self {
            meta,
            index,
            members: Arc::new(RwLock::new(std::collections::HashSet::new())),
            writer: RwLock::new(None),
        }
    }

    /// Snapshot of the current writer handle, if spawned. Cheap
    /// clone of the `Arc` — held only for the duration of the
    /// enqueue so a concurrent `shutdown_writer` can still swap it
    /// out.
    pub fn writer_handle(&self) -> Option<Arc<WriterHandle>> {
        self.writer.read().clone()
    }

    /// Spawn a background writer using `cfg`. Idempotent — a second
    /// call shuts the previous writer down (flushing its outstanding
    /// buffer) before replacing it.
    pub fn spawn_writer(&self, cfg: WriterConfig) {
        let mut slot = self.writer.write();
        // Drop any prior writer first so its drain runs before we
        // swap in the replacement. Dropping while holding the slot
        // is safe because `WriterHandle::drop` only touches its own
        // thread state.
        slot.take();
        let handle = WriterHandle::spawn(self.index.clone(), cfg);
        *slot = Some(Arc::new(handle));
    }

    /// Flush the async writer (if any) and tear it down. The sync
    /// fallback is re-enabled for every hot-path write made after
    /// this returns. No-op when no writer is registered.
    pub fn shutdown_writer(&self) -> Result<()> {
        let Some(handle) = self.writer.write().take() else {
            return Ok(());
        };
        // Attempt a best-effort flush before the drop signal fires.
        // `flush_blocking` returns cleanly if the writer already
        // drained, so the combined flush + drop sequence is safe to
        // invoke even when the writer just shut itself down.
        let _ = handle.flush_blocking();
        // Dropping the Arc here would still leave it live if another
        // clone is in flight. Extract the inner handle via
        // `Arc::try_unwrap` so the drop side-effects (graceful
        // shutdown) run deterministically.
        match Arc::try_unwrap(handle) {
            Ok(h) => drop(h),
            Err(arc) => drop(arc),
        }
        Ok(())
    }
}

/// Thread-safe registry of named full-text indexes.
#[derive(Clone, Default)]
pub struct FullTextRegistry {
    inner: Arc<RwLock<HashMap<String, Arc<NamedFullTextIndex>>>>,
    base_dir: Arc<RwLock<Option<PathBuf>>>,
    /// phase6_fulltext-async-writer: master switch read at
    /// create / reload time. When `true`, every registered index
    /// gets a background writer; when `false`, hot paths commit
    /// inline against the synchronous `FullTextIndex` API.
    async_writers_enabled: Arc<RwLock<bool>>,
}

/// Build a [`WriterConfig`] from a registered index's metadata.
/// `refresh_ms` comes straight from the meta (so the sidecar is the
/// authority); channel capacity and batch size fall back to the
/// constants advertised by the writer module.
fn default_writer_cfg(meta: &FullTextIndexMeta) -> WriterConfig {
    let refresh_ms = if meta.refresh_ms == 0 {
        super::fulltext_writer::DEFAULT_REFRESH_MS
    } else {
        meta.refresh_ms
    };
    WriterConfig {
        channel_capacity: super::fulltext_writer::DEFAULT_CHANNEL_CAPACITY,
        refresh: Duration::from_millis(refresh_ms),
        max_batch_size: super::fulltext_writer::DEFAULT_MAX_BATCH,
    }
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
        self.create_index_with_config(
            name,
            FullTextEntity::Node,
            labels,
            properties,
            AnalyzerConfig::of_name(analyzer),
        )
    }

    /// Relationship-scoped variant.
    pub fn create_relationship_index(
        &self,
        name: &str,
        types: &[&str],
        properties: &[&str],
        analyzer: Option<&str>,
    ) -> Result<()> {
        self.create_index_with_config(
            name,
            FullTextEntity::Relationship,
            types,
            properties,
            AnalyzerConfig::of_name(analyzer),
        )
    }

    /// Node-scoped create with a fully populated [`AnalyzerConfig`]
    /// (name + optional ngram sizes). Called by the `db.index.
    /// fulltext.createNodeIndex(..., config)` procedure once the
    /// `config` map has been unpacked.
    pub fn create_node_index_with_config(
        &self,
        name: &str,
        labels: &[&str],
        properties: &[&str],
        config: AnalyzerConfig,
    ) -> Result<()> {
        self.create_index_with_config(name, FullTextEntity::Node, labels, properties, config)
    }

    /// Relationship-scoped variant of
    /// [`create_node_index_with_config`].
    pub fn create_relationship_index_with_config(
        &self,
        name: &str,
        types: &[&str],
        properties: &[&str],
        config: AnalyzerConfig,
    ) -> Result<()> {
        self.create_index_with_config(
            name,
            FullTextEntity::Relationship,
            types,
            properties,
            config,
        )
    }

    fn create_index_with_config(
        &self,
        name: &str,
        entity: FullTextEntity,
        labels_or_types: &[&str],
        properties: &[&str],
        config: AnalyzerConfig,
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

        let analyzer_kind = resolve_analyzer(&config.name, config.ngram_min, config.ngram_max)?;
        let analyzer_display = analyzer_kind.display_name();

        let base = self.resolve_base()?;
        let dir = base.join(name);
        let index = Arc::new(FullTextIndex::with_analyzer(&dir, analyzer_kind)?);

        let meta = FullTextIndexMeta {
            name: name.to_string(),
            entity,
            labels_or_types: labels_or_types.iter().map(|s| s.to_string()).collect(),
            properties: properties.iter().map(|s| s.to_string()).collect(),
            analyzer: analyzer_display,
            refresh_ms: 1000,
            top_k: 100,
            path: dir.clone(),
        };
        // Persist to disk so restarts rebuild the registry without
        // requiring a full WAL replay. Written before the in-memory
        // insert so a crash between filesystem + process state leaves
        // the index discoverable on startup.
        Self::write_meta_sidecar(&dir, &meta)?;
        let entry = Arc::new(NamedFullTextIndex::new(meta, index));
        // Honour the registry-wide async-writer switch: once
        // `enable_async_writers` has been invoked, every subsequent
        // create also spins up a writer so the hot path never falls
        // back to the sync commit cost.
        if *self.async_writers_enabled.read() {
            entry.spawn_writer(default_writer_cfg(&entry.meta));
        }
        self.inner.write().insert(name.to_string(), entry);
        Ok(())
    }

    fn write_meta_sidecar(dir: &std::path::Path, meta: &FullTextIndexMeta) -> Result<()> {
        let persisted = PersistedMeta::from_runtime(meta);
        let bytes = serde_json::to_vec_pretty(&persisted)?;
        let sidecar = dir.join("_meta.json");
        // Atomic replace via tmp-then-rename so a crash mid-write
        // cannot leave a truncated sidecar behind.
        let tmp = dir.join("_meta.json.tmp");
        std::fs::write(&tmp, &bytes)?;
        std::fs::rename(&tmp, &sidecar)?;
        Ok(())
    }

    fn read_meta_sidecar(dir: &std::path::Path) -> Result<Option<PersistedMeta>> {
        let sidecar = dir.join("_meta.json");
        if !sidecar.exists() {
            return Ok(None);
        }
        let bytes = std::fs::read(&sidecar)?;
        let persisted: PersistedMeta = serde_json::from_slice(&bytes)?;
        Ok(Some(persisted))
    }

    /// Rebuild the in-memory registry from on-disk state. Scans the
    /// base directory for index subdirectories, loads each
    /// `_meta.json` sidecar, and re-opens the Tantivy index with the
    /// catalogued analyzer.
    ///
    /// Idempotent — already-loaded indexes are skipped. Malformed
    /// sidecars (missing / unparseable / unknown analyzer) are
    /// logged and skipped rather than aborting the whole rebuild so
    /// a single corrupt directory cannot break the boot path.
    pub fn load_from_disk(&self) -> Result<usize> {
        let base = self.resolve_base()?;
        if !base.exists() {
            return Ok(0);
        }
        let mut loaded = 0usize;
        let mut inner = self.inner.write();
        for entry in std::fs::read_dir(&base)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let dir = entry.path();
            let persisted = match Self::read_meta_sidecar(&dir) {
                Ok(Some(p)) => p,
                Ok(None) => continue,
                Err(e) => {
                    tracing::warn!("FTS: skipping {dir:?}: {e}");
                    continue;
                }
            };
            if inner.contains_key(&persisted.name) {
                continue;
            }
            let analyzer_kind = match resolve_analyzer(&persisted.analyzer, None, None) {
                Ok(k) => k,
                Err(e) => {
                    // Parameterised ngram (e.g. `ngram(3,5)`) round-
                    // trips through `display_name`; strip back to the
                    // canonical form before re-resolving.
                    match parse_ngram_display(&persisted.analyzer) {
                        Some((min, max)) => resolve_analyzer("ngram", Some(min), Some(max))?,
                        None => {
                            tracing::warn!(
                                "FTS: unknown analyzer {:?} for index {:?}: {e}",
                                persisted.analyzer,
                                persisted.name
                            );
                            continue;
                        }
                    }
                }
            };
            let index = Arc::new(FullTextIndex::with_analyzer(&dir, analyzer_kind)?);
            let meta = FullTextIndexMeta {
                name: persisted.name.clone(),
                entity: persisted.entity_runtime()?,
                labels_or_types: persisted.labels_or_types.clone(),
                properties: persisted.properties.clone(),
                analyzer: persisted.analyzer.clone(),
                refresh_ms: persisted.refresh_ms,
                top_k: persisted.top_k,
                path: dir,
            };
            let entry = Arc::new(NamedFullTextIndex::new(meta, index));
            if *self.async_writers_enabled.read() {
                entry.spawn_writer(default_writer_cfg(&entry.meta));
            }
            inner.insert(persisted.name.clone(), entry);
            loaded += 1;
        }
        Ok(loaded)
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
    ///
    /// When the per-index async writer is spawned
    /// (phase6_fulltext-async-writer), the document is enqueued onto
    /// its bounded channel and the call returns after incrementing
    /// the membership set. The commit + reader-reload happen in the
    /// background on the configured `refresh_ms` cadence or once the
    /// batch-size threshold is hit. Without the writer the call
    /// takes the synchronous commit path — identical to the original
    /// behaviour that every test predating this task asserts.
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
        if let Some(writer) = entry.writer_handle() {
            writer.enqueue(WriterCommand::Add {
                node_id,
                label_id,
                key_id,
                content: text.to_string(),
            })?;
        } else {
            entry.index.add_document(DocumentParams {
                node_id,
                label_id,
                key_id,
                content: text.to_string(),
                value: text.to_string(),
                language: None,
                boost: None,
            })?;
        }
        entry.members.write().insert(node_id);
        Ok(())
    }

    /// Apply a single FTS-shaped WAL entry against this registry.
    /// Used by the crash-recovery dispatcher in `wal::recover_fts`
    /// (phase6_fulltext-wal-integration §1.3 + §5.1-§5.2).
    ///
    /// Returns `Ok(false)` when the entry is not FTS-shaped — callers
    /// loop over all recovered entries and feed every one through.
    /// FTS ops on an index name that no longer exists after replay
    /// (e.g. an `FtsAdd` followed by `FtsDropIndex` in the same
    /// segment) are silently skipped rather than aborting recovery.
    pub fn apply_wal_entry(&self, entry: &crate::wal::WalEntry) -> Result<bool> {
        use crate::wal::WalEntry;
        match entry {
            WalEntry::FtsCreateIndex {
                name,
                entity,
                labels_or_types,
                properties,
                analyzer,
            } => {
                if self.inner.read().contains_key(name) {
                    return Ok(true);
                }
                let config = match parse_ngram_display(analyzer) {
                    Some((min, max)) => AnalyzerConfig {
                        name: "ngram".to_string(),
                        ngram_min: Some(min),
                        ngram_max: Some(max),
                    },
                    None => AnalyzerConfig {
                        name: analyzer.clone(),
                        ngram_min: None,
                        ngram_max: None,
                    },
                };
                let labels_str: Vec<&str> = labels_or_types.iter().map(|s| s.as_str()).collect();
                let props_str: Vec<&str> = properties.iter().map(|s| s.as_str()).collect();
                let entity_kind = match entity {
                    0 => FullTextEntity::Node,
                    1 => FullTextEntity::Relationship,
                    other => {
                        return Err(Error::storage(format!(
                            "ERR_FTS_WAL_CORRUPT: unknown entity discriminant {other}"
                        )));
                    }
                };
                self.create_index_with_config(name, entity_kind, &labels_str, &props_str, config)?;
                Ok(true)
            }
            WalEntry::FtsDropIndex { name } => {
                let _ = self.drop_index(name)?;
                Ok(true)
            }
            WalEntry::FtsAdd {
                name,
                entity_id,
                label_or_type_id,
                key_id,
                content,
            } => {
                if self.get(name).is_some() {
                    self.add_node_document(name, *entity_id, *label_or_type_id, *key_id, content)?;
                }
                Ok(true)
            }
            WalEntry::FtsDel { name, entity_id } => {
                if let Some(entry) = self.get(name) {
                    entry.index.remove_document(*entity_id, 0, 0)?;
                    entry.members.write().remove(entity_id);
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// List every registered index name that currently contains the
    /// given entity id. Used by the SET / REMOVE / DELETE refresh
    /// paths to find matching indexes without consulting the
    /// (potentially stale) engine-level label index.
    pub fn indexes_containing(&self, entity_id: u64) -> Vec<String> {
        let mut out = Vec::new();
        for (name, entry) in self.inner.read().iter() {
            if entry.members.read().contains(&entity_id) {
                out.push(name.clone());
            }
        }
        out
    }

    /// Remove the given entity from the named index's Tantivy
    /// backend + membership set. No-op when the index is unknown
    /// (mirrors `apply_wal_entry`'s forgiveness policy). Does not
    /// touch the WAL — callers enqueue the matching `FtsDel` entry.
    ///
    /// Routes through the async writer when present — the delete is
    /// batched with any pending adds and committed together on the
    /// next cadence tick. The membership set is updated
    /// synchronously so `indexes_containing` stops reporting the
    /// entity immediately.
    pub fn remove_entity(&self, name: &str, entity_id: u64) -> Result<()> {
        let Some(entry) = self.get(name) else {
            return Ok(());
        };
        if let Some(writer) = entry.writer_handle() {
            writer.enqueue(WriterCommand::Del { node_id: entity_id })?;
        } else {
            entry.index.remove_document(entity_id, 0, 0)?;
        }
        entry.members.write().remove(&entity_id);
        Ok(())
    }

    /// Bulk ingest variant of [`add_node_document`]. Opens a single
    /// Tantivy writer, pushes every tuple, commits once. Required
    /// for meaningful bench throughput and for bulk-load scripts;
    /// the per-doc path commits on every call.
    ///
    /// When the async writer is live, each tuple is enqueued
    /// individually so mid-call crashes leave the WAL + Tantivy
    /// states consistent at every batch boundary; the writer
    /// already amortises the commit cost across up to
    /// `max_batch_size` docs per segment flush. Without the writer,
    /// the path takes the original single-writer sync commit.
    pub fn add_node_documents_bulk(
        &self,
        name: &str,
        docs: &[(u64, u32, u32, &str)],
    ) -> Result<()> {
        let entry = self
            .get(name)
            .ok_or_else(|| Error::storage(format!("ERR_FTS_INDEX_NOT_FOUND: {name:?}")))?;
        if let Some(writer) = entry.writer_handle() {
            for (node_id, label_id, key_id, content) in docs {
                writer.enqueue(WriterCommand::Add {
                    node_id: *node_id,
                    label_id: *label_id,
                    key_id: *key_id,
                    content: (*content).to_string(),
                })?;
            }
        } else {
            entry.index.add_documents_bulk(docs)?;
        }
        {
            let mut members = entry.members.write();
            for (node_id, _, _, _) in docs {
                members.insert(*node_id);
            }
        }
        Ok(())
    }

    /// Names known to the registry — used by duplicate-detection at
    /// creation time and by the `db.indexes()` procedure.
    pub fn names(&self) -> Vec<String> {
        self.inner.read().keys().cloned().collect()
    }

    /// Enable async writers registry-wide. Spawns a background
    /// writer for every currently registered index (using each
    /// index's `refresh_ms`) and flips the "spawn on create" flag
    /// so future `create_index_with_config` + `load_from_disk`
    /// calls do the same.
    ///
    /// Idempotent — indexes that already own a writer keep it.
    pub fn enable_async_writers(&self) {
        *self.async_writers_enabled.write() = true;
        for entry in self.inner.read().values() {
            if entry.writer_handle().is_none() {
                entry.spawn_writer(default_writer_cfg(&entry.meta));
            }
        }
    }

    /// Tear every async writer down — used by shutdown paths and
    /// by tests that want to observe the sync-fallback semantics.
    /// Each writer's drop path runs its drain + final commit before
    /// this method returns.
    pub fn disable_async_writers(&self) -> Result<()> {
        *self.async_writers_enabled.write() = false;
        for entry in self.inner.read().values() {
            entry.shutdown_writer()?;
        }
        Ok(())
    }

    /// Block until every async writer has committed every enqueued
    /// doc. No-op on indexes that never spawned a writer. Used by
    /// tests + by shutdown paths that want "the catalogue is
    /// durable" before moving on.
    pub fn flush_all(&self) -> Result<()> {
        for entry in self.inner.read().values() {
            if let Some(writer) = entry.writer_handle() {
                writer.flush_blocking()?;
            }
        }
        Ok(())
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

    #[test]
    fn unknown_analyzer_rejected_at_create_time() {
        let (reg, _dir) = fresh_registry();
        let err = reg
            .create_node_index_with_config(
                "bad",
                &["L"],
                &["p"],
                AnalyzerConfig {
                    name: "klingon".to_string(),
                    ngram_min: None,
                    ngram_max: None,
                },
            )
            .unwrap_err();
        assert!(err.to_string().contains("ERR_FTS_UNKNOWN_ANALYZER"));
    }

    #[test]
    fn ngram_analyzer_matches_substrings() {
        // With a `ngram(2,3)` analyzer, an indexed value "photograph"
        // should match a search for the substring "tog" — something a
        // whitespace-default analyzer would miss.
        let (reg, _dir) = fresh_registry();
        reg.create_node_index_with_config(
            "imgs",
            &["Image"],
            &["caption"],
            AnalyzerConfig {
                name: "ngram".to_string(),
                ngram_min: Some(2),
                ngram_max: Some(3),
            },
        )
        .unwrap();
        reg.add_node_document("imgs", 42, 0, 0, "photograph")
            .unwrap();
        let results = reg.query("imgs", "tog", None).unwrap();
        assert!(
            results.iter().any(|r| r.node_id == 42),
            "expected substring match via ngram, got {results:?}"
        );
    }

    #[test]
    fn keyword_analyzer_is_exact_match_only() {
        let (reg, _dir) = fresh_registry();
        reg.create_node_index_with_config(
            "kv",
            &["Tag"],
            &["value"],
            AnalyzerConfig {
                name: "keyword".to_string(),
                ngram_min: None,
                ngram_max: None,
            },
        )
        .unwrap();
        reg.add_node_document("kv", 7, 0, 0, "Hello World").unwrap();
        // Querying "hello" alone must NOT match: the value is stored
        // as a single token "Hello World" and keyword does not
        // lowercase.
        let partial = reg.query("kv", "hello", None).unwrap();
        assert!(
            partial.is_empty(),
            "keyword analyzer must not split tokens, got {partial:?}"
        );
        // Exact-phrase query against the keyword should hit. Tantivy
        // query parser treats a quoted string as a phrase; we supply
        // the exact token text.
        let exact = reg.query("kv", "\"Hello World\"", None).unwrap();
        assert!(
            exact.iter().any(|r| r.node_id == 7),
            "exact keyword hit missing, got {exact:?}"
        );
    }

    #[test]
    fn metadata_echoes_resolved_analyzer_name() {
        let (reg, _dir) = fresh_registry();
        reg.create_node_index_with_config(
            "story",
            &["Chapter"],
            &["text"],
            AnalyzerConfig {
                name: "ngram".to_string(),
                ngram_min: Some(3),
                ngram_max: Some(5),
            },
        )
        .unwrap();
        let meta = &reg.list()[0];
        assert_eq!(meta.analyzer, "ngram(3,5)");
    }

    #[test]
    fn english_analyzer_is_usable_end_to_end() {
        let (reg, _dir) = fresh_registry();
        reg.create_node_index_with_config(
            "blog",
            &["Post"],
            &["body"],
            AnalyzerConfig {
                name: "english".to_string(),
                ngram_min: None,
                ngram_max: None,
            },
        )
        .unwrap();
        reg.add_node_document("blog", 1, 0, 0, "running runners ran")
            .unwrap();
        // English stemmer collapses run / running / ran / runners,
        // so a query for "run" must reach the document.
        let results = reg.query("blog", "run", None).unwrap();
        assert!(
            results.iter().any(|r| r.node_id == 1),
            "english stemmer did not reduce forms, got {results:?}"
        );
    }

    // phase6_fulltext-wal-integration §2 — sidecar persistence.
    #[test]
    fn metadata_sidecar_is_written_on_create() {
        let (reg, dir) = fresh_registry();
        reg.create_node_index("persisted", &["Doc"], &["body"], Some("standard"))
            .unwrap();
        let sidecar = dir.path().join("persisted").join("_meta.json");
        assert!(sidecar.exists(), "expected _meta.json sidecar");
        let raw = std::fs::read_to_string(&sidecar).unwrap();
        assert!(raw.contains("\"persisted\""));
        assert!(raw.contains("\"standard\""));
    }

    #[test]
    fn load_from_disk_rebuilds_registry_after_drop() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path().to_path_buf();

        // First registry: create one standard + one ngram.
        let r1 = FullTextRegistry::new();
        r1.set_base_dir(base.clone());
        r1.create_node_index("std_idx", &["Doc"], &["body"], Some("standard"))
            .unwrap();
        r1.create_node_index_with_config(
            "ngr_idx",
            &["Img"],
            &["caption"],
            AnalyzerConfig {
                name: "ngram".to_string(),
                ngram_min: Some(2),
                ngram_max: Some(4),
            },
        )
        .unwrap();
        r1.add_node_document("std_idx", 1, 0, 0, "hello world")
            .unwrap();
        drop(r1);

        // Fresh registry pointed at the same base should pick up
        // both indexes and recover their ingested content.
        let r2 = FullTextRegistry::new();
        r2.set_base_dir(base);
        let loaded = r2.load_from_disk().unwrap();
        assert_eq!(loaded, 2, "expected 2 indexes restored");
        let names = r2.names();
        assert!(names.contains(&"std_idx".to_string()));
        assert!(names.contains(&"ngr_idx".to_string()));
        // Ngram analyzer display round-trips with parameters.
        let metas: std::collections::HashMap<String, String> = r2
            .list()
            .into_iter()
            .map(|m| (m.name, m.analyzer))
            .collect();
        assert_eq!(metas["std_idx"], "standard");
        assert_eq!(metas["ngr_idx"], "ngram(2,4)");
        // Content survives the restart.
        let hits = r2.query("std_idx", "hello", None).unwrap();
        assert!(hits.iter().any(|h| h.node_id == 1));
    }

    #[test]
    fn apply_wal_entry_creates_and_drops_index() {
        let (reg, _dir) = fresh_registry();
        use crate::wal::WalEntry;
        let create = WalEntry::FtsCreateIndex {
            name: "from_wal".to_string(),
            entity: 0,
            labels_or_types: vec!["Doc".to_string()],
            properties: vec!["body".to_string()],
            analyzer: "standard".to_string(),
        };
        assert!(reg.apply_wal_entry(&create).unwrap());
        assert!(reg.get("from_wal").is_some());
        // Replay is idempotent — duplicate create must not error.
        assert!(reg.apply_wal_entry(&create).unwrap());
        assert_eq!(reg.list().len(), 1);

        let add = WalEntry::FtsAdd {
            name: "from_wal".to_string(),
            entity_id: 7,
            label_or_type_id: 0,
            key_id: 0,
            content: "replayed content".to_string(),
        };
        assert!(reg.apply_wal_entry(&add).unwrap());
        let hits = reg.query("from_wal", "replayed", None).unwrap();
        assert!(hits.iter().any(|h| h.node_id == 7));

        let drop_op = WalEntry::FtsDropIndex {
            name: "from_wal".to_string(),
        };
        assert!(reg.apply_wal_entry(&drop_op).unwrap());
        assert!(reg.get("from_wal").is_none());
    }

    #[test]
    fn apply_wal_entry_skips_non_fts_ops() {
        let (reg, _dir) = fresh_registry();
        let non_fts = crate::wal::WalEntry::CreateNode {
            node_id: 1,
            label_bits: 0,
        };
        assert!(
            !reg.apply_wal_entry(&non_fts).unwrap(),
            "non-FTS ops must return false so the caller skips them"
        );
    }

    #[test]
    fn apply_wal_entry_tolerates_missing_index() {
        let (reg, _dir) = fresh_registry();
        // Add/del against an unregistered index — must not error;
        // recovery replay can reach add-before-create in corrupted
        // logs, and the committed-create path is the authority.
        let add = crate::wal::WalEntry::FtsAdd {
            name: "ghost".to_string(),
            entity_id: 1,
            label_or_type_id: 0,
            key_id: 0,
            content: "x".to_string(),
        };
        assert!(reg.apply_wal_entry(&add).unwrap());
        let del = crate::wal::WalEntry::FtsDel {
            name: "ghost".to_string(),
            entity_id: 1,
        };
        assert!(reg.apply_wal_entry(&del).unwrap());
    }

    #[test]
    fn load_from_disk_is_idempotent() {
        let (reg, _dir) = fresh_registry();
        reg.create_node_index("one", &["A"], &["p"], Some("standard"))
            .unwrap();
        // Second call must not double-insert or error.
        let loaded_again = reg.load_from_disk().unwrap();
        assert_eq!(loaded_again, 0, "already-loaded indexes must be skipped");
        assert_eq!(reg.list().len(), 1);
    }
}
