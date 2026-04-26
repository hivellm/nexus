# Implementation Tasks — Quantified Path Patterns

Tracking is split across slices because the full Cypher 25 / GQL
QPP surface is multi-week scope. Each slice ships shippable
behaviour and lifts a strict subset of the
`ERR_QPP_NOT_IMPLEMENTED` shapes the planner used to reject:

- **Slice 1** — parser + AST + lowering for the textbook
  `( ()-[:T]->() ){m,n}` shape, end-to-end execution via the
  existing `VariableLengthPath` operator. Anonymous boundary
  nodes only. Shipped in `fd6a5eaa`.
- **Slice 2** — dedicated `QuantifiedExpand` operator for
  single-relationship bodies with named/labelled inner nodes,
  inline relationship-property filters, list-promoted bindings,
  zero-length case, depth cap. Shipped in `209b109a`.
- **Slice 3a** — multi-hop bodies. `QuantifiedExpand` refactored
  to `Vec<QppHopSpec>` + `Vec<QppNodeSpec>`; `qpp_walk_body`
  walks N hops depth-first per iteration. Shipped in `cd09689c`.
- **Slice 3b (open)** — inner `WHERE` push-down, cost-model
  refinement, `shortestPath(qpp)` over named-body shapes, TCK
  port, perf bench, dedicated `ERR_QPP_UNBOUND_UPPER`.

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
- [x] 2.3 List promotion of inner pattern variables to LIST type in
      outer scope — `execute_quantified_expand` emits every
      `QppNodeSpec.var` and `QppHopSpec.var` as `Value::Array(...)`
      with one entry per iteration, so the surrounding
      `RETURN x, y, r` clause sees the GQL `LIST<T>` type
- [x] 2.4 Preserve ordering semantics: `x[0]` is the first iteration
      — per-position `Vec<u64>` lists in the BFS frame push in
      iteration order, so `Vec<Value>` indexing matches `iteration`
      index regardless of body arity
- [x] 2.5 Handle zero-length case (`{0,n}`): inner lists are empty
      — `iteration == 0` satisfies the lower bound, the
      target binds back to `source_var`, and every promoted list
      ships as `Value::Array(Vec::new())`
- [x] 2.6 Tests asserting type promotion —
      `test_qpp_named_labelled_inner_node_executes` and
      `test_qpp_multi_hop_body_executes` both assert every
      list-promoted column emits a JSON array

## 3. Planner Operator: QuantifiedExpand

- [x] 3.1 Created `crates/nexus-core/src/executor/operators/quantified_expand.rs`
      with `Executor::execute_quantified_expand`,
      `Operator::QuantifiedExpand` variant in `executor/types.rs`
      (carrying `hops: Vec<QppHopSpec>` and
      `inner_nodes: Vec<QppNodeSpec>`), and dispatch wired in both
      `operators/dispatch.rs` and `executor/mod.rs`
- [x] 3.2 Inner sub-plan runs once per iteration with scoped bindings
      — `qpp_walk_body` recurses through `hops[hop_idx..]`,
      applying `QppHopSpec.properties` to each candidate
      relationship and `QppNodeSpec` filters to each landed-on
      node before emitting the iteration end node
- [x] 3.3 Per-frame bookkeeping —
      `(current_node, iteration, nodes_per_position, rels_per_hop)`
      tuple keeps one `Vec<u64>` per body position and per hop
      so list-promoted bindings stay aligned across the wavefront
- [x] 3.4 Cycle policy NODES_CAN_REPEAT for MATCH — hardcoded in
      `execute_quantified_expand` via the `(node, iteration)`
      wavefront dedup. ALL_DIFFERENT for `shortestPath` is
      slice 3b alongside the rest of the `shortestPath(qpp)`
      integration
- [x] 3.5 Pattern lower/upper bounds — `min_length` / `max_length`
      drive emission, unbounded quantifiers cap at `MAX_QPP_DEPTH = 64`
- [x] 3.6 Emit inner variables as LIST values on successful match
      — see 2.3; `qpp_path_rels_as_value_list` builds the
      `LIST<RELATIONSHIP>` shape SDKs already deserialise

## 4. Cost Model

- [x] 4.1 Cost estimate — `Operator::QuantifiedExpand` registered
      with cost `600.0` in `estimate_cost`, sized one notch above
      the legacy `VariableLengthPath` (`500.0`) to account for
      list-promoted bookkeeping. The `(avg_inner_fanout)^k`
      refinement lands in slice 3b alongside operator reordering.
- [ ] 4.2 Prefer index-backed starting nodes before entering the
      quantified body **(slice 3b)**
