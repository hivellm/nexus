# Tasks: phase0_fix-optional-match-var-scoping

OPTIONAL MATCH decides which variables may be NULL by unconditionally skipping
the pattern's first node (`planner_core.rs:336-345`), assuming it is always the
pre-bound anchor from a prior clause. That assumption is false for a standalone
OPTIONAL MATCH (no prior anchor at all) and for reverse-direction patterns (the
anchor appears later in the pattern), so `execute_optional_filter`
(`operators/filter.rs:343-467`) either inverts which side is nullable or the
WHERE clause silently drops rows that Cypher requires to be preserved with NULL.

Order matters: prove both failure modes with failing tests (§1) before touching
any code, so the fix is verified against real symptoms, not just code reading.
Confirm what bound-variable state the planner already tracks at the OPTIONAL
MATCH site (§2) before implementing the replacement (§3), because the fix
depends on subtracting a bound-variable set that may or may not already be
exposed at that point in the loop. Docs and the full test/lint gate come last
(§4).

## 1. Reproduce the defect first
- [ ] 1.1 Write a failing integration test for the reverse-direction case:
  `MATCH (a:Person {name:'Alice'}) OPTIONAL MATCH (b:Person)-[:KNOWS]->(a) WHERE
  b.age > 30 RETURN a.name, b.name` against an Alice with no qualifying KNOWS
  edge — assert `a` is returned exactly once with `b.name = NULL`. Confirm it
  fails today (the bound anchor `a` is incorrectly treated as nullable per
  `planner_core.rs:340-345`, producing an inverted or missing row)
- [ ] 1.2 Write a failing integration test for the standalone-OPTIONAL-MATCH
  case: `MATCH (a:Person) OPTIONAL MATCH (c:Company) WHERE c.rating > 4 RETURN
  a.name, c.name`, with an `a` that has no qualifying `c` — assert the `a` row is
  still returned (with `c.name = NULL`). Confirm it fails today: `c` is skipped
  as "the anchor", `last_optional_vars` stays empty, and `strategy.rs:426-428`
  lowers the WHERE as a plain `Filter`, dropping the row
- [ ] 1.3 Add a baseline test for the case the current skip-first-node heuristic
  gets right — forward pattern where the first node genuinely IS the previously
  bound anchor: `MATCH (a:Person) OPTIONAL MATCH (a)-[:KNOWS]->(b) WHERE b.age >
  30 RETURN a.name, b.name`. Confirm it passes today, so the fix in §3 must not
  regress it

## 2. Confirm the fix boundary
- [ ] 2.1 Trace what "variables already bound by prior clauses" state the
  planner holds at the point OPTIONAL MATCH is processed
  (`planner_core.rs:324-370`) — identify the existing bound-variable
  accumulator (or confirm one must be introduced) that `optional_vars =
  pattern_vars - bound_vars` needs to read
- [ ] 2.2 Confirm the `QuantifiedGroup` branch (`planner_core.rs:352-368`) has no
  anchor-skip today — it unconditionally collects all inner variables — so its
  bound-var-diff treatment after the fix must match the corrected Node and
  Relationship branches rather than gaining a new positional skip

## 3. Fix
- [ ] 3.1 Replace the `is_first_node` position-based skip in
  `planner_core.rs:336-345` with a bound-variable-set subtraction: collect every
  variable referenced in the OPTIONAL MATCH pattern (Node, Relationship,
  QuantifiedGroup — same traversal as today), then remove any variable already
  present in the accumulated bound-variable set from prior clauses, leaving
  `last_optional_vars`
- [ ] 3.2 Confirm `strategy.rs:424-441` and `operators/filter.rs:343-467`
  (`execute_optional_filter`) need no changes — the `optional_vars` contract is
  unchanged, only its computation changes; reason through `:380-439`
  (nullable/mandatory split semantics) against both §1 trigger cases to confirm
  the new computation produces the correct split
- [ ] 3.3 Make the §1.1 and §1.2 tests pass while keeping §1.3 green; add a
  chained-OPTIONAL-MATCH case (two consecutive OPTIONAL MATCH clauses in one
  query) to check the bound-variable accumulation does not carry stale vars
  forward incorrectly

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 4.1 Update `docs/specs/cypher-subset.md`'s OPTIONAL MATCH section with the
  corrected variable-scoping rule (bound-variable-set difference, not
  positional first-node skip); add a CHANGELOG entry
- [ ] 4.2 Tests: reverse-direction OPTIONAL MATCH, standalone OPTIONAL MATCH
  with empty `optional_vars`, forward-anchor baseline, chained OPTIONAL MATCH —
  all passing
- [ ] 4.3 Run `cargo +nightly fmt --all`, `cargo clippy --workspace
  --all-targets --all-features -- -D warnings`, `cargo +nightly test
  --workspace` — all green

## Related
- `phase0_fix-plan-reorder-drops-predicates`,
  `phase0_fix-where-predicate-reparse-precedence` — other WHERE/planner
  correctness defects found in the same audit
