## 1. Implementation
- [x] 1.1 Filter null-valued keys out of the inline-property map before it reaches `store_properties` / `store_properties_if_any` on the CREATE path, and derive `inline_prop_count` from the filtered map instead of its own private `is_null()` filter (both sites: create.rs:130 and create.rs:319)
  - Confirmed by probe before changing anything: `CREATE (n:P {id: 12, name: null})`
    reported `+properties 1` while `keys(n)` returned `["id","name"]`, and the
    relationship form `CREATE (:P)-[:R {w: 1, tag: null}]->(:P)` had the same
    defect (`keys(r)` → `["tag","w"]`).
  - New `strip_null_valued_keys` applied at the top of both write paths, so the
    filtered map is what everything downstream sees; `inline_prop_count` is now
    `map.len()` on that map instead of a second, private `is_null()` filter. The
    count and the stored bytes are derived from one value and cannot disagree
    again — which is the whole shape of the original bug.
  - Top-level keys only, stated in the helper's doc: a `null` ELEMENT inside a
    list-valued property is a different question this rule does not speak to.
- [x] 1.2 Apply the same rule to the update paths: `SET n.p = null` removes an existing key (`-properties 1`) and is a no-op on an absent one; `SET n = {map}` / `SET n += {map}` / MERGE `ON CREATE` + `ON MATCH` never store a null-valued key
  - **Audited, and every update path was ALREADY conformant** — contrary to the
    proposal's "those paths need auditing too, not just CREATE", which implied they
    were broken. Probed all six: `SET n.p = null` on an existing key removes it
    with `-properties 1`; on an absent key it is a 0/0 no-op; `SET n += {p: null}`
    removes; whole-entity replace stores no null key; `MERGE … ON CREATE SET
    n.p = null` stores nothing (its `+properties 1` is the merge pattern's own
    `id`, not the null). No code change was warranted, so none was made — they are
    pinned by tests instead.
  - **One suspicion checked rather than "fixed":** whole-entity replace reported
    `+properties 1 / -properties 2` for `{id: 1, name: 'x'}` → `{id: 2, name:
    null}`, which looked like an over-count. openCypher TCK
    `clauses/set/Set4.feature` [2] settles it — the rule is not a diff: EVERY
    pre-existing key counts as removed and every new non-null key as set. Nexus's
    numbers are exactly conformant. Had I "corrected" them I would have broken a
    passing scenario.
  - `MERGE (n {p: null})` is rejected ("Cannot merge node using null property
    value"), matching Neo4j: a null in a MERGE pattern makes the pattern
    unsatisfiable rather than absent. Pinned, so the null-is-absent rule is not
    over-applied to MERGE's matching half.
- [x] 1.3 Regression tests that assert observable graph state, not just the counter — `keys(n)`, `properties(n)`, `n.p IS NULL` — alongside the `+properties` / `-properties` values, for every path touched in 1.1 and 1.2
  - 10 tests in `tests/cypher/null_property_write_semantics_test.rs`, each
    asserting `keys(...)` (read back after `refresh_executor`, so it sees the
    committed graph) AND the counter. One test asserts the pair that makes the
    distinction visible: `n.name IS NULL` is `true` either way, while
    `size(keys(n))` is what proves the key was never stored — a counter-only
    assertion is precisely what let the original divergence through.

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation
  - `docs/specs/cypher-subset.md` § CREATE: a property written with `null` is
    absent, with the worked `keys(n)` / `+properties` example; that reading it back
    is indistinguishable from a stored null so `keys()`/`properties()` are the
    observable difference; the update-path rules; the note that records written by
    earlier versions may still carry a null-valued key and that reads tolerate
    them; and the `MERGE` carve-out.
  - `CHANGELOG.md`: a Fixed entry describing the counter-vs-storage disagreement,
    that relationships had it too, and that no migration is performed.
- [x] 2.2 Write tests covering the new behavior
  - The 10 tests above (4 for the CREATE paths including the all-null case and the
    relationship form, 6 pinning the already-conformant update paths and the
    `MERGE` rejection).
- [x] 2.3 Run tests and confirm they pass
  - `--test cypher` 665, `--test executor` 272, `--test regression` 227,
    `--test compatibility` 245 — 0 failures. Workspace `--no-fail-fast`:
    **5805 passed, 3 failed**, the 3 being the documented flakes
    (`property_index_survives_restart`, the two `exists_` var-length tests).
    `fmt` and `clippy --workspace --all-targets --all-features -D warnings` clean.
  - **TCK: +1 attributable, zero regressions.** The newly passing scenario names
    the fix exactly: `expressions/graph [8] Using keys() and IN to check property
    existence` — `keys()` no longer reports the phantom null key.
  - Total reads 1831 → 1830 because two `clauses/with-orderBy` scenarios flipped
    to failing: `[15] Sort by an aliased aggregate projection…` and `[23] Sort by
    an expression that is only partially orderable…`. That is the documented
    oscillator, and the evidence is direct rather than a band argument: `[23]` had
    flipped the OTHER way in the immediately preceding run (it appears as a fix in
    the previous task's diff), and this category measured 89↔91 across two runs of
    one identical build earlier the same day. Nothing in this change touches
    `ORDER BY`.
  - Attribution again done by diffing every failing scenario across runs, not the
    per-category totals, so a hidden +1/−1 pair inside a category could not slip
    past.
