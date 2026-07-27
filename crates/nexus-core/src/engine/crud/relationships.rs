//! Relationship CRUD: create and get operations.

use super::super::Engine;
use crate::{Result, catalog, storage, transaction, wal};

impl Engine {
    /// Create a new relationship
    /// If `session_tx` is provided, uses that transaction instead of creating a new one
    pub fn create_relationship(
        &mut self,
        from: u64,
        to: u64,
        rel_type: String,
        properties: serde_json::Value,
    ) -> Result<u64> {
        let mut tx_ref: Option<&mut transaction::Transaction> = None;
        self.create_relationship_with_transaction(from, to, rel_type, properties, &mut tx_ref)
    }

    /// Create a new relationship with optional transaction from session
    pub(in crate::engine) fn create_relationship_with_transaction(
        &mut self,
        from: u64,
        to: u64,
        rel_type: String,
        properties: serde_json::Value,
        session_tx: &mut Option<&mut transaction::Transaction>,
    ) -> Result<u64> {
        self.create_relationship_inner(from, to, rel_type, properties, session_tx, true)
            .map(|(rel_id, _type_id)| rel_id)
    }

    /// Like `create_relationship` but does NOT bump the catalog rel-count.
    /// The caller MUST record the count itself (the `/ingest` bulk path
    /// batches it once per batch via `Catalog::batch_increment_rel_counts`,
    /// avoiding a durable LMDB fsync per edge). Returns `(rel_id, type_id)`.
    pub fn create_relationship_uncounted(
        &mut self,
        from: u64,
        to: u64,
        rel_type: String,
        properties: serde_json::Value,
    ) -> Result<(u64, catalog::TypeId)> {
        let mut no_tx: Option<&mut transaction::Transaction> = None;
        self.create_relationship_inner(from, to, rel_type, properties, &mut no_tx, false)
    }

    /// Shared implementation behind `create_relationship_with_transaction`
    /// and `create_relationship_uncounted`. When `increment_count` is
    /// `false`, the catalog relationship-type counter is left untouched and
    /// the caller is responsible for reconciling it (e.g. via a batched
    /// `Catalog::batch_increment_rel_counts` flush).
    fn create_relationship_inner(
        &mut self,
        from: u64,
        to: u64,
        rel_type: String,
        properties: serde_json::Value,
        session_tx: &mut Option<&mut transaction::Transaction>,
        increment_count: bool,
    ) -> Result<(u64, catalog::TypeId)> {
        let has_session_tx = session_tx.is_some();
        let mut own_tx = if has_session_tx {
            None
        } else {
            Some(self.transaction_manager.write().begin_write()?)
        };

        let tx = if let Some(stx) = session_tx.as_mut() {
            stx
        } else {
            own_tx.as_mut().unwrap()
        };

        let type_id = self.catalog.get_or_create_type(&rel_type)?;

        // phase6_opencypher-constraint-enforcement §6 — relationship
        // NOT NULL / property-type enforcement. Runs before the
        // storage write so a violation aborts atomically.
        self.enforce_rel_constraints(type_id, &properties)?;

        let rel_id = self
            .storage
            .create_relationship(tx, from, to, type_id, properties.clone())?;

        // Register every property key with the catalog so `db.propertyKeys()`
        // sees relationship-property keys too, not just node ones — mirrors
        // the label/type registration above. See
        // `Catalog::register_property_keys`.
        self.catalog.register_property_keys(&properties);

        // Update relationship index for performance (Phase 3 optimization)
        if let Err(e) = self
            .cache
            .relationship_index()
            .add_relationship(rel_id, from, to, type_id)
        {
            // #18: do NOT silently swallow. The storage write is authoritative,
            // so the operation still succeeds, but a missing exact-edge index
            // entry would silently degrade MERGE existence to an O(degree) chain
            // walk. Mark the index dirty so the next `find_relationship_between`
            // rebuilds it from storage and restores the O(1) fast path.
            tracing::error!(
                rel_id,
                from,
                to,
                type_id,
                "relationship-index update failed ({e}); marking index dirty for \
                 rebuild (exact-edge fast path degraded until next lookup)"
            );
            self.relationship_index_dirty
                .store(true, std::sync::atomic::Ordering::Release);
        }

        // Phase 8's `RelationshipStorageManager` / `RelationshipPropertyIndex`
        // sync is dead work: every read of those structures is
        // commented-out in production, test-only, or gated behind the
        // unreachable `AdvancedTraversalEngine` path (hardcoded
        // `use_optimized_traversal = false` in
        // `executor/operators/path.rs`) — live traversal reads the
        // `RecordStore` adjacency index instead (`path.rs`'s
        // `outgoing_relationships` / `incoming_relationships` /
        // `connected_relationships`). The per-edge write below costs a
        // lock acquisition plus a full JSON->HashMap property copy on
        // every relationship create for a store nothing on the read path
        // consults.
        //
        // Retiring the write outright would leave
        // `Executor::relationship_storage()` / `relationship_property_index()`
        // — this call site is their only caller in the workspace — as
        // dead code under `-D warnings`. Rather than deleting those
        // accessors (out of scope: the `RelationshipStorageManager` type,
        // its module, and its tests are kept intact), the write is
        // disabled via this off-by-default constant instead. See
        // `docs/analysis/relationship-ingest-perf/` for the throughput
        // measurement backing this change.
        const ENABLE_RELATIONSHIP_STORAGE_SYNC: bool = false;
        if ENABLE_RELATIONSHIP_STORAGE_SYNC {
            if let Some(rel_storage) = self.executor.relationship_storage() {
                // Convert properties from JSON Value to HashMap<String, Value>
                let mut props_map = std::collections::HashMap::new();
                if let serde_json::Value::Object(obj) = properties {
                    for (key, value) in obj {
                        props_map.insert(key, value);
                    }
                }

                // Add relationship to specialized storage
                if let Err(e) =
                    rel_storage
                        .write()
                        .create_relationship(from, to, type_id, props_map.clone())
                {
                    tracing::warn!("Failed to update RelationshipStorageManager: {}", e);
                    // Don't fail the operation, just log the warning
                }

                // Update property index if there are properties
                if !props_map.is_empty() {
                    if let Some(prop_index) = self.executor.relationship_property_index() {
                        if let Err(e) = prop_index
                            .write()
                            .index_properties(rel_id, type_id, &props_map)
                        {
                            tracing::warn!("Failed to update RelationshipPropertyIndex: {}", e);
                            // Don't fail the operation, just log the warning
                        }
                    }
                }
            }
        }

        // Only commit if we created our own transaction
        if !has_session_tx {
            self.transaction_manager.write().commit(tx)?;

            // Write WAL entry for relationship creation (async) after commit
            let wal_entry = wal::WalEntry::CreateRel {
                rel_id,
                src: from,
                dst: to,
                type_id,
            };
            self.write_wal_async(wal_entry)?;

            // PERFORMANCE OPTIMIZATION: Don't flush WAL immediately for single operations
            // Let it accumulate and flush in batches or on transaction end
            // self.flush_async_wal()?;
            // PERFORMANCE OPTIMIZATION: Skip executor refresh for single operations
            // Executor will see changes on next query execution
            // self.refresh_executor()?;
        }

        if increment_count {
            self.catalog.increment_rel_count(type_id)?;
        }

        Ok((rel_id, type_id))
    }

    /// Get relationship by ID
    pub fn get_relationship(&mut self, id: u64) -> Result<Option<storage::RelationshipRecord>> {
        let tx = self.transaction_manager.write().begin_read()?;
        self.storage.get_relationship(&tx, id)
    }
}
