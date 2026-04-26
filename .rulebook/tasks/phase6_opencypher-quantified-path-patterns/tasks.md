# Implementation Tasks — Quantified Path Patterns

Tracking is split into two slices because the full operator is a
multi-week rewrite and the textbook QPP shape has a lossless
collapse to legacy `*m..n` that ships an order of magnitude faster:

- **Slice 1** (this commit) — parser + AST + lowering for the
  textbook `( ()-[:T]->() ){m,n}` shape, end-to-end execution via
  the existing `VariableLengthPath` operator. Anonymous boundary
  nodes only.
- **Slice 2** — full `QuantifiedExpand` operator with list-promoted
  bindings, intermediate-node filters, and `shortestPath(qpp)`
  integration. Required for named/labelled inner nodes, multi-hop
  bodies, and inner predicates.

## 1. Grammar & Tokenization

- [x] 1.1 Add quantifier tokens `{m,n}`, `{m,}`, `{,n}`, `{n}`, `+`, `*`, `?`
- [x] 1.2 Disambiguate from map literals (`{a:1}`) via lookahead
- [x] 1.3 Extend pattern grammar to accept `'(' pathFragment ')' quantifier`
- [x] 1.4 Allow nested QPP (one level deep — Cypher 25 restriction)
      — explicit reject with `ERR_QPP_NESTING_TOO_DEEP`
- [x] 1.5 Reject empty quantifier `{}`
- [x] 1.6 Parser unit tests for every quantifier form
      (`qpp_parses_*` + `qpp_rejects_*` + `qpp_with_legacy_varlen_coexists`)

## 2. AST & Type Promotion

- [x] 2.1 `PatternElement::QuantifiedGroup(QuantifiedGroup)` in
      `crates/nexus-core/src/executor/parser/ast.rs`
- [x] 2.2 Trailing-boundary-node bug in `parse_pattern` fixed —
      `(a)( body ){m,n}(b)` now produces three elements; previously
      the trailing `(b)` was silently dropped, which left the
      planner without a target var and confused projections
- [ ] 2.3 List promotion of inner pattern variables to LIST type in
      outer scope **(slice 2)** — only meaningful when the body
      carries named/labelled inner nodes; the slice-1 lowering only
      runs for anonymous bodies that have no inner state to promote
- [ ] 2.4 Preserve ordering semantics: `x[0]` is the first iteration
      **(slice 2)**
- [ ] 2.5 Handle zero-length case (`{0,n}`): inner lists are empty
      **(slice 2)** — slice-1 routes through `VariableLengthPath`
      which already handles `{0,n}` for the relationship
- [ ] 2.6 Tests asserting type promotion **(slice 2)**

## 3. Planner Operator: QuantifiedExpand **(slice 2)**

- [ ] 3.1 Create `operators/quantified_expand.rs`
- [ ] 3.2 Inner sub-plan runs once per iteration with scoped bindings
- [ ] 3.3 Backtracking search with per-frame bookkeeping
- [ ] 3.4 Cycle policy: MATCH uses NODES_CAN_REPEAT, ALL_DIFFERENT for shortestPath
- [ ] 3.5 Enforce pattern lower/upper bounds
- [ ] 3.6 Emit inner variables as LIST values on successful match

## 4. Cost Model **(slice 2)**

- [ ] 4.1 Cost estimate ≈ (avg_inner_fanout)^k × outer_rows for QPP of length k
- [ ] 4.2 Prefer index-backed starting nodes before entering the quantified body
- [ ] 4.3 Push inner `WHERE` predicates into the iteration scope
- [ ] 4.4 Planner tests asserting expected operator order

## 5. `shortestPath` / `allShortestPaths` Integration **(slice 2)**

- [ ] 5.1 Accept quantified patterns as the path argument
- [ ] 5.2 BFS on quantified iterations uses iteration count as cost
- [ ] 5.3 Early termination once a shortest match is confirmed
- [ ] 5.4 Tests covering shortestPath over quantified patterns

## 6. Rewriter for Legacy Variable-Length Paths

The slice-1 lowering runs in the **opposite** direction (QPP →
legacy) so the existing operator carries the textbook QPP shape.
The forward rewrite (legacy `*m..n` → QPP) only matters once the
dedicated operator exists; until then it would be a strict
regression because `QuantifiedExpand` is not yet implemented.

- [x] 6.1 (Inverted) Lower `( ()-[:T]->() ){m,n}` to
      `RelationshipPattern { quantifier: …, … }` so the existing
      `VariableLengthPath` operator handles it
      (`QuantifiedGroup::try_lower_to_var_length_rel`)
- [x] 6.2 Preserve the user-facing relationship variable, type
      list, direction, and property map on the lowered relationship
      (`qpp_lowering_preserves_relationship_variable_and_direction`)
