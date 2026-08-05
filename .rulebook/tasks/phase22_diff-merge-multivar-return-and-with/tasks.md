## 1. Implementation
- [x] 1.1 Generalise `build_return_result_with_executor` to every context variable the RETURN references (one `(v)` pattern + one `id(v) IN [...]` conjunct each), replacing the arbitrary `context.keys().next()` pick
  - Variables are selected by walking the RETURN's expression trees and keeping the
    names the write context actually binds, in first-appearance order — so the
    generated query is stable rather than dependent on `HashMap` iteration order.
    Reused the existing structural walker (`semantic_validation::child_exprs`,
    widened to `pub(crate)`) instead of duplicating an AST walk.
  - `collect_expr_binders` looked like the tool for this and is NOT: it collects the
    names an expression BINDS (a comprehension's loop variable), not the ones it
    references. Checked before using it.
  - Any referenced variable bound to nothing collapses the result to zero rows,
    since the generated patterns are conjunctive.
- [x] 1.2 `build_return_result` delegates to 1.1 when the RETURN spans more than one variable instead of raising `Multiple different variables in RETURN not supported for write queries` — 15.08 returns one row `A | B`
  - Delegating rather than reconstructing was the deliberate choice: the write
    path's per-variable id lists are INDEPENDENT, not row-aligned columns
    (`project-multi-pattern-write-materialization`), so building rows there would
    have meant inventing a second row model. The executor already has one.
- [x] 1.3 Implement `Clause::With` in the write dispatcher as a scope cut over BARE variable projections (optional alias = rename); every other projected item keeps erroring, with a message naming what is supported instead of a blanket "unsupported clause" — 15.12 returns `cnt = 1`
  - `With` had been sitting in a catch-all beside `Unwind`/`Union`/`OrderBy`/
    `Limit`/`Skip`. It now rebuilds both `context` and `rel_context` from the
    projected variables, so a relationship binding survives a `WITH` too.
  - The same rejection was hit from an unrelated direction earlier
    (`MATCH … WITH n, duration(…) AS d SET n.d = d`), so this was never
    MERGE-specific — the dispatcher had no notion of a scope boundary at all.
- [x] 1.4 Verify the second `MERGE` after the `WITH` still MATCHES rather than duplicating (assert node count, not just the returned count — the differential case is named "verify single node" for that reason)
  - `cnt = 1` AND `MATCH (p:Product) RETURN count(p)` = 1. Both asserted.

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation
  - `docs/specs/cypher-subset.md` § write clauses: a write query may RETURN several
    variables and may carry a `WITH` between writes, with both worked examples; the
    multi-variable RETURN is answered by the executor so its row semantics are not a
    second model; `WITH` is a scope cut; and the bare-variable-only restriction is
    listed as a current limitation rather than left for a user to discover.
- [x] 2.2 Write tests covering the new behavior
  - 9 tests in `tests/cypher/merge_write_path_return_and_with_test.rs`: both
    differential shapes; three merged variables; the single-variable control that
    must keep using the fast path; `WITH` renaming; `WITH` cutting a variable it does
    not project; the rejection message for a projected expression; and the two
    `count(*)` cases below.
- [x] 2.3 Run tests and confirm they pass
  - `--test cypher` 714 (9 new), 0 failures.

## 3. Gates (every item, no exceptions)
- [x] 3.1 `cargo +nightly fmt --all` and `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean
- [x] 3.2 `cargo +nightly test --workspace --no-fail-fast` green (a plain `--workspace` run aborts at the first failing target)
  - **5848 passed / 3 failed** — the 3 documented flakes.
- [x] 3.3 Neo4j differential suite: 310/325, with 15.08 and 15.12 both passing and no case that passed before now failing
  - **310 passed / 0 failed / 15 skipped — 100% pass rate**, up from 308/2/15.
  - **The first cut of this change REGRESSED 15.05** (`MERGE (n…) RETURN count(*)`),
    and the gate's "no case that passed before now failing" clause is what caught
    it. Cause: selecting materialisation variables from the RETURN makes the list
    EMPTY for a RETURN that names no variable, and I raised an error there; the old
    arbitrary `keys().next()` had masked exactly that case. Fixed by falling back to
    every context variable, sorted for determinism, and pinned by two tests
    (`count(*)` over one and over two merged variables).
- [x] 3.4 TCK re-run: no category regressed against an identical re-run
  - Gained as well: `clauses/merge` 25 → 29, `clauses/create` 47 → 48, total
    2352 → 2356.
  - `clauses/with-orderBy` read 147 → 146, so it was A/B'd: an identical re-run of
    this build gives 146 and 147, the re-run landing exactly on the pre-change
    value. `clauses/return` oscillated 30↔29 between the same two runs. Both are the
    documented oscillators; totals identical at 2356 across the pair.
- [x] 3.5 Regenerate `docs/compatibility/OPENCYPHER_TCK_REPORT.md`
