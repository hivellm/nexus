//! MERGE execution: node MERGE, relationship MERGE (including the
//! `ON CREATE` / `ON MATCH` SET application), and the exact-edge lookup
//! MERGE relies on to decide match-vs-create. Extracted from
//! `engine/write_exec.rs`.

use super::super::Engine;
use crate::storage::external_id::ConflictPolicy;
use crate::{Error, Result, executor};
use serde_json::{Map, Value};
use std::collections::HashMap;

impl Engine {
    pub(super) fn process_merge_clause(
        &mut self,
        merge_clause: &executor::parser::MergeClause,
    ) -> Result<(String, Vec<u64>)> {
        let node_pattern = merge_clause
            .pattern
            .elements
            .iter()
            .find_map(|element| {
                if let executor::parser::PatternElement::Node(node) = element {
                    Some(node.clone())
                } else {
                    None
                }
            })
            .ok_or_else(|| Error::CypherExecution("MERGE requires a node pattern".to_string()))?;

        let variable = node_pattern
            .variable
            .clone()
            .ok_or_else(|| Error::CypherExecution("MERGE requires a variable alias".to_string()))?;

        // Null-key contract (Neo4j parity): MERGE cannot use a null property
        // value. Reject before match-or-create so behaviour is identical
        // whether or not an existing node would match. Mirrors Neo4j's
        // "Cannot merge node using null property value for <key>".
        if let Some(prop_map) = &node_pattern.properties {
            for (key, expr) in &prop_map.properties {
                if matches!(
                    self.expression_to_json_value(expr)?,
                    serde_json::Value::Null
                ) {
                    return Err(Error::CypherExecution(format!(
                        "Cannot merge node using null property value for {key}"
                    )));
                }
            }
        }

        // `_id` (issue #29): resolve the magic `_id` property the parser
        // hoisted out of `node_pattern.properties` into `external_id_expr`.
        // The external id is a stronger key than the property-based search
        // below — the search is now `_id`-blind, since the parser already
        // stripped `_id` out of `node_pattern.properties` — so a hit here
        // short-circuits that search entirely.
        let ext_id = merge_clause
            .external_id_expr
            .as_ref()
            .map(|expr| self.resolve_external_id(expr))
            .transpose()?;

        let existing_by_ext_id = if let Some(ext) = &ext_id {
            let txn = self.catalog.read_txn()?;
            let found = self.catalog.external_id_index().get_internal(&txn, ext)?;
            drop(txn);
            found
        } else {
            None
        };

        let mut node_ids = if let Some(id) = existing_by_ext_id {
            vec![id]
        } else {
            let mut ids = self.find_nodes_by_node_pattern(&node_pattern)?;
            ids.sort_unstable();
            ids.dedup();
            ids
        };

        if node_ids.is_empty() {
            let labels = node_pattern.labels.clone();
            let mut props = Map::new();
            if let Some(prop_map) = &node_pattern.properties {
                for (key, expr) in &prop_map.properties {
                    let value = self.expression_to_json_value(expr)?;
                    props.insert(key.clone(), value);
                }
            }
            // create_node_with_external_id already checks constraints, so
            // we can call it directly. `ConflictPolicy::Match` closes the
            // TOCTOU window between the `existing_by_ext_id` lookup above
            // and this create: if a concurrent MERGE raced in and won,
            // this falls back to the now-existing internal id instead of
            // erroring (`MergeClause` has no `conflict_policy` of its own —
            // find-or-create is always the semantics here).
            let node_id = self.create_node_with_external_id(
                labels,
                Value::Object(props),
                ext_id,
                ConflictPolicy::Match,
            )?;
            node_ids.push(node_id);

            if let Some(on_create) = &merge_clause.on_create {
                let mut ctx = HashMap::new();
                ctx.insert(variable.clone(), vec![node_id]);
                self.apply_set_clause(&ctx, &HashMap::new(), on_create)?;
            }
        } else if let Some(on_match) = &merge_clause.on_match {
            let mut ctx = HashMap::new();
            ctx.insert(variable.clone(), node_ids.clone());
            self.apply_set_clause(&ctx, &HashMap::new(), on_match)?;
        }

        Ok((variable, node_ids))
    }

