//! Node lookup and property-match helpers: loading node properties,
//! resolving node patterns, matching property maps, updating label
//! bitmaps, and the write-path state cache helpers.
//!
//! Methods are `pub(in crate::engine)` when called from engine-level
//! siblings, or `pub(super)` when only needed within the `crud`
//! directory module.

use super::super::Engine;
use super::NodeWriteState;
use crate::{Error, Result, executor};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

impl Engine {
    pub(in crate::engine) fn ensure_node_state<'a>(
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

    pub(in crate::engine) fn persist_node_state(
        &mut self,
        node_id: u64,
        state: NodeWriteState,
    ) -> Result<()> {
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
        self.update_node_labels_with_ids(node_id, label_ids.clone())?;

        // phase6_fulltext-wal-integration §4 — refresh every matching
        // FTS index so SET / REMOVE / SET-label paths stay consistent
        // with the authoritative node state.
        //
        // `label_ids` above is derived from the NodeWriteState's
        // `labels` set, which is itself loaded via the `label_bits`
        // bitmap. In practice that set can be empty — the MATCH
        // pipeline resolves label membership through the catalog's
        // label index, not the record bitmap, so a node matched by
        // `(n:News)` may still surface here with an empty label set.
        // Fall back to the stored record's bitmap when the resolved
        // list is empty so FTS refresh can still find matching
        // indexes.
        let effective_label_ids = if label_ids.is_empty() {
            self.effective_label_ids_from_record(node_id)
                .unwrap_or_default()
        } else {
            label_ids
        };
        let props_value = Value::Object(properties);
        self.fts_refresh_node(node_id, &effective_label_ids, &props_value);
        // phase6_spatial-index-autopopulate §3 — refresh spatial indexes
        // after SET / REMOVE so the tree stays in sync with node state.
        self.spatial_refresh_node(node_id, &effective_label_ids, &props_value);
        Ok(())
    }

    /// Read a node's label ids by decoding its stored `label_bits`
    /// bitmap. Returns an empty vec on read failure — callers treat
    /// that as "no labels", which is the same conservative default
    /// the bitmap-based loader uses elsewhere.
    pub(super) fn effective_label_ids_from_record(&self, node_id: u64) -> Result<Vec<u32>> {
        let record = self.storage.read_node(node_id)?;
        let mut ids = Vec::new();
        for bit in 0..64u32 {
            if (record.label_bits & (1u64 << bit)) != 0 {
                ids.push(bit);
            }
        }
        Ok(ids)
    }

    pub(in crate::engine) fn load_node_properties_map(
        &self,
        node_id: u64,
    ) -> Result<Map<String, Value>> {
        if let Some(Value::Object(map)) = self.storage.load_node_properties(node_id)? {
            return Ok(map);
        }
        Ok(Map::new())
    }

    pub(in crate::engine) fn node_to_result_value(&mut self, node_id: u64) -> Result<Value> {
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

    pub(in crate::engine) fn find_nodes_by_node_pattern(
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

        // Fast path: when the pattern has exactly one label and at least one
        // property whose (label_id, key_id) pair has a registered B-tree entry,
        // intersect the per-property bitmaps from `find_exact` and verify only
        // those candidates against the full property map.  This reduces the
        // candidate set from O(N_label) to O(matches) for indexed properties.
        //
        // We fall through to the label-bitmap scan when:
        //   - there is no label (full scan), or more than one label,
        //   - there are no properties to filter on,
        //   - no property in the map has a registered index, or
        //   - an expression cannot be resolved to a concrete PropertyValue
        //     (e.g. it references a variable that isn't bound here).
        if label_ids.len() == 1 {
            if let Some(prop_map) = &node_pattern.properties {
                if !prop_map.properties.is_empty() {
                    let label_id = label_ids[0];
                    // Collect (key_id, PropertyValue) for every property whose
                    // key is registered AND whose expression resolves to a literal.
                    let mut indexed_filters: Vec<(u32, crate::index::PropertyValue)> = Vec::new();
                    for (key_name, expr) in &prop_map.properties {
                        if let Ok(key_id) = self.catalog.get_key_id(key_name) {
                            if self.indexes.property_index.has_index(label_id, key_id) {
                                if let Ok(json_val) = self.expression_to_json_value(expr) {
                                    let pv = super::super::json_to_property_value(&json_val);
                                    indexed_filters.push((key_id, pv));
                                }
                            }
                        }
                    }

                    if !indexed_filters.is_empty() {
                        // Intersect all per-property bitmaps to get the candidate set.
                        let mut candidate_bitmap = self.indexes.property_index.find_exact(
                            label_id,
                            indexed_filters[0].0,
                            indexed_filters[0].1.clone(),
                        )?;
                        for (key_id, pv) in indexed_filters.into_iter().skip(1) {
                            let bm = self
                                .indexes
                                .property_index
                                .find_exact(label_id, key_id, pv)?;
                            candidate_bitmap &= bm;
                            if candidate_bitmap.is_empty() {
                                return Ok(Vec::new());
                            }
                        }

                        // Verify each candidate against the complete property map
                        // (covers non-indexed properties and handles deleted nodes).
                        let mut matches = Vec::new();
                        for node_id in candidate_bitmap.iter() {
                            let node_id = node_id as u64;
                            let record = self.storage.read_node(node_id)?;
                            if record.is_deleted() {
                                continue;
                            }
                            if !self.node_matches_properties(node_id, prop_map)? {
                                continue;
                            }
                            matches.push(node_id);
                        }
                        return Ok(matches);
                    }
                }
            }
        }

        // Fallback: label-bitmap (or full) scan with per-node property check.
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

    pub(in crate::engine) fn node_matches_properties(
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

    pub(in crate::engine) fn update_node_labels_with_ids(
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
}
