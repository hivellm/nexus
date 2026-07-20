# Tasks: phase0_perf-store-reverse-incoming-adjacency-index

The store has no authoritative reverse (incoming) adjacency, so incoming-edge
liveness is either a full scan or the non-authoritative `RelationshipIndex`
hint. This keeps `node_has_live_relationship` (incoming tier) and
`delete_node_relationships` at O(total edges). See proposal.md.

## 1. Implementation
- [ ] 1.1 Confirm the single write chokepoint: verify that executor CREATE
  (`executor/operators/create.rs`), the bulk loader (`loader/mod.rs`), and the
  engine path all funnel through `storage::record_store_ops::create_relationship`
  (and a single relationship-deletion path). If any bypass it, that must be
  routed through the maintained chokepoint or the index will desync again.
- [ ] 1.2 Decide the reverse structure: in-memory `HashMap<node_id,
  HashSet<rel_id>>` owned by the store (rebuilt on open) vs. an on-disk incoming
  chain (record-format change). Default to in-memory unless restart-without-
  rebuild durability is required. Record the decision in an analysis file.
- [ ] 1.3 Maintain the reverse index in `create_relationship` (add incoming edge
  keyed by `dst_id`) and in the relationship-deletion path (remove it), so ALL
  write paths keep it authoritative.
- [ ] 1.4 Rebuild the reverse index from storage on engine open, alongside
  `rebuild_relationship_index_from_storage` (engine/mod.rs).
- [ ] 1.5 Repoint the incoming tier of `node_has_live_relationship`
  (engine/crud/nodes.rs) at the authoritative reverse lookup (O(in-degree)),
  dropping the full-scan fallback once the source is authoritative.
- [ ] 1.6 Repoint `delete_node_relationships` (DETACH DELETE) at outgoing chain
  walk + reverse lookup so it is O(degree) end to end.
- [ ] 1.7 Verify the same fix restores the MERGE exact-edge fast path (edges
  created via executor CREATE are now indexed).

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 2.1 Update or create documentation covering the implementation (storage /
  index doc describing the reverse adjacency and its maintenance invariant)
- [ ] 2.2 Write tests covering the new behavior: incoming-only guard is O(degree)
  and still authoritative under every create path (Cypher CREATE, bulk load,
  engine); reverse index survives an engine reopen (rebuild-on-open); DETACH
  DELETE clears incoming edges; a delete/scan never misses a live incoming edge.
- [ ] 2.3 Run tests and confirm they pass (`cargo +nightly fmt --all`,
  `cargo clippy -p nexus-core --all-targets --all-features -- -D warnings`,
  `cargo +nightly test -p nexus-core` — all green)

## Related
- `phase0_perf-delete-node-relationship-check-full-scan` — did the outgoing half
  (O(out-degree) fast path); this task does the incoming half authoritatively.
- `phase0_fix-cypher-relationship-delete-noop` — deletion path this index must
  stay consistent with.
