## 1. Implementation
- [x] 1.1 Type-kind gate for the ORDERING operators (`<`, `<=`, `>`, `>=`): yield `null` when the two operands are not the same kind, counting INT and FLOAT as one numeric kind; keep the existing `null`-operand rule. Applied in the operator path only — `compare_values_for_sort` must keep its total order for `ORDER BY`
  - New `ValueKind` + `value_type_kind` in `eval/predicate.rs`, and
    `Executor::comparable_kinds` on top of it. The four ordering arms collapse
    into one gated arm in each path.
  - **One taxonomy, two orderings.** `operators::project::order_by_type_rank`
    (added by the ORDER BY task) now derives its rank from the same classifier
    instead of re-deriving the same match. Two classifications of "what is this
    value" would eventually disagree.
  - Entity kinds come from `is_node_value` / `is_relationship_value`, never from
    the presence of a property — a node with a `type` property is not a
    relationship.
- [x] 1.2 Cross-kind `=` / `<>` yield `false` / `true` instead of a coerced verdict (`1.0 = '1.0'` is `false`, not `true`), while numeric cross-subtype equality (`1 = 1.0`) keeps working
  - Deleted the two `(String, Number)` / `(Number, String)` arms of
    `values_equal_for_comparison`, which parsed the string and compared numerically.
    The catch-all now requires the kinds to match before comparing structurally.
  - **Found while implementing, not predicted by the proposal: `=` and `<>` were
    not negations of each other.** `=` used the numeric-aware helper while `<>`
    used a raw `!=`, and serde's `Number(1)` and `Number(1.0)` differ
    structurally — so `RETURN 1 = 1.0, 1 <> 1.0` returned `[true, true]`. Both
    paths now negate the helper. Measured before and after.
- [x] 1.3 Cover BOTH evaluation paths — `eval/predicate.rs` and `eval/projection/core.rs` implement these operators separately; a fix in one leaves the other wrong (verify with a query routed through each)
  - Both patched. **And there is a THIRD, which the task did not know about:**
    `evaluate_predicate` (the `WHERE` path) does not use either comparator — it
    routes through `compare_values`, which coerces both operands to `f64` via
    `value_to_number` (parsing strings, mapping booleans to 1/0, erroring on
    null). Measured: `WHERE n.name < 5` drops the row (accidentally right) while
    `WHERE n.age < 'bob'` KEEPS it (wrong).
  - Left alone deliberately, not overlooked. The TCK scenarios in scope evaluate
    their comparison in a projection (`WITH lhs, rhs, lhs < rhs AS result`), so
    they exercise the expression paths; and rewriting the `WHERE` comparator is a
    second, larger change with its own regression surface (it returns `bool`, so
    `null` has to become "drop the row" at every call site). Recorded as a
    remaining gap in `docs/specs/cypher-subset.md` with the exact reproduction.

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation
  - `docs/specs/cypher-subset.md`: a new "Comparison and equality across types"
    section — the two different rules side by side with worked examples, the
    `null`-drops-the-row consequence under `WHERE`, inline property matching
    following equality, the temporal exception, and a "remaining gaps" note naming
    map-equality 3VL, list comparison, and the `WHERE`-path coercion.
  - `CHANGELOG.md`: three entries, two marked BREAKING (a boolean becomes `null`;
    `1.0 = '1.0'` becomes `false`, which also changes inline property matching).
- [x] 2.2 Write tests covering the new behavior
  - 8 tests in `tests/cypher/cross_type_comparison_test.rs`: all four ordering
    operators across several type pairs → `null`; a control that ordering WITHIN a
    type still works (numbers, strings, booleans, lists); INT/FLOAT as one kind;
    cross-type equality → `false`; `=`/`<>` as exact negations; `null` operands
    still `null`; the duration two-representation regression guard; and inline
    property matching no longer coercing.
- [x] 2.3 Run tests and confirm they pass
  - `--test cypher` 675, `--test executor` 272, `--test regression` 227,
    `--test compatibility` 245 — 0 failures. Workspace `--no-fail-fast`:
    **5822 passed, 3 failed**, the 3 being the documented flakes. `fmt` and
    `clippy --workspace --all-targets --all-features -D warnings` clean.
  - **One real regression was caught by the suite and fixed:**
    `where_less_than_and_greater_than_compare_durations_by_component_value` went
    `false` → `null`, because `duration('PT10H')` is a tagged OBJECT in flight
    while the stored value is a canonical STRING, so the new gate saw
    Map-vs-String. `compare_values_for_sort` canonicalizes both operands before
    comparing; `value_type_kind` now does the same first. Pinned by its own test.
  - **TCK: +4 attributable, zero regressions.** `expressions/comparison`
    **44 → 48 of 72 (61.1% → 66.7%)**, total 1889 → 1891. The two newly passing
    scenarios are exactly the targeted ones: `[6] Comparability between numbers
    and strings` and `[9] Equality between strings and numbers` (2 expanded each).
  - The two categories that dropped are the two documented oscillators —
    `clauses/return [9] Returning a projected map` (map-literal key order) and
    `clauses/with-orderBy [15]`, which flipped both directions across today's runs.
    Neither is reachable from a comparison-operator change: `[9]`'s query is
    `RETURN {a: 1, b: 'foo'}`, which compares nothing.
  - `Comparison2` [3] "Comparing across types yields null, except numbers" (4
    scenarios) did NOT move, as expected: its fixture puts a PATH in a list, and a
    path does not survive a list today — the same gap noted in the ORDER BY task.
