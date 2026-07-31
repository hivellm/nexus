//! Standalone CREATE pattern execution — `execute_create_pattern_with_variables`
//! / `execute_create_pattern_internal` realise a CREATE pattern into actual
//! nodes/relationships, threading previously-bound variables into the
//! pattern. No upstream MATCH/UNWIND row exists on this path; property
//! expressions route through
//! `super::properties::Executor::resolve_standalone_create_property`.

use super::super::super::context::RelationshipInfo;
use super::super::super::engine::Executor;
use super::super::super::parser;
use crate::catalog::TypeId;
use crate::{Error, Result};
use serde_json::Value;

impl Executor {
    /// Resolve a parsed `_id` expression (string-literal or parameter) into
    /// an [`ExternalId`]. Anything else is rejected at parse time, so this
    /// function only needs to handle those two cases.
    pub(in crate::executor) fn resolve_external_id(
        &self,
        expr: &parser::Expression,
        params: &std::collections::HashMap<String, serde_json::Value>,
    ) -> Result<crate::storage::external_id::ExternalId> {
        use std::str::FromStr;
        let raw: String = match expr {
            parser::Expression::Literal(parser::Literal::String(s)) => s.clone(),
            parser::Expression::Parameter(name) => match params.get(name) {
                Some(serde_json::Value::String(s)) => s.clone(),
                Some(other) => {
                    return Err(Error::executor(format!(
                        "_id parameter `{}` must be a string, got {:?}",
                        name, other
                    )));
                }
                None => {
                    return Err(Error::executor(format!(
                        "_id parameter `{}` not provided",
                        name
                    )));
                }
            },
            _ => {
                return Err(Error::executor(
                    "_id expression must be a string literal or parameter (parser invariant)",
                ));
            }
        };
        crate::storage::external_id::ExternalId::from_str(&raw)
            .map_err(|e| Error::executor(format!("invalid _id `{}`: {}", raw, e)))
    }

    pub(in crate::executor) fn execute_create_pattern_with_variables(
        &self,
        pattern: &parser::Pattern,
        external_id: Option<crate::storage::external_id::ExternalId>,
        policy: crate::storage::external_id::ConflictPolicy,
        params: &std::collections::HashMap<String, Value>,
    ) -> Result<(
        std::collections::HashMap<String, u64>,
        std::collections::HashMap<String, RelationshipInfo>,
    )> {
        let mut created_nodes: std::collections::HashMap<String, u64> =
            std::collections::HashMap::new();
        let mut created_relationships: std::collections::HashMap<String, RelationshipInfo> =
            std::collections::HashMap::new();

        // Call the original implementation
        self.execute_create_pattern_internal(
            pattern,
            &mut created_nodes,
            &mut created_relationships,
            external_id,
            policy,
            params,
        )?;

        Ok((created_nodes, created_relationships))
    }

