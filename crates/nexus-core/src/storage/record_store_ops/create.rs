use std::sync::atomic::Ordering;

use crate::error::{Error, Result};
use crate::storage::external_id::{ConflictPolicy, ExternalId};
use crate::storage::property_store;
use crate::storage::record_store::RecordStore;
use crate::storage::records::{NodeRecord, RelationshipRecord};

/// Drop every top-level key whose value is `null` from an inline property map.
///
/// openCypher treats a map key written with a null value as **absent**:
/// `CREATE (n {id: 12, name: null})` yields a node whose `keys(n)` is `["id"]`
/// and reports `+properties 1`. Only the counter used to implement that rule —
/// it filtered nulls for itself while the UNFILTERED map went on to
/// `store_properties`, so the graph state contradicted the count that reported
/// it, and `keys(n)` returned a phantom `"name"`.
///
/// Applied here, at the storage write, because that is the one point every
/// inline-property create path crosses (node with or without an external id,
/// relationship) and the only place the rule is observable. Top-level keys only:
/// a `null` ELEMENT inside a list-valued property is a different question, and
/// this rule does not speak to it.
///
/// Reads stay tolerant of null-valued keys already on disk from earlier
/// versions — nothing here rewrites existing records.
fn strip_null_valued_keys(properties: serde_json::Value) -> serde_json::Value {
    match properties {
        serde_json::Value::Object(map) => {
            serde_json::Value::Object(map.into_iter().filter(|(_, v)| !v.is_null()).collect())
        }
        other => other,
    }
}

impl RecordStore {
    /// Create a new node
    pub fn create_node(
        &mut self,
        _tx: &mut crate::transaction::Transaction,
        labels: Vec<String>,
        properties: serde_json::Value,
    ) -> Result<u64> {
        // Compute label bits from label names (positional mapping).
        let mut label_bits = 0u64;
        for (i, _label) in labels.iter().enumerate() {
            if i < 64 {
                label_bits |= 1u64 << i;
            }
        }
        self.create_node_with_label_bits_inner(
            label_bits,
            properties,
            None,
            ConflictPolicy::Error,
            None,
        )
    }

    /// Create a new node with pre-computed label bits
    pub fn create_node_with_label_bits(
        &mut self,
        _tx: &mut crate::transaction::Transaction,
        label_bits: u64,
        properties: serde_json::Value,
    ) -> Result<u64> {
        self.create_node_with_label_bits_inner(
            label_bits,
            properties,
            None,
            ConflictPolicy::Error,
            None,
        )
    }

    /// Create a node carrying an optional external id with a specified conflict policy.
    ///
    /// When `external_id` is `None` the behaviour is identical to
    /// [`RecordStore::create_node`].  When it is `Some(ext)`:
    ///
    /// - `catalog` is used to open a write transaction on the LMDB env.
    /// - The external-id index is consulted via `put_if_absent`.
    /// - If no entry exists the mapping is committed together with the new record.
    /// - If an entry already exists, `policy` decides the outcome:
    ///   - [`ConflictPolicy::Error`] — returns [`Error::ExternalIdConflict`].
    ///   - [`ConflictPolicy::Match`] — returns the existing internal id.
    ///   - [`ConflictPolicy::Replace`] — overwrites properties, returns existing id.
    pub fn create_node_with_external_id(
        &mut self,
        _tx: &mut crate::transaction::Transaction,
        labels: Vec<String>,
        properties: serde_json::Value,
        external_id: Option<ExternalId>,
        policy: ConflictPolicy,
        catalog: &crate::catalog::Catalog,
    ) -> Result<u64> {
        let mut label_bits = 0u64;
        for (i, _label) in labels.iter().enumerate() {
            if i < 64 {
                label_bits |= 1u64 << i;
            }
        }
        self.create_node_with_label_bits_inner(
            label_bits,
            properties,
            external_id,
            policy,
            Some(catalog),
        )
    }

