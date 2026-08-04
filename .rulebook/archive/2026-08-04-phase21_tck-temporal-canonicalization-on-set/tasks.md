## 1. Implementation
- [x] 1.1 Canonicalise temporal values on the SET write path in `engine/write_exec/properties.rs` before they are persisted, covering `SET n.p = <expr>`, `SET n = {map}` (whole-entity replace) and `SET n += {map}`, plus the MERGE `ON CREATE` / `ON MATCH` variants
  - **The proposal's premise was wrong, and running its own example is what
    showed it.** It claimed
    `MATCH (n) SET n.d = duration({days: 1})` → stores
    `{"_nexus_temporal_type": …}`. It does not: the query is **rejected**, with
    `Unsupported expression type in SET clause`. The inconsistent on-disk
    representation it describes could not be produced through `SET` at all. The
    claim came from reading `properties.rs` for a missing canonicalise call
    rather than from executing the query.
  - The real defect is upstream and much wider: `Engine::evaluate_set_expression`
    (`engine/match_exec.rs`) is a **parallel, strictly smaller
    re-implementation** of expression evaluation — literals, property reads,
    `Variable`, arithmetic, unary, map literal, `$param`, and a catch-all
    `Err("Unsupported expression type in SET clause")`. So EVERY function call on
    a `SET` RHS was rejected: `duration(…)`, `date(…)`, `toUpper(n.name)`,
    `size(…)`. Probed all forms before changing anything: `SET n.p = duration(…)`,
    `SET n += {d: duration(…)}` and `SET n.p = date(…)` all errored; only a plain
    literal worked.
  - Fixed by delegating instead of extending the parallel evaluator: the
    catch-all arm now calls the executor's row-aware evaluator through the shared
    storage boundary. The row it evaluates against carries what `SET` knows — the
    target variable bound to its own current properties (so `toUpper(n.name)`
    resolves) plus any `UNWIND` row bindings.
  - Canonicalisation is applied at the **exit** of `evaluate_set_expression`, not
    per arm. Every SET form funnels through that one function on its way to
    storage — `n.p = <expr>`, `n = {map}`, `n += {map}`, the relationship
    variants, and MERGE's `ON CREATE`/`ON MATCH` — so one call covers them all,
    reaches inside a map (canonicalisation recurses), and a future arm cannot
    silently skip it.
  - Verified against the store, not through a `RETURN`: a `duration({days: 1})`
    written by `CREATE`, by `SET n.p =`, and inside `SET n +=` are all
    `"P1D"` on disk, and `date('2020-01-01')` written by `SET` is
    `"2020-01-01"`.
- [x] 1.2 Consolidate canonicalisation into a single property-write choke point every write path converges on (replacing the manual boundary points enumerated in temporal_value.rs:604-632) — or, if they must stay separate, document that reason in place of the consolidation
  - New `Executor::resolve_persisted_property_value` (evaluate → canonicalise) is
    that choke point. `CREATE` reaches it through
    `resolve_property_expr_for_create`, which now only adds its own
    property-validity check on top; `SET`/`MERGE`-`ON …` reaches it through the
    delegating arm above.
  - **Both kinds of boundary could NOT be consolidated, and the distinction is
    now documented rather than left implicit.** The three points the old doc
    enumerated are two different things: one *storage* boundary (consolidated,
    above) and two *output* boundaries — the executor's projection boundary and
    the write path's own inline `RETURN` builder — which build a `ResultSet`
    without ever meeting, so there is no single funnel an outgoing value is
    guaranteed to cross. The doc comment on `canonicalize_value_in_place` was
    rewritten to say exactly that, and to drop its now-false remark that `SET`
    "could still carry a stale tag".
  - Property-value validity is deliberately NOT enforced in the shared helper:
    `SET n += {…}` legitimately evaluates to a MAP that is consumed key-by-key
    rather than stored, so the caller decides. `CREATE` keeps its
    primitive-or-array-of-primitives check.
- [x] 1.3 Round-trip test reading through the property store (not only through the projection boundary, which masks the stale tag): a value written by CREATE and the same value written by SET have identical on-disk representation, and a value stored in the legacy raw tagged form still reads back correctly
  - Both halves covered, in `engine/tests/write.rs` (in-crate, so `storage` is
    reachable): the CREATE-vs-SET on-disk comparison reads
    `storage.load_node_properties` directly, and the legacy test hand-writes the
    tagged shape into storage and asserts a `MATCH` still renders `"P1D"`.

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation
  - `docs/specs/cypher-subset.md` § SET: function calls (including temporal
    constructors) now evaluate on the RHS and inside a `+=` map, with an example;
    plus a paragraph stating that a temporal property persists in one canonical
    representation whichever clause wrote it, that this is what allows an index or
    constraint over a temporal column, and that reads stay tolerant of the legacy
    tagged form.
  - `CHANGELOG.md`: both halves — function calls on the `SET` RHS (the class fix)
    and the single on-disk representation, with why it matters beyond cosmetics
    and the explicit note that no migration is needed.
  - The stale doc comment on `canonicalize_value_in_place` corrected (see 1.2).
- [x] 2.2 Write tests covering the new behavior
  - 3 tests: the CREATE-vs-SET on-disk identity (covering `SET n.p =`, `SET +=`
    and a `date()`), the legacy-tagged-value read tolerance, and
    `SET n.upper = toUpper(n.name)` — the last one pinning that the fix is the
    whole function-call class, not a temporal special case, so a later narrowing
    fails loudly.
- [x] 2.3 Run tests and confirm they pass
  - `engine::tests::write` 22, `--test cypher` 655, `--test executor` 272,
    `--test regression` 227, `--test compatibility` 245 — all 0 failures.
    Workspace `--no-fail-fast`: **5795 passed, 3 failed**, the 3 being the
    documented flakes. `fmt` and `clippy --workspace --all-targets --all-features
    -D warnings` clean.
  - **TCK: +2 attributable, zero regressions.** Total 1827 → 1831.
    `expressions/list` 94 → 96, and the two newly passing scenarios name the
    reason exactly: `List6 [2] Setting and returning the size of a list property`
    and `List9 [1] Returning nested expressions based on list property` — both
    `SET` with a function call, i.e. the class this change unblocked.
  - Attribution done by diffing EVERY failing scenario across the two runs, not
    just the categories whose totals moved (a category can hide a +1/−1 pair).
    Four scenarios fixed, one newly failing. The other two fixes are documented
    noise: `clauses/merge` `Merge5 [4]` is the recorded run-to-run coin-flip, and
    `clauses/with-orderBy` `WithOrderBy2 [23]` is in the category with the
    recorded 89↔92 band. Neither is claimed.
  - The single new failure is `clauses/return [9] Returning a projected map`,
    already root-caused in `phase21_tck-consecutive-relationship-match-clauses` as
    map-literal key-order nondeterminism (`Expression::Map` is a `HashMap`, so the
    unaliased column name renders in arbitrary order). Its query is
    `RETURN {a: 1, b: 'foo'}` — no `SET`, no `MATCH` — so this change cannot reach
    it. A root cause beats an A/B band here.
  - Out of scope, confirmed still failing: `WITH` between `MATCH` and `SET`
    (`MATCH (n) WITH n, duration({days: 1}) AS d SET n.x = d`) errors with
    `Unsupported clause in write query`. That is the write path's own
    clause-composition gap, unrelated to expression evaluation.
