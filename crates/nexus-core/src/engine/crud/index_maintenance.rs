//! Secondary-index maintenance helpers — FTS, spatial R-tree, typed
//! B-tree, composite B-tree, and pending-update batch apply.
//!
//! All methods are `impl Engine` blocks. Methods that are called only
//! from within the `crud` directory module are `pub(super)` (visible
//! to `crud/mod.rs` and its submodule siblings). Methods that are
//! also called from engine-level siblings (`write_exec`, `transactions`,
//! etc.) are `pub(in crate::engine)`.

use super::super::Engine;
use crate::{Result, session, wal};
use serde_json::Value;

impl Engine {
    /// Delete-then-add the node's doc in every FTS index whose
    /// label/property set still matches. Called after SET / REMOVE
    /// / SET-label paths so the FTS view tracks the authoritative
    /// property state. Emits the matching `FtsDel` + `FtsAdd` WAL
    /// entries so crash recovery replays the refresh.
    ///
    /// All failures are logged via `tracing::warn!` and swallowed —
    /// FTS is an index, never the source of truth.
    pub(super) fn fts_refresh_node(
        &mut self,
        node_id: u64,
        _label_ids: &[u32],
        properties: &serde_json::Value,
    ) {
        let registry = self.indexes.fulltext.clone();
        let props_obj = properties.as_object();
        // Membership authority: ask the registry itself which
        // indexes currently contain this entity. FTS tracks its own
        // `members` set on every add/del so the SET / REMOVE refresh
        // paths work even when the engine- and executor-side label
        // indexes have drifted (a known consequence of
        // `refresh_executor` cloning from engine state).
        for name in registry.indexes_containing(node_id) {
            let Some(entry) = registry.get(&name) else {
                continue;
            };
            let meta = entry.meta.clone();
            // Remove the stale doc first.
            if let Err(e) = registry.remove_entity(&name, node_id) {
                tracing::warn!("FTS: refresh-remove on {name:?} for node {node_id} failed: {e}");
                continue;
            }
            let del = wal::WalEntry::FtsDel {
                name: name.clone(),
                entity_id: node_id,
            };
            if let Err(e) = self.write_wal_async(del) {
                tracing::warn!("FTS: WAL FtsDel on refresh for {name:?} / {node_id} failed: {e}");
            }
            // Re-add if any indexed string property is still present.
            let Some(obj) = props_obj else {
                continue;
            };
            let mut parts: Vec<String> = Vec::new();
            for prop in &meta.properties {
                if let Some(v) = obj.get(prop) {
                    if let Some(s) = v.as_str() {
                        parts.push(s.to_string());
                    }
                }
            }
            if parts.is_empty() {
                continue;
            }
            let content = parts.join(" ");
            if let Err(e) = registry.add_node_document(&name, node_id, 0, 0, &content) {
                tracing::warn!("FTS: refresh-add on {name:?} for node {node_id} failed: {e}");
                continue;
            }
            let add = wal::WalEntry::FtsAdd {
                name: name.clone(),
                entity_id: node_id,
                label_or_type_id: 0,
                key_id: 0,
                content,
            };
            if let Err(e) = self.write_wal_async(add) {
                tracing::warn!("FTS: WAL FtsAdd on refresh for {name:?} / {node_id} failed: {e}");
            }
        }
    }

    /// Insert `node_id`'s tuple into every composite B-tree that
    /// matches a label this node carries. Silent no-op when no
    /// composite index is registered for those labels.
    pub(in crate::engine) fn index_composite_tuples(
        &self,
        node_id: u64,
        label_ids: &[u32],
        properties: &Value,
    ) -> Result<()> {
        let obj = match properties.as_object() {
            Some(m) => m,
            None => return Ok(()),
        };
        for (lbl, keys, _unique, _name) in self.indexes.composite_btree.list() {
            if !label_ids.contains(&lbl) {
                continue;
            }
            // Build the tuple in key order; abort if any component is
            // missing / NULL — NODE KEY enforcement rejects those
            // writes upstream so this is a defence-in-depth skip.
            let mut tuple: Vec<crate::index::PropertyValue> = Vec::with_capacity(keys.len());
            let mut ok = true;
            for k in &keys {
                match obj.get(k) {
                    Some(Value::Null) | None => {
                        ok = false;
                        break;
                    }
                    Some(v) => tuple.push(super::super::json_to_property_value(v)),
                }
            }
            if !ok {
                continue;
            }
            if let Some(idx) = self.indexes.composite_btree.find(lbl, &keys) {
                let mut g = idx.write();
                g.insert(node_id, tuple)?;
            }
        }
        Ok(())
    }