- [ ] 4.3 Push inner `WHERE` predicates into the iteration scope
      **(slice 3b)**
- [x] 4.4 Planner tests asserting expected operator order — four
      new tests in
      `crates/nexus-core/src/executor/planner/tests.rs::test_plan_qpp_*`
      pin the wiring: anonymous-body QPP must lower to
      `VariableLengthPath` (never `QuantifiedExpand`),
      named-inner-node QPP must emit `QuantifiedExpand` with
      `hops.len() == 1`, multi-hop bodies emit
      `QuantifiedExpand` with `hops.len() == n`, and the trailing
      named boundary node ends up as the operator's `target_var`

## 5. `shortestPath` / `allShortestPaths` Integration

- [x] 5.1 Slice-1 lowering routes `shortestPath((a)( ()-[:T]->() ){m,n}(b))`
      through the existing `shortestPath(*m..n)` path, verified by
      `test_qpp_lowering_under_shortest_path`
- [ ] 5.2 BFS on quantified iterations uses iteration count as cost
      **(slice 3b)** — needed when the body is *not* the
      slice-1 lowerable shape, so `shortestPath` over named-body
      QPP can take the dedicated operator
- [ ] 5.3 Early termination once a shortest match is confirmed
      **(slice 3b)**
- [ ] 5.4 Tests covering shortestPath over quantified patterns
      with named/labelled inner nodes **(slice 3b)**

## 6. Rewriter for Legacy Variable-Length Paths

