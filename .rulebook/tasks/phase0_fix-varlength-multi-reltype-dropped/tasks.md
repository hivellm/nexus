# Tasks: phase0_fix-varlength-multi-reltype-dropped

The default (non-QPP) lowering for variable-length relationship patterns
keeps only the first relationship type: `let type_id =
type_ids.first().copied();` (`relationships.rs:160`), and
`Operator::VariableLengthPath.type_id` is a single `Option<u32>`
(`executor/types.rs:456-471`), so the operator cannot structurally represent
more than one type. Trigger: `MATCH (a:Person {name:'Alice'})-[:KNOWS|FOLLOWS*1..3]->(b:Person)
RETURN b.name` only traverses `KNOWS`; any `b` reachable solely via `FOLLOWS`
is silently dropped, with no error.

Order matters: reproduce the drop first (§1) so the fix has a concrete
target, confirm every place `type_id` is narrowed to one element (§2) before
retyping the field (§3) — the struct change and its call sites must land
together or the code will not compile, so §3 is a single cohesive step, not
split further.

## 1. Reproduce the loss first
- [ ] 1.1 Write a failing integration test: create `:Person` nodes
  `Alice -[:FOLLOWS]-> Carol` with no `KNOWS` edge in the path, run
  `MATCH (a:Person {name:'Alice'})-[:KNOWS|FOLLOWS*1..3]->(b:Person) RETURN
  b.name`, and assert `Carol` is in the result. Confirm it fails today
  (`Carol` is absent)
- [ ] 1.2 Extend the test with a mixed-type path (`Alice -[:KNOWS]-> Bob
  -[:FOLLOWS]-> Dave`) and assert both `Bob` and `Dave` are returned for
  `[:KNOWS|FOLLOWS*1..3]`. Confirm `Dave` is missing today (the BFS drops the
  `FOLLOWS` hop after the first `KNOWS` hop, since only `KNOWS` survives
  lowering)
- [ ] 1.3 Add a control test: an unqualified `[*1..3]` (no type filter) over
  the same graph still returns every reachable node — confirms the
  "empty type_ids = match all" path is not accidentally broken by the fix
  before it's even made

## 2. Confirm the mechanism
- [ ] 2.1 Confirm the parser produces the full `type_ids: Vec<u32>` for a
  `[:R1|R2*m..n]` pattern (same parse path as the single-hop case) before it
  reaches `relationships.rs:158-169`
- [ ] 2.2 Confirm the default (non-QPP) branch discards all but the first
  type at `relationships.rs:160` (`type_ids.first().copied()`), while the
  sibling single-hop `Expand` lowering two lines below keeps the full
  `type_ids` (`relationships.rs:173`)
- [ ] 2.3 Enumerate every `executor/operators/path.rs` call site that narrows
  `Operator::VariableLengthPath.type_id: Option<u32>` back into an
  at-most-one-element slice via `type_id.into_iter().collect()` (path.rs:1062,
  1164, 1218, 1283, 1311, and the `visit_relationship`/BFS filter at
  `path.rs:804-807`), confirming each is a fix site
- [ ] 2.4 Confirm `find_relationships`'s type-matching already supports a
  multi-element slice correctly (`type_ids.is_empty() || type_ids.contains(&record_type_id)`,
  path.rs:56/345/490/601) — the BFS layer needs the full list threaded in,
  not new matching logic

## 3. Fix the type handling
- [ ] 3.1 Change `Operator::VariableLengthPath.type_id: Option<u32>` to
  `type_ids: Vec<u32>` in `executor/types.rs:456-471`, and update the
  lowering at `relationships.rs:158-169` to construct the operator with the
  full `type_ids.clone()` instead of `type_ids.first().copied()`
  (preserving "empty vec = match all types", mirroring `Expand`)
- [ ] 3.2 Update every call site enumerated in §2.3 in `path.rs` to pass the
  operator's `type_ids: Vec<u32>` (or a borrowed slice of it) directly into
  `find_relationships`/the BFS type filter, removing the
  `type_id.into_iter().collect()` single-element reconstruction
- [ ] 3.3 Rebuild and fix any other exhaustive match/construction of
  `Operator::VariableLengthPath` that references the old `type_id` field
  (e.g. plan-cost estimation, plan explain/debug formatting, any test
  fixture) so the workspace compiles
- [ ] 3.4 Make the §1 tests pass; re-run them to confirm both `Carol` and
  `Dave` are now returned, and the unqualified `[*1..3]` control test is
  unaffected

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 4.1 Update `docs/specs/cypher-subset.md` if it documents
  variable-length relationship type-union semantics; add a CHANGELOG entry
- [ ] 4.2 Tests: the §1 regression tests pass; add a case with three or more
  unioned types (`[:A|B|C*1..2]`) and a case where the QPP-rewrite path
  (`NEXUS_QPP_REWRITE_LEGACY=1`) is exercised to confirm it was already
  correct and remains so
- [ ] 4.3 Run `cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test --workspace` — all green

## Related
- `phase0_fix-plan-reorder-drops-predicates` — another `VariableLengthPath`-
  adjacent planner correctness defect, in operator ordering rather than type
  handling