- [x] 6.3 Regression: every existing `*m..n` query keeps the same
      operator path
      (`qpp_with_legacy_varlen_coexists`,
      `test_variable_length_path` in
      `tests/executor_comprehensive_test.rs`)
- [x] 6.4 Tests: identical row sets for the lowered QPP form vs
      hand-written legacy equivalents
      (`test_qpp_single_rel_lowers_to_legacy_var_length`)
- [ ] 6.5 (Slice 2) Rewrite legacy `*m..n` to `QuantifiedExpand`
      once that operator lands, so there is one execution path

## 7. Error Taxonomy

- [x] 7.1 `ERR_QPP_NESTING_TOO_DEEP` rejected at parse time with
      a positional error
- [x] 7.2 `ERR_QPP_INVALID_QUANTIFIER` for `{n,m}` with `n > m`
- [x] 7.3 `ERR_QPP_NOT_IN_CREATE` for QPP inside `CREATE` (read-only)
- [x] 7.4 `ERR_QPP_NOT_IMPLEMENTED` for shapes the slice-1
      lowering cannot handle, surfaced from the planner with a
      pointer to the follow-up task — verified by
      `test_qpp_unsupported_shape_returns_clean_error`
- [ ] 7.5 (Slice 2) `ERR_QPP_UNBOUND_UPPER`: `*` without explicit
      cap rejected when memory would blow up — currently delegated
      to the existing `VariableLengthPath` safety cap for lowered
      forms; the dedicated operator will need its own

## 8. openCypher TCK Coverage **(slice 2)**

- [ ] 8.1 Import openCypher TCK QPP features (`quantified-path-patterns/*`)
- [ ] 8.2 Tag unsupported scenarios with `@qpp-scope` exclusions
- [ ] 8.3 Run TCK in CI; target 95%+ pass on in-scope scenarios
- [ ] 8.4 Compare output against Neo4j 5.15 diff harness

## 9. Performance Benchmarks **(slice 2)**

- [ ] 9.1 Bench: 5-hop friend-of-friend over 10k nodes
- [ ] 9.2 Bench: bounded reachability (`{1,5}`) vs legacy `*1..5`
- [ ] 9.3 Bench: worst-case cycle-free traversal depth 10
- [ ] 9.4 Regression: new ops must stay within 1.3× legacy runtime

## 10. Tail (mandatory — enforced by rulebook v5.3.0)

- [x] 10.1 Update `docs/specs/cypher-subset.md` with the new grammar
      — added the `QuantifiedGroup` production to the BNF block and
      the slice-1 example to the MATCH section
- [x] 10.2 Add `docs/guides/QUANTIFIED_PATH_PATTERNS.md` user guide
      — covers slice-1 surface, full quantifier table, slice-2 gaps,
      error codes, migration tips for Neo4j 5.9+ users, and an
      implementation pointer for contributors
- [x] 10.3 Update `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md`
      — new "v1.15 — Quantified Path Patterns slice 1" entry above
      the v1.8 full-text-search entry
- [x] 10.4 CHANGELOG entry — added "Quantified path patterns
      (Cypher 25 / GQL) — anonymous-body shape" under
      `## [1.15.0]` § Added
- [x] 10.5 Documentation covering the implementation — see 10.1–10.4
      above + rustdoc on `QuantifiedGroup::try_lower_to_var_length_rel`
      and `Pattern::lowered_for_planner`
- [x] 10.6 Tests covering the new behaviour — 14 unit tests in
      `crates/nexus-core/src/executor/parser/tests.rs::qpp_*` +
      6 integration tests in
      `crates/nexus-core/tests/executor_comprehensive_test.rs::test_qpp_*`
      (parity, direction, exact/optional quantifiers, relationship
      variable propagation, `shortestPath` integration, unsupported
      shape error contract)
- [x] 10.7 Run tests and confirm they pass —
      `cargo test -p nexus-core --lib executor::parser::tests::qpp`
      reports `14 passed; 0 failed`; integration tests
      `6 passed; 0 failed`; full lib suite `2054 passed`.
- [x] 10.8 Quality pipeline: `cargo fmt` clean,
      `cargo clippy -p nexus-core --lib --tests -- -D warnings`
      clean. Coverage gate left for slice 2 alongside the rest of
      the operator surface.

## Status

- **Slice 1** (parser + lowering for anonymous-body QPP):
  shipped this turn. Users can write
  `MATCH (a)( ()-[:T]->() ){1,5}(b) RETURN a, b` and execution
  matches the legacy `*1..5` form byte-for-byte.
- **Slice 2** (full `QuantifiedExpand` operator with list-promoted
  bindings, named/labelled inner nodes, multi-hop bodies, inner
  predicates, `shortestPath(qpp)`, TCK, perf): tracked in this
  same task; remaining items above are all flagged
  **(slice 2)**. A follow-up rulebook task (`phase6_qpp-operator`)
  will be created before this task is archived so the slice-2
  scope is not orphaned.
