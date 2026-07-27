# Proposal: phase0_fix-materialize-recrosses-aligned-columns

**Priority: CRITICAL — unbounded memory / host freeze.** A multi-pattern
`MATCH (a), (b)` (comma-separated patterns, e.g.
`UNWIND $rows AS r MATCH (a:P {id: r.s}), (b:P {id: r.d})`) materialises a
**cubic** intermediate — `N^k` rows for `k` patterns of aligned length `N` —
because `materialize_rows_from_variables` RE-crosses columns that
`apply_cartesian_product` has ALREADY aligned. The OOM budget guard on
`apply_cartesian_product` does NOT cover this downstream step, so the process
allocates until it exhausts RAM and freezes the host.

Discovered while running the (uncommitted) stronger OOM verification test
`crates/nexus-core/tests/oom_budget_verification_test.rs`: with `NODES = 8`,
`ROWS = 6` the aligned product is 384 rows (guard passes it), then `materialize`
re-crosses to `384^3 ~= 56.6M` rows — a ~13 GB allocation that froze a 128 GB
workstation. The detonating test (`raising_budget_lets_the_same_query_return_exact_rows`)
is currently `#[ignore]`d with a pointer here; the fix must re-enable it.

## Why

Trace of `UNWIND $rows AS r MATCH (a:P {id: r.s}), (b:P {id: r.d}) RETURN a.id, b.id`
(no index on `:P(id)`, plan `[Unwind, NodeByLabel(a), NodeByLabel(b), Filter, Filter, Project]`):

1. **`NodeByLabel(a)`** — `dispatch.rs::seed_scan_main_loop` "existing rows, no
   variables yet" branch cross-joins `ROWS x NODES = 48` and binds `r`, `a` as
   aligned length-48 arrays (`handled_cross_product = true`, so `materialize`
   is NOT called here).
2. **`NodeByLabel(b)`** — `context.variables` now non-empty, so it routes
   through `apply_cartesian_product` (`eval/helpers.rs:56`). That correctly
   aligns `r`, `a`, `b` to length `48 x 8 = 384` and its OOM budget guard checks
   exactly that 384-row product — and passes it under any reasonable budget.
   `handled_cross_product` stays `false`.
3. **`materialize_rows_from_variables`** (`eval/helpers.rs:177`) then runs
   because `handled_cross_product == false`. It sees three arrays of the SAME
   length (`r=384, a=384, b=384`), all multi-element, and sets
   `needs_cartesian_product = true` (`:261`). It then builds the FULL cartesian
   product of the three arrays — `384 x 384 x 384 = 56.6M` `HashMap` rows
   (`:263-305`) — **with no budget check**. This is the ~13 GB allocation.
4. Only AFTER that do the two `Filter`s (and row dedup) collapse the result back
   to the 6 correct rows. The over-production is invisible in the final result
   but catastrophic in peak memory.

### Root cause

`apply_cartesian_product` and `materialize_rows_from_variables` BOTH cross the
same columns. Once `apply_cartesian_product` has aligned the variables (each
index `i` is one output row), `materialize` must **zip** them index-aligned, not
re-cross them. Its `needs_cartesian_product` heuristic
(`has_multiple_arrays && all_multi_element && all_same_len`) cannot tell
"independent scan results that still need crossing" from "already-aligned
columns of one result" — same-length arrays satisfy it in both cases, so it
re-crosses the already-crossed columns.

This over-production is normally masked downstream by the row dedup
(`operators/filter.rs::compute_row_dedup_key`) that collapses the duplicated
combinations by `_nexus_id`, which is why comma-separated `MATCH` returns
correct results and existing tests pass — but the masked intermediate is `N^k`
and blows up before any filter/dedup can run.

## What Changes

- Reproduce the cubic blowup with a SAFE, tiny fixture (small enough that the
  wrong `N^k` intermediate is affordable but assert-detectably larger than the
  correct `N` — e.g. peak row count or a counter — so the test proves the fix
  without risking the host).
- Make `materialize_rows_from_variables` distinguish already-aligned columns
  (produced by `apply_cartesian_product` / the seed cross-join) from independent
  arrays that still need crossing, and ZIP the former instead of re-crossing.
  Alternatively, stop calling `materialize` after `apply_cartesian_product` has
  already aligned and can update the result set directly. Pin the correct
  approach before changing code.
- Re-enable and green the `raising_budget_lets_the_same_query_return_exact_rows`
  test (remove its `#[ignore]`) and confirm the peak intermediate is `O(N)`, not
  `O(N^k)`.

## Impact
- Affected specs: executor cross-product / materialisation semantics
- Affected code: `crates/nexus-core/src/executor/eval/helpers.rs`
  (`materialize_rows_from_variables`, `apply_cartesian_product`),
  `crates/nexus-core/src/executor/dispatch.rs` (`seed_scan_main_loop`)
- Breaking change: NO — final results are already correct (dedup masks the
  over-production); this only removes the catastrophic peak-memory intermediate
- User benefit: multi-pattern `MATCH` over more than a handful of driving rows
  stops exhausting RAM / freezing the host; a k-pattern join costs `O(N)` peak
  instead of `O(N^k)`. Related: `phase0_fix-cypher-oom-process-abort` (added the
  guard this step bypasses), `phase0_fix-unindexed-correlated-match-drops-rows`
  (its dedup widening is what masks this over-production in the final result)
