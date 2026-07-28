## 1. Implementation
- [x] 1.1 binary-tree-N fixture step — CREATE scripts reproduced verbatim from upstream `tck/graphs/binary-tree-{1,2}/binary-tree-{1,2}.cypher` at the pinned commit (`tck_common::{BINARY_TREE_1_CYPHER,BINARY_TREE_2_CYPHER,binary_tree_cypher}`); new `Given the binary-tree-(1|2) graph` step in tck_opencypher.rs seeds a fresh isolated engine, same as `having executed:`; skip-list entry removed
- [ ] 1.2 triadic selection support — NOT implemented: requires executor/MATCH changes (`crates/nexus-core/src/executor/**`), out of this task's scope (tests/** only, per team-lead scoping — a concurrent agent owns executor/eval). With the fixture now loading, real grounds show: 2/19 scenarios pass unaided, 17/19 fail on genuine MATCH+OPTIONAL MATCH semantics over the loaded graph (not a fixture problem) — a separate executor-track task is needed to close these

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation (OPENCYPHER_TCK_REPORT.md regenerated; VENDOR.md provenance already covers the corpus, fixture provenance documented inline in tck_common.rs)
- [x] 2.2 Write tests covering the new behavior (tests/tck_fixtures.rs: node/rel/label counts for both fixtures + `binary_tree_cypher` resolution)
- [x] 2.3 Run tests and confirm they pass (tck_fixtures.rs 3/3 pass; clippy/fmt clean; TCK skip-list 90 -> 71 (-19 binary-tree entries removed), useCases/triadicSelection 0/0/19 -> 2/17/0)
