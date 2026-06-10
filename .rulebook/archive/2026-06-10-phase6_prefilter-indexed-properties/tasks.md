## 1. Investigation
- [x] 1.1 Confirm the O(props x labels) loop + per-prop LMDB read in `maintain_indexed_properties` (crud.rs:1293) and the no-early-exit behavior — confirmed (now `engine/crud/index_maintenance.rs` post-split); first slice (399ae329) added the `has_any_index` global early-exit
- [x] 1.2 Identify the source of truth for indexed (label_id,key_id) pairs to mirror cheaply (and keep in sync with CREATE/DROP INDEX + #11 startup rebuild) — the `PropertyIndex.property_trees` map keys ARE the registration set; `create_index`/`drop_index` and the #11 startup rebuild already maintain it, so no parallel mirror is needed (nothing to drift)

## 2. Implementation
- [x] 2.1 Maintain an in-memory indexed-pairs set; pre-filter the property map so only indexed (label,key) pairs are resolved/inserted, with an early exit when the node has no indexed label — added `PropertyIndex::has_index_for_label(label_id)` (O(#registered indexes) key scan) and a prefilter in `maintain_indexed_properties`: when none of the node's labels has a registered index, return before any per-property `get_key_id` LMDB read. The per-(label,key) `has_index` guard inside the loop already restricted inserts to indexed pairs.
- [x] 2.2 Keep the set updated on CREATE INDEX / DROP INDEX and the startup rebuild — satisfied structurally: the set is the live index map itself (see 1.2)

## 3. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 3.1 Update or create documentation (CHANGELOG / GH #21) — CHANGELOG [Unreleased] Changed entry
- [x] 3.2 Write tests: indexed properties are still maintained correctly; a node with no indexed label does zero index work; CREATE/DROP INDEX updates the prefilter set — `has_index_for_label_reflects_registration` (create → true for that label only, drop → false) + existing maintenance coverage (`explicit_commit_keeps_property_index_seek`, `explicit_commit_incremental_indexes_match_full_rebuild`, property_index module 27/27)
- [x] 3.3 Run tests and confirm they pass — property_index 27/27, transactions 9/9, full lib 2380/2380, clippy 0 warnings, fmt applied
