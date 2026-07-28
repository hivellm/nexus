use crate::error::Result;
use crate::storage::property_store;
use crate::storage::record_store::RecordStore;

impl RecordStore {
    /// Load properties for a node
    /// PHASE 3: Enhanced validation with safe fallback to reverse_index
    pub fn load_node_properties(&self, node_id: u64) -> Result<Option<serde_json::Value>> {
        let prop_ptr = self.read_node(node_id).ok().map(|r| r.prop_ptr);
        self.load_node_properties_inner(node_id, prop_ptr)
    }

    /// Same as [`Self::load_node_properties`], but for callers that
    /// already hold a `NodeRecord` (and thus its `prop_ptr`) from a
    /// `read_node`/`read_node_header`-family call moments earlier.
    ///
    /// phase8_neo4j-concurrency-gaps §2 — `load_node_properties(node_id)`
    /// re-reads the node record internally purely to recover `prop_ptr`.
    /// `Executor::read_node_as_value` (the single most-called node
    /// materialiser in the executor — every scan, expand hop, and index
    /// seek routes through it) already has that `NodeRecord` in hand, so
    /// that internal re-read was a second `nodes_mmap` lock acquisition
    /// (plus a second `property_store` corruption cross-check) on every
    /// single node materialisation. Multiplied across every node a scan
    /// or expand hop touches, this was a meaningful share of the
    /// per-node lock traffic behind `traversal.small_two_hop_from_hub`'s
    /// concurrency ceiling. Identical validation/fallback logic to
    /// `load_node_properties` — only the `prop_ptr` source differs.
    pub fn load_node_properties_with_ptr(
        &self,
        node_id: u64,
        prop_ptr: u64,
    ) -> Result<Option<serde_json::Value>> {
        self.load_node_properties_inner(node_id, Some(prop_ptr))
    }

    /// Shared body of [`Self::load_node_properties`] and
    /// [`Self::load_node_properties_with_ptr`]. `prop_ptr = None` means
    /// "the caller could not read a `NodeRecord` at all" (mirrors the
    /// original `self.read_node(node_id)` failure branch); `Some(0)`
    /// means "read a record, but it has no properties yet".
    fn load_node_properties_inner(
        &self,
        node_id: u64,
        prop_ptr: Option<u64>,
    ) -> Result<Option<serde_json::Value>> {
        // phase8_neo4j-concurrency-gaps §2 — acquire the `property_store`
        // read lock ONCE for this whole call instead of once per branch
        // below (up to 3 separate acquisitions previously: the entity-
        // info validation, the offset load, and the reverse-index
        // fallback). Every node materialisation in the executor
        // (`read_node_as_value`, called from every scan/expand/index-seek
        // path) goes through this function, so this is on the hottest
        // per-node lock in the read path.
        let prop_guard = self.property_store.read().unwrap();

        // First try to use prop_ptr from NodeRecord (more reliable)
        if let Some(prop_ptr) = prop_ptr {
            tracing::debug!(
                "load_node_properties: node_id={}, prop_ptr={}",
                node_id,
                prop_ptr
            );
            if prop_ptr != 0 {
                // PHASE 3: Double validation - verify that prop_ptr points to Node properties
                // Check the entity_type stored at this offset BEFORE loading
                if let Some((stored_entity_id, stored_entity_type)) =
                    prop_guard.get_entity_info_at_offset(prop_ptr)
                {
                    if stored_entity_type != property_store::EntityType::Node
                        || stored_entity_id != node_id
                    {
                        // PHASE 3: Prop_ptr corruption detected - fallback to reverse_index
                        tracing::warn!(
                            "load_node_properties: node_id={} prop_ptr={} points to wrong entity (type={:?}, id={}), using reverse_index instead",
                            node_id,
                            prop_ptr,
                            stored_entity_type,
                            stored_entity_id
                        );
                        // Fall through to reverse_index lookup - prop_ptr is corrupted
                    } else {
                        // PHASE 3: Entity type and ID match - safe to load from prop_ptr
                        match prop_guard.load_properties_at_offset(prop_ptr) {
                            Ok(Some(props)) => {
                                let keys = props.as_object().map(|m| m.keys().collect::<Vec<_>>());
                                tracing::debug!(
                                    "load_node_properties: node_id={}, loaded properties from prop_ptr={}, keys={:?}",
                                    node_id,
                                    prop_ptr,
                                    keys
                                );
                                // PHASE 3: Additional validation - check for relationship-like properties
                                if let Some(obj) = props.as_object() {
                                    if obj.contains_key("since") || obj.contains_key("type") {
                                        tracing::warn!(
                                            "load_node_properties: node_id={} prop_ptr={} returned relationship-like properties: {:?}. Falling back to reverse_index",
                                            node_id,
                                            prop_ptr,
                                            keys
                                        );
                                        // Fall through to reverse_index - properties look wrong
                                    } else {
                                        return Ok(Some(props));
                                    }
                                } else {
                                    return Ok(Some(props));
                                }
                            }
                            Ok(None) => {
                                tracing::debug!(
                                    "load_node_properties: node_id={}, prop_ptr={} returned None, using reverse_index",
                                    node_id,
                                    prop_ptr
                                );
                            }
                            Err(e) => {
                                tracing::debug!(
                                    "load_node_properties: node_id={}, error loading from prop_ptr={}: {}, using reverse_index",
                                    node_id,
                                    prop_ptr,
                                    e
                                );
                            }
                        }
                    }
                } else {
                    tracing::warn!(
                        "load_node_properties: node_id={} prop_ptr={} not found in property_store",
                        node_id,
                        prop_ptr
                    );
                    // Fall through to reverse_index lookup
                }
            } else {
                tracing::debug!(
                    "load_node_properties: node_id={}, prop_ptr is 0, trying reverse_index",
                    node_id
                );
            }
        } else {
            tracing::debug!(
                "load_node_properties: node_id={}, failed to read node record, trying reverse_index",
                node_id
            );
        }

        // PHASE 3: Safe fallback to reverse_index lookup (always reliable)
        let result = prop_guard.load_properties(node_id, property_store::EntityType::Node);
        let keys_debug = result.as_ref().ok().and_then(|opt| {
            opt.as_ref()
                .map(|v| v.as_object().map(|m| m.keys().collect::<Vec<_>>()))
        });
        tracing::debug!(
            "load_node_properties: node_id={}, reverse_index result: {:?}",
            node_id,
            keys_debug
        );
        // PHASE 3: Final validation - check if reverse_index returned relationship-like properties
        if let Ok(Some(props)) = &result {
            if let Some(obj) = props.as_object() {
                if obj.contains_key("since") || obj.contains_key("type") {
                    tracing::warn!(
                        "load_node_properties: node_id={} reverse_index has relationship-like properties: {:?}. This indicates severe data corruption!",
                        node_id,
                        keys_debug
                    );
                }
            }
        }
        result
    }