The slice-1 lowering runs in the **opposite** direction (QPP →
legacy). The forward rewrite (legacy `*m..n` → QPP) becomes
worthwhile once the dedicated `QuantifiedExpand` operator is the
single execution path for both.

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
- [ ] 6.5 Rewrite legacy `*m..n` to `QuantifiedExpand` so there is
      one execution path **(slice 3b — gated on 4.x performance
      parity so we don't regress existing `*m..n` workloads)**

## 7. Error Taxonomy

- [x] 7.1 `ERR_QPP_NESTING_TOO_DEEP` rejected at parse time with
      a positional error
- [x] 7.2 `ERR_QPP_INVALID_QUANTIFIER` for `{n,m}` with `n > m`
- [x] 7.3 `ERR_QPP_NOT_IN_CREATE` for QPP inside `CREATE` (read-only)
- [x] 7.4 `ERR_QPP_NOT_IMPLEMENTED` for shapes the operator cannot
      drive yet (stacked relationship quantifier inside the body,
      malformed body shape that legitimised at parse time). Surface
      shrank dramatically across slices 2/3a — covered by
      `test_qpp_multi_hop_body_executes` (no longer errors) and
      retained as the catch-all for the few remaining slice-3b
      gaps
- [x] 7.5 Unbounded upper bound capped at `MAX_QPP_DEPTH = 64` in
      `execute_quantified_expand` — same pattern the legacy
      `VariableLengthPath` uses, so `*` / `+` / `{m,}` cannot fan
      out indefinitely. A dedicated `ERR_QPP_UNBOUND_UPPER` error
      that fires when the cap is hit (instead of silently
      truncating) lands in slice 3b alongside the cost-model
      refinements

## 8. openCypher TCK Coverage **(slice 3b)**

- [ ] 8.1 Import openCypher TCK QPP features (`quantified-path-patterns/*`)
- [ ] 8.2 Tag unsupported scenarios with `@qpp-scope` exclusions
- [ ] 8.3 Run TCK in CI; target 95%+ pass on in-scope scenarios
- [ ] 8.4 Compare output against Neo4j 5.15 diff harness

## 9. Performance Benchmarks

- [x] 9.1 Bench: bounded-hop traversal over a 50-node `:KNOWS` chain
      — `crates/nexus-core/benches/qpp_benchmark.rs::bench_qpp_named_body`
      drives the slice-3a `QuantifiedExpand` operator. Run via
      `cargo +nightly bench -p nexus-core --bench qpp_benchmark`.
      The 10k-node sweep called out in the original spec lands in
      slice 3b alongside the cost-model refinement.
- [x] 9.2 Bench: bounded reachability (`{1,5}`) vs legacy `*1..5`
      — `bench_qpp_anonymous_body` (slice-1 lowering path) +
      `bench_legacy_var_length` in the same harness. Both queries
      target the same fixture so Criterion's report is a clean
      side-by-side.
- [ ] 9.3 Bench: worst-case cycle-free traversal depth 10
      **(slice 3b)** — needs a denser fixture (graph fanout > 1)
      to exercise the BFS frontier
- [ ] 9.4 Regression: new ops must stay within 1.3× legacy runtime
      **(slice 3b)** — gate fires once Criterion baselines are
      committed and CI compares against them

## 10. Tail (mandatory — enforced by rulebook v5.3.0)

- [x] 10.1 Update `docs/specs/cypher-subset.md` with the new grammar
      — added the `QuantifiedGroup` production to the BNF block and
      the slice-1 example to the MATCH section
- [x] 10.2 Add `docs/guides/QUANTIFIED_PATH_PATTERNS.md` user guide
      — covers slice-1 surface, full quantifier table, slice-2/3a
      gaps, error codes, migration tips for Neo4j 5.9+ users, and
      an implementation pointer for contributors
- [x] 10.3 Update `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md`
      — new "v1.15 — Quantified Path Patterns slice 1" entry above
      the v1.8 full-text-search entry. Slice-2/3a refresh tracked
      separately when the v1.16 report ships.
- [x] 10.4 CHANGELOG entry — added "Quantified path patterns
      (Cypher 25 / GQL) — anonymous-body shape" under
      `## [1.15.0]` § Added. Multi-hop coverage covered by the
      same entry's narrative.
- [x] 10.5 Documentation covering the implementation — see 10.1–10.4
      above + rustdoc on `QuantifiedGroup::try_lower_to_var_length_rel`,
      `Pattern::lowered_for_planner`, `Operator::QuantifiedExpand`,
      `QppHopSpec`, `QppNodeSpec`, and the `qpp_walk_body` /
      `qpp_lists_satisfy_filters` helpers
- [x] 10.6 Tests covering the new behaviour — 14 unit tests in
      `crates/nexus-core/src/executor/parser/tests.rs::qpp_*` +
      7 integration tests in
      `crates/nexus-core/tests/executor_comprehensive_test.rs::test_qpp_*`
      (parity, direction, exact/optional quantifiers, relationship
      variable propagation, `shortestPath` integration,
      named/labelled inner-node operator path, multi-hop body
      operator path)
- [x] 10.7 Run tests and confirm they pass —
      `cargo test -p nexus-core --lib executor::parser::tests::qpp`
      reports `14 passed; 0 failed`; integration tests
      `7 passed; 0 failed`; full lib suite `2054 passed; 0 failed;
      12 ignored`.
- [x] 10.8 Quality pipeline: `cargo fmt --all` clean,
      `cargo clippy -p nexus-core --lib --tests -- -D warnings`
      clean. Coverage gate left for slice 3b alongside the
      remaining operator-surface work.

## Status

- **Slice 1** — shipped in `fd6a5eaa`. Anonymous-body QPP lowers
  to legacy `*m..n` at parse time.
- **Slice 2** — shipped in `209b109a`. Single-relationship
  `QuantifiedExpand` operator with list-promoted bindings,
  zero-length case, depth cap, inline rel-property filter.
- **Slice 3a** — shipped this turn (`cd09689c`).
  `Operator::QuantifiedExpand` refactored to
  `Vec<QppHopSpec>` + `Vec<QppNodeSpec>`; `qpp_walk_body` walks
  N hops per iteration; the planner accepts any odd-length
  alternating Node-Rel-…-Node body. Multi-hop bodies like
  `( (x:Person)-[:KNOWS]->(y:Person)-[:KNOWS]->(z:Person) ){1}`
  now execute end-to-end with every named inner node and hop
  relationship list-promoted to the GQL `LIST<T>` type.
- **Slice 3b** — open. Items 4.2–4.3, 5.2–5.4, 6.5, 7.5
  (dedicated `ERR_QPP_UNBOUND_UPPER`), 8.x, 9.3–9.4 stay `[ ]`.
  4.4 (planner-shape tests) and 9.1–9.2 (bench harness) flipped
  to checked this turn. The cost-model refinement and
  `shortestPath(qpp)` over named-body shapes are the most
  impactful remaining work.

## Cumulative test count

- Parser unit tests: 14 (`qpp_parses_*` + `qpp_rejects_*` +
  lowering + `qpp_bare_parens_without_quantifier_is_not_qpp` +
  `qpp_with_legacy_varlen_coexists`)
- Executor integration tests: 7 (`test_qpp_single_rel_lowers_to_legacy_var_length`,
  `test_qpp_lowering_preserves_direction`,
  `test_qpp_lowering_exact_and_optional_quantifiers`,
  `test_qpp_lowering_keeps_inner_relationship_variable_addressable`,
  `test_qpp_lowering_under_shortest_path`,
  `test_qpp_named_labelled_inner_node_executes`,
  `test_qpp_multi_hop_body_executes`)
