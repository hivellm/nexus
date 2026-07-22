# Proposal: phase0_de-flake-shared-catalog-cypher-tests

## Why

Several test modules in the `cypher` grouped harness still create engines on
the process-shared default catalog path, producing intermittent
`Database(DatabaseClosing)` failures under full-workspace parallel execution
(one engine's drop closes the shared LMDB env while another test's
`Engine::new()` is mid-open). Observed repeatedly on 2026-07-21/22: the
failing test varies run to run (`in_operator_tests`,
`cypher_non_ascii_test`, `cypher_groupby_expression_key_test`,
`test_substring_negative_index_large`) while the error signature stays
identical; each passes standalone and the whole cypher group passes 362/362
serially. The identical flake in the `executor` group was already root-caused
and fixed by converting `query_analysis_test` to isolated per-test engines
(commit 1acc3bb1) — the remaining shared-catalog users in the cypher group
need the same conversion. The flake blocks pre-push (full suite is the
pre-push hook) intermittently.

## What Changes

- Inventory every module in `crates/nexus-core/tests/cypher/` (and any other
  test binary) still calling `Engine::new()` / sharing the default catalog
  path; convert each to the `TestContext` + `Engine::with_isolated_catalog`
  per-test pattern (as in `side_effects.rs` and commit 1acc3bb1).
- Re-run the full workspace suite several times in parallel mode to confirm
  the DatabaseClosing signature is gone.
- If any test genuinely REQUIRES the shared default path (e.g. pins
  process-wide catalog behavior), isolate it behind `serial_test` or document
  why it must stay shared.

## Impact

- Affected specs: none (test infrastructure)
- Affected code: `crates/nexus-core/tests/cypher/*` (test-only)
- Breaking change: NO
- User benefit: deterministic CI/pre-push; no more spurious
  DatabaseClosing failures masking real regressions
