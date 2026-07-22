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

    /// Inverse of [`index_composite_tuples`]: remove `node_id`'s tuple from
    /// every composite B-tree matching a label it carries. Called from
    /// `delete_node` so a deleted node's NODE KEY / composite tuple is freed
    /// for reuse — node ids are never recycled and the NODE KEY existence
    /// check (`seek_exact`) does not skip soft-deleted rows, so a leftover
    /// entry would permanently and falsely reject re-creating the same tuple.
    /// Silent no-op when no composite index covers those labels; best-effort,
    /// the index is an accelerator, never the source of truth.
    pub(in crate::engine) fn unindex_composite_tuples(
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
            // Rebuild the exact tuple `index_composite_tuples` inserted, in key
            // order; abort on any missing / NULL component (those were never
            // indexed, so there is nothing to remove).
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
                idx.write().remove(node_id, &tuple);
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

    /// phase0_fix-knn-index-divergence §4.2 — evict `node_id`'s vector
    /// from the KNN (HNSW) index, mirroring `fts_evict_node` /
    /// `spatial_evict_node`. Unlike those two, the KNN index is not a
    /// named per-label registry (`self.indexes.knn_index` is a single
    /// global index — see `index/mod.rs::IndexManager`), so there is no
    /// `indexes_containing` lookup: `KnnIndex::remove_vector` is already
    /// a no-op, not an error, for a node id with no vector.
    ///
    /// **Standalone per the §1.1(b) scope decision**: `add_vector` /
    /// `remove_vector` have no production caller yet (no CREATE/SET path
    /// maintains the KNN index), so this is not wired into `delete_node`
    /// — wiring full KNN write-path maintenance (`add_vector` on
    /// CREATE/SET plus this call from `delete_node`) is an explicit
    /// follow-up task; see `proposal.md` "Related". This function exists
    /// so that follow-up has a correct, already-tested eviction primitive
    /// to call.
    ///
    /// Best-effort like its FTS/spatial siblings: `remove_vector`'s
    /// `Result` can only carry a dimension-mismatch style error, which
    /// eviction (no embedding argument) cannot trigger; a failure is
    /// logged via `tracing::warn!`, never escalated.
    pub(super) fn knn_evict_node(&mut self, node_id: u64) {
        if let Err(e) = self.indexes.knn_index.remove_vector(node_id) {
            tracing::warn!("KNN: remove_vector for node {node_id} failed: {e}");
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
        // #21: prefilter — indexes exist, but none for THIS node's labels:
        // skip the per-property `get_key_id` catalog (LMDB) reads too.
        if !label_ids
            .iter()
            .any(|&l| self.indexes.property_index.has_index_for_label(l))
        {
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

    /// Inverse of [`maintain_indexed_properties`]: remove `node_id`'s
    /// `(label, key, value)` entries from the typed property B-tree for every
    /// registered index covering its labels. Called from `delete_node` so the
    /// typed index does not retain dead entries after a delete. Best-effort;
    /// the typed index is never the source of truth (reads re-check
    /// `is_deleted()`), so failures are logged, never escalated.
    pub(in crate::engine) fn unindex_node_properties(
        &self,
        node_id: u64,
        label_ids: &[u32],
        properties: &serde_json::Value,
    ) {
        if !self.indexes.property_index.has_any_index() {
            return;
        }
        let serde_json::Value::Object(props) = properties else {
            return;
        };
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
                        .remove_property(node_id, label_id, key_id, pv)
                    {
                        tracing::warn!(
                            "typed property-index remove failed on delete for node {node_id} \
                             (label {label_id}, key {key_id}): {e}"
                        );
                    }
                }
            }
        }
    }

    /// Refresh the typed property B-tree after a SET / REMOVE / SET-label
    /// write: remove the node's OLD `(label, key, value)` entries and add
    /// the NEW ones, restricted to registered indexes. Without this, a
    /// SET on an indexed property left the index stale — the node became
    /// unreachable by its new value (seek miss) while still matching its
    /// old value (wrong results both ways).
    ///
    /// Best-effort like the FTS / spatial refresh siblings: failures are
    /// logged, never escalated — the index is never the source of truth.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::engine) fn typed_index_refresh_node(
        &self,
        node_id: u64,
        old_label_ids: &[u32],
        old_properties: &serde_json::Map<String, serde_json::Value>,
        new_label_ids: &[u32],
        new_properties: &serde_json::Value,
    ) {
        if !self.indexes.property_index.has_any_index() {
            return;
        }
        for (prop_name, prop_value) in old_properties {
            let Ok(key_id) = self.catalog.get_key_id(prop_name) else {
                continue;
            };
            for &label_id in old_label_ids {
                if self.indexes.property_index.has_index(label_id, key_id) {
                    let pv = super::super::json_to_property_value(prop_value);
                    if let Err(e) = self
                        .indexes
                        .property_index
                        .remove_property(node_id, label_id, key_id, pv)
                    {
                        tracing::warn!(
                            "typed property-index remove failed for node {node_id} \
                             (label {label_id}, key {key_id}): {e}"
                        );
                    }
                }
            }
        }
        if let Err(e) = self.maintain_indexed_properties(node_id, new_label_ids, new_properties) {
            tracing::warn!("typed property-index refresh failed for node {node_id}: {e}");
        }
    }

    /// Maintain the typed property B-tree for every node with id in
    /// `from..self.storage.node_count()` — the ids a just-finished
    /// executor CREATE allocated (exact under the single-writer model).
    /// The executor CREATE operator maintains the label index but not the
    /// typed property index, so without this a freshly created node was
    /// invisible to `find_exact` / `NodeIndexSeek` until a restart or an
    /// explicit-tx commit. Best-effort: failures are logged, never
    /// escalated.
    pub(in crate::engine) fn index_typed_properties_for_new_nodes(&mut self, from: u64) {
        if !self.indexes.property_index.has_any_index() {
            return;
        }
        for node_id in from..self.storage.node_count() {
            let Ok(record) = self.storage.read_node(node_id) else {
                continue;
            };
            if record.is_deleted() {
                continue;
            }
            let mut label_ids = Vec::new();
            for bit in 0..64u32 {
                if (record.label_bits & (1u64 << bit)) != 0 {
                    label_ids.push(bit);
                }
            }
            if label_ids.is_empty() {
                continue;
            }
            if let Ok(Some(properties)) = self.storage.load_node_properties(node_id) {
                if let Err(e) = self.maintain_indexed_properties(node_id, &label_ids, &properties) {
                    tracing::warn!(
                        "typed property-index maintenance failed for new node {node_id}: {e}"
                    );
                }
            }
        }
    }

    /// Enforce extended node constraints (`NODE KEY` / property-type) and
    /// populate every registered composite B-tree for the nodes a just-finished
    /// executor CREATE allocated in `node_from..self.storage.node_count()`.
    ///
    /// The executor CREATE operator runs only its own local `check_constraints`
    /// (catalog UNIQUE / EXISTS) and never reaches the engine's extended
    /// constraint set (`enforce_extended_node_constraints`) or
    /// `index_composite_tuples`. Without this a bare `CREATE` silently bypassed
    /// `NODE KEY` enforcement and left composite / NODE KEY indexes un-backed —
    /// the sibling engine-level create path (`create_node`) has always done both
    /// (`nodes.rs`). Runs post-write on the engine side, mirroring
    /// [`Engine::index_typed_properties_for_new_nodes`].
    ///
    /// Enforcement is checked against the composite B-tree populated so far
    /// (self-excluded), so a duplicate `NODE KEY` tuple — whether from an
    /// earlier statement already in the index or an earlier node in the same
    /// multi-node CREATE — is caught. On a violation the WHOLE CREATE statement
    /// is rolled back (Neo4j rejects the entire write, never a partial one) via
    /// [`Engine::rollback_created_range`], and the violation error is returned.
    pub(in crate::engine) fn enforce_and_index_new_created_nodes(
        &mut self,
        node_from: u64,
        rel_from: u64,
    ) -> Result<()> {
        let node_to = self.storage.node_count();
        for node_id in node_from..node_to {
            let record = match self.storage.read_node(node_id) {
                Ok(r) => r,
                Err(_) => continue,
            };
            if record.is_deleted() {
                continue;
            }
            let mut label_ids = Vec::new();
            for bit in 0..64u32 {
                if (record.label_bits & (1u64 << bit)) != 0 {
                    label_ids.push(bit);
                }
            }
            if label_ids.is_empty() {
                continue;
            }
            let properties = match self.storage.load_node_properties(node_id) {
                Ok(Some(p)) => p,
                _ => continue,
            };
            // Enforce NODE KEY / property-type against the composite B-tree
            // populated so far (self excluded). On violation roll back the
            // whole statement so nothing partial survives.
            if let Err(e) =
                self.enforce_extended_node_constraints(&label_ids, &properties, Some(node_id))
            {
                self.rollback_created_range(node_from, node_to, rel_from);
                return Err(e);
            }
            // Populate every registered composite B-tree matching this node's
            // labels so later nodes (this statement or a subsequent one) observe
            // the tuple for uniqueness — the same maintenance `create_node` does.
            if let Err(e) = self.index_composite_tuples(node_id, &label_ids, &properties) {
                self.rollback_created_range(node_from, node_to, rel_from);
                return Err(e);
            }
        }
        Ok(())
    }

    /// Undo an entire CREATE statement whose extended-constraint enforcement
    /// failed. Soft-deletes relationships in `rel_from..relationship_count()`
    /// FIRST — so the [`Engine::delete_node`] live-relationship guard sees zero
    /// remaining edges and passes — then nodes in `node_from..node_to`. Both
    /// delete paths also evict the composite / typed / FTS / spatial index
    /// entries and free the property blob, so the rollback leaves no index or
    /// storage residue. Best-effort per record: a rollback that cannot fully
    /// complete still reverts the statement maximally, and the caller surfaces
    /// the original violation error.
    fn rollback_created_range(&mut self, node_from: u64, node_to: u64, rel_from: u64) {
        let rel_to = self.storage.relationship_count();
        for rel_id in rel_from..rel_to {
            if let Err(e) = self.delete_relationship(rel_id) {
                tracing::warn!("CREATE rollback: deleting relationship {rel_id} failed: {e}");
            }
        }
        for node_id in node_from..node_to {
            if let Err(e) = self.delete_node(node_id) {
                tracing::warn!("CREATE rollback: deleting node {node_id} failed: {e}");
            }
        }
    }

    /// ISSUE #15: scoped per-commit index maintenance for explicit
    /// transactions. Replaces the previous per-COMMIT
    /// `rebuild_indexes_from_storage()` full O(N_nodes + N_rels) scan —
    /// commit cost now scales with the transaction's own write set, not
    /// with total graph size.
    ///
    /// The write set is the union of the session's storage watermark
    /// range (ids allocated since BEGIN — covers executor-path CREATEs,
    /// which do not report into the session) and the session's
    /// `created_nodes` / `created_relationships` lists (engine-path
    /// CREATEs). All index inserts are set/bitmap-based and idempotent,
    /// so overlap between the two sources is harmless.
    ///
    /// For every node: re-assert its label-bitmap membership and
    /// maintain the typed property B-tree for registered `(label, key)`
    /// indexes. The typed-index step is the part the full rebuild was
    /// load-bearing for: the explicit-tx CREATE path does not
    /// synchronously maintain `find_exact` / `NodeIndexSeek` (see the
    /// `explicit_commit_keeps_property_index_seek` contract guard).
    ///
    /// For every relationship: re-assert it in the in-memory
    /// relationship index (idempotent; on failure the #18 dirty-flag
    /// self-heal path takes over).
    ///
    /// SET-modified properties of pre-existing nodes are NOT reindexed
    /// here — same behavior as the non-transactional SET path, which
    /// does not maintain the typed index either (pre-existing gap,
    /// independent of #15).
    pub(in crate::engine) fn apply_committed_entity_index_updates(
        &mut self,
        session: &session::Session,
    ) -> Result<()> {
        let node_range = session.tx_begin_node_watermark..self.storage.node_count();
        let rel_range = session.tx_begin_rel_watermark..self.storage.relationship_count();

        for node_id in node_range.chain(session.created_nodes.iter().copied()) {
            let record = match self.storage.read_node(node_id) {
                Ok(r) => r,
                Err(_) => continue,
            };
            if record.is_deleted() {
                continue;
            }
            let mut label_ids = Vec::new();
            for bit in 0..64u32 {
                if (record.label_bits & (1u64 << bit)) != 0 {
                    label_ids.push(bit);
                }
            }
            if !label_ids.is_empty() {
                self.indexes.label_index.add_node(node_id, &label_ids)?;
            }
            if let Ok(Some(properties)) = self.storage.load_node_properties(node_id) {
                self.maintain_indexed_properties(node_id, &label_ids, &properties)?;
            }
        }
        for rel_id in rel_range.chain(session.created_relationships.iter().copied()) {
            let rel = match self.storage.read_rel(rel_id) {
                Ok(r) => r,
                Err(_) => continue,
            };
            if rel.is_deleted() {
                continue;
            }
            // packed struct: copy fields to locals before use.
            let (src, dst, type_id) = (rel.src_id, rel.dst_id, rel.type_id);
            if let Err(e) = self
                .cache
                .relationship_index()
                .add_relationship(rel_id, src, dst, type_id)
            {
                tracing::error!("commit relationship-index update failed for rel {rel_id}: {e}");
                self.relationship_index_dirty
                    .store(true, std::sync::atomic::Ordering::Release);
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

#[cfg(test)]
mod knn_evict_node_tests {
    use super::super::super::Engine;
    use crate::index::DEFAULT_VECTORIZER_DIMENSION;
    use crate::testing::TestContext;

    // ── phase0_fix-knn-index-divergence §4.3 ──────────────────────────

    #[test]
    fn knn_evict_node_clears_both_mappings() {
        let ctx = TestContext::new();
        let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();

        let embedding = vec![1.0_f32; DEFAULT_VECTORIZER_DIMENSION];
        engine.indexes.knn_index.add_vector(42, embedding).unwrap();
        assert!(engine.indexes.knn_index.has_vector(42));
        assert_eq!(engine.indexes.knn_index.get_stats().total_vectors, 1);

        engine.knn_evict_node(42);

        assert!(!engine.indexes.knn_index.has_vector(42));
        assert!(engine.indexes.knn_index.get_all_nodes().is_empty());
        assert_eq!(engine.indexes.knn_index.get_stats().total_vectors, 0);
    }

    #[test]
    fn knn_evict_node_is_a_noop_for_a_node_with_no_vector() {
        let ctx = TestContext::new();
        let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();

        // Must not panic / error when the node never had a vector.
        engine.knn_evict_node(999);
        assert!(!engine.indexes.knn_index.has_vector(999));
    }
}
