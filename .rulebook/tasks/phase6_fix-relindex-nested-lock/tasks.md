## 1. Investigation
- [ ] 1.1 Confirm the nested `node_index.read()` inside `stats.write()` (relationship_index.rs:164-169) and scan for any inverse `node_index.write()` -> `stats.write()` order that would close the deadlock cycle
- [ ] 1.2 Confirm `total_nodes` is approximate/diagnostic and safe to maintain via an atomic counter

## 2. Implementation
- [ ] 2.1 Maintain `total_nodes` via an atomic counter (incremented on first insert of a node_id key) and remove the `node_index.read()` from the `stats.write()` block
- [ ] 2.2 Audit `add_relationship`/`remove_relationship` for any other nested cross-field lock acquisition

## 3. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 3.1 Update or create documentation (CHANGELOG / GH #17)
- [ ] 3.2 Write tests: concurrent edge inserts make progress (no deadlock) and `total_nodes` stays correct across add/remove
- [ ] 3.3 Run tests and confirm they pass
