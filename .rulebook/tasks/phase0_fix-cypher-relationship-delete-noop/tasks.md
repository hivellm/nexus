# Tasks: phase0_fix-cypher-relationship-delete-noop

Cypher `MATCH (a)-[r:KNOWS]->(b) DELETE r` returns `Ok` but never removes the
relationship: `execute_delete` (executor/operators/expand.rs ~606-621) is a stub
that never calls `storage::delete_rel` (~348-353). See proposal.md for the
empirical evidence.

## 1. Implementation
- [x] 1.1 Reproduced first: an empirical probe confirmed `DELETE r` left the
  record `is_deleted()==false`, `count(r)` still `1`, `relationships_deleted==0`.
  The four new tests are red on the old code, green on the fix.
- [x] 1.2 Traced the REAL handler. The executor `execute_delete`
  (operators/expand.rs) is a post-hoc stub; the actual DELETE runs at the engine
  level in `Engine::execute_match_delete_query` (engine/match_exec.rs). Root
  cause found there: it collected/projected ONLY node variables
  (`PatternElement::Node`), so a relationship variable `r` never reached the
  delete loop, and the loop only called `delete_node`. `storage::delete_rel`
  exists (mark_deleted + write_rel) but was never invoked from the DELETE path.
- [x] 1.3 Fixed at the engine level: (a) added `Engine::delete_relationship`
  (crud/nodes.rs) — authoritative single-edge soft-delete (mark_deleted +
  write_rel inside a write transaction + relationship-index upkeep, idempotent);
  (b) `execute_match_delete_query` now also collects relationship variables,
  projects them in the synthetic RETURN so they materialize with `_nexus_id`,
  and deletes them. (Deleted edges count into the returned `count`, the DELETE
  path's existing result mechanism; `SideEffects` is not populated on this path.)
- [x] 1.4 DETACH vs bare `DELETE r` handled consistently, and the delete now runs
  in two passes — relationships first, then nodes — so `DELETE a, r, b` in one
  clause no longer trips the live-edge guard on `a`. Double-delete /
  non-matching delete is an idempotent no-op (returns `false`, not counted, no
  error).
- [x] 1.5 Parity: all Cypher surfaces funnel through
  `execute_match_delete_query`, so the fix covers Cypher/REST/RPC/RESP3 issuing
  Cypher. `Engine::delete_relationship` is `pub` so any non-Cypher endpoint can
  reuse the same authoritative path.

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation — DONE
  (waived as a standalone doc): DELETE semantics were not materially redefined
  (the clause simply now works); the behavior and two-pass ordering are captured
  in doc/inline comments on `Engine::delete_relationship` and
  `execute_match_delete_query`.
- [x] 2.2 Write tests covering the new behavior — DONE: new
  `tests/executor/relationship_delete_test.rs` with 4 tests (soft-delete
  visibility via `count(r)` + store-level `is_deleted()`; endpoint becomes
  non-DETACH-deletable after edge removal; idempotent double-delete; combined
  `DELETE r, a` ordering). Refreshed the now-stale "no-op stub" comment on
  `non_detach_delete_allowed_after_outgoing_edge_soft_deleted` (kept it
  storage-level to isolate the Tier-1 test from the DELETE path).
- [x] 2.3 Run tests and confirm they pass — DONE (green): `cargo +nightly fmt
  --all`; `cargo clippy -p nexus-core --all-targets --all-features -- -D
  warnings` (0 warnings); full `cargo +nightly test -p nexus-core` — 2422 lib +
  all integration groups pass, 0 failed (executor group 141 incl. the 4 new).

## Related
- `phase0_fix-delete-node-dangling-relationships` — the guard that stays
  permanently tripped because "deleted" edges remain live.
- `phase0_perf-delete-node-relationship-check-full-scan` — surfaced this bug.