    /// Internal implementation of CREATE pattern execution
    pub(in crate::executor) fn execute_create_pattern_internal(
        &self,
        pattern: &parser::Pattern,
        created_nodes: &mut std::collections::HashMap<String, u64>,
        created_relationships: &mut std::collections::HashMap<String, RelationshipInfo>,
        external_id: Option<crate::storage::external_id::ExternalId>,
        policy: crate::storage::external_id::ConflictPolicy,
        params: &std::collections::HashMap<String, Value>,
    ) -> Result<()> {
        // PERFORMANCE OPTIMIZATION: Reuse shared transaction manager
        let mut tx_mgr = self.transaction_manager().lock();
        let mut tx = tx_mgr.begin_write()?;
        // Tracks whether the external id has already been consumed by the
        // first node in the pattern (a single CREATE may not assign the same
        // external id to more than one node).
        let ext_id_consumed = std::cell::Cell::new(false);

        // Phase 1 Optimization: Cache label lookups and batch catalog updates
        let mut label_cache: std::collections::HashMap<String, u32> =
            std::collections::HashMap::new();
        let mut label_count_updates: std::collections::HashMap<u32, u32> =
            std::collections::HashMap::new();
        // Symmetric to `label_count_updates` above but for relationship
        // types: this executor CREATE path writes edges directly to the
        // record store (bypassing `Engine::create_relationship`), so
        // without this accumulator + its post-commit flush below,
        // `catalog.rel_counts` never sees these edges.
        let mut rel_count_updates: std::collections::HashMap<TypeId, u32> =
            std::collections::HashMap::new();
        // Track exact (node_id, label_ids) pairs as we create them, so the
        // post-commit label-index update doesn't have to reverse-engineer
        // labels from `NodeRecord.label_bits`. The bitmap is a u64 and
        // silently loses labels with `label_id >= 64` — a real bug when
        // the catalog has accumulated many labels (phase6 §1). Carrying the
        // original list sidesteps the cap entirely.
        let mut created_nodes_with_labels: Vec<(u64, Vec<u32>)> = Vec::new();

        // Phase 1.5.2: Pre-allocate label/type IDs in batches
        // Collect all unique labels and types from the pattern first
        let mut all_labels = std::collections::HashSet::new();
        let mut all_types = std::collections::HashSet::new();

        for element in &pattern.elements {
            match element {
                parser::PatternElement::Node(node) => {
                    for label in &node.labels {
                        all_labels.insert(label.as_str());
                    }
                }
                parser::PatternElement::Relationship(rel) => {
                    for rel_type in &rel.types {
                        all_types.insert(rel_type.as_str());
                    }
                }
                parser::PatternElement::QuantifiedGroup(_) => {
                    return Err(Error::Executor(
                        "ERR_QPP_NOT_IN_CREATE: quantified path patterns \
                         are read-only; use a MATCH clause instead"
                            .to_string(),
                    ));
                }
            }
        }

        // Batch allocate all labels in a single transaction
        if !all_labels.is_empty() {
            let labels_vec: Vec<&str> = all_labels.iter().copied().collect();
            let batch_results = self.catalog().batch_get_or_create_labels(&labels_vec)?;
            label_cache.extend(batch_results);
        }

        // Batch allocate all types in a single transaction
        if !all_types.is_empty() {
            let types_vec: Vec<&str> = all_types.iter().copied().collect();
            let batch_results = self.catalog().batch_get_or_create_types(&types_vec)?;
            label_cache.extend(batch_results); // Reuse label_cache for types too
        }

        // Use the passed-in created_nodes HashMap (don't create a new one)
        let mut last_node_id: Option<u64> = None;
        let mut skip_next_node = false; // Flag to skip node already created in relationship

        // Process pattern elements in sequence
        // Pattern alternates: Node -> Relationship -> Node -> Relationship ...
        for (i, element) in pattern.elements.iter().enumerate() {
            match element {
                parser::PatternElement::QuantifiedGroup(_) => {
                    return Err(Error::Executor(
                        "ERR_QPP_NOT_IN_CREATE: quantified path patterns \
                         are read-only; use a MATCH clause instead"
                            .to_string(),
                    ));
                }
                parser::PatternElement::Node(node) => {
                    // Skip if this node was already created as part of the previous relationship
                    if skip_next_node {
                        skip_next_node = false;
                        continue;
                    }

                    // Bound-variable reuse: if this node carries a
                    // variable name that was already declared earlier
                    // in the same CREATE, rebind to the existing node
                    // instead of creating an unbound duplicate.
                    // Repro: `CREATE (a:X),(b:X),(a)-[:R]->(b)` — the
                    // `(a)` and `(b)` in the edge pattern must bind to
                    // the first two nodes, not produce two extra
                    // anonymous :X nodes. Tracked in
                    // phase6_nexus-create-bound-var-duplication.
                    if let Some(var) = &node.variable {
                        if let Some(&existing_id) = created_nodes.get(var) {
                            last_node_id = Some(existing_id);
                            continue;
                        }
                    }

                    // Phase 1.5.2: Build label bitmap with pre-allocated IDs
                    // All labels should already be in label_cache from batch allocation
                    let mut label_bits = 0u64;
                    let mut label_ids_for_update = Vec::new();
                    for label in &node.labels {
                        // Labels should already be in cache from batch allocation
                        // Fallback to individual lookup if not found (shouldn't happen, but be safe)
                        let label_id = if let Some(&id) = label_cache.get(label) {
                            id
                        } else {
                            // Fallback: individual lookup (shouldn't happen with batch allocation)
                            let id = self.catalog().get_or_create_label(label)?;
                            label_cache.insert(label.clone(), id);
                            id
                        };

                        if label_id < 64 {
                            label_bits |= 1u64 << label_id;
                        }
                        label_ids_for_update.push(label_id);
                    }

                    // Phase 1 Optimization: Pre-size properties Map to avoid reallocations
                    let properties = if let Some(props_map) = &node.properties {
                        let prop_count = props_map.properties.len();
                        let mut json_props = serde_json::Map::with_capacity(prop_count);
                        for (key, value_expr) in &props_map.properties {
                            let json_value =
                                self.resolve_standalone_create_property(value_expr, params)?;
                            json_props.insert(key.clone(), json_value);
                        }
                        tracing::trace!(
                            "execute_create_pattern_internal: creating node with variable {:?}, labels {:?}, properties={:?}",
                            node.variable,
                            node.labels,
                            serde_json::Value::Object(json_props.clone())
                        );
                        serde_json::Value::Object(json_props)
                    } else {
                        tracing::trace!(
                            "execute_create_pattern_internal: creating node with variable {:?}, labels {:?}, NO PROPERTIES",
                            node.variable,
                            node.labels
                        );
                        serde_json::Value::Null
                    };

                    // Register every property key with the catalog so
                    // `db.propertyKeys()` sees it — mirrors the label
                    // registration above. See
                    // `Catalog::register_property_keys`.
                    self.catalog().register_property_keys(&properties);

                    // Check constraints before creating node
                    self.check_constraints(&label_ids_for_update, &properties)?;

                    // Create the node — route through the external-id path when
                    // an `_id` expression was present in the pattern.
                    let node_id = if let Some(ref ext) = external_id {
                        // Phase 4.4: only the first node in the pattern may
                        // carry the external id; the parser places it there.
                        let used = ext_id_consumed.replace(true);
                        if used {
                            return Err(Error::executor(
                                "_id can be set on at most one node per CREATE pattern",
                            ));
                        }
                        self.store_mut()
                            .create_node_with_label_bits_and_external_id(
                                &mut tx,
                                label_bits,
                                properties.clone(),
                                Some(ext.clone()),
                                policy,
                                self.catalog(),
                            )?
                    } else {
                        self.store_mut().create_node_with_label_bits(
                            &mut tx,
                            label_bits,
                            properties.clone(),
                        )?
                    };

                    tracing::trace!(
                        "execute_create_pattern_internal: created node_id={}, variable={:?}",
                        node_id,
                        node.variable
                    );

                    // phase6_fulltext-wal-integration §4 — auto-populate
                    // every registered FTS index whose label/property
                    // set matches this node. See
                    // `Executor::fts_autopopulate_node` for the match
                    // rule and error-containment policy.
                    self.fts_autopopulate_node(node_id, &label_ids_for_update, &properties);
                    // phase6_spatial-index-autopopulate §2 — same
                    // pattern for R-tree indexes.
                    self.spatial_autopopulate_node(node_id, &label_ids_for_update, &properties);
                    // phase20_knn-write-path-wiring §2.4 — same
                    // pattern for the active vector (HNSW) index.
                    self.knn_autopopulate_node(node_id, &label_ids_for_update, &properties);

                    // Phase 1 Optimization: Batch catalog metadata updates (defer to end)
                    for label_id in &label_ids_for_update {
                        *label_count_updates.entry(*label_id).or_insert(0) += 1;
                    }

                    if !label_ids_for_update.is_empty() {
                        created_nodes_with_labels.push((node_id, label_ids_for_update));
                    }

                    // Store node ID if variable exists
                    if let Some(var) = &node.variable {
                        created_nodes.insert(var.clone(), node_id);
                    }

                    // Track last node for relationship creation
                    last_node_id = Some(node_id);
                }
                parser::PatternElement::Relationship(rel) => {
                    // Neo4j requires an explicit relationship direction in
                    // CREATE; the undirected `-[:TYPE]-` form has no defined
                    // write orientation and is rejected outright rather than
                    // silently defaulting to one. Checked before either
                    // endpoint node is resolved/created so an invalid
                    // pattern never has partial side effects.
                    if matches!(rel.direction, parser::RelationshipDirection::Both) {
                        return Err(Error::CypherExecution(
                            "CREATE requires an explicit relationship direction \
                             (`->` or `<-`); undirected `-[:TYPE]-` is not supported"
                                .to_string(),
                        ));
                    }

                    // Get source node (previous element should be a node)
                    let source_id = if i > 0 {
                        last_node_id.ok_or_else(|| {
                            Error::CypherExecution("Relationship must follow a node".to_string())
                        })?
                    } else {
                        return Err(Error::CypherExecution(
                            "Pattern must start with a node".to_string(),
                        ));
                    };

                    // Get target node (next element should be a node).
                    // If the target's variable is already in
                    // `created_nodes`, rebind instead of creating a
                    // duplicate — mirrors the bound-variable fix in
                    // the Node branch above. Without this branch the
                    // source-side fix is asymmetric and still leaks
                    // one unbound `:Label` node per edge.
                    // Tracked in phase6_nexus-create-bound-var-duplication.
                    let target_id = if i + 1 < pattern.elements.len() {
                        if let parser::PatternElement::Node(target_node) = &pattern.elements[i + 1]
                        {
                            let bound_target_id = target_node
                                .variable
                                .as_ref()
                                .and_then(|var| created_nodes.get(var).copied());

                            if let Some(existing_id) = bound_target_id {
                                // Skip the duplicate node creation —
                                // the outer loop still needs to
                                // advance past this pattern element
                                // in the next iteration, so the
                                // skip-flag contract stays intact.
                                last_node_id = Some(existing_id);
                                skip_next_node = true;
                                existing_id
                            } else {
                                // Phase 1 Optimization: Build label bitmap with cached lookups
                                let mut target_label_bits = 0u64;
                                let mut target_label_ids_for_update = Vec::new();
                                for label in &target_node.labels {
                                    let label_id = if let Some(&cached_id) = label_cache.get(label)
                                    {
                                        cached_id
                                    } else {
                                        let id = self.catalog().get_or_create_label(label)?;
                                        label_cache.insert(label.clone(), id);
                                        id
                                    };

                                    if label_id < 64 {
                                        target_label_bits |= 1u64 << label_id;
                                    }
                                    target_label_ids_for_update.push(label_id);
                                }

                                let target_properties = if let Some(props_map) =
                                    &target_node.properties
                                {
                                    let prop_count = props_map.properties.len();
                                    let mut json_props = serde_json::Map::with_capacity(prop_count);
                                    for (key, value_expr) in &props_map.properties {
                                        let json_value = self.resolve_standalone_create_property(
                                            value_expr, params,
                                        )?;
                                        json_props.insert(key.clone(), json_value);
                                    }
                                    serde_json::Value::Object(json_props)
                                } else {
                                    serde_json::Value::Null
                                };

                                // Register every property key with the
                                // catalog so `db.propertyKeys()` sees it.
                                // See `Catalog::register_property_keys`.
                                self.catalog().register_property_keys(&target_properties);

                                let tid = self.store_mut().create_node_with_label_bits(
                                    &mut tx,
                                    target_label_bits,
                                    target_properties.clone(),
                                )?;
                                self.fts_autopopulate_node(
                                    tid,
                                    &target_label_ids_for_update,
                                    &target_properties,
                                );
                                self.spatial_autopopulate_node(
                                    tid,
                                    &target_label_ids_for_update,
                                    &target_properties,
                                );
                                self.knn_autopopulate_node(
                                    tid,
                                    &target_label_ids_for_update,
                                    &target_properties,
                                );

                                for label_id in &target_label_ids_for_update {
                                    *label_count_updates.entry(*label_id).or_insert(0) += 1;
                                }

                                if !target_label_ids_for_update.is_empty() {
                                    created_nodes_with_labels
                                        .push((tid, target_label_ids_for_update));
                                }

                                if let Some(var) = &target_node.variable {
                                    created_nodes.insert(var.clone(), tid);
                                }

                                last_node_id = Some(tid);
                                skip_next_node = true;
                                tid
                            }
                        } else {
                            return Err(Error::CypherExecution(
                                "Relationship must be followed by a node".to_string(),
                            ));
                        }
                    } else {
                        return Err(Error::CypherExecution(
                            "Pattern must end with a node".to_string(),
                        ));
                    };

                    // Honour the parsed arrow direction when choosing which
                    // endpoint is the relationship's source vs. target.
                    // `source_id`/`target_id` above are resolved in
                    // pattern/array order (left element, right element),
                    // which is only correct for `Outgoing` (`->`).
                    // `(x)<-[:T]-(y)` must store the edge as y->x, so an
                    // `Incoming` direction swaps the pair. `Both` was
                    // already rejected above.
                    let (source_id, target_id) = match rel.direction {
                        parser::RelationshipDirection::Incoming => (target_id, source_id),
                        _ => (source_id, target_id),
                    };

                    // Get relationship type
                    let rel_type = rel.types.first().ok_or_else(|| {
                        Error::CypherExecution("Relationship must have a type".to_string())
                    })?;

                    // Phase 1.5.2: Use pre-allocated type ID
                    // Type should already be in cache from batch allocation
                    // Fallback to individual lookup if not found (shouldn't happen, but be safe)
                    let type_id = if let Some(&id) = label_cache.get(rel_type) {
                        id
                    } else {
                        // Fallback: individual lookup (shouldn't happen with batch allocation)
                        let id = self.catalog().get_or_create_type(rel_type)?;
                        label_cache.insert(rel_type.to_string(), id);
                        id
                    };

                    // Phase 1 Optimization: Pre-size properties Map for relationships
                    let rel_properties = if let Some(props_map) = &rel.properties {
                        let prop_count = props_map.properties.len();
                        let mut json_props = serde_json::Map::with_capacity(prop_count);
                        for (key, value_expr) in &props_map.properties {
                            let json_value =
                                self.resolve_standalone_create_property(value_expr, params)?;
                            json_props.insert(key.clone(), json_value);
                        }
                        serde_json::Value::Object(json_props)
                    } else {
                        serde_json::Value::Null
                    };

                    // Register every property key with the catalog so
                    // `db.propertyKeys()` sees relationship-property keys
                    // too. See `Catalog::register_property_keys`.
                    self.catalog().register_property_keys(&rel_properties);

                    // Acquire row locks on source and target nodes before creating relationship
                    let (_source_lock, _target_lock) =
                        self.acquire_relationship_locks(source_id, target_id)?;

                    // Create the relationship (locks held by guards)
                    let rel_id = self.store_mut().create_relationship(
                        &mut tx,
                        source_id,
                        target_id,
                        type_id,
                        rel_properties,
                    )?;

                    // Phase 1 Optimization: Batch catalog metadata updates (defer to end),
                    // mirrors the node-count accumulation above.
                    *rel_count_updates.entry(type_id).or_insert(0) += 1;

                    // Locks are released when guards are dropped

                    // Store relationship ID if variable exists
                    if let Some(var) = &rel.variable {
                        created_relationships.insert(
                            var.clone(),
                            RelationshipInfo {
                                id: rel_id,
                                source_id,
                                target_id,
                                type_id,
                            },
                        );
                    }
                }
            }
        }

        // Commit transaction
        tx_mgr.commit(&mut tx)?;

        // Phase 1 Optimization: Batch apply catalog metadata updates (reduces I/O)
        // Convert HashMap to Vec for batch update
        let updates: Vec<(u32, u32)> = label_count_updates.into_iter().collect();
        if !updates.is_empty() {
            if let Err(e) = self.catalog().batch_increment_node_counts(&updates) {
                // Log error but don't fail the operation
                tracing::warn!("Failed to batch update node counts: {}", e);
            }
        }

        // Symmetric batch flush for relationship-type counts — see
        // `rel_count_updates` above.
        let rel_updates: Vec<(TypeId, u32)> = rel_count_updates.into_iter().collect();
        if !rel_updates.is_empty() {
            if let Err(e) = self.catalog().batch_increment_rel_counts(&rel_updates) {
                tracing::warn!("Failed to batch update relationship counts: {}", e);
            }
        }

        // PERFORMANCE OPTIMIZATION: Use async flush for better throughput
        // The transaction commit above ensures data integrity
        // Async flush triggers write without blocking on OS confirmation
        // Memory barrier below ensures visibility across threads
        self.store_mut().flush_async()?;

        // Update label index with created nodes. Use the list we accumulated
        // during node creation rather than re-reading `NodeRecord.label_bits`:
        // the bitmap is a u64 and drops every label with `label_id >= 64`,
        // which silently breaks MATCH on those labels (phase6 §1). The
        // tracked list carries the full set of label IDs, so labels above
        // the 64-bitmap cap land in the index correctly.
        for (node_id, label_ids) in &created_nodes_with_labels {
            self.label_index_mut().add_node(*node_id, label_ids)?;
        }

        Ok(())
    }
}
