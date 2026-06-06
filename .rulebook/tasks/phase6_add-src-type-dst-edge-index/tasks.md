## 1. Implementation
- [ ] 1.1 Add a `(src_id, type_id, dst_id)` edge-existence index structure (storage layer), maintained on relationship create/delete
- [ ] 1.2 `find_relationship_between` consults the index first (O(1)); falls back to the source-chain walk when unavailable
- [ ] 1.3 Maintain consistency under WAL replay and alongside the typed adjacency store

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 2.1 Update or create documentation covering the implementation
- [ ] 2.2 Write tests covering the new behavior (correctness + a high-degree-hub scaling guard showing O(1) per edge)
- [ ] 2.3 Run tests and confirm they pass
