//! Row-aware CREATE — `create_pattern_node_with_context` (shared node
//! creation helper) and `execute_create_with_context`, which drives a
//! `CREATE` clause fed by an upstream `MATCH`/`UNWIND` row.

use super::super::super::context::{ExecutionContext, RelationshipInfo};
use super::super::super::engine::Executor;
use super::super::super::parser;
use super::super::super::types::Row;
use crate::catalog::TypeId;
use crate::{Error, Result};
use serde_json::{Map, Value};

impl Executor {
    /// Create a single fresh node for a CREATE pattern element, resolved
    /// against the current MATCH-driven `row` scope.
    ///
    /// Shared by `execute_create_with_context`'s node-processing arm and
    /// its relationship-processing arm (the latter calls this to create a
    /// relationship's target/source node inline when it isn't already
    /// bound — see the fix for
    /// `phase0_fix-match-create-inline-node-rel-dropped`). Keeping node
    /// creation in one place means both call sites stay identical in
    /// label resolution, property resolution, FTS/spatial autopopulation
    /// and label-index bookkeeping — the exact kind of divergence that
    /// let the relationship-arm's inline-target case silently skip all of
    /// this before.
    #[allow(clippy::too_many_arguments)]
    fn create_pattern_node_with_context(
        &self,
        node: &parser::NodePattern,
        row: &std::collections::HashMap<String, Value>,
        params: &std::collections::HashMap<String, Value>,
        external_id: Option<&crate::storage::external_id::ExternalId>,
        policy: crate::storage::external_id::ConflictPolicy,
        tx: &mut crate::transaction::Transaction,
        created_nodes_with_labels: &mut Vec<(u64, Vec<u32>)>,
    ) -> Result<u64> {
        let label_ids: Vec<u32> = node
            .labels
            .iter()
            .filter_map(|l| self.catalog().get_or_create_label(l).ok())
            .collect();

        let mut label_bits = 0u64;
        for label_id in &label_ids {
            if *label_id < 64 {
                label_bits |= 1u64 << label_id;
            }
        }

        let properties = if let Some(props_map) = &node.properties {
            let mut resolved = Map::with_capacity(props_map.properties.len());
            for (k, v) in &props_map.properties {
                let val = self.resolve_property_expr_for_create(v, row, params)?;
                resolved.insert(k.clone(), val);
            }
            Value::Object(resolved)
        } else {
            Value::Object(Map::new())
        };

        let node_id = if let Some(ext) = external_id {
            self.store_mut()
                .create_node_with_label_bits_and_external_id(
                    tx,
                    label_bits,
                    properties.clone(),
                    Some(ext.clone()),
                    policy,
                    self.catalog(),
                )?
        } else {
            self.store_mut()
                .create_node_with_label_bits(tx, label_bits, properties.clone())?
        };

        self.fts_autopopulate_node(node_id, &label_ids, &properties);
        self.spatial_autopopulate_node(node_id, &label_ids, &properties);
        self.knn_autopopulate_node(node_id, &label_ids, &properties);
        if !label_ids.is_empty() {
            created_nodes_with_labels.push((node_id, label_ids));
        }

        Ok(node_id)
    }

