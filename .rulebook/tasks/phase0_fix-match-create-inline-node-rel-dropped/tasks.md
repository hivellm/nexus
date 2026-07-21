# Tasks: phase0_fix-match-create-inline-node-rel-dropped

`MATCH … CREATE (bound)-[:T]->(new:Label {…})` silently drops the relationship:
the inline-created target node IS persisted and visible, but the relationship
record is never written. No error is raised — the query reports success.

**Confirmed repro (2026-07-21, discovered while writing
`tests/executor/write_refresh_visibility_test.rs`; verified NOT a
refresh-staleness artifact — the relationship stays missing even after a manual
`engine.refresh_executor()`):**

```cypher
CREATE (a:X {k: 1})
MATCH (a:X {k: 1}) CREATE (a)-[:CR]->(x:Y {k: 2})
MATCH (n:Y) RETURN count(n)              -- 1  (node persisted)
MATCH (a:X)-[r:CR]->(x:Y) RETURN count(r) -- 0  (relationship LOST — expected 1)
```

Affects both the top-level dispatch and the `PROFILE`/CALL-subquery internal
dispatch identically (`query_pipeline.rs` MATCH…CREATE arm →
`execute_match_create_query`). The failing shape is specifically: relationship
target node created INLINE in the same CREATE clause. Two adjacent shapes work
fine and are already pinned green by `write_refresh_visibility_test.rs`:
CREATE of a node only from a MATCH context, and CREATE of a relationship where
BOTH endpoints pre-exist and are matched.

Sibling-but-distinct known bugs (do not conflate):
`phase0_fix-merge-relationship-dropped` (variable-less MERGE relationship
bails to `Ok(None)` and merges only the first node) and
`phase0_fix-relationship-write-clauses-dropped` (MERGE SET filtering / DELETE r
collection). This task is the MATCH…CREATE inline-target-node path.

## 1. Implementation
- [ ] 1.1 Root-cause: trace `execute_match_create_query` for the inline-new-node
      relationship shape — establish exactly where the relationship write is
      skipped (target-node binding never entered into the rel-creation context?
      relationship arm unreached when the pattern element chain contains a
      fresh node?) with file:line evidence before changing code
- [ ] 1.2 Fix so the relationship record is written with the freshly created
      node id as its endpoint; the fix must also cover the mirrored direction
      (`(new)<-[:T]-(bound)`) and multi-hop patterns that mix bound and inline
      nodes, or explicitly error on unsupported shapes — never silent success
- [ ] 1.3 Un-split the deliberately narrowed regression tests: extend
      `tests/executor/write_refresh_visibility_test.rs` (tests 12/14/15 were
      restructured to avoid this bug) or add a dedicated test module asserting
      node AND relationship visibility for the inline shape, both dispatch
      paths (top-level and PROFILE/internal)

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 2.1 Update or create documentation covering the implementation
- [ ] 2.2 Write tests covering the new behavior
- [ ] 2.3 Run tests and confirm they pass