    /// MERGE (find-or-create) a single node pattern with no `ON
    /// CREATE`/`ON MATCH` handling of its own (#25/G3). Used by
    /// [`Self::process_merge_relationship`] to resolve the endpoint nodes
    /// of a standalone relationship-MERGE pattern (`MERGE (a:L1 {..})-[r:T]->(b:L2
    /// {..})`) that has no preceding MATCH to bind them — `ON CREATE` /
    /// `ON MATCH` on the enclosing `MergeClause` still targets the
    /// relationship only, per the existing `process_merge_relationship`
    /// contract; mirrors the match-or-create logic in
    /// [`Self::process_merge_clause`] (including its `_id` external-id
    /// fast path) minus that per-clause SET handling.
    pub(super) fn merge_single_node(
        &mut self,
        node_pattern: &executor::parser::NodePattern,
    ) -> Result<u64> {
        // Null-key contract (Neo4j parity): MERGE cannot use a null
        // property value.
        if let Some(prop_map) = &node_pattern.properties {
            for (key, expr) in &prop_map.properties {
                if matches!(
                    self.expression_to_json_value(expr)?,
                    serde_json::Value::Null
                ) {
                    return Err(Error::CypherExecution(format!(
                        "Cannot merge node using null property value for {key}"
                    )));
                }
            }
        }

        // `_id` (relationship-MERGE endpoints): resolve the per-node `_id`
        // the parser hoisted out of `node_pattern.properties` into
        // `external_id_expr` (populated per-node for MERGE patterns — see
        // `extract_underscore_id_from_pattern`). A hit in the external-id
        // index short-circuits the property-based search below, mirroring
        // `Self::process_merge_clause`.
        let ext_id = node_pattern
            .external_id_expr
            .as_ref()
            .map(|expr| self.resolve_external_id(expr))
            .transpose()?;

        let existing_by_ext_id = if let Some(ext) = &ext_id {
            let txn = self.catalog.read_txn()?;
            let found = self.catalog.external_id_index().get_internal(&txn, ext)?;
            drop(txn);
            found
        } else {
            None
        };

        if let Some(id) = existing_by_ext_id {
            return Ok(id);
        }

        let mut node_ids = self.find_nodes_by_node_pattern(node_pattern)?;
        node_ids.sort_unstable();
        node_ids.dedup();

        if let Some(&id) = node_ids.first() {
            return Ok(id);
        }

        let labels = node_pattern.labels.clone();
        let mut props = Map::new();
        if let Some(prop_map) = &node_pattern.properties {
            for (key, expr) in &prop_map.properties {
                let value = self.expression_to_json_value(expr)?;
                props.insert(key.clone(), value);
            }
        }
        // `ConflictPolicy::Match` closes the TOCTOU window between the
        // `existing_by_ext_id` lookup above and this create — mirrors
        // `Self::process_merge_clause`. When `ext_id` is `None` this
        // behaves identically to a plain `create_node`: `create_node_inner`
        // only takes the external-id path when an id is actually supplied.
        self.create_node_with_external_id(
            labels,
            Value::Object(props),
            ext_id,
            ConflictPolicy::Match,
        )
    }

