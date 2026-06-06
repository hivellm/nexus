## 1. Reproduce & confirm root cause
- [x] 1.1 Reliably reproduce the failure (full multi-binary `cargo +nightly test -p nexus-core`; failure rate tracks machine load)
- [x] 1.2 Instrument the scoped query: captured failing signature `count(:X)=4` for 2 nodes, `X.id_values=[0,0,1,1]`, scoped=5 — the label scan yields each `:X` node TWICE under load
- [x] 1.3 Ruled out in-process race: 16-thread x 50-iter single-process stress = 0 failures; small-dataset count path is scalar (par-count only >1000 rows)
- [x] 1.4 Ruled out double-CREATE: edge sanity = 8 (correct), so records are fine — the duplication is in label-scan resolution, not the data
- [ ] 1.5 Pin the exact double-yield source: duplicate node RECORDS vs a duplicated label-id in `get_nodes_with_labels` resolution (internal-id DIAG inconclusive so far)

## 2. Implementation
- [x] 2.1 Fixed a real cross-binary test-isolation bug found en route: the test catalog (`catalog/mod.rs`) and auth storage (`auth/storage.rs`) used a FIXED shared temp dir across all `cargo test` binaries (separate processes), wiping/concurrently writing the same LMDB env. Now process-scoped (`..._<pid>`), keeping one LMDB env per process (still avoids Windows TlsFull) while isolating concurrent test binaries. NOTE: this did NOT by itself stop the flake (still reproduces), so the catalog sharing was not the sole cause.
- [ ] 2.2 Fix the confirmed root cause (label-scan double-yield under load) once 1.5 pins it
- [ ] 2.3 Make `match_scopes_by_label_and_property_together` pass deterministically under full parallel `cargo test -p nexus-core`

## 3. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 3.1 Update or create documentation covering the fix
- [ ] 3.2 Write a test that reproduces the load-sensitive failure deterministically and now passes
- [ ] 3.3 Run tests and confirm they pass (including full parallel `cargo +nightly test -p nexus-core`)

## Notes (investigation log)
- Symptom: only under full multi-binary parallel run; passes serially / `--lib`-only / isolated. Load-dependent and observer-sensitive (instrumentation shifts it).
- Confirmed: `MATCH (n:X)` returns 4 rows for 2 nodes (`[0,0,1,1]`) under load; edge-count sanity stays 8. So the scan double-yields; the underlying records and the property filter are correct.
- Next: add DIAG for internal `id(n)` of `:X` and total node count on a forced-failure run to distinguish 4 records vs 2 records yielded twice; then inspect `LabelIndex::get_nodes_with_labels` (index/mod.rs:259-285) and the NodeByLabel scan for a duplicated label-id path.
