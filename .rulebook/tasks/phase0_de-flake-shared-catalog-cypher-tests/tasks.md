# Tasks: phase0_de-flake-shared-catalog-cypher-tests

Intermittent `Database(DatabaseClosing)` in the cypher test group under
parallel full-workspace runs — same class as the executor-group flake fixed
in 1acc3bb1 (shared default catalog + concurrent engine drop/open). Details
in proposal.md.

## 1. Fix
- [ ] 1.1 Inventory shared-catalog users: grep `Engine::new()` across all
      test binaries; list modules and hit counts
- [ ] 1.2 Convert each to TestContext + Engine::with_isolated_catalog
      (pattern from side_effects.rs / 1acc3bb1); serial_test-guard or
      document any test that must stay on the shared path
- [ ] 1.3 Stability proof: full workspace suite green 3x consecutively in
      parallel mode with zero DatabaseClosing failures

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 2.1 Update or create documentation covering the implementation
- [ ] 2.2 Write tests covering the new behavior
- [ ] 2.3 Run tests and confirm they pass