    #[tracing::instrument(skip_all, level = "debug")]
    pub(in crate::executor) fn execute_create_with_context(
        &self,
        context: &mut ExecutionContext,
        pattern: &parser::Pattern,
        external_id: Option<crate::storage::external_id::ExternalId>,
        policy: crate::storage::external_id::ConflictPolicy,
        has_downstream_row_consumer: bool,
    ) -> Result<()> {
        // Note: TransactionManager is now accessed via self.transaction_manager() (shared)
        use serde_json::Value as JsonValue;

        // CRITICAL FIX: Always try to use context.variables first for MATCH...CREATE
        // The variables contain the full node objects with _nexus_id, while result_set.rows
        // may contain only projected values (strings) without _nexus_id.
        // Only fall back to result_set.rows if variables are empty.

        tracing::trace!(
            "execute_create_with_context: variables={:?}, result_set.rows={}",
            context.variables.keys().collect::<Vec<_>>(),
            context.result_set.rows.len()
        );

        let current_rows = if !context.variables.is_empty() {
            // PERFORMANCE OPTIMIZATION: Fast-path for simple single-value variables
            // This avoids the expensive materialize_rows_from_variables() for common cases
            // like MATCH (p:Person {name: 'X'}), (c:Company {name: 'Y'}) CREATE ...
            let all_single_values = context
                .variables
                .values()
                .all(|v| !matches!(v, JsonValue::Array(_)));

            if all_single_values {
                // Fast path: directly create a single row from variables
                let mut row = std::collections::HashMap::with_capacity(context.variables.len());
                let mut has_node_ids = false;
                for (var, value) in &context.variables {
                    if let JsonValue::Object(obj) = value {
                        if obj.contains_key("_nexus_id") {
                            has_node_ids = true;
                        }
                    }
                    row.insert(var.clone(), value.clone());
                }
                if has_node_ids {
                    vec![row]
                } else if !context.result_set.rows.is_empty() {
                    // Fallback to result_set if no node IDs
                    let columns = context.result_set.columns.clone();
                    context
                        .result_set
                        .rows
                        .iter()
                        .map(|row| self.row_to_map(row, &columns))
                        .collect()
                } else {
                    vec![row]
                }
            } else {
                // Slow path: array variables — one CREATE per driving row.
                //
                // Use the ALIGNED materialisation (zip columns by index), NOT
                // `materialize_rows_from_variables` (which re-crosses). By the
                // time a CREATE runs, the read pipeline has already produced
                // the driving rows as aligned columns: a comma-joined
                // `MATCH (a:A), (b:B)` materialises its cartesian product into
                // `a = [a1, a2]`, `b = [b1, b2]` (index i = one row), and an
                // UNWIND aligns the same way. Re-crossing those equal-length
                // multi-element columns turned N driving rows into N² CREATEs —
                // `MATCH (a:A), (b:B) CREATE (a)-[:R]->(b)` over 3 A's and 1 B
                // wrote 9 edges, not 3. The CREATE never introduces its own
                // cartesian; the MATCH already did. Mirrors the read path's
                // `seed_scan_main_loop`, which zips for exactly this reason.
                let materialized = self.materialize_aligned_rows(context);

                // Verify materialized rows have node objects with _nexus_id
                let has_node_ids = materialized.iter().any(|row| {
                    row.values().any(|v| {
                        if let JsonValue::Object(obj) = v {
                            obj.contains_key("_nexus_id")
                        } else {
                            false
                        }
                    })
                });

                if has_node_ids {
                    materialized
                } else if !context.result_set.rows.is_empty() {
                    let columns = context.result_set.columns.clone();
                    context
                        .result_set
                        .rows
                        .iter()
                        .map(|row| self.row_to_map(row, &columns))
                        .collect()
                } else {
                    materialized
                }
            }
        } else if !context.result_set.rows.is_empty() {
            // No variables - use result_set.rows
            let columns = context.result_set.columns.clone();
            let rows: Vec<_> = context
                .result_set
                .rows
                .iter()
                .map(|row| self.row_to_map(row, &columns))
                .collect();
            tracing::trace!(
                "execute_create_with_context: no variables, using {} rows from result_set.rows",
                rows.len()
            );
            rows
        } else {
            // No variables and no rows
            tracing::trace!("execute_create_with_context: no variables and no rows");
            Vec::new()
        };

        // If no rows from MATCH, nothing to create
        if current_rows.is_empty() {
            return Ok(());
        }

        // DEBUG: Print row contents to see if they contain _nexus_id
        for (idx, row) in current_rows.iter().enumerate() {}

        // PERFORMANCE OPTIMIZATION: Reuse shared transaction manager instead of creating new
        // This saves ~1-2ms per operation by avoiding TransactionManager::new() overhead
        let mut tx_mgr = self.transaction_manager().lock();
        let mut tx = tx_mgr.begin_write()?;

        // Track (node_id, label_ids) for every node we actually create so the
        // label-bitmap index can be updated in a single pass after the
        // transaction commits (MATCH queries depend on this index; without
        // the update UNWIND + CREATE creates nodes the planner can't find).
        let mut created_nodes_with_labels: Vec<(u64, Vec<u32>)> = Vec::new();

        // This CREATE path also writes relationships directly to the
        // record store (bypassing `Engine::create_relationship`), so
        // `catalog.rel_counts` is never updated without this accumulator
        // and its post-commit flush below — mirrors
        // `execute_create_pattern_internal`'s `rel_count_updates`.
        let mut rel_count_updates: std::collections::HashMap<TypeId, u32> =
            std::collections::HashMap::new();

        // Node-count twin of `rel_count_updates` above: this path also
        // writes nodes directly to the record store (bypassing
        // `Engine::create_node`), so `catalog.node_counts` stays a lower
        // bound (never incremented past 1 per label) without this
        // accumulator and its post-commit flush below — mirrors
        // `execute_create_pattern_internal`'s `label_count_updates`.
        // Populated once after the row loop below from
        // `created_nodes_with_labels`, which both node-creation call
        // sites in this path (`create_pattern_node_with_context`) already
        // populate with the exact (node_id, label_ids) pair for every
        // node created — counting from it is equivalent to incrementing
        // at each call site, without duplicating label-id extraction.
        let mut label_count_updates: std::collections::HashMap<u32, u32> =
            std::collections::HashMap::new();

        // For each row in the MATCH result, create the pattern
        // PERFORMANCE OPTIMIZATION: Pre-calculate expected capacity for node_ids
        let expected_vars = pattern
            .elements
            .iter()
            .filter(|e| matches!(e, parser::PatternElement::Node(n) if n.variable.is_some()))
            .count();

        for row in current_rows.iter() {
            // Pre-allocate HashMap with expected capacity
            let mut node_ids: std::collections::HashMap<String, u64> =
                std::collections::HashMap::with_capacity(expected_vars);

            // First, resolve existing node variables from the row
            for (var_name, var_value) in row {
                if let JsonValue::Object(obj) = var_value {
                    if let Some(JsonValue::Number(id)) = obj.get("_nexus_id") {
                        if let Some(node_id) = id.as_u64() {
                            node_ids.insert(var_name.clone(), node_id);
                        }
                    }
                }
            }

            // DEBUG: Print node_ids after extraction

            // CRITICAL FIX: If no node IDs were resolved from the row and the pattern requires
            // existing nodes from MATCH, skip this row (Filter removed all valid rows)
            // This prevents CREATE from executing when Filter filtered out all rows
            if node_ids.is_empty() {
                // Check if pattern requires existing nodes (has variables that should come from MATCH)
                let pattern_requires_existing_nodes = pattern.elements.iter().any(|elem| {
                    match elem {
                        parser::PatternElement::Node(node) => {
                            if let Some(_var) = &node.variable {
                                // If node has no properties or labels, it's likely from MATCH
                                // If it has properties/labels, it's a new node to create
                                node.properties.is_none() && node.labels.is_empty()
                            } else {
                                false
                            }
                        }
                        parser::PatternElement::Relationship(_) => false,
                        parser::PatternElement::QuantifiedGroup(_) => false,
                    }
                });

                if pattern_requires_existing_nodes {
                    continue; // Skip this row - Filter removed all valid matches
                }
            }

            // Now process the pattern elements to create new nodes and
            // relationships. `last_node_id` (not a variable-name lookup)
            // tracks the most recently resolved node regardless of
            // whether it carries a variable, mirroring
            // `execute_create_pattern_internal`'s `last_node_id` above —
            // this lets a relationship anchor on an anonymous node too.
            // `skip_next_node` is set whenever the relationship arm below
            // already resolved (bound-reuse) or created (inline) the
            // node at `idx + 1`, so the following Node-arm iteration for
            // that same element doesn't try to create it a second time.
            let mut last_node_id: Option<u64> = None;
            let mut skip_next_node = false;

            for (idx, element) in pattern.elements.iter().enumerate() {
                match element {
                    parser::PatternElement::QuantifiedGroup(_) => {
                        return Err(Error::Executor(
                            "ERR_QPP_NOT_IN_CREATE: quantified path patterns \
                             are read-only; use a MATCH clause instead"
                                .to_string(),
                        ));
                    }
                    parser::PatternElement::Node(node) => {
                        if skip_next_node {
                            // Already resolved by the relationship arm
                            // that precedes this element (bound-reuse or
                            // inline creation) — `last_node_id` is
                            // already up to date.
                            skip_next_node = false;
                            continue;
                        }

                        // Skip the create when the variable is already
                        // bound by an upstream MATCH (existing-node
                        // reference). Otherwise create a fresh node —
                        // for both named and anonymous fresh shapes.
                        // The anonymous arm is exercised by CREATE
                        // patterns inside `CALL { … }` subqueries
                        // (phase6_opencypher-subquery-transactions);
                        // before that lift the dispatch path silently
                        // dropped them on the floor.
                        let already_bound = node
                            .variable
                            .as_ref()
                            .is_some_and(|v| node_ids.contains_key(v));
                        let node_id = if already_bound {
                            // `already_bound` guarantees the variable and
                            // the node_ids entry both exist; the
                            // `ok_or_else` only guards `?` ergonomics.
                            node.variable
                                .as_ref()
                                .and_then(|v| node_ids.get(v).copied())
                                .ok_or_else(|| {
                                    Error::executor(
                                        "CREATE: node marked already-bound but missing from \
                                         node_ids (invariant violation)",
                                    )
                                })?
                        } else {
                            let new_id = self.create_pattern_node_with_context(
                                node,
                                row,
                                &context.params,
                                external_id.as_ref(),
                                policy,
                                &mut tx,
                                &mut created_nodes_with_labels,
                            )?;
                            // phase6_opencypher-subquery-transactions §3 —
                            // register the inverse op so a failing
                            // `CALL { … } IN TRANSACTIONS` batch can
                            // unwind this node. No-op when the
                            // executor is not running inside a
                            // batch attempt.
                            context.push_undo(
                                super::super::super::context::CompensatingUndoOp::DeleteNode(
                                    new_id,
                                ),
                            );
                            if let Some(var) = &node.variable {
                                node_ids.insert(var.clone(), new_id);
                            }
                            new_id
                        };

                        last_node_id = Some(node_id);
                    }
                    parser::PatternElement::Relationship(rel) => {
                        // Create relationship between last_node and next_node
                        if let Some(rel_type) = rel.types.first() {
                            let type_id = self.catalog().get_or_create_type(rel_type)?;

                            // Extract relationship properties. Errors
                            // propagate instead of silently dropping the
                            // key: the previous `.ok()` discarded ANY
                            // unresolvable property value — including a
                            // function-call value like
                            // `duration({days: 1})` — leaving the
                            // relationship persisted with that key simply
                            // missing rather than surfacing the
                            // evaluation failure. Mirrors
                            // `create_pattern_node_with_context`'s node
                            // property resolution just above.
                            let properties = if let Some(props_map) = &rel.properties {
                                let mut resolved = Map::with_capacity(props_map.properties.len());
                                for (k, v) in &props_map.properties {
                                    let val = self.resolve_property_expr_for_create(
                                        v,
                                        row,
                                        &context.params,
                                    )?;
                                    resolved.insert(k.clone(), val);
                                }
                                JsonValue::Object(resolved)
                            } else {
                                JsonValue::Object(Map::new())
                            };

                            let Some(source_id) = last_node_id else {
                                tracing::warn!(
                                    "execute_create_with_context: relationship at pattern \
                                     index {idx} has no preceding node"
                                );
                                continue;
                            };

                            // Neo4j requires an explicit relationship
                            // direction in CREATE; the undirected
                            // `-[:TYPE]-` form has no defined write
                            // orientation and is rejected outright rather
                            // than silently defaulting to one. Checked
                            // before the target node is resolved/created so
                            // an invalid pattern never has partial side
                            // effects — mirrors the standalone-CREATE fix
                            // in `execute_create_pattern_internal`.
                            if matches!(rel.direction, parser::RelationshipDirection::Both) {
                                return Err(Error::CypherExecution(
                                    "CREATE requires an explicit relationship direction \
                                     (`->` or `<-`); undirected `-[:TYPE]-` is not supported"
                                        .to_string(),
                                ));
                            }

                            // Resolve the target node — reuse it if its
                            // variable is already bound (existing-node
                            // reference, the pre-existing working case),
                            // otherwise create it INLINE right here
                            // rather than deferring to the Node arm's
                            // next iteration. This is the fix for
                            // phase0_fix-match-create-inline-node-rel-dropped:
                            // previously the target lookup only ever
                            // checked `node_ids` (populated by the Node
                            // arm on ITS OWN turn, which for a fresh
                            // target runs strictly AFTER this
                            // relationship's turn), so a target created
                            // inline in the same CREATE clause
                            // (`(bound)-[:T]->(new:Label {...})`) was
                            // never found here — the relationship write
                            // was silently skipped (a `tracing::warn!`
                            // only) while the target node still got
                            // created on the next loop iteration,
                            // producing the reported silent data loss.
                            let target_id = if idx + 1 < pattern.elements.len() {
                                if let parser::PatternElement::Node(target_node) =
                                    &pattern.elements[idx + 1]
                                {
                                    let bound_target_id = target_node
                                        .variable
                                        .as_ref()
                                        .and_then(|var| node_ids.get(var).copied());

                                    if let Some(existing_id) = bound_target_id {
                                        skip_next_node = true;
                                        last_node_id = Some(existing_id);
                                        Some(existing_id)
                                    } else {
                                        let new_id = self.create_pattern_node_with_context(
                                            target_node,
                                            row,
                                            &context.params,
                                            external_id.as_ref(),
                                            policy,
                                            &mut tx,
                                            &mut created_nodes_with_labels,
                                        )?;
                                        context.push_undo(
                                            super::super::super::context::CompensatingUndoOp::DeleteNode(
                                                new_id,
                                            ),
                                        );
                                        if let Some(var) = &target_node.variable {
                                            node_ids.insert(var.clone(), new_id);
                                        }
                                        skip_next_node = true;
                                        last_node_id = Some(new_id);
                                        Some(new_id)
                                    }
                                } else {
                                    tracing::warn!(
                                        "execute_create_with_context: Next element is not a Node"
                                    );
                                    None
                                }
                            } else {
                                tracing::warn!(
                                    "execute_create_with_context: No next element after relationship"
                                );
                                None
                            };

                            let Some(target_id) = target_id else {
                                continue;
                            };

                            // Honour the parsed arrow direction when
                            // choosing which endpoint is the relationship's
                            // source vs. target — mirrors the fix in
                            // `execute_create_pattern_internal`.
                            // `source_id`/`target_id` above are resolved in
                            // pattern/array order (preceding element,
                            // following element), which is only correct
                            // for `Outgoing` (`->`). `(x)<-[:T]-(y)` must
                            // store the edge as y->x, so an `Incoming`
                            // direction swaps the pair. `Both` was already
                            // rejected above.
                            let (source_id, target_id) = match rel.direction {
                                parser::RelationshipDirection::Incoming => (target_id, source_id),
                                _ => (source_id, target_id),
                            };

                            // PERFORMANCE OPTIMIZATION: Skip row-level locking when lock-free mode is enabled
                            // The transaction manager mutex already provides serialization
                            // Row locks are only needed for concurrent writers
                            let _locks = if !self.config.enable_lock_free_structures {
                                Some(self.acquire_relationship_locks(source_id, target_id)?)
                            } else {
                                None
                            };

                            // Create the relationship
                            let rel_id = self.store_mut().create_relationship(
                                &mut tx, source_id, target_id, type_id, properties,
                            )?;
                            *rel_count_updates.entry(type_id).or_insert(0) += 1;
                            context.push_undo(
                                super::super::super::context::CompensatingUndoOp::DeleteRelationship(
                                    rel_id,
                                ),
                            );
                            tracing::trace!(
                                "execute_create_with_context: relationship created successfully, rel_id={}",
                                rel_id
                            );

                            // CRITICAL FIX: Populate relationship variable if specified
                            // This ensures that queries like CREATE (a)-[r:KNOWS]->(b) RETURN r work correctly
                            if let Some(rel_var) = &rel.variable {
                                if !rel_var.is_empty() {
                                    let rel_info = RelationshipInfo {
                                        id: rel_id,
                                        source_id,
                                        target_id,
                                        type_id,
                                    };
                                    if let Ok(rel_value) =
                                        self.read_relationship_as_value(&rel_info)
                                    {
                                        // Store relationship in context for RETURN clause
                                        context.variables.insert(rel_var.clone(), rel_value);
                                    }
                                }
                            }

                            // Locks are released when guards are dropped
                            // Relationship created successfully
                        }
                    }
                }
            }
        }

        // Derive per-label node counts from every node actually created
        // above. `created_nodes_with_labels` already carries the exact
        // (node_id, label_ids) pair for both node-creation call sites in
        // this path (fresh Node-arm creates and inline
        // relationship-target creates), so a single pass here is
        // equivalent to incrementing `label_count_updates` at each call
        // site.
        for (_, label_ids) in &created_nodes_with_labels {
            for label_id in label_ids {
                *label_count_updates.entry(*label_id).or_insert(0) += 1;
            }
        }

        // Commit transaction
        tx_mgr.commit(&mut tx)?;
        drop(tx_mgr);

        // Batch-flush relationship-type counts accumulated above — see
        // `rel_count_updates`. One catalog commit for the whole batch,
        // mirroring `execute_create_pattern_internal`'s post-commit flush.
        let rel_updates: Vec<(TypeId, u32)> = rel_count_updates.into_iter().collect();
        if !rel_updates.is_empty() {
            if let Err(e) = self.catalog().batch_increment_rel_counts(&rel_updates) {
                tracing::warn!("Failed to batch update relationship counts: {}", e);
            }
        }

        // Symmetric batch flush for per-label node counts — see
        // `label_count_updates` above. One catalog commit for the whole
        // batch, mirroring `execute_create_pattern_internal`'s
        // post-commit flush (and the `rel_updates` flush directly above).
        let node_updates: Vec<(u32, u32)> = label_count_updates.into_iter().collect();
        if !node_updates.is_empty() {
            if let Err(e) = self.catalog().batch_increment_node_counts(&node_updates) {
                tracing::warn!("Failed to batch update node counts: {}", e);
            }
        }

        // Register the created nodes in the label-bitmap index so subsequent
        // MATCH queries can find them. The engine's `create_node` path does
        // this automatically, but the Cypher CREATE path goes through the
        // storage layer directly and must maintain the index itself.
        if !created_nodes_with_labels.is_empty() {
            for (node_id, label_ids) in &created_nodes_with_labels {
                if let Err(e) = self.label_index_mut().add_node(*node_id, label_ids) {
                    tracing::warn!(
                        node_id = *node_id,
                        error = %e,
                        "execute_create_with_context: failed to update label index",
                    );
                }
            }
        }

        // PERFORMANCE OPTIMIZATION: Use async flush instead of sync flush
        // The sync flush was costing ~15-20ms per relationship creation
        // Async flush triggers the write but doesn't wait for OS confirmation
        // Data integrity is still maintained by the transaction commit above
        // For critical durability, callers can explicitly call flush() after the query
        self.store_mut().flush_async()?;

        // Memory barrier to ensure writes are visible to subsequent reads
        // Using Acquire/Release is sufficient here since we're in single-writer context
        std::sync::atomic::fence(std::sync::atomic::Ordering::Release);

        // A write-only `MATCH ... CREATE` (no `RETURN`/`WITH` downstream)
        // must yield an EMPTY result set per openCypher/TCK semantics
        // (Create2[5]/[6]/[10]-[12] all assert "the result should be
        // empty" for this exact shape). Only synthesize `result_set` from
        // the bound variables when the caller confirms a downstream row
        // consumer actually exists — `Project`, `With`, or `Aggregate`
        // (a pure-aggregate RETURN like `RETURN count(*)` plans as a bare
        // `Operator::Aggregate`, no `Project`). All three fall back to
        // reading `context.variables` when `result_set.rows`/`columns`
        // are empty (`Project`/`With` via `materialize_rows_from_variables`;
        // `Aggregate` via `execute_aggregate_with_projections`'s own
        // `has_match_columns` check in `operators/aggregate/core.rs`,
        // which treats an EMPTY `result_set.columns` as "no MATCH
        // projection to respect" and materializes from `context.variables`
        // instead) — so a RETURN-bearing statement's rows are unaffected
        // by skipping this synthesis. This also means the `else` branch's
        // `context.result_set.columns.clear()` below is load-bearing, not
        // cosmetic: it is exactly what keeps `has_match_columns` false for
        // that Aggregate fallback on the write-only path. A future
        // "optimization" that preserves the columns here (e.g. to avoid
        // reallocating) would silently break aggregation-after-write by
        // making the Aggregate operator think a real MATCH projection ran
        // and skip materializing from variables. Mirrors the same fix
        // applied to the standalone-CREATE fast path in
        // `executor/dispatch/operator_loop.rs`.
        if has_downstream_row_consumer {
            let mut columns: Vec<String> = context.variables.keys().cloned().collect();
            columns.sort(); // Ensure consistent column order

            if !columns.is_empty() {
                let mut row_values = Vec::new();
                for col in &columns {
                    if let Some(value) = context.variables.get(col) {
                        // CRITICAL FIX: Unwrap arrays to get the actual node object
                        // Variables from MATCH are arrays, but we need single objects
                        let unwrapped = match value {
                            JsonValue::Array(arr) if arr.len() == 1 => arr[0].clone(),
                            _ => value.clone(),
                        };
                        row_values.push(unwrapped);
                    } else {
                        row_values.push(JsonValue::Null);
                    }
                }
                context.result_set.columns = columns;
                context.result_set.rows = vec![Row { values: row_values }];
            } else {
                // No variables created - clear result_set
                context.result_set.rows.clear();
                context.result_set.columns.clear();
            }
        } else {
            // Write-only: no downstream Project/With/Aggregate to consume
            // a synthesized row, so leave result_set empty.
            context.result_set.rows.clear();
            context.result_set.columns.clear();
        }

        tracing::trace!(
            "After CREATE: result_set.columns={:?}, result_set.rows.len()={}, variables.len()={}",
            context.result_set.columns,
            context.result_set.rows.len(),
            context.variables.len()
        );

        Ok(())
    }
}