    /// Load properties for a relationship
    pub fn load_relationship_properties(&self, rel_id: u64) -> Result<Option<serde_json::Value>> {
        // For relationships, use reverse_index lookup
        // (Relationship records are accessed differently, so we use the index)
        self.property_store
            .read()
            .unwrap()
            .load_properties(rel_id, property_store::EntityType::Relationship)
    }

    /// Update properties for a node
    /// CRITICAL FIX: Also updates node record's prop_ptr to ensure consistency
    pub fn update_node_properties(
        &mut self,
        node_id: u64,
        properties: serde_json::Value,
    ) -> Result<()> {
        let new_prop_ptr = if properties.is_object() && !properties.as_object().unwrap().is_empty()
        {
            let prop_ptr = self.property_store.write().unwrap().store_properties(
                node_id,
                property_store::EntityType::Node,
                properties,
            )?;
            tracing::debug!(
                "update_node_properties: node_id={}, stored properties, new_prop_ptr={}",
                node_id,
                prop_ptr
            );
            prop_ptr
        } else {
            self.property_store
                .write()
                .unwrap()
                .delete_properties(node_id, property_store::EntityType::Node)?;
            tracing::debug!(
                "update_node_properties: node_id={}, deleted properties, new_prop_ptr=0",
                node_id
            );
            0
        };

        // CRITICAL FIX: Update the node record's prop_ptr to match the new offset
        // This ensures load_node_properties reads from the correct location
        if let Ok(mut node_record) = self.read_node(node_id) {
            if node_record.prop_ptr != new_prop_ptr {
                tracing::debug!(
                    "update_node_properties: node_id={}, updating prop_ptr from {} to {}",
                    node_id,
                    node_record.prop_ptr,
                    new_prop_ptr
                );
                node_record.prop_ptr = new_prop_ptr;
                self.write_node(node_id, &node_record)?;
            }
        }
        Ok(())
    }

    /// Update properties for a relationship
    pub fn update_relationship_properties(
        &mut self,
        rel_id: u64,
        properties: serde_json::Value,
    ) -> Result<()> {
        if properties.is_object() && !properties.as_object().unwrap().is_empty() {
            self.property_store.write().unwrap().store_properties(
                rel_id,
                property_store::EntityType::Relationship,
                properties,
            )?;
        } else {
            self.property_store
                .write()
                .unwrap()
                .delete_properties(rel_id, property_store::EntityType::Relationship)?;
        }
        Ok(())
    }

    /// Delete properties for a node
    pub fn delete_node_properties(&mut self, node_id: u64) -> Result<()> {
        self.property_store
            .write()
            .unwrap()
            .delete_properties(node_id, property_store::EntityType::Node)
    }

    /// Delete properties for a relationship
    pub fn delete_relationship_properties(&mut self, rel_id: u64) -> Result<()> {
        self.property_store
            .write()
            .unwrap()
            .delete_properties(rel_id, property_store::EntityType::Relationship)
    }

    /// Get property store statistics
    pub fn property_count(&self) -> usize {
        self.property_store.read().unwrap().property_count()
    }
}
