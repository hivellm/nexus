# Tasks: phase0_fix-where-in-prefix-param-index-seek

Remaining WHERE index-seek forms split out of
`phase0_fix-where-clause-index-seek-extensions` (range `>`/`>=`/`<`/`<=` +
EXPLAIN accuracy landed there). The B-tree already supports the primitives:
`find_exact` (for `IN`) and `find_prefix` (for `STARTS WITH`). Mirror the
`where_range_seek_operand` / `NodeIndexRangeSeek` shape the parent added. All
plan-selection only — results must not change.

## 1. `IN` seek
- [x] 1.1 Reproduce: `MATCH (n:Person) WHERE n.age IN [1,2,3]` full-scans today.
  Baseline captured in `tests/cypher/where_in_prefix_param_index_seek_test.rs`
  through a real `Engine` — `create_isolated_test_executor` installs NO
  `PropertyIndex`, so the parent's `tests/cypher/where_range_index_seek_test.rs`
  can only ever assert scan-path parity, never that a seek was planned.
- [x] 1.2 Added `Operator::NodeIndexInSeek { label_id, key_id, values, variable }`
  (`types.rs`), `execute_node_index_in_seek` (union of `find_exact`, `scan.rs`),
  `where_in_seek_operand` lift (`strategy.rs`), both dispatch arms and the cost
  arm. Numeric keys probe both `Integer` and `Float` index entries
  (`numeric_alias`) because Cypher compares `10 = 10.0` while the B-tree keys
  them apart — the lifted conjunct is removed, so a false negative would be a
  wrong answer.
- [x] 1.3 Result-parity + plan tests: absent value, `IN []`, `IN [null]`,
  float/int cross-type, extra conjunct, unindexed property, `$parameter` list.

## 2. `STARTS WITH` seek
- [x] 2.1 Reproduce: `WHERE n.name STARTS WITH 'A'` full-scans today (same
  baseline test file).
- [x] 2.2 Added `PropertyIndex::find_prefix` (the pre-existing `find_prefix`
  lived on the unrelated string-keyed `cache/property_index.rs`, not on the
  index the seeks use), `Operator::NodeIndexPrefixSeek`,
  `execute_node_index_prefix_seek`, `where_prefix_seek_operand`, dispatch +
  cost arms. The scan walks from `String(prefix)` and stops at the first
  non-matching key — exact even for strings containing `char::MAX`, which a
  `prefix ..= prefix + char::MAX` range would miss.
- [x] 2.3 Result-parity + plan tests: exact-value prefix, empty prefix, no
  match, prefix boundary (`alpha` must not drag in `alpine`), extra conjunct.

## 3. `$parameter` equality seek
- [x] 3.1 Reproduce: `WHERE n.prop = $x` full-scans (same baseline test file).
- [x] 3.2 Added `Operator::NodeIndexParamSeek { parameter, .. }` +
  `execute_node_index_param_seek`, which resolves the key from
  `ExecutionContext::params` — no driving rows, so it works as the first scan
  of the query. `where_param_seek_operand` lifts it in a SECOND pass, after
  every literal-keyed lift has been tried (a literal seek can never degrade)
  and — uniquely — does NOT consume the conjunct: a list/map-bound parameter
  makes the operator fall back to a label scan, and the retained `Filter` is
  what keeps that correct. A `null` parameter short-circuits to no rows.
- [x] 3.3 Result-parity + plan tests: missing value, `null`, list-bound
  parameter, float/int cross-type, mirrored operand order, literal-wins-the-lift
  precedence, unindexed property.

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [x] 4.1 `docs/specs/cypher-subset.md` — WHERE-form seek coverage rewritten
  (every form and its exact limitation). Notification text/doc corrected: it
  claimed "only a plain equality predicate uses the index", stale since the
  parent landed range.
- [x] 4.2 Tests: `tests/cypher/where_in_prefix_param_index_seek_test.rs` (10
  engine-level tests), 2 planner unit tests, and
  `tests/executor/where_clause_index_seek_test.rs` updated — its three
  "range/IN/STARTS WITH must notify" tests encoded the pre-seek contract; they
  now assert silence when the predicate seeks and notification when the operand
  is a `$parameter` (no plan-time key ⇒ a real scan).
- [x] 4.3 Full gate green: `cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test --workspace` (0 failed).

## Fixed along the way
- `optimize_operator_order` (`cost.rs`) buckets every variable-BINDING operator
  into `scans` (recombined BEFORE `filters`); `NodeIndexRangeSeek` was never
  added there by the parent task, so it fell into `others` and any residual
  `Filter` would run BEFORE the seek bound its variable — silently dropping
  every row. The parent's own tests missed it because they run on an executor
  with no `PropertyIndex`, so no seek was ever planned. All four seek operators
  are now in both lists.

## Related
- `phase0_fix-where-clause-index-seek-extensions` — parent (range + EXPLAIN)