    /// Walk every registered FTS index and, for each one whose
    /// label / property match the node just created, enqueue an
    /// `FtsAdd` into both the Tantivy backend and the WAL.
    ///
    /// The match rule mirrors Neo4j: a node is indexed by a given
    /// FTS index when (a) it carries at least one of the index's
    /// labels and (b) at least one of the index's properties has
    /// a string value on the node. The indexed content is the
    /// whitespace-joined concatenation of every matching string
    /// property, in the order the index declared them.
    ///
    /// Errors from individual FTS writes do NOT abort the caller —
    /// FTS is an index, not a source of truth. Problems surface via
    /// `tracing::warn!` so the node-write path stays durable even
    /// when one Tantivy index is misbehaving.
    pub(super) fn fts_autopopulate_node(
        &mut self,
        node_id: u64,
        label_ids: &[u32],
        properties: &serde_json::Value,
    ) -> Result<()> {
        use crate::index::fulltext_registry::FullTextEntity;
        let props_obj = match properties.as_object() {
            Some(o) => o,
            None => return Ok(()),
        };
        for meta in self.indexes.fulltext.list() {
            if meta.entity != FullTextEntity::Node {
                continue;
            }
            // Match by label name (registry persists names; storage
            // carries ids). Resolving names to ids on every hop is
            // cheap because the catalog keeps an in-memory cache.
            let mut matches_label = false;
            for label_name in &meta.labels_or_types {
                if let Ok(id) = self.catalog.get_label_id(label_name) {
                    if label_ids.contains(&id) {
                        matches_label = true;
                        break;
                    }
                }
            }
            if !matches_label {
                continue;
            }
            let mut parts: Vec<String> = Vec::new();
            for prop in &meta.properties {
                if let Some(v) = props_obj.get(prop) {
                    if let Some(s) = v.as_str() {
                        parts.push(s.to_string());
                    }
                }
            }
            if parts.is_empty() {
                continue;
            }
            let content = parts.join(" ");
            if let Err(e) = self
                .indexes
                .fulltext
                .add_node_document(&meta.name, node_id, 0, 0, &content)
            {
                tracing::warn!(
                    "FTS: autopopulate on index {:?} for node {node_id} failed: {e}",
                    meta.name
                );
                continue;
            }
            let wal_entry = wal::WalEntry::FtsAdd {
                name: meta.name.clone(),
                entity_id: node_id,
                label_or_type_id: 0,
                key_id: 0,
                content,
            };
            if let Err(e) = self.write_wal_async(wal_entry) {
                tracing::warn!(
                    "FTS: WAL append for index {:?} / node {node_id} failed: {e}",
                    meta.name
                );
            }
        }
        Ok(())
    }

    // ── Spatial auto-populate hooks ───────────────────────────────

    /// Walk every registered spatial index and, for each one whose
    /// `(label, property)` matches the node just created, insert the
    /// node's Point into the R-tree and emit a matching
    /// `WalEntry::RTreeInsert` so crash recovery replays the write.
    ///
    /// Match rule (mirrors `fts_autopopulate_node`):
    /// - The node carries at least one of the index's label ids, AND
    /// - the indexed property holds a well-formed Point value.
    ///
    /// Errors from individual tree writes are logged via
    /// `tracing::warn!` and swallowed — spatial indexes are secondary
    /// structures, never the source of truth.
    pub(super) fn spatial_autopopulate_node(
        &mut self,
        node_id: u64,
        label_ids: &[u32],
        properties: &serde_json::Value,
    ) -> Result<()> {
        let props_obj = match properties.as_object() {
            Some(o) => o,
            None => return Ok(()),
        };
        let registry = self.indexes.rtree.clone();
        for (name, label_name, property_key) in registry.definitions() {
            // Resolve the label name to an id; skip if unknown.
            let label_id = match self.catalog.get_label_id(&label_name) {
                Ok(id) => id,
                Err(_) => continue,
            };
            if !label_ids.contains(&label_id) {
                continue;
            }
            // Extract a Point from the indexed property.
            let Some(val) = props_obj.get(&property_key) else {
                continue;
            };
            let point = match crate::geospatial::Point::from_json_value(val) {
                Ok(p) => p,
                Err(_) => continue,
            };
            registry.insert_point(&name, node_id, point.x, point.y);
            let wal_entry = wal::WalEntry::RTreeInsert {
                index_name: name.clone(),
                node_id,
                x: point.x,
                y: point.y,
            };
            if let Err(e) = self.write_wal_async(wal_entry) {
                tracing::warn!(
                    "Spatial: WAL RTreeInsert on autopopulate for {name:?} / {node_id} failed: {e}"
                );
            }
        }
        Ok(())
    }

