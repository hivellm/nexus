## 1. Implementation

Full openCypher `EXISTS { MATCH ... [WHERE ...] [WITH ...] [RETURN ...] }`
subqueries shipped (commit 57493db2), on top of the pattern probe landed
earlier. 2-round opus review (round 1: MAJOR — unconditional trailing-RETURN
drop flipped aggregating-RETURN existence; fixed by gating on aggregate-free
items with a single shared `Expression::is_aggregate_free` pinned against the
semantic pass's list — plus 5 minors all fixed; round 2: APPROVE with the four
re-checks verified in code).

- [x] 1.1 EXISTS subquery grammar parses (commit 57493db2) — clause-boundary lookahead routes any clause-opening body through a shared subquery clause loop (extracted from COLLECT, behavior-preserving, both callers now expect the closing brace); write clauses rejected from ANY position with InvalidClauseComposition; `ExistsInner::{Pattern, Subquery}` AST split with the 3VL decision recorded on the type (Subquery deliberately two-valued; Pattern keeps NULL propagation; desugar rewrites syntax only).
- [x] 1.2 subquery correlation + scoping (commit 57493db2) — correlated sub-executor threads outer bindings via a synthetic sorted `WITH` prefix (defeats the planner's unconditional rescan); inner bindings do not leak to the outer scope (pinned); shadowing = same variable per Cypher semantics (reviewer-verified).
- [x] 1.3 EXISTS subquery evaluation (commit 57493db2) — parse-time desugar folds MATCH[+WHERE][+aggregate-free RETURN] into the short-circuiting probe; complex shapes (WITH/aggregation/nesting) run evaluate_exists_subquery mirroring the COLLECT sub-executor; aggregating RETURN never folded (`EXISTS { MATCH (:Missing) RETURN count(*) }` = true); nesting works recursively (pattern-in-full + full-in-full pinned).

Remaining TCK context: existentialSubqueries 2 → 3/10; the other 7 fail only on
whole-node RETURN comparison (the systemic @tck_node serialization mismatch,
457 occurrences repo-wide — separate workstream) plus one on the unimplemented
bare-pattern-predicate-in-WHERE parser feature (only `NOT (pattern)` is sugared
today).

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation — CHANGELOG [3.0.0] "Full EXISTS subqueries" entry, commit 57493db2.
- [x] 2.2 Write tests covering the new behavior — 12 integration tests in `tests/executor/exists_subquery_test.rs` (full-form true/false, write-clause rejection from all positions, WITH+aggregation, nested both ways, correlation, no-leak, aggregating-RETURN-true-on-zero-matches, RETURN DISTINCT, LIMIT-0-forces-false) + `aggregate_fn_lists_stay_in_sync` drift pin.
- [x] 2.3 Run tests and confirm they pass — subquery 12/12; exists_ probe 33/33 untouched; collect 7/7 no regression; full lib 2602/2604 (documented environment-sensitive trio, isolated-pass confirmed); per-file rustfmt clean; clippy -D warnings zero.
