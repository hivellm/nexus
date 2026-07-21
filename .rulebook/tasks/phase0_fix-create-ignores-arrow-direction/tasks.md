# Tasks: phase0_fix-create-ignores-arrow-direction

CREATE ignores `<-`/`->`: elements are always written source->target in array
order (`RelationshipDirection` never read in `executor/operators/create.rs`).
`CREATE (x)<-[:T]-(a)` stores the edge reversed. Details in proposal.md.

## 1. Root cause and fix
- [ ] 1.1 Failing tests first: forward and reversed arrow forms must store
      identical edges (assert via direction-sensitive MATCH from both
      endpoints), on BOTH create paths (standalone CREATE and MATCH...CREATE);
      confirm the reversed form fails today
- [ ] 1.2 Fix src/dst selection to honour the parsed direction in both create
      paths; decide the undirected `-[:T]-` CREATE semantics (prefer explicit
      error, Neo4j parity) and pin it with a test
- [ ] 1.3 Audit MERGE relationship writing (process_merge_relationship) and
      any other edge-writing path for the same array-order assumption; fix
      here or file follow-ups with file:line evidence

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 2.1 Update or create documentation covering the implementation
- [ ] 2.2 Write tests covering the new behavior
- [ ] 2.3 Run tests and confirm they pass