    /// Delete-then-conditional-add the node's point in every spatial
    /// index whose `(label, property)` still matches after a SET /
    /// REMOVE. Called from `persist_node_state`.
    ///
    /// For every index the node currently belongs to: drop the stale
    /// entry and emit `RTreeDelete`. Then, if the new property value
    /// is still a valid Point, re-insert and emit `RTreeInsert`.
    ///
    /// All errors are swallowed with `tracing::warn!`.
    pub(super) fn spatial_refresh_node(
        &mut self,
        node_id: u64,
        label_ids: &[u32],
        new_props: &serde_json::Value,
    ) {
        let registry = self.indexes.rtree.clone();
        let props_obj = new_props.as_object();

        // Phase 1: evict the stale entry from every index the node
        // currently belongs to.
        let containing = registry.indexes_containing(node_id);
        for name in &containing {
            registry.delete_point(name, node_id);
            let del = wal::WalEntry::RTreeDelete {
                index_name: name.clone(),
                node_id,
            };
            if let Err(e) = self.write_wal_async(del) {
                tracing::warn!(
                    "Spatial: WAL RTreeDelete on refresh for {name:?} / {node_id} failed: {e}"
                );
            }
        }

        // Phase 2: re-insert where the new value is still a valid Point
        // AND the node's labels match the index definition.
        let Some(obj) = props_obj else { return };
        for (idx_name, idx_label, prop_key) in registry.definitions() {
            // Label check — resolve index label name to an id and confirm
            // the node carries it. Skip (don't re-add) when labels don't
            // match.
            let label_matches = match self.catalog.get_label_id(&idx_label) {
                Ok(id) => label_ids.contains(&id),
                Err(_) => false,
            };
            if !label_matches {
                continue;
            }
            let Some(val) = obj.get(&prop_key) else {
                continue;
            };
            let point = match crate::geospatial::Point::from_json_value(val) {
                Ok(p) => p,
                Err(_) => continue,
            };
            registry.insert_point(&idx_name, node_id, point.x, point.y);
            let ins = wal::WalEntry::RTreeInsert {
                index_name: idx_name.clone(),
                node_id,
                x: point.x,
                y: point.y,
            };
            if let Err(e) = self.write_wal_async(ins) {
                tracing::warn!(
                    "Spatial: WAL RTreeInsert on refresh for {idx_name:?} / {node_id} failed: {e}"
                );
            }
        }
    }

    /// Evict `node_id` from every spatial index that currently lists
    /// it as a member. Called from `delete_node` before the storage
    /// record is marked deleted. Emits `WalEntry::RTreeDelete` per
    /// index so crash recovery can replay the eviction.
    ///
    /// Best-effort: all errors are logged and swallowed.
    pub(super) fn spatial_evict_node(&mut self, node_id: u64) {
        let registry = self.indexes.rtree.clone();
        for name in registry.indexes_containing(node_id) {
            registry.delete_point(&name, node_id);
            let wal_entry = wal::WalEntry::RTreeDelete {
                index_name: name.clone(),
                node_id,
            };
            if let Err(e) = self.write_wal_async(wal_entry) {
                tracing::warn!(
                    "Spatial: WAL RTreeDelete on evict for {name:?} / {node_id} failed: {e}"
                );
            }
        }
    }

    /// phase6_fulltext-wal-integration §4.3 — evict a node from
    /// every registered FTS index. Called from DELETE paths. Emits
    /// an `FtsDel` WAL entry alongside the Tantivy removal so crash
    /// recovery can replay the delete.
    pub(super) fn fts_evict_node(&mut self, node_id: u64) {
        let registry = self.indexes.fulltext.clone();
        for name in registry.indexes_containing(node_id) {
            if let Err(e) = registry.remove_entity(&name, node_id) {
                tracing::warn!("FTS: remove_entity on {name:?} for node {node_id} failed: {e}");
                continue;
            }
            let wal_entry = wal::WalEntry::FtsDel {
                name: name.clone(),
                entity_id: node_id,
            };
            if let Err(e) = self.write_wal_async(wal_entry) {
                tracing::warn!("FTS: WAL FtsDel for {name:?} / {node_id} failed: {e}");
            }
        }
    }

