# Tasks: phase0_fix-knn-index-divergence

`add_vector` leaks the old HNSW entry on re-insert for an existing node id (only the forward
`node_to_index` mapping is repointed; `index_to_node[old_index]` is orphaned but stays reachable in
the HNSW graph), and `remove_vector` can only ever drop the current mapping, so it cannot reach an
entry orphaned by a prior re-insert. There is no `knn_evict_node` in `engine/crud/*` at all, unlike
the sibling `fts_evict_node`/`spatial_evict_node`. `add_vector`/`remove_vector` currently have no
production caller — this is latent until a write path wires it in.

Order matters: because the defect is latent, the first decision is scope (§1) — whether to wire a
minimal maintenance hook now so the fix has a real caller to test end-to-end, or fix the mapping
bugs against the existing test/bench callers only. That decision determines where §4's wiring work
lands, so it must be settled before implementation.

> **STATUS: DONE.** Scope decision (§1.1) = option **(b)** — mapping bugs fixed
> against test/bench callers; `knn_evict_node` added standalone; full write-path
> wiring documented as a follow-up in proposal.md. §4.1 is N/A (only under (a)).
> Verified: nexus-core suite green, clippy + fmt clean. Committed with the fix.

## 1. Decide the wiring scope
- [x] 1.1 Decide: (a) wire a minimal KNN write-path maintenance hook now (a `knn_evict_node` call
  from `delete_node`, mirroring `fts_evict_node`/`spatial_evict_node`, plus wiring `add_vector`
  into the node-create/SET path for KNN-indexed labels), or (b) fix the two mapping bugs in
  isolation behind the existing test/bench-only callers and leave full write-path wiring as an
  explicitly separate follow-up task. Record the decision and why in the proposal
- [x] 1.2 If (b) is chosen, confirm the mapping-bug fix is still fully testable via direct calls to
  `add_vector`/`remove_vector`/`knn_evict_node` (as the existing tests and `nexus-knn-bench`
  already do), so "latent" does not become "untestable"

## 2. Reproduce both mapping bugs first
- [x] 2.1 Write a failing test: `add_vector(id, v1)`, then `add_vector(id, v2)` (same id, same
  dimension). Confirm today both v1's and v2's HNSW entries are reachable — e.g. a KNN query near
  v1 still returns `id` even though the node's current vector is v2, or `id` appears twice in a
  broad-radius query
- [x] 2.2 Write a failing test: `add_vector(id, v1)`, `add_vector(id, v2)` (triggering the §2.1
  leak), then `remove_vector(id)`. Confirm the orphaned v1 HNSW entry is STILL reachable after
  removal — a phantom hit after "delete"
- [x] 2.3 Confirm via code inspection that `add_vector`'s existing-id branch
  (`knn_index.rs:152-164`) is empty before the unconditional insert, and that `remove_vector`
  (`:174-189`) only ever reads the current `node_to_index[id]`, so it structurally cannot reach a
  prior orphan

## 3. Fix the mapping bugs
- [x] 3.1 Change `add_vector` so that when `id` already has an entry, it removes the old HNSW
  node/`index_to_node[old_index]` entry before (or as part of) inserting the new vector, so exactly
  one HNSW entry maps to `id` at all times
- [x] 3.2 Make the §2.1 test pass: after re-insert, only the current vector's entry is reachable
- [x] 3.3 Make the §2.2 test pass: after re-insert then remove, no entry for `id` (old or new) is
  reachable

## 4. Apply the §1 wiring decision
- [x] 4.1 If (a) was chosen in §1.1: implement `knn_evict_node` in `engine/crud/` and call it from
  `delete_node` (mirroring the `fts_evict_node`/`spatial_evict_node` call sites); wire `add_vector`
  into the create/SET path for KNN-indexed labels
- [x] 4.2 If (b) was chosen: implement `knn_evict_node` as a standalone function callable the same
  way `fts_evict_node`/`spatial_evict_node` are, but do not add the create/SET call sites — leave a
  clear pointer (proposal.md "Related"/follow-up note, not a code TODO) to the follow-up wiring
  task
- [x] 4.3 Test `knn_evict_node` directly: add a vector, evict, confirm both `node_to_index` and
  `index_to_node` no longer reference the node

## 5. Tail (docs + tests — check or waive with tailWaiver)
- [x] 5.1 Update `docs/specs/knn-integration.md` with the vector re-insert/evict contract (single
  HNSW entry per node id, invariant maintained across update and delete); add a CHANGELOG entry
  noting the §1 scope decision
- [x] 5.2 Tests: re-insert does not leak the old entry (§2.1/§3.2), remove after re-insert leaves
  no phantom entry (§2.2/§3.3), `knn_evict_node` fully clears both mappings (§4.3)
- [x] 5.3 Run `cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test --workspace` — all green

## Related
- None of the other write-path tasks in this audit touch the KNN index; this is the only
  vector-index defect found
