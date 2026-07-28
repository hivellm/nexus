## 1. Implementation

Write-only statements no longer synthesize a phantom result row (commit
91cdd606). The synthesis existed at TWO sites: the standalone-CREATE fast path
(`executor/dispatch/operator_loop.rs`) and `execute_create_with_context`
(`executor/operators/create.rs`, the MATCH+CREATE main-loop path). Both now
gate on a downstream row consumer (Project/With/Aggregate lookahead). The fix
also surfaced and closed a second gap: the fast path had NO Aggregate execution
arm, so `CREATE (n) RETURN count(*)` previously returned a phantom node row —
it now returns the count. 3-round opus review (round 1: REQUEST_CHANGES —
Aggregate missing from the lookahead; round 2 fix went one justified step
further adding the execution arm; round 3: APPROVE).

- [x] 1.1 write-only CREATE statements produce an empty result set (commit 91cdd606) — both mechanisms gated; statements WITH a RETURN keep their rows, incl. pure-aggregate returns (`count(*)` = 1 pinned exactly); UNION/JOIN/CALL-subquery callers explicitly preserve prior behavior.
- [x] 1.2 other write-only families checked (commit 91cdd606) — DELETE/DETACH DELETE already correct (fixed pre-task by 0b3ae5d8, dedicated query_pipeline branch); SET/REMOVE/MERGE never had the bug (execute_write_query only builds a result on Clause::Return); all five pinned with empty-result + side-effect regression tests. TCK clauses/delete's remaining 34 fails have a different, unexamined root cause (scoped out per the task allowance).

Follow-ups recorded (not filed as tasks yet): (a) fast path `_ => {}` still
drops Distinct/Skip/Unwind/EnsureNullRowIfEmpty after standalone CREATE;
(b) write-only `CALL { CREATE (n) }` bodies can still leak a synthesized row
(deliberate `true` at operators/dispatch.rs:279 — no lookahead available
there); (c) mixed `CREATE ... RETURN a.v, count(*)` on the fast path runs
Project-then-Aggregate in plan order without the main loop's deferral — zero
coverage, pin before changing.

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation — CHANGELOG [3.0.0] "Write-only statements return empty result sets" entry, commit 91cdd606.
- [x] 2.2 Write tests covering the new behavior — 11 tests in `tests/regression/write_only_statement_empty_result_test.rs` (both CREATE mechanisms with/without RETURN, SET/REMOVE/MERGE/DELETE/DETACH pins, count(*) exact pin) + empty-result assertion added to the cross-clause chain test + 5 stale tests updated that had encoded the phantom row as expected behavior.
- [x] 2.3 Run tests and confirm they pass — regression 227/227; executor 263/263; cypher 456/456; full lib 2602/2604 (documented trio, isolated-pass reconfirmed); clippy -D warnings zero. TCK: clauses/create 25 → 34/78; overall 943 → 951 (tree shared with the EXISTS-subquery changeset; per-scenario Create2 trace is the unconfounded evidence).
