## 1. Investigation
- [x] 1.1 Confirm the nested `node_index.read()` inside `stats.write()` (relationship_index.rs:164-169) and scan for any inverse `node_index.write()` -> `stats.write()` order that would close the deadlock cycle — confirmed and fixed in 46bb9101; full-file scan found two remaining one-directional nestings (`node_index.read()` → `stats.write()` in `get_high_degree_relationships` / `optimize_high_degree_nodes`, and `type_index.read()` + `node_index.read()` held together in `health_check`) — none has an inverse order anywhere, so no cycle exists; the invariant is now documented on the struct.
- [x] 1.2 Confirm `total_nodes` is approximate/diagnostic and safe to maintain via an atomic counter — confirmed approximate ("nodes with ≥1 rel", read only by stats()/get_traversal_stats diagnostics).
## 2. Implementation
- [x] 2.1 Maintain `total_nodes` via an atomic counter (incremented on first insert of a node_id key) and remove the `node_index.read()` from the `stats.write()` block — the nested read was removed in 46bb9101 with a simpler equivalent: read `node_index.len()` into a local BEFORE `stats.write()` (sequential, never nested). An atomic counter would add state to keep in sync for no additional safety; the chosen approach also self-corrects on every call. `remove_relationship` now follows the same discipline.
- [x] 2.2 Audit `add_relationship`/`remove_relationship` for any other nested cross-field lock acquisition — both are fully sequential scoped blocks (type → node → node → edge → stats); lock-order invariant documented on the struct doc.

## 3. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 3.1 Update or create documentation (CHANGELOG / GH #17) — CHANGELOG [Unreleased] Fixed entry
- [x] 3.2 Write tests: concurrent edge inserts make progress (no deadlock) and `total_nodes` stays correct across add/remove — `concurrent_add_relationship_does_not_deadlock` (8×500 + interleaved stats) and new `total_nodes_stays_correct_across_add_and_remove` (remove previously left `total_nodes` stale; fixed)
- [x] 3.3 Run tests and confirm they pass — `cargo test -p nexus-core --lib cache::relationship_index` 5/5
