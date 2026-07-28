## 1. Implementation

Consecutive `CREATE` clauses in one query did not share variable bindings:
`CREATE (a:A), (b:B)` then `CREATE (a)-[:T]->(b)` minted fresh anonymous nodes
(5 where the TCK expects 3). Empirical probing corrected the brief's root-cause
pointer: the DUAL cause was (1) the executor's standalone-CREATE fast path
(`executor/dispatch/operator_loop.rs`, the path plain multi-clause CREATE
actually routes through) running each Create operator with a fresh call-local
binding map, and (2) the engine write-clause loop
(`engine/write_exec/dispatch.rs`, reached when CREATE mixes with
MERGE/SET/REMOVE/FOREACH) never consulting `context` before creating. Both
fixed in commit 26fb15a4; opus review APPROVE (round 1, 2 MINOR + 2 NIT polish
applied). TCK 930 → 943 (24.4%); Create3[13] passes end-to-end.

- [x] 1.1 later CREATE clauses reuse variables bound by earlier CREATE/MATCH clauses in the same query (commit 26fb15a4) — fast path threads one created_node_ids/created_rel_ids accumulator across all Create operators via execute_create_pattern_internal; write-clause loop reuses a var bound to exactly one id in the Node arm and rel-target resolution (no context overwrite on reuse); unbound names still create fresh entities. Leans on the semantic pass's VariableAlreadyBound guarantee (bound var reaching CREATE = bare `(a)` shape). Multi-id bindings (MATCH cross-product through the write loop) keep prior behavior — pre-existing duplicate-node gap now documented in-code.
- [x] 1.2 mixed sequences work (commit 26fb15a4) — MATCH-then-CREATE (third path, execute_create_with_context) verified untouched; CREATE-then-CREATE chains incl. 3-clause TCK Create2[3] shape; CREATE+SET routing through the write loop; RETURN of variables bound in earlier clauses reads the original entity; MERGE idempotency and named-UNWIND per-row accumulation pinned by discriminating guards.

Follow-up filed as phase21_tck-create-no-return-phantom-row: the fast path
synthesizes a result row from named CREATE variables even without RETURN
(operator_loop.rs `!columns.is_empty()` block), still blocking
Create2[2]/[3]/[5]-[12] on "result should be empty".

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation — CHANGELOG [3.0.0] "Fixed — Consecutive CREATE clauses share variable bindings" entry, commit 26fb15a4.
- [x] 2.2 Write tests covering the new behavior — 9 regression tests in `tests/regression/cross_clause_create_binding_test.rs` (repro, 3-clause chain, single-clause + MATCH-CREATE controls, unbound-fresh negative, RETURN-reads-original, CREATE+SET write-loop path, MERGE idempotency, named-UNWIND leak guard).
- [x] 2.3 Run tests and confirm they pass — new tests 9/9; regression binary 217/217; targeted write/merge/unwind/create filters 83/46/11/65 green; full lib 2601/2604 (the 3 = documented environment-sensitive tests, pass isolated); clippy -D warnings zero. TCK re-measured: 943/3868 (24.4%).