    /// Process MERGE with relationship pattern when nodes are already bound
    /// Returns Some((rel_variable, rel_id, rel_type)) if this is a relationship MERGE
    ///
    /// `rel_variable` is the empty string when the pattern's relationship has
    /// no bound variable (e.g. `MERGE (a)-[:T]->(b)`) — callers MUST treat an
    /// empty string as "do not bind", never insert it into `rel_context`
    /// under that key. The empty string is a safe sentinel: the parser never
    /// produces an empty identifier (`is_identifier_start` requires at least
    /// one letter/underscore), so it can never collide with a real,
    /// user-typed variable and can never be the `target` of a user `SET`
    /// item either.
    pub(super) fn process_merge_relationship(
        &mut self,
        merge_clause: &executor::parser::MergeClause,
        context: &mut HashMap<String, Vec<u64>>,
    ) -> Result<Option<Vec<(String, u64, String)>>> {
        // Check if pattern has: Node, Relationship, Node structure
        let elements = &merge_clause.pattern.elements;
        if elements.len() != 3 {
            return Ok(None);
        }

        // Extract source node, relationship, and target node
        let src_node = match &elements[0] {
            executor::parser::PatternElement::Node(n) => n,
            _ => return Ok(None),
        };
        let rel_pattern = match &elements[1] {
            executor::parser::PatternElement::Relationship(r) => r,
            _ => return Ok(None),
        };
        let dst_node = match &elements[2] {
            executor::parser::PatternElement::Node(n) => n,
            _ => return Ok(None),
        };

        // The relationship type is still mandatory: `MERGE ()-[r]->()` with
        // no type at all remains rejected here exactly as before — only the
        // *variable*-presence checks below were relaxed by this fix.
        let rel_type = match rel_pattern.types.first() {
            Some(t) => t.clone(),
            None => return Ok(None),
        };

        // G3 / anonymous-endpoint support — resolve source/destination node
        // ids. When an enclosing MATCH/UNWIND already bound the variable
        // (existing contract: present in `context` with a non-empty id
        // list), reuse it as-is. When the variable is bound but resolved to
        // ZERO nodes (e.g. a MATCH that found nothing), preserve the prior
        // behaviour of bailing out to the node-only MERGE fallback. When the
        // variable is not in `context` at all — including an ANONYMOUS
        // endpoint, which has no variable to look up in the first place —
        // this is a (partially or fully) standalone relationship-MERGE
        // pattern with no preceding binding for that endpoint (`MERGE
        // (a:L1 {..})-[r:T]->(b:L2 {..})`, harness cases 10/11, and the
        // anonymous-endpoint forms `MERGE (:L1{..})-[:T]->(b)` /
        // `MERGE (a)-[:T]->(:L2{..})` / fully-anonymous both sides) —
        // MERGE (find-or-create) the endpoint node inline via
        // `merge_single_node` (which also resolves per-node `_id` for
        // anonymous endpoints, same as named ones).
        //
        // Anonymous endpoints are resolved WITHOUT ever writing into the
        // shared `context` map: nothing in the query can reference a
        // variable it never wrote, and some `context` consumers (e.g.
        // `build_return_result_with_executor`'s `context.keys().next()`)
        // assume every key is a real, single, user-visible binding — an
        // extra synthesized key could be picked instead of the real one.
        // Resolve each endpoint to the FULL set of bound node ids, not just
        // the first. A preceding `MATCH (c:C), (d:D)` binds `c`/`d` to every
        // matched node (see `process_match_clause_multi`, which stores an
        // independent id list per variable), so the MERGE must run once per
        // (src, dst) pair — the cartesian product of the two lists, exactly
        // what the read-side relationship binder in `process_match_clause_multi`
        // already does. Collapsing each list to `ids[0]` silently dropped
        // every driving row after the first: `MATCH (c:C), (d:D) MERGE
        // (c)-[:S]->(d)` over 2 C's and 1 D created ONE edge, not two.
        //
        // A bound-but-EMPTY endpoint (a MATCH that found nothing) still bails
        // to the node-only MERGE fallback via `Ok(None)`, unchanged. An
        // anonymous or standalone endpoint (no prior binding) is find-or-create
        // through `merge_single_node`, yielding a single-element list — so a
        // pattern with one created endpoint and one bound-list endpoint fans
        // out across the list, and the all-single-node cases (inline `MERGE
        // (a:L{..})-[:T]->(b:L{..})`, and the per-row UNWIND path whose
        // `row_context` binds one node per endpoint) still produce exactly one
        // edge each.
        let resolve = |this: &mut Self,
                       node: &executor::parser::NodePattern,
                       context: &mut HashMap<String, Vec<u64>>|
         -> Result<Option<Vec<u64>>> {
            match &node.variable {
                Some(v) => match context.get(v) {
                    Some(ids) if !ids.is_empty() => Ok(Some(ids.clone())),
                    Some(_) => Ok(None),
                    None => {
                        let id = this.merge_single_node(node)?;
                        context.insert(v.clone(), vec![id]);
                        Ok(Some(vec![id]))
                    }
                },
                None => Ok(Some(vec![this.merge_single_node(node)?])),
            }
        };
        let Some(src_ids) = resolve(self, src_node, context)? else {
            return Ok(None);
        };
        let Some(dst_ids) = resolve(self, dst_node, context)? else {
            return Ok(None);
        };

        // Relationship variable: the real name when the pattern bound one,
        // otherwise the empty-string sentinel documented on this function.
        // Never inserted anywhere the caller could confuse it for a real
        // binding — see `apply_merge_relationship_set` and both call sites.
        let rel_var = rel_pattern.variable.clone().unwrap_or_default();

        let mut merged: Vec<(String, u64, String)> = Vec::new();
        for &raw_src in &src_ids {
            for &raw_dst in &dst_ids {
                // Honour the parsed arrow direction per pair. `raw_src`/`raw_dst`
                // are in pattern/array order (`elements[0]`, `elements[2]`),
                // correct for `Outgoing` (`->`); `MERGE (a)<-[:T]-(b)` must
                // write/match b->a, so `Incoming` swaps the pair. `Both`
                // (`-[:T]-`) keeps Neo4j's documented default of treating the
                // pattern as outgoing for both the existing-edge lookup and the
                // create fallback — see openCypher TCK Merge5 scenarios 11/12.
                let (src_id, dst_id) = match rel_pattern.direction {
                    executor::parser::RelationshipDirection::Incoming => (raw_dst, raw_src),
                    _ => (raw_src, raw_dst),
                };

                // Check if the relationship already exists.
                let rel_id =
                    if let Some(rid) = self.find_relationship_between(src_id, dst_id, &rel_type)? {
                        // Exists — apply ON MATCH SET to its properties (#14).
                        if let Some(on_match) = &merge_clause.on_match {
                            self.apply_merge_relationship_set(
                                context, &rel_var, rid, &rel_type, on_match,
                            )?;
                        }
                        rid
                    } else {
                        // Create with the pattern's inline properties (#25 —
                        // previously dropped), then layer ON CREATE SET on top
                        // (which may override them). `eval_write_value` resolves
                        // UNWIND `row.*` bindings on the per-row MERGE path.
                        let mut props_map = Map::new();
                        if let Some(prop_map) = &rel_pattern.properties {
                            for (key, expr) in &prop_map.properties {
                                props_map.insert(key.clone(), self.eval_write_value(expr)?);
                            }
                        }
                        let new_rel_id = self.create_relationship(
                            src_id,
                            dst_id,
                            rel_type.clone(),
                            Value::Object(props_map),
                        )?;
                        if let Some(on_create) = &merge_clause.on_create {
                            self.apply_merge_relationship_set(
                                context, &rel_var, new_rel_id, &rel_type, on_create,
                            )?;
                        }
                        new_rel_id
                    };

                merged.push((rel_var.clone(), rel_id, rel_type.clone()));
            }
        }

        Ok(Some(merged))
    }

