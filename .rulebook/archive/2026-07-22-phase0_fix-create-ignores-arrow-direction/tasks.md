# Tasks: phase0_fix-create-ignores-arrow-direction

CREATE ignores `<-`/`->`: elements are always written source->target in array
order (`RelationshipDirection` never read in `executor/operators/create.rs`).
`CREATE (x)<-[:T]-(a)` stores the edge reversed. Details in proposal.md.

## 1. Root cause and fix
- [x] 1.1 Failing tests first: forward and reversed arrow forms must store
      identical edges (assert via direction-sensitive MATCH from both
      endpoints), on BOTH create paths (standalone CREATE and MATCH...CREATE);
      confirm the reversed form fails today — DONE (test_create_arrow_direction.rs).
- [x] 1.2 Fix src/dst selection to honour the parsed direction in both create
      paths; decide the undirected `-[:T]-` CREATE semantics (prefer explicit
      error, Neo4j parity) and pin it with a test — DONE. `Incoming` (`<-`)
      swaps src/dst in all three CREATE write paths in
      executor/operators/create.rs; undirected `-[:T]-` is rejected with an
      explicit error before any node side effect (Neo4j parity), pinned by
      two rejection tests.
- [x] 1.3 Audit MERGE relationship writing and any other edge-writing path for
      the same array-order assumption; fix here or file follow-ups — DONE.
      MERGE (engine/write_exec.rs) now honours direction too (swaps on
      `Incoming`, keeps `Both` = outgoing per openCypher TCK Merge5);
      covered by merge_relationship_arrow_direction_test.rs.

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation — CHANGELOG
      entry added under [3.0.0]; fix sites carry explanatory comments citing
      Neo4j/openCypher-TCK semantics.
- [x] 2.2 Write tests covering the new behavior — test_create_arrow_direction.rs
      (5 tests: reversed arrow, forward/reversed equivalence, MATCH...CREATE,
      mixed-direction multi-hop, undirected rejection x2) +
      merge_relationship_arrow_direction_test.rs.
- [x] 2.3 Run tests and confirm they pass — full nexus-core suite green (3981
      passed, 0 failed); clippy + fmt clean.
