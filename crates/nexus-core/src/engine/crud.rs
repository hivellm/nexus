//! Engine-level CRUD (create / read / update / delete) over nodes
//! and relationships, plus the private property-indexing helpers the
//! write path relies on.
//!
//! These are the methods the REST / Cypher layers ultimately call
//! through when a write goes past the executor pipeline — things like
//! `MERGE` spawning a node outside a pattern-match plan, or the
//! server's `POST /data/nodes` endpoint creating a node directly.
//!
//! Extracted from `engine/mod.rs` during the split. Public API
//! surface is unchanged; methods still resolve as `Engine::create_node`,
//! `Engine::update_node`, etc. via Rust's multi-file `impl` blocks.

use super::Engine;
use crate::{Error, Result, executor, session, storage, transaction, wal};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

/// Ephemeral write-state kept during a Cypher write pass — pair of
/// `properties` + `labels` that later get persisted via
/// [`Engine::persist_node_state`]. Internal to the engine write path.
pub(super) struct NodeWriteState {
    pub(super) properties: Map<String, Value>,
    pub(super) labels: HashSet<String>,
}

impl Engine {
    pub(super) fn ensure_node_state<'a>(
        &mut self,
        node_id: u64,
        cache: &'a mut HashMap<u64, NodeWriteState>,
    ) -> Result<&'a mut NodeWriteState> {
        use std::collections::hash_map::Entry;
        match cache.entry(node_id) {
            Entry::Vacant(e) => {
                let properties = self.load_node_properties_map(node_id)?;
                let record = self.storage.read_node(node_id)?;
                if record.is_deleted() {
                    return Err(Error::CypherExecution(format!(
                        "Node {} is deleted",
                        node_id
                    )));
                }
                let labels = self.catalog.get_labels_from_bitmap(record.label_bits)?;
                Ok(e.insert(NodeWriteState {
                    properties,
                    labels: labels.into_iter().collect(),
                }))
            }
            Entry::Occupied(e) => Ok(e.into_mut()),
        }
    }

    pub(super) fn persist_node_state(&mut self, node_id: u64, state: NodeWriteState) -> Result<()> {
        tracing::info!("[persist_node_state] node_id={}", node_id);
        let NodeWriteState { properties, labels } = state;
        tracing::info!(
            "[persist_node_state] Calling update_node_properties with properties={:?}",
            properties
        );
        self.storage
            .update_node_properties(node_id, Value::Object(properties.clone()))?;
        tracing::info!("[persist_node_state] update_node_properties returned OK");

        let mut label_ids = Vec::new();
        for label in labels {
            let label_id = self.catalog.get_or_create_label(&label)?;
            label_ids.push(label_id);
        }
        self.update_node_labels_with_ids(node_id, label_ids)?;
        Ok(())
    }

    pub(super) fn load_node_properties_map(&self, node_id: u64) -> Result<Map<String, Value>> {
        if let Some(Value::Object(map)) = self.storage.load_node_properties(node_id)? {
            return Ok(map);
        }
        Ok(Map::new())
    }

    pub(super) fn node_to_result_value(&mut self, node_id: u64) -> Result<Value> {
        let record = self.storage.read_node(node_id)?;
        if record.is_deleted() {
            return Ok(Value::Null);
        }

        let mut properties = self.load_node_properties_map(node_id)?;
        properties.insert("_nexus_id".to_string(), Value::Number(node_id.into()));
        let label_names = self.catalog.get_labels_from_bitmap(record.label_bits)?;
        let label_values = label_names.into_iter().map(Value::String).collect();
        properties.insert("_nexus_labels".to_string(), Value::Array(label_values));

        Ok(Value::Object(properties))
    }

    pub(super) fn find_nodes_by_node_pattern(
        &mut self,
        node_pattern: &executor::parser::NodePattern,
    ) -> Result<Vec<u64>> {
        let mut label_ids = Vec::new();
        for label in &node_pattern.labels {
            match self.catalog.get_label_id(label) {
                Ok(id) => label_ids.push(id),
                Err(_) => return Ok(Vec::new()),
            }
        }

        let mut candidates = Vec::new();
        if label_ids.is_empty() {
            let total_nodes = self.storage.node_count();
            for node_id in 0..total_nodes {
                candidates.push(node_id);
            }
        } else {
            let bitmap = self.indexes.label_index.get_nodes_with_labels(&label_ids)?;
            for node_id in bitmap.iter() {
                candidates.push(node_id as u64);
            }
        }

        let mut matches = Vec::new();
        for node_id in candidates {
            let record = self.storage.read_node(node_id)?;
            if record.is_deleted() {
                continue;
            }
            if let Some(prop_map) = &node_pattern.properties {
                if !self.node_matches_properties(node_id, prop_map)? {
                    continue;
                }
            }
            matches.push(node_id);
        }

        Ok(matches)
    }

    pub(super) fn node_matches_properties(
        &mut self,
        node_id: u64,
        prop_map: &executor::parser::PropertyMap,
    ) -> Result<bool> {
        let properties = self.load_node_properties_map(node_id)?;
        for (key, expr) in &prop_map.properties {
            let expected = self.expression_to_json_value(expr)?;
            match properties.get(key) {
                Some(existing) if existing == &expected => {}
                _ => return Ok(false),
            }
        }
        Ok(true)
    }

    pub(super) fn update_node_labels_with_ids(
        &mut self,
        node_id: u64,
        new_label_ids: Vec<u32>,
    ) -> Result<()> {
        let mut record = self.storage.read_node(node_id)?;
        if record.is_deleted() {
            return Err(Error::CypherExecution(format!(
                "Node {} is deleted",
                node_id
            )));
        }

        let current_ids = record.get_labels();
        let current_set: HashSet<u32> = current_ids.iter().copied().collect();
        let new_set: HashSet<u32> = new_label_ids.iter().copied().collect();

        let added: Vec<u32> = new_set.difference(&current_set).copied().collect();
        let removed: Vec<u32> = current_set.difference(&new_set).copied().collect();

        let mut new_bits = 0u64;
        for label_id in &new_label_ids {
            if *label_id < 64 {
                new_bits |= 1u64 << label_id;
            }
        }
        record.label_bits = new_bits;

        let mut tx = self.transaction_manager.write().begin_write()?;
        self.storage.write_node(node_id, &record)?;
        self.transaction_manager.write().commit(&mut tx)?;

        self.indexes
            .label_index
            .set_node_labels(node_id, &new_label_ids)?;

        for id in added {
            self.catalog.increment_node_count(id)?;
        }
        for id in removed {
            self.catalog.decrement_node_count(id)?;
        }

        Ok(())
    }

    /// Create a new node
    /// If `session_tx` is provided, uses that transaction instead of creating a new one
    pub fn create_node(
        &mut self,
        labels: Vec<String>,
        properties: serde_json::Value,
    ) -> Result<u64> {
        let mut tx_ref: Option<&mut transaction::Transaction> = None;
        self.create_node_with_transaction(labels, properties, &mut tx_ref, None)
    }

    /// Create a new node with optional transaction from session
    pub(super) fn create_node_with_transaction(
        &mut self,
        labels: Vec<String>,
        properties: serde_json::Value,
        session_tx: &mut Option<&mut transaction::Transaction>,
        created_nodes_tracker: Option<&mut Vec<u64>>,
    ) -> Result<u64> {
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

        // Create labels in catalog and get their IDs
        let mut label_bits = 0u64;
        let mut label_ids = Vec::new();
        for label in &labels {
            let label_id = self.catalog.get_or_create_label(label)?;
            if label_id < 64 {
                label_bits |= 1u64 << label_id;
            }
            label_ids.push(label_id);
        }

        // Check constraints before creating node
        self.check_constraints(&label_ids, &properties, None)?;

        let node_id =
            self.storage
                .create_node_with_label_bits(tx, label_bits, properties.clone())?;

        // Track node creation if we're in a session transaction
        if let Some(tracker) = created_nodes_tracker {
            tracker.push(node_id);
        }

        // For session transactions, defer index updates until commit (Phase 1 optimization)
        // For non-session transactions, update immediately
        if has_session_tx {
            // Index updates will be applied in batch during commit
            // For now, still update immediately for MATCH visibility during transaction
            // TODO: Optimize to defer updates but maintain visibility
            self.indexes.label_index.add_node(node_id, &label_ids)?;
            self.index_node_properties(node_id, &properties)?;
        } else {
            // Non-session transaction: update immediately
            self.indexes.label_index.add_node(node_id, &label_ids)?;
            self.index_node_properties(node_id, &properties)?;
        }

        // Only commit if we created our own transaction
        if !has_session_tx {
            self.transaction_manager.write().commit(tx)?;

            // Write WAL entry for node creation (async) after commit
            let wal_entry = wal::WalEntry::CreateNode {
                node_id,
                label_bits,
            };
            self.write_wal_async(wal_entry)?;

            // PERFORMANCE OPTIMIZATION: Don't flush WAL immediately for single operations
            // Let it accumulate and flush in batches or on transaction end
            // self.flush_async_wal()?;
            // PERFORMANCE OPTIMIZATION: Skip executor refresh for single operations
            // Executor will see changes on next query execution
            // self.refresh_executor()?;
        }
        // When there's a session transaction, index is updated immediately for MATCH visibility
        // On rollback, we'll remove nodes from index and mark them as deleted in storage

        Ok(node_id)
    }

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
    pub(super) fn create_relationship_with_transaction(
        &mut self,
        from: u64,
        to: u64,
        rel_type: String,
        properties: serde_json::Value,
        session_tx: &mut Option<&mut transaction::Transaction>,
    ) -> Result<u64> {
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
        let rel_id = self
            .storage
            .create_relationship(tx, from, to, type_id, properties.clone())?;

        // Update relationship index for performance (Phase 3 optimization)
        if let Err(e) = self
            .cache
            .relationship_index()
            .add_relationship(rel_id, from, to, type_id)
        {
            tracing::warn!("Failed to update relationship index: {}", e);
            // Don't fail the operation, just log the warning
        }

        // Phase 8: Update RelationshipStorageManager and RelationshipPropertyIndex
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

        self.catalog.increment_rel_count(type_id)?;

        Ok(rel_id)
    }

    /// Get node by ID
    pub fn get_node(&mut self, id: u64) -> Result<Option<storage::NodeRecord>> {
        let tx = self.transaction_manager.write().begin_read()?;
        self.storage.get_node(&tx, id)
    }

    /// Get relationship by ID
    pub fn get_relationship(&mut self, id: u64) -> Result<Option<storage::RelationshipRecord>> {
        let tx = self.transaction_manager.write().begin_read()?;
        self.storage.get_relationship(&tx, id)
    }

    /// Update a node with new labels and properties
    pub fn update_node(
        &mut self,
        id: u64,
        labels: Vec<String>,
        properties: serde_json::Value,
    ) -> Result<()> {
        // Check if node exists
        if self.get_node(id)?.is_none() {
            return Err(Error::NotFound(format!("Node {} not found", id)));
        }

        // Get or create label IDs
        let mut label_bits = 0u64;
        let mut label_ids = Vec::new();
        for label in &labels {
            let label_id = self.catalog.get_or_create_label(label)?;
            if label_id < 64 {
                label_bits |= 1u64 << label_id;
            }
            label_ids.push(label_id);
        }

        // Check constraints before updating node (exclude current node from uniqueness check)
        self.check_constraints(&label_ids, &properties, Some(id))?;

        // Create updated node record
        let mut node_record = storage::NodeRecord::new();
        node_record.label_bits = label_bits;

        // Store properties and get property pointer
        node_record.prop_ptr =
            if properties.is_object() && !properties.as_object().unwrap().is_empty() {
                self.storage
                    .property_store
                    .write()
                    .unwrap()
                    .store_properties(id, storage::property_store::EntityType::Node, properties)?
            } else {
                0
            };

        // Write updated record
        let mut tx = self.transaction_manager.write().begin_write()?;
        self.storage.write_node(id, &node_record)?;
        self.transaction_manager.write().commit(&mut tx)?;

        // Update statistics
        for label in &labels {
            if let Ok(label_id) = self.catalog.get_or_create_label(label) {
                self.catalog.increment_node_count(label_id)?;
            }
        }

        Ok(())
    }

    /// Delete a node by ID
    pub fn delete_node(&mut self, id: u64) -> Result<bool> {
        // Check if node exists
        if let Ok(Some(node_record)) = self.get_node(id) {
            // Remove node from label index before marking as deleted
            // This removes the node from all labels it belongs to
            self.indexes.label_index.remove_node(id)?;

            // Mark node as deleted
            let mut deleted_record = node_record;
            deleted_record.mark_deleted();

            let mut tx = self.transaction_manager.write().begin_write()?;
            self.storage.write_node(id, &deleted_record)?;
            self.transaction_manager.write().commit(&mut tx)?;

            // Update statistics
            for bit in 0..64 {
                if (node_record.label_bits & (1u64 << bit)) != 0 {
                    if let Ok(label_id) = self.catalog.get_label_id_by_id(bit as u32) {
                        self.catalog.decrement_node_count(label_id)?;
                    }
                }
            }

            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Delete all relationships connected to a node (for DETACH DELETE)
    pub fn delete_node_relationships(&mut self, node_id: u64) -> Result<()> {
        let mut tx = self.transaction_manager.write().begin_write()?;

        // Find all relationships connected to this node
        let total_rels = self.storage.relationship_count();
        let mut rels_to_delete = Vec::new();

        for rel_id in 0..total_rels {
            if let Ok(rel_record) = self.storage.read_rel(rel_id) {
                if !rel_record.is_deleted() {
                    // Check if this relationship is connected to the node
                    if rel_record.src_id == node_id || rel_record.dst_id == node_id {
                        rels_to_delete.push(rel_id);
                    }
                }
            }
        }

        // Mark all connected relationships as deleted
        for rel_id in rels_to_delete {
            if let Ok(rel_record) = self.storage.read_rel(rel_id) {
                let mut deleted_record = rel_record;
                deleted_record.mark_deleted();
                self.storage.write_rel(rel_id, &deleted_record)?;

                // Update relationship index for performance (Phase 3 optimization)
                if let Err(e) = self.cache.relationship_index().remove_relationship(
                    rel_id,
                    rel_record.src_id,
                    rel_record.dst_id,
                    rel_record.type_id,
                ) {
                    tracing::warn!("Failed to update relationship index on deletion: {}", e);
                    // Don't fail the operation, just log the warning
                }
            }
        }

        self.transaction_manager.write().commit(&mut tx)?;
        Ok(())
    }

    // Observability / maintenance methods
    // (knn_search, export_to_json, get_graph_statistics,
    // clear_all_data, validate_graph, graph_health_check, health_check)
    // live in `engine/maintenance.rs`.

    // Clustering methods (cluster_nodes, convert_to_simple_graph,
    // group_nodes_by_labels, group_nodes_by_property,
    // kmeans_cluster_nodes, detect_communities) live in
    // `engine/clustering.rs`.

    /// Index node properties for WHERE clause optimization (Phase 5)
    ///
    /// This method indexes node properties in the property index manager
    /// to enable fast lookups for WHERE clauses.
    pub(super) fn index_node_properties(
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

    /// Apply pending index updates in batch (Phase 1 optimization)
    ///
    /// This method applies all accumulated index updates from a session transaction
    /// in batch during commit, improving write performance.
    pub(super) fn apply_pending_index_updates(
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
                IndexUpdate::RemoveNodeFromPropertyIndex { node_id } => {
                    // TODO: Implement property index removal if needed
                    // For now, property indexes don't need explicit removal
                }
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
                        tracing::warn!("Failed to update relationship index: {}", e);
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
                        tracing::warn!("Failed to remove from relationship index: {}", e);
                    }
                }
            }
        }

        Ok(())
    }
}
