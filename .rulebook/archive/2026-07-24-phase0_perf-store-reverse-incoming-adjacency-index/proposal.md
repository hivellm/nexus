# Proposal: phase0_perf-store-reverse-incoming-adjacency-index

**Priority: MEDIUM (performance / architectural gap).** Follow-up to
`phase0_perf-delete-node-relationship-check-full-scan`.

## Why

The store has NO authoritative reverse (incoming) adjacency. `NodeRecord` has a
single `first_rel_ptr` that heads only the node's OUTGOING chain
(`create_relationship` deliberately never updates it on the destination node —
see `storage::record_store_ops::create_relationship`). So any "who points at
node N?" question has only two sources:

1. A full O(total_relationships) store scan (authoritative but slow), or
2. `cache::RelationshipIndex` — which is a non-authoritative **hint**: the
   executor `CREATE` operator (`executor/operators/create.rs`) and the bulk
   loader (`loader/mod.rs`) write edges to the store WITHOUT calling
   `add_relationship` and WITHOUT setting the dirty flag, so it can silently miss
   live incoming edges until the next restart. It must not back correctness.

Because of this, two hot paths are stuck at O(total edges):

- `node_has_live_relationship` — the outgoing half is now O(out-degree), but the
  INCOMING half (and therefore every isolated/incoming-only node delete) still
  full-scans.
- `delete_node_relationships` (DETACH DELETE) — must find EVERY connected edge,
  so it full-scans for incoming edges.

A store-maintained, authoritative reverse index closes both.

## What Changes

- Add an authoritative incoming (reverse) adjacency maintained at the single
  chokepoint every write path funnels through — `storage::create_relationship`
  (and the matching relationship-deletion path) — so executor CREATE, bulk
  loader, and engine paths all keep it correct. Rebuild it from the store on
  open (mirroring `rebuild_relationship_index_from_storage`).
- Options to evaluate in 1.x: (a) an in-memory `HashMap<node_id, {rel_ids}>`
  reverse index owned by the store; (b) a real on-disk incoming chain via a
  second head pointer — larger, a record-format change, likely out of scope.
  Prefer (a) unless durability across restart without a rebuild is required.
- Repoint `node_has_live_relationship` (incoming tier) and
  `delete_node_relationships` at the new authoritative reverse lookup → both
  become O(degree).
- Bonus: fixing the coverage gap also repairs the MERGE exact-edge fast path,
  which currently degrades because executor CREATE never populates the index.

## Impact

- Affected specs: none (internal storage/index change)
- Affected code: `crates/nexus-core/src/storage/record_store_ops.rs`
  (`create_relationship`, `delete_rel`), `crates/nexus-core/src/cache/`
  (index/reverse-adjacency), `engine/mod.rs` (rebuild-on-open),
  `engine/crud/nodes.rs` (consume the lookup),
  `executor/operators/create.rs` + `loader/mod.rs` (ensure they route through
  the maintained chokepoint)
- Breaking change: NO (same results, faster; index is an internal accelerator)
- User benefit: node deletes and DETACH DELETE stay O(degree) on large graphs
  instead of O(total relationships)
- Related: `phase0_perf-delete-node-relationship-check-full-scan` (outgoing half
  already done), `phase0_fix-cypher-relationship-delete-noop`
