## 1. Investigation
- [x] 1.1 Confirm the swallowed error at crud.rs:992-998 and trace the downstream effect (missing exact-edge entry -> find_relationship_between O(degree) fallback -> possible duplicate MERGE edge) — confirmed in 06ac218d (now `engine/crud/relationships.rs` post-split); correctness was preserved by the authoritative chain-walk fallback, the hazard was silent persistent O(degree) degradation
- [x] 1.2 Decide propagate-error vs dirty-bit-forced-rebuild (which preserves write success while guaranteeing index/storage consistency) — decided dirty-bit + self-heal (06ac218d): storage write is authoritative so the operation succeeds; the next `find_relationship_between` rebuilds the index from storage and clears the flag

## 2. Implementation
- [x] 2.1 Stop silently swallowing the `relationship_index().add_relationship` error on the create_relationship path (propagate or set a rebuild dirty-bit consumed before the next query) — shipped in 06ac218d (tracing::error + `relationship_index_dirty` flag + `rebuild_relationship_index_from_storage` self-heal); the same discipline is also applied at the explicit-commit path (#15's `apply_committed_entity_index_updates`)
- [x] 2.2 Keep the phase-8 manager updates logged; apply the same guarantee if low-cost — kept logged at warn. Assessed: `RelationshipStorageManager` / `RelationshipPropertyIndex` are secondary executor-side acceleration structures that do not participate in MERGE existence checks (`find_relationship_between` uses `cache.relationship_index`); a dirty-bit guarantee would require building dedicated rebuild-from-storage paths for both — not low-cost, not warranted.

## 3. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 3.1 Update or create documentation (CHANGELOG / GH #18) — CHANGELOG [Unreleased] Fixed entry
- [x] 3.2 Write tests: a forced relationship-index add failure does not leave MERGE able to create a duplicate edge / silently fall back (existence stays correct) — existing `relationship_index_self_heals_when_dirty` (lookup self-heal + dirty-flag clear) + new `merge_does_not_duplicate_edge_after_failed_index_add` (wiped index + dirty flag, repeated Cypher edge-MERGE, relationship_count unchanged)
- [x] 3.3 Run tests and confirm they pass — engine::tests::transactions 10/10