    /// Apply a MERGE `ON CREATE` / `ON MATCH SET` clause following a
    /// relationship-MERGE (#14). Delegates to the general
    /// [`Self::apply_set_clause`] so a `SET` item may target either the
    /// relationship variable OR any node variable already visible in
    /// `context` (the pattern's own src/dst node variables when real, or any
    /// variable bound by an earlier clause in the same query) — e.g. `MERGE
    /// (a)-[:KNOWS]->(b) ON CREATE SET a.since = date()` now actually
    /// applies to `a`, which the old relationship-only SET application
    /// silently dropped.
    ///
    /// `rel_var` may be the empty-string sentinel documented on
    /// [`Self::process_merge_relationship`] for an anonymous relationship —
    /// in that case it is deliberately left out of the local rel-context
    /// below, so it can never be the `target` of a user `SET` item (matching
    /// the fact that an anonymous relationship has no user-referenceable
    /// name by construction).
    pub(super) fn apply_merge_relationship_set(
        &mut self,
        context: &HashMap<String, Vec<u64>>,
        rel_var: &str,
        rel_id: u64,
        rel_type: &str,
        set_clause: &executor::parser::SetClause,
    ) -> Result<()> {
        let mut local_rel_context: HashMap<String, Vec<(u64, String)>> = HashMap::new();
        if !rel_var.is_empty() {
            local_rel_context.insert(rel_var.to_string(), vec![(rel_id, rel_type.to_string())]);
        }
        self.apply_set_clause(context, &local_rel_context, set_clause)
    }

