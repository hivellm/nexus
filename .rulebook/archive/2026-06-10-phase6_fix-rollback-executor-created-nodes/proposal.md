# Proposal: phase6_fix-rollback-executor-created-nodes

## Why
Manual Docker validation of the 2.3.3 audit batch found that `ROLLBACK TRANSACTION`
does not undo a standalone `CREATE` executed inside an explicit transaction:
the node survives the rollback and is visible to subsequent reads. Confirmed
pre-existing (reproduces on the published 2.3.2 image). Root cause is the same
family as the #15 finding: a standalone CREATE inside a transaction routes
through the EXECUTOR write path, which never reports created ids into
`session.created_nodes` — so the rollback arm (which iterates exactly that
list to mark nodes deleted and evict them from indexes) has nothing to undo.

## What Changes
- Extend the ROLLBACK arm in `engine/transactions.rs` to also undo entities in
  the session's storage watermark range (`tx_begin_node_watermark..node_count`,
  `tx_begin_rel_watermark..relationship_count`) — the same write-set source the
  #15 scoped-commit fix uses (single-writer model makes the id range exact),
  unioned with the session's tracked created lists (idempotent: delete of an
  already-deleted record is a no-op).
- Relationships created in the rolled-back tx are deleted before nodes (chain
  integrity), mirroring the existing list-based cleanup order.

## Impact
- Affected specs: transaction / rollback
- Affected code: `crates/nexus-core/src/engine/transactions.rs`
- Breaking change: NO (restores intended rollback semantics)
- User benefit: ROLLBACK actually rolls back Cypher CREATEs — no phantom
  committed-anyway data after an aborted transaction.
