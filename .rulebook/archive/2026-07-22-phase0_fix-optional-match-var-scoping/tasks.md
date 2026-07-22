# Tasks: phase0_fix-optional-match-var-scoping

OPTIONAL MATCH decided which variables may be NULL by unconditionally skipping
the pattern's first node (an `is_first_node` skip in `planner_core.rs`), assuming
it is always the pre-bound anchor. False for standalone OPTIONAL MATCH (no prior
anchor) and reverse-direction patterns (anchor appears later), so
`execute_optional_filter` inverted which side is nullable, or the WHERE silently
dropped rows Cypher requires preserved with NULL. Fixed by identifying the anchor
by binding state: `optional_vars = pattern_vars - bound_vars`. Landed in commit
877b81df.

## 1. Reproduce the defect first
- [x] 1.1 Reverse-direction test written + confirmed failing pre-fix: the bound
  anchor `a` was nulled (`reverse_direction_optional_match_nulls_the_new_node_not_the_anchor`)
- [x] 1.2 Standalone-OPTIONAL-MATCH test: driver `a` row was dropped instead of
  preserved with `c = NULL` pre-fix
  (`standalone_optional_match_preserves_driver_row_when_filter_excludes_new_node`)
- [x] 1.3 Forward-anchor baseline confirmed passing pre- and post-fix
  (`forward_anchor_optional_match_where_still_left_outer_joins`)

## 2. Confirm the fix boundary
- [x] 2.1 Finding: NO existing bound-variable accumulator at the OPTIONAL MATCH
  site (a similarly-named `previously_bound_vars` exists in `strategy.rs` but is
  built later, for a different purpose, and not in scope here). Introduced a new
  `bound_vars: HashSet<String>` accumulated across the `plan_query` clause walk
- [x] 2.2 Confirmed the `QuantifiedGroup` branch had no positional skip; the new
  `collect_pattern_variables` helper treats it uniformly with Node/Relationship

## 3. Fix
- [x] 3.1 Replaced the `is_first_node` skip with a bound-variable-set
  subtraction: extracted `collect_pattern_variables` (Node/Relationship/
  QuantifiedGroup, no skip), computed `last_optional_vars = pattern_vars -
  bound_vars`, and `bound_vars.extend(pattern_vars)` after every MATCH (optional
  or not) so chained OPTIONAL MATCH accumulates scope correctly
- [x] 3.2 Confirmed `strategy.rs` WHERE-lowering and `filter.rs`
  `execute_optional_filter` need no changes — only the upstream `optional_vars`
  computation changed
- [x] 3.3 §1.1/§1.2 pass, §1.3 stays green; added two chained-OPTIONAL-MATCH
  guards (`chained_optional_match_does_not_leak_the_first_clauses_new_var_into_the_second`,
  `chained_optional_match_both_links_resolve`)

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [x] 4.1 Update or create documentation covering the implementation — CHANGELOG
  entry added ("OPTIONAL MATCH variable scoping and LEFT OUTER JOIN semantics").
  `docs/specs/cypher-subset.md`'s OPTIONAL MATCH section documents syntax +
  `EnsureNullRowIfEmpty` but not the nullable-variable rule, so no spec edit was
  warranted (the binding-state computation is an internal detail)
- [x] 4.2 Write tests covering the new behavior — `optional_match_var_scoping_test.rs`
  (5): reverse-direction, standalone, forward-anchor baseline, and two chained
  cases
- [x] 4.3 Run tests and confirm they pass — `cargo +nightly fmt --all` + clippy
  clean (pre-commit hook); full workspace `cargo +nightly test --workspace` green
  (5097 passed / 0 failed; executor 195, cypher 370, regression 141)

## Related
- `phase0_fix-plan-reorder-drops-predicates`,
  `phase0_fix-where-predicate-reparse-precedence` — other WHERE/planner
  correctness defects from the same audit