    /// Create a node with pre-computed label bits and an optional external id.
    ///
    /// Fast-path variant used by the executor (which already has label bits
    /// computed).  Conflict-policy semantics mirror
    /// [`RecordStore::create_node_with_external_id`].
    pub fn create_node_with_label_bits_and_external_id(
        &mut self,
        _tx: &mut crate::transaction::Transaction,
        label_bits: u64,
        properties: serde_json::Value,
        external_id: Option<ExternalId>,
        policy: ConflictPolicy,
        catalog: &crate::catalog::Catalog,
    ) -> Result<u64> {
        self.create_node_with_label_bits_inner(
            label_bits,
            properties,
            external_id,
            policy,
            Some(catalog),
        )
    }

    /// Central implementation used by all node-creation paths.
    ///
    /// `catalog` is required only when `external_id` is `Some`.  Passing
    /// `None` for the catalog with a `Some` external id falls through to
    /// plain creation (the external-id is ignored) — this should not occur in
    /// production code but keeps the function total.
    fn create_node_with_label_bits_inner(
        &mut self,
        label_bits: u64,
        properties: serde_json::Value,
        external_id: Option<ExternalId>,
        policy: ConflictPolicy,
        catalog: Option<&crate::catalog::Catalog>,
    ) -> Result<u64> {
        // openCypher: a map key written with a NULL value is ABSENT, not present
        // and null. Strip those keys from the map itself, before anything can
        // persist them — see [`strip_null_valued_keys`].
        let properties = strip_null_valued_keys(properties);
        // Side-effect count (openCypher TCK `+properties`): captured before
        // `properties` may be moved into `store_properties` on either path.
        // Added to `properties_created` only where a record is actually
        // written (never on a `ConflictPolicy::Match`/`Replace` that resolves
        // to an existing node). Derived from the already-filtered map, so the
        // count and the stored bytes cannot disagree — they used to, because
        // this counter carried its own private null filter while the unfiltered
        // map went to storage.
        let inline_prop_count = properties.as_object().map(|m| m.len() as u64).unwrap_or(0);
        // ── External-id path ──────────────────────────────────────────────────
        //
        // peek-then-allocate:
        //  1. Read next_node_id without consuming it (the probe id).
        //  2. Call put_if_absent with the probe id inside a catalog write txn.
        //  3a. No conflict → allocate (consume the id), write record, commit.
        //  3b. Conflict → dispatch on policy without allocating.
        //
        // Single-writer model means no other thread changes next_node_id
        // between step 1 and step 3a.
        if let (Some(ext), Some(cat)) = (&external_id, catalog) {
            let probe_id = self.next_node_id.load(Ordering::SeqCst);
            let mut wtxn = cat.write_txn()?;
            let idx = cat.external_id_index();

            match idx.put_if_absent(&mut wtxn, ext, probe_id)? {
                None => {
                    // No conflict — consume the id and write the record.
                    let node_id = self.allocate_node_id();
                    debug_assert_eq!(
                        node_id, probe_id,
                        "single-writer invariant violated between probe and alloc"
                    );

                    let prop_ptr = self.store_properties_if_any(node_id, &properties)?;
                    let mut record = NodeRecord::new();
                    record.label_bits = label_bits;
                    record.prop_ptr = prop_ptr;

                    tracing::debug!("create_node (ext): node_id={node_id}, prop_ptr={prop_ptr}");

                    self.write_node(node_id, &record)?;
                    wtxn.commit()?;
                    self.nodes_created.fetch_add(1, Ordering::SeqCst);
                    // openCypher `+labels` counts DISTINCT labels added across
                    // the whole statement, not per (node,label): `CREATE
                    // (:L),(:L)` is `+labels 1`. Accumulate the UNION of every
                    // created node's label bits; `labels_created()` returns its
                    // `count_ones()`.
                    self.labels_created.fetch_or(label_bits, Ordering::SeqCst);
                    self.properties_created
                        .fetch_add(inline_prop_count, Ordering::SeqCst);
                    return Ok(node_id);
                }
                Some(existing_id) => {
                    // Conflict — abort the catalog txn.
                    drop(wtxn);
                    return match policy {
                        ConflictPolicy::Error => Err(Error::ExternalIdConflict {
                            existing_internal_id: existing_id,
                            attempted_external_id: ext.to_string(),
                        }),
                        ConflictPolicy::Match => Ok(existing_id),
                        ConflictPolicy::Replace => {
                            if properties.is_object()
                                && !properties.as_object().map(|m| m.is_empty()).unwrap_or(true)
                            {
                                // store_properties may return a new offset
                                // (when the new property bytes don't fit
                                // in-place). Capture it and re-write the
                                // NodeRecord so subsequent reads via
                                // NodeRecord.prop_ptr see the fresh data
                                // — without this the load path follows
                                // the stale offset and reads the
                                // pre-replace properties (phase9 §2.4
                                // invariant).
                                let new_prop_ptr = self
                                    .property_store
                                    .write()
                                    .map_err(|_| Error::storage("property store lock poisoned"))?
                                    .store_properties(
                                        existing_id,
                                        property_store::EntityType::Node,
                                        properties,
                                    )?;
                                if let Ok(mut record) = self.read_node(existing_id) {
                                    record.prop_ptr = new_prop_ptr;
                                    self.write_node(existing_id, &record)?;
                                }
                            }
                            Ok(existing_id)
                        }
                    };
                }
            }
        }

        // ── Plain creation (no external id, or catalog not supplied) ─────────
        let node_id = self.allocate_node_id();

        let has_properties = properties.is_object()
            && properties
                .as_object()
                .map(|m| !m.is_empty())
                .unwrap_or(false);

        tracing::debug!(
            "create_node_with_label_bits_inner: node_id={node_id}, \
             has_properties={has_properties}"
        );

        let prop_ptr = if has_properties {
            let p = self
                .property_store
                .write()
                .map_err(|_| Error::storage("property store lock poisoned"))?
                .store_properties(node_id, property_store::EntityType::Node, properties)?;
            tracing::debug!("create_node_with_label_bits_inner: node_id={node_id}, prop_ptr={p}");
            p
        } else {
            0
        };

        let mut record = NodeRecord::new();
        record.label_bits = label_bits;
        record.prop_ptr = prop_ptr;

        self.write_node(node_id, &record)?;

        if let Ok(verify_record) = self.read_node(node_id) {
            tracing::debug!(
                "create_node_with_label_bits_inner: node_id={node_id}, \
                 verified prop_ptr={}",
                verify_record.prop_ptr
            );
        }

        self.nodes_created.fetch_add(1, Ordering::SeqCst);
        // Union of created-node label bits (distinct `+labels` per statement) —
        // see `create_node_with_label_bits_inner`'s external-id branch and
        // `labels_created()`.
        self.labels_created.fetch_or(label_bits, Ordering::SeqCst);
        self.properties_created
            .fetch_add(inline_prop_count, Ordering::SeqCst);
        Ok(node_id)
    }