    /// Find a relationship of a specific type between two nodes
    pub(in crate::engine) fn find_relationship_between(
        &self,
        src_id: u64,
        dst_id: u64,
        rel_type: &str,
    ) -> Result<Option<u64>> {
        // #18: if a prior incremental relationship-index update failed, rebuild
        // the index from storage once before trusting the exact-edge fast path.
        self.heal_relationship_index_if_dirty();

        // Get the type ID
        let type_id = match self.catalog.get_type_id(rel_type)? {
            Some(id) => id,
            None => return Ok(None),
        };

        // Fast path: the exact-edge existence index gives an O(1) hint for
        // `(src, type, dst)`. It is only a hint — verify against storage (the
        // record may be deleted, or the index may not have been rebuilt yet
        // after a restart). On any mismatch fall through to the authoritative
        // chain walk so correctness never depends on the index being complete.
        if let Some(rid) = self
            .cache
            .relationship_index()
            .find_edge(src_id, type_id, dst_id)
        {
            if let Ok(rel) = self.storage.read_rel(rid) {
                if !rel.is_deleted()
                    && rel.src_id == src_id
                    && rel.dst_id == dst_id
                    && rel.type_id == type_id
                {
                    return Ok(Some(rid));
                }
            }
        }

        // #20: make the fast-path miss itself observable (debug level — entry
        // is common on small graphs; the warn below covers the pathology).
        tracing::debug!(
            src_id,
            rel_type,
            "exact-edge index miss — falling back to O(out-degree) adjacency walk"
        );

        // Authoritative fallback: the store's own outgoing adjacency
        // (`storage::adjacency_index`, maintained in `RecordStore::write_rel`
        // and rebuilt from the records on open). It replaced a walk of
        // `first_rel_ptr`/`next_src_ptr`, which was the same O(out-degree) but
        // depended on chain integrity — the chain is patched up heuristically
        // by `create_relationship` when the mmap looks stale, and a single
        // broken link silently ended the walk early, making MERGE re-create an
        // edge that already existed. Each candidate is still read back and
        // fully re-checked, so the index only chooses which records to read.
        // See phase0_perf-store-reverse-incoming-adjacency-index §1.7.
        let outgoing = self.storage.outgoing_relationships(src_id);

        // Telemetry (issue #12): still O(out-degree). For a hub node
        // accumulating thousands of same-type edges, each edge-MERGE existence
        // check that misses the exact-edge index scans them all, which under a
        // sustained edge-write burst manifests as a no-query-running CPU
        // climb. Warn past a threshold so the pathology is observable
        // (RUST_LOG=nexus_core=warn) instead of an opaque stall.
        if outgoing.len() >= 1000 {
            tracing::warn!(
                src_id,
                rel_type,
                out_degree = outgoing.len(),
                "find_relationship_between is scanning a high-degree node's \
                 outgoing edges (>= 1000) — exact-edge index miss on a hub; \
                 sustained edge-MERGE here can pin CPU (issue #12)"
            );
        }

        for rel_id in outgoing {
            let rel_record = self.storage.read_rel(rel_id)?;
            // Skip deleted records — the fast path above verifies deletion
            // too, and MERGE must not treat a deleted edge as existing.
            if !rel_record.is_deleted()
                && rel_record.src_id == src_id
                && rel_record.dst_id == dst_id
                && rel_record.type_id == type_id
            {
                return Ok(Some(rel_id));
            }
        }

        Ok(None)
    }
}
