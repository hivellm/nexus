//! Transaction command execution (BEGIN, COMMIT, ROLLBACK, SAVEPOINT).
//! Extracted from `engine/mod.rs`.

use super::Engine;
use crate::{Error, Result, executor, transaction};

impl Engine {
    /// Execute transaction commands (BEGIN, COMMIT, ROLLBACK)
    /// Requires a session_id to track transaction context across queries
    pub(super) fn execute_transaction_commands(
        &mut self,
        ast: &executor::parser::CypherQuery,
        session_id: Option<&str>,
    ) -> Result<executor::ResultSet> {
        // Use provided session_id or generate a default one
        // In a full implementation, session_id would come from HTTP headers or connection context
        let session_id = session_id.unwrap_or("default");

        for clause in &ast.clauses {
            match clause {
                executor::parser::Clause::BeginTransaction => {
                    // Get or create session
                    let mut session = self
                        .session_manager
                        .get_or_create_session(session_id.to_string());

                    // Begin transaction for this session
                    session.begin_transaction()?;

                    // ISSUE #15: capture storage watermarks so COMMIT can
                    // index exactly the entities this transaction creates
                    // (single-writer model — no concurrent id allocation).
                    session.tx_begin_node_watermark = self.storage.node_count();
                    session.tx_begin_rel_watermark = self.storage.relationship_count();

                    // Update session in manager
                    self.session_manager.update_session(session);
                }
                executor::parser::Clause::CommitTransaction => {
                    // Get session
                    let mut session = self
                        .session_manager
                        .get_session(&session_id.to_string())
                        .ok_or_else(|| {
                            Error::transaction(format!(
                                "Session {} not found or expired",
                                session_id
                            ))
                        })?;

                    // Apply pending index updates in batch before commit (Phase 1 optimization)
                    self.apply_pending_index_updates(&mut session)?;

                    // 3.4: External-id reservations are now permanent — clear
                    // the pending list so no stale entries carry over.
                    self.pending_external_ids.clear();

                    // ISSUE #15: scoped index maintenance over the session's
                    // own write set (created nodes + relationships) replaces
                    // the previous per-COMMIT `rebuild_indexes_from_storage()`
                    // full O(N) scan, so commit cost no longer scales with
                    // total graph size. The typed property index — the part
                    // the rebuild was load-bearing for — is maintained per
                    // created node via `maintain_indexed_properties`.
                    self.apply_committed_entity_index_updates(&session)?;

                    // Commit transaction
                    session.commit_transaction()?;

                    // Flush storage to ensure durability
                    self.storage.flush()?;

                    // Left unconditional (conservative): COMMIT
                    // is the single consolidated refresh point for every
                    // statement executed inside this transaction (each
                    // per-statement write path skips its own refresh while
                    // `in_transaction` — see the standalone-CREATE and
                    // write-query dispatch sites), so an accurate "did the
                    // whole transaction mutate anything" signal would have
                    // to aggregate side effects across an arbitrary number
                    // of prior `execute_cypher_*` calls. The node/rel-count
                    // watermarks captured at BEGIN (`tx_begin_node_watermark`
                    // / `tx_begin_rel_watermark`) cannot rule out a
                    // property- or label-only transaction (no id-count
                    // change), so they are not a complete signal either.
                    // Refresh unconditionally rather than guess.
                    self.refresh_executor()?;

                    // Update session in manager
                    self.session_manager.update_session(session);
                }
                executor::parser::Clause::RollbackTransaction => {
                    // Get session
                    let mut session = self
                        .session_manager
                        .get_session(&session_id.to_string())
                        .ok_or_else(|| {
                            Error::transaction(format!(
                                "Session {} not found or expired",
                                session_id
                            ))
                        })?;

                    // CRITICAL: Clone created_nodes list before marking as deleted
                    // because get_session may return a cloned session.
                    //
                    // Union with the session's storage watermark range: a
                    // standalone CREATE inside an explicit tx routes through
                    // the EXECUTOR write path, which does not report into
                    // `created_nodes` — the watermark range (captured at
                    // BEGIN, exact under the single-writer model) covers
                    // those, same source as the #15 scoped-commit fix.
                    // Without it, ROLLBACK silently kept executor-created
                    // entities. Gated on an active transaction so a stray
                    // ROLLBACK can never sweep ids from a stale watermark.
                    let mut nodes_to_delete = session.created_nodes.clone();
                    let mut rels_to_delete = session.created_relationships.clone();
                    if session.has_active_transaction() {
                        nodes_to_delete
                            .extend(session.tx_begin_node_watermark..self.storage.node_count());
                        rels_to_delete.extend(
                            session.tx_begin_rel_watermark..self.storage.relationship_count(),
                        );
                        nodes_to_delete.sort_unstable();
                        nodes_to_delete.dedup();
                        rels_to_delete.sort_unstable();
                        rels_to_delete.dedup();
                    }

                    // Remove nodes from index and mark as deleted in storage BEFORE rollback
                    // This ensures we clean up nodes that were written to storage (mmap writes immediately)
                    for node_id in &nodes_to_delete {
                        // First, mark as deleted in storage (this prevents reads from returning the node)
                        if let Err(e) = self.storage.delete_node(*node_id) {
                            tracing::warn!("Failed to delete node {} from storage: {}", node_id, e);
                        }

                        // Read node properties before deletion to remove from property index
                        if let Ok(Some(properties)) = self.storage.load_node_properties(*node_id) {
                            if let serde_json::Value::Object(props) = properties {
                                let property_index = self.cache.property_index_manager();
                                for prop_name in props.keys() {
                                    if let Err(e) =
                                        property_index.remove_property(prop_name, *node_id)
                                    {
                                        // Property index may not exist for this property, ignore error
                                        let _ = e;
                                    }
                                }
                            }
                        }

                        // Remove from label index AFTER marking as deleted
                        // remove_node removes the node from all label bitmaps
                        if let Err(e) = self.indexes.label_index.remove_node(*node_id) {
                            tracing::warn!(
                                "Failed to remove node {} from label index: {}",
                                node_id,
                                e
                            );
                        }
                    }

                    // Mark all relationships created during this transaction as deleted
                    for rel_id in &rels_to_delete {
                        if let Err(e) = self.storage.delete_rel(*rel_id) {
                            tracing::warn!(
                                "Failed to delete relationship {} from storage: {}",
                                rel_id,
                                e
                            );
                        }
                    }

                    // Flush storage to ensure consistency (must be done before rollback)
                    if let Err(e) = self.storage.flush() {
                        tracing::warn!("Failed to flush storage: {}", e);
                    }

                    // 3.4: Undo any external-id reservations made during
                    // this transaction before the storage records are
                    // deleted, so the catalog index stays consistent.
                    self.rollback_external_id_reservations();

                    // Rollback transaction (abort the transaction)
                    session.rollback_transaction()?;

                    // Clear tracking lists after rollback
                    session.created_nodes.clear();
                    session.created_relationships.clear();
                    // Clear pending index updates (they should not be applied on rollback)
                    session.pending_index_updates.clear();

                    // Update session in manager BEFORE refreshing executor
                    // This ensures the session state is saved before executor refresh
                    self.session_manager.update_session(session);

                    // Refresh executor to see the updated indexes
                    // Note: We don't rebuild indexes here because we've already removed
                    // nodes from indexes manually above. Rebuilding would be redundant and
                    // could potentially reintroduce deleted nodes if there's a timing issue.
                    //
                    // This ROLLBACK arm's only storage/index
                    // mutation is the cleanup loop over `nodes_to_delete` /
                    // `rels_to_delete` above (storage delete + label-index +
                    // property-index removal); an empty transaction (`BEGIN;
                    // ROLLBACK;` with no writes in between) leaves both
                    // lists empty and this refresh has nothing to pick up.
                    let mutated = !nodes_to_delete.is_empty() || !rels_to_delete.is_empty();
                    self.refresh_executor_if_mutated(mutated)?;
                }
                // phase6_opencypher-advanced-types §5 — savepoint
                // lifecycle statements. All three require an active
                // explicit transaction; outside one they raise
                // ERR_SAVEPOINT_NO_TX.
                executor::parser::Clause::Savepoint(s) => {
                    // phase6_opencypher-advanced-types §5 — SAVEPOINT
                    // outside an explicit tx must return ERR_SAVEPOINT_NO_TX,
                    // not a generic session-not-found error. Autovivify
                    // a session here so the no-tx check runs even for
                    // first-call clients.
                    let mut session = self
                        .session_manager
                        .get_or_create_session(session_id.to_string());
                    if !session.has_active_transaction() {
                        return Err(Error::CypherExecution(
                            "ERR_SAVEPOINT_NO_TX: SAVEPOINT outside an explicit transaction"
                                .to_string(),
                        ));
                    }
                    session.savepoints.push(
                        &s.name,
                        transaction::SavepointMarker {
                            undo_log_offset: session.created_nodes.len(),
                            staged_ops_offset: session.created_relationships.len(),
                        },
                    );
                    self.session_manager.update_session(session);
                }
                executor::parser::Clause::RollbackToSavepoint(s) => {
                    let mut session = self
                        .session_manager
                        .get_or_create_session(session_id.to_string());
                    if !session.has_active_transaction() {
                        return Err(Error::CypherExecution(
                            "ERR_SAVEPOINT_NO_TX: ROLLBACK TO SAVEPOINT outside an explicit \
                             transaction"
                                .to_string(),
                        ));
                    }
                    let marker = session.savepoints.rollback_to(&s.name)?;
                    // Replay node undo-log: every node created after
                    // the marker's offset gets marked deleted and
                    // pulled from the label index. Relationships
                    // follow the same pattern.
                    let to_undo_nodes: Vec<u64> = session
                        .created_nodes
                        .drain(marker.undo_log_offset..)
                        .collect();
                    let to_undo_rels: Vec<u64> = session
                        .created_relationships
                        .drain(marker.staged_ops_offset..)
                        .collect();
                    for node_id in &to_undo_nodes {
                        let _ = self.storage.delete_node(*node_id);
                        if let Ok(Some(serde_json::Value::Object(props))) =
                            self.storage.load_node_properties(*node_id)
                        {
                            let property_index = self.cache.property_index_manager();
                            for prop_name in props.keys() {
                                let _ = property_index.remove_property(prop_name, *node_id);
                            }
                        }
                        if let Ok(_record) = self.storage.read_node(*node_id) {
                            let _ = self.indexes.label_index.remove_node(*node_id);
                        }
                    }
                    for rel_id in &to_undo_rels {
                        let _ = self.storage.delete_rel(*rel_id);
                    }
                    self.session_manager.update_session(session);
                }
                executor::parser::Clause::ReleaseSavepoint(s) => {
                    let mut session = self
                        .session_manager
                        .get_or_create_session(session_id.to_string());
                    if !session.has_active_transaction() {
                        return Err(Error::CypherExecution(
                            "ERR_SAVEPOINT_NO_TX: RELEASE SAVEPOINT outside an explicit \
                             transaction"
                                .to_string(),
                        ));
                    }
                    session.savepoints.release(&s.name)?;
                    self.session_manager.update_session(session);
                }
                _ => {}
            }
        }

        Ok(executor::ResultSet::new(
            vec!["status".to_string()],
            vec![executor::Row {
                values: vec![serde_json::Value::String("ok".to_string())],
            }],
        ))
    }
}