    /// Index node properties for WHERE clause optimization (Phase 5).
    ///
    /// Indexes node properties in the property index manager to enable
    /// fast lookups for WHERE clauses.
    pub(in crate::engine) fn index_node_properties(
        &self,
        node_id: u64,
        properties: &serde_json::Value,
    ) -> Result<()> {
        if let serde_json::Value::Object(props) = properties {
            let property_index = self.cache.property_index_manager();

            for (prop_name, prop_value) in props {
                // Convert property value to string for indexing
                let value_str = match prop_value {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    _ => continue, // Skip complex values for now
                };

                // Index the property (create index if it doesn't exist)
                if let Err(e) =
                    property_index.insert_property(prop_name.clone(), node_id, value_str)
                {
                    // Log error but don't fail the operation
                    tracing::warn!(
                        "Failed to index property {} for node {}: {}",
                        prop_name,
                        node_id,
                        e
                    );
                }
            }
        }

        Ok(())
    }

    /// Maintain the typed property B-tree (`self.indexes.property_index`,
    /// read by `find_exact`/`has_index`) for a freshly written node — but
    /// ONLY for `(label, key)` pairs that already have a registered index.
    /// `add_property` would otherwise auto-create a tree (turning every
    /// property into a phantom index), so the `has_index` guard is required.
    pub(in crate::engine) fn maintain_indexed_properties(
        &self,
        node_id: u64,
        label_ids: &[u32],
        properties: &serde_json::Value,
    ) -> Result<()> {
        let serde_json::Value::Object(props) = properties else {
            return Ok(());
        };
        // #21: fast-path — when no property index is registered at all (the
        // common case for un-indexed graphs), skip the per-property ×
        // per-label `get_key_id` / `has_index` loop entirely on every write.
        if !self.indexes.property_index.has_any_index() {
            return Ok(());
        }
        for (prop_name, prop_value) in props {
            let Ok(key_id) = self.catalog.get_key_id(prop_name) else {
                continue;
            };
            for &label_id in label_ids {
                if self.indexes.property_index.has_index(label_id, key_id) {
                    let pv = super::super::json_to_property_value(prop_value);
                    if let Err(e) = self
                        .indexes
                        .property_index
                        .add_property(node_id, label_id, key_id, pv)
                    {
                        tracing::warn!(
                            "typed property-index add failed for node {node_id} \
                             (label {label_id}, key {key_id}): {e}"
                        );
                    }
                }
            }
        }
        Ok(())
    }

    /// Apply pending index updates in batch (Phase 1 optimization).
    ///
    /// Applies all accumulated index updates from a session transaction
    /// in batch during commit, improving write performance.
    pub(in crate::engine) fn apply_pending_index_updates(
        &mut self,
        session: &mut session::Session,
    ) -> Result<()> {
        use crate::index::pending_updates::IndexUpdate;

        // Take all pending updates
        let updates = session.pending_index_updates.take_updates();

        // Apply updates in batch
        for update in updates {
            match update {
                IndexUpdate::AddNodeToLabel { node_id, label_ids } => {
                    self.indexes.label_index.add_node(node_id, &label_ids)?;
                }
                IndexUpdate::RemoveNodeFromLabel { node_id, label_ids } => {
                    for label_id in &label_ids {
                        self.indexes.remove_node_from_label(node_id, *label_id)?;
                    }
                }
                IndexUpdate::IndexNodeProperties {
                    node_id,
                    properties,
                } => {
                    self.index_node_properties(node_id, &properties)?;
                }
                // Property index removal is a no-op: property indexes
                // are keyed by (label, property_name) and do not need
                // explicit per-node cleanup on node deletion.
                IndexUpdate::RemoveNodeFromPropertyIndex { node_id: _ } => {}
                IndexUpdate::AddRelationship {
                    rel_id,
                    source_id,
                    target_id,
                    type_id,
                } => {
                    if let Err(e) = self
                        .cache
                        .relationship_index()
                        .add_relationship(rel_id, source_id, target_id, type_id)
                    {
                        tracing::error!("Failed to update relationship index: {e}");
                        self.relationship_index_dirty
                            .store(true, std::sync::atomic::Ordering::Release);
                    }
                }
                IndexUpdate::RemoveRelationship {
                    rel_id,
                    source_id,
                    target_id,
                    type_id,
                } => {
                    if let Err(e) = self
                        .cache
                        .relationship_index()
                        .remove_relationship(rel_id, source_id, target_id, type_id)
                    {
                        tracing::error!("Failed to remove from relationship index: {e}");
                        self.relationship_index_dirty
                            .store(true, std::sync::atomic::Ordering::Release);
                    }
                }
            }
        }

        Ok(())
    }
}
