# Tasks: phase0_de-flake-shared-catalog-cypher-tests

Intermittent `Database(DatabaseClosing)` in the cypher test group under
parallel full-workspace runs — same class as the executor-group flake fixed
in 1acc3bb1 (shared default catalog + concurrent engine drop/open). Details
in proposal.md.

## 1. Fix
- [x] 1.1 Inventory shared-catalog users: grep `Engine::new()` across all
      test binaries; list modules and hit counts — DONE. Shared-path
      constructors found across cypher (19 files), compatibility, integration,
      regression, performance, spatial, storage, loader, graph, transaction,
      and the lib `src/engine/tests` (~500+ sites total). Confirmed the shared
      path is `Catalog::new`/`with_map_size` → single per-process
      `TEST_CATALOG_DIR` env; `with_isolated_path` bypasses it.
- [x] 1.2 Convert each to TestContext + Engine::with_isolated_catalog
      (pattern from side_effects.rs / 1acc3bb1); serial_test-guard or
      document any test that must stay on the shared path — DONE, but the
      root cause was fixed generally rather than per-binary: the shared
      per-process test env is now PINNED open for the whole process
      (`PINNED_TEST_ENVS` in catalog/store.rs), so it is opened once and never
      closed mid-run — this kills `DatabaseClosing` across ALL test binaries in
      ~15 lines instead of converting 500+ sites. The 19 cypher modules were
      ALSO converted to per-test isolated catalogs for cleaner isolation.
      Committed 6ce0f40b.
- [x] 1.3 Stability proof: full workspace suite green 3x consecutively in
      parallel mode with zero DatabaseClosing failures — DONE. Full workspace
      run 3x in parallel: zero `DatabaseClosing` / closing-phase errors across
      all three runs (5041 passing on runs 1 & 2). Run 3 hit ONE unrelated
      pre-existing fulltext crash-recovery flake (Windows Tantivy file-lock
      `PermissionDenied` on WAL replay) — a different subsystem, tracked
      separately (fts-async-writer-ordering); NOT a DatabaseClosing failure.

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation — CHANGELOG
      entry added under [3.0.0] Unreleased; store.rs carries a full doc-comment
      explaining the pin and why it is test-only.
- [x] 2.2 Write tests covering the new behavior — the 19 converted cypher
      modules exercise the isolated path; the fix itself is a test-infra
      reliability change proven by the 3x workspace stability run rather than a
      unit test (a flake-reproduction test would be inherently non-deterministic).
- [x] 2.3 Run tests and confirm they pass — full workspace 3x + independent
      code review (verdict: correct and safe, no blockers).