    /// Helper: store properties and return the property pointer (0 when empty).
    fn store_properties_if_any(&self, node_id: u64, properties: &serde_json::Value) -> Result<u64> {
        let has = properties.is_object()
            && properties
                .as_object()
                .map(|m| !m.is_empty())
                .unwrap_or(false);
        if has {
            self.property_store
                .write()
                .map_err(|_| Error::storage("property store lock poisoned"))?
                .store_properties(
                    node_id,
                    property_store::EntityType::Node,
                    properties.clone(),
                )
        } else {
            Ok(0)
        }
    }

    /// Create a new relationship
    /// Phase 1 Optimization: Optimized relationship creation with reduced node reads
    pub fn create_relationship(
        &mut self,
        _tx: &mut crate::transaction::Transaction,
        from: u64,
        to: u64,
        type_id: u32,
        properties: serde_json::Value,
    ) -> Result<u64> {
        let rel_id = self.allocate_rel_id();

        let mut record = RelationshipRecord::new(from, to, type_id);

        // A null-valued key is absent here exactly as on the node path above.
        let properties = strip_null_valued_keys(properties);
        // Phase 1 Optimization: Batch property storage check (avoid multiple is_object checks)
        let has_properties = properties.is_object()
            && properties
                .as_object()
                .map(|m| !m.is_empty())
                .unwrap_or(false);
        // Side-effect count (openCypher TCK `+properties`), derived from the
        // filtered map so it cannot disagree with what is stored.
        let inline_prop_count = properties.as_object().map(|m| m.len() as u64).unwrap_or(0);

        // Store properties first to get property pointer (if needed)
        record.prop_ptr = if has_properties {
            self.property_store.write().unwrap().store_properties(
                rel_id,
                property_store::EntityType::Relationship,
                properties,
            )?
        } else {
            0
        };

        // Phase 3 Deep Optimization: Optimize node reads and writes
        // Read both nodes first, then write both (better cache locality)
        let mut source_prev_ptr = 0u64;
        let mut target_prev_ptr = 0u64;
        let mut source_node_opt = None;
        let mut target_node_opt = None;

        // Memory barrier to ensure visibility of previous writes
        // Acquire is sufficient for single-writer model
        std::sync::atomic::fence(std::sync::atomic::Ordering::Acquire);

        // PHASE 1: Read source node ONCE at the beginning and preserve prop_ptr
        let mut source_node = self.read_node(from)?;

        // CRITICAL FIX: Isolate and preserve prop_ptr - never modify it during relationship creation
        // BUT: Validate prop_ptr first - if it's corrupted, reset it to 0 to prevent write failure
        let mut preserved_source_prop_ptr = source_node.prop_ptr;

        // CRITICAL FIX: Validate prop_ptr before preserving it
        // If prop_ptr points to a Relationship, it's corrupted - reset to 0
        if preserved_source_prop_ptr != 0 {
            if let Some((stored_entity_id, stored_entity_type)) = self
                .property_store
                .read()
                .unwrap()
                .get_entity_info_at_offset(preserved_source_prop_ptr)
            {
                if stored_entity_type == property_store::EntityType::Relationship {
                    tracing::warn!(
                        "[create_relationship] Source node {} prop_ptr corruption detected (points to Relationship {}), resetting to 0",
                        from,
                        stored_entity_id
                    );
                    preserved_source_prop_ptr = 0;
                }
            }
        }

        source_prev_ptr = source_node.first_rel_ptr;

        // CRITICAL FIX: If first_rel_ptr is 0 but this is not the first relationship (rel_id > 0),
        // try to find the actual first_rel_ptr by scanning existing relationships
        // This handles the case where mmap synchronization fails between queries
        if source_prev_ptr == 0 && rel_id > 0 {
            // Scan backwards to find the most recent relationship for this node
            // CRITICAL FIX: Since 'from' is the SOURCE node for the new relationship,
            // we must only look for relationships where src_id == from (not dst_id == from)
            // Relationships where dst_id == from are INCOMING to this node, not OUTGOING
            let mut found_rel_id = None;
            let mut scanned_count = 0;
            for check_rel_id in (0..rel_id).rev() {
                scanned_count += 1;
                if let Ok(rel_record) = self.read_rel(check_rel_id) {
                    if !rel_record.is_deleted() {
                        // Check if this relationship originates from the source node
                        // We only care about OUTGOING relationships (src_id == from)
                        let check_src_id = rel_record.src_id;
                        let check_dst_id = rel_record.dst_id;
                        // CRITICAL: Only consider relationships where this node is the SOURCE
                        if check_src_id == from {
                            found_rel_id = Some(check_rel_id);
                            break;
                        }
                    }
                }
                // Limit scan to avoid performance issues - only scan last 100 relationships
                if scanned_count >= 100 {
                    tracing::debug!(
                        "[create_relationship] Scan limit reached (100 relationships), stopping"
                    );
                    break;
                }
            }

            // If we found a previous relationship, use it as the prev_ptr
            if let Some(prev_rel_id) = found_rel_id {
                source_prev_ptr = prev_rel_id + 1;
                tracing::debug!(
                    "[create_relationship] Corrected source_prev_ptr from 0 to {} (prev_rel_id={})",
                    source_prev_ptr,
                    prev_rel_id
                );
            } else {
                tracing::debug!(
                    "[create_relationship] No previous relationship found after scanning {} relationships, keeping source_prev_ptr=0",
                    scanned_count
                );
            }
        }

        // CRITICAL DEBUG: Log first_rel_ptr update
        tracing::debug!(
            "[create_relationship] Source node {}: old first_rel_ptr={}, new first_rel_ptr={} (rel_id={})",
            from,
            source_prev_ptr,
            rel_id + 1,
            rel_id
        );

        // PHASE 1: Update only first_rel_ptr, FORCE prop_ptr preservation
        source_node.first_rel_ptr = rel_id + 1;
        // CRITICAL: Explicitly restore prop_ptr to the preserved value before writing
        source_node.prop_ptr = preserved_source_prop_ptr;

        // Validate that prop_ptr was correctly preserved
        if source_node.prop_ptr != preserved_source_prop_ptr {
            tracing::error!(
                "[create_relationship] FATAL ERROR: Source node {} prop_ptr corruption detected! Expected {}, got {}",
                from,
                preserved_source_prop_ptr,
                source_node.prop_ptr
            );
            return Err(Error::Storage(format!(
                "prop_ptr corruption detected for node {}",
                from
            )));
        }

        tracing::debug!(
            "[create_relationship] Source node {}: preserving prop_ptr={}, updating first_rel_ptr from {} to {}",
            from,
            preserved_source_prop_ptr,
            source_prev_ptr,
            rel_id + 1
        );
        source_node_opt = Some(source_node);

        // PHASE 1: Read target node (if different from source) - preserve prop_ptr
        if to == from {
            target_prev_ptr = source_prev_ptr;
            // For self-loops, reuse source node (prop_ptr already preserved)
            if let Some(ref source_node) = source_node_opt {
                target_node_opt = Some(*source_node);
            }
        } else {
            // Read target node ONCE and preserve prop_ptr
            let mut target_node = self.read_node(to)?;
            let mut preserved_target_prop_ptr = target_node.prop_ptr;

            // CRITICAL FIX: Validate prop_ptr before preserving it
            // If prop_ptr points to a Relationship, it's corrupted - reset to 0
            if preserved_target_prop_ptr != 0 {
                if let Some((stored_entity_id, stored_entity_type)) = self
                    .property_store
                    .read()
                    .unwrap()
                    .get_entity_info_at_offset(preserved_target_prop_ptr)
                {
                    if stored_entity_type == property_store::EntityType::Relationship {
                        tracing::warn!(
                            "[create_relationship] Target node {} prop_ptr corruption detected (points to Relationship {}), resetting to 0",
                            to,
                            stored_entity_id
                        );
                        preserved_target_prop_ptr = 0;
                    }
                }
            }

            target_prev_ptr = target_node.first_rel_ptr;

            // CRITICAL FIX: Don't update first_rel_ptr on target nodes for incoming relationships
            // first_rel_ptr should only point to OUTGOING relationships from a node
            // For incoming relationships, we use next_dst_ptr to traverse the linked list
            // Updating first_rel_ptr here causes issues when querying outgoing relationships
            // from the target node (it points to relationships where the node is destination)
            tracing::debug!(
                "[create_relationship] Target node {}: NOT updating first_rel_ptr (incoming relationship, rel_id={})",
                to,
                rel_id
            );

            // Don't update first_rel_ptr for incoming relationships
            // Just preserve prop_ptr
            target_node.prop_ptr = preserved_target_prop_ptr;

            // Validate that prop_ptr was correctly preserved
            if target_node.prop_ptr != preserved_target_prop_ptr {
                tracing::error!(
                    "[create_relationship] FATAL ERROR: Target node {} prop_ptr corruption detected! Expected {}, got {}",
                    to,
                    preserved_target_prop_ptr,
                    target_node.prop_ptr
                );
                return Err(Error::Storage(format!(
                    "prop_ptr corruption detected for node {}",
                    to
                )));
            }

            tracing::debug!(
                "[create_relationship] Target node {}: preserving prop_ptr={}, NOT updating first_rel_ptr (incoming relationship)",
                to,
                preserved_target_prop_ptr
            );
            target_node_opt = Some(target_node);
        }

        // === Publish ordering: record BEFORE pointer ===
        // Nexus has no record-level MVCC, so the ORDER in which a relationship
        // insert publishes its writes IS the isolation contract for lock-free
        // readers (the server read path clones the executor/store and runs
        // holding no engine lock, against these same shared mmaps). The new
        // relationship record must be fully initialized in `rels_mmap` BEFORE
        // the source node's `first_rel_ptr` links it in — otherwise a reader
        // that observes the new pointer can `read_rel` a still-zeroed slot and
        // either surface a phantom edge to node 0 or stop its adjacency walk on
        // the `next_src_ptr == 0` end-of-chain sentinel (truncating the list).

        // 1. Finish building the record: its next-pointers chain to the prior
        //    list heads captured (above) before any node was modified.
        record.next_src_ptr = source_prev_ptr;
        record.next_dst_ptr = target_prev_ptr;

        tracing::debug!(
            "[create_relationship] Relationship {}: src={}, dst={}, next_src_ptr={}, next_dst_ptr={}",
            rel_id,
            from,
            to,
            source_prev_ptr,
            target_prev_ptr
        );

        // 2. Write the fully-initialized relationship record FIRST.
        self.write_rel(rel_id, &record)?;

        // 3. Release fence: the record write above must be visible to another
        //    thread BEFORE the `first_rel_ptr` publish below. Pairs with the
        //    Acquire fence on the reader's node read
        //    (`executor/operators/path.rs::find_relationships`) to form the
        //    happens-before edge the no-MVCC lock-free read path relies on.
        std::sync::atomic::fence(std::sync::atomic::Ordering::Release);

        // 4. Only now link the record in by publishing `first_rel_ptr` on the
        //    source node — the last, externally-visible step. (The target
        //    node's `first_rel_ptr` is intentionally NOT updated for an
        //    incoming edge, so its write publishes no adjacency pointer.)
        if let Some(source_node) = source_node_opt {
            tracing::debug!(
                "[create_relationship] Publishing source node {} first_rel_ptr={}",
                from,
                source_node.first_rel_ptr
            );
            self.write_node(from, &source_node)?;
        }
        if let Some(target_node) = target_node_opt {
            tracing::debug!(
                "[create_relationship] Writing target node {} (first_rel_ptr={} unchanged)",
                to,
                target_node.first_rel_ptr
            );
            self.write_node(to, &target_node)?;
        }

        // Phase 3 Deep Optimization: Lazy adjacency list updates (defer to improve CREATE performance)
        // For now, update immediately but with optimizations
        // TODO: Future optimization - batch updates or lazy updates (update on first read)
        if let Some(ref mut adj_store) = self.adjacency_store {
            // Phase 3 Optimization: Single relationship update (optimized path)
            // Fast append path for single relationships (skips expensive traversal)
            let outgoing_rels = [(rel_id, type_id)];
            if let Err(e) = adj_store.add_outgoing_relationships(from, &outgoing_rels) {
                tracing::warn!(
                    "Failed to update adjacency list for outgoing relationship: {}",
                    e
                );
            }

            // Only update incoming if different node (avoid duplicate work for self-loops)
            if from != to {
                let incoming_rels = [(rel_id, type_id)];
                if let Err(e) = adj_store.add_incoming_relationships(to, &incoming_rels) {
                    tracing::warn!(
                        "Failed to update adjacency list for incoming relationship: {}",
                        e
                    );
                }
            }
            // Self-loop: skip incoming update (same as outgoing)
        }

        self.relationships_created.fetch_add(1, Ordering::SeqCst);
        self.properties_created
            .fetch_add(inline_prop_count, Ordering::SeqCst);
        Ok(rel_id)
    }
}
