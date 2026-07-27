## 1. Implementation
- [x] 1.1 SET n = {map} replace parses + applies — SetItem::Replace variant; whole-entity replace with stale-key removal + null-value drop + correct +/-properties side-effect counts; RHS may be map literal, node/map variable (SET n = m), or map param
- [x] 1.2 SET (n).prop parses — parenthetical target unwrap in write.rs (also SET (n) = {map} / SET (n) += {map})

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation — docs/specs/cypher-subset.md § SET (replace/merge/paren + honest limitations) + CHANGELOG [3.0.0]
- [x] 2.2 Write tests covering the new behavior — 6 regression tests in tests/executor/side_effects.rs (Set4 [1]-[4] + variable-copy + parenthetical)
- [x] 2.3 Run tests and confirm they pass — executor group 251/0; full workspace green (only the known-flaky KNN HNSW-race test, passes isolated)
