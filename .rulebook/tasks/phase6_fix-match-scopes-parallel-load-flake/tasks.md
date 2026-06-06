## 1. Reproduce & confirm root cause
- [ ] 1.1 Reliably reproduce the failure (full multi-binary `cargo +nightly test -p nexus-core`)
- [ ] 1.2 Instrument the scoped query to capture the actual plan/operators and returned count under load
- [ ] 1.3 Confirm or refute the mmap-page-residency-under-memory-pressure hypothesis (which read returns stale/zero)
- [ ] 1.4 Rule in/out a planner/operator edge for `(:Label {prop})-[:R]->()` count

## 2. Implementation
- [ ] 2.1 Fix the confirmed root cause on the hot read path (page fault-in / verified read / non-mmap integrity read, or planner fix)
- [ ] 2.2 Ensure no throughput regression on the read path
- [ ] 2.3 Make `match_scopes_by_label_and_property_together` deterministic without masking the defect

## 3. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 3.1 Update or create documentation covering the fix
- [ ] 3.2 Write a test that reproduces the load-sensitive failure deterministically and now passes
- [ ] 3.3 Run tests and confirm they pass (including full parallel `cargo +nightly test -p nexus-core`)
