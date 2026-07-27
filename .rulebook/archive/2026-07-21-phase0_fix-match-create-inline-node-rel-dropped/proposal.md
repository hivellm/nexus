# Proposal: phase0_fix-match-create-inline-node-rel-dropped

## Why

`MATCH … CREATE (bound)-[:T]->(new:Label {…})` silently loses the relationship:
the inline-created target node is persisted, but the relationship record is
never written, and the query still reports success. This is silent data loss on
one of the most common write shapes (attach a new node to an existing one in a
single statement). Confirmed by reproduction on 2026-07-21 during development of
`tests/executor/write_refresh_visibility_test.rs`, and verified NOT to be an
executor-staleness artifact: the relationship stays missing even after a manual
`engine.refresh_executor()`. Both the top-level dispatch and the
`PROFILE`/CALL-subquery internal dispatch are affected identically.

Reproduction:

```cypher
CREATE (a:X {k: 1})
MATCH (a:X {k: 1}) CREATE (a)-[:CR]->(x:Y {k: 2})
MATCH (n:Y) RETURN count(n)               -- 1 (node persisted)
MATCH (a:X)-[r:CR]->(x:Y) RETURN count(r) -- 0 (relationship LOST — expected 1)
```

Adjacent shapes work correctly (already pinned green by the visibility test
suite): node-only CREATE from a MATCH context, and relationship CREATE where
both endpoints pre-exist and are matched. Distinct from the sibling backlog
bugs `phase0_fix-merge-relationship-dropped` (variable-less relationship MERGE)
and `phase0_fix-relationship-write-clauses-dropped` (MERGE SET filtering /
DELETE r): this one is specific to the MATCH…CREATE path when the relationship
target node is created inline in the same CREATE clause.

## What Changes

- Root-cause `execute_match_create_query` (engine `query_pipeline.rs` dispatch →
  match/create execution) for the inline-new-node relationship shape and fix it
  so the relationship record is written with the freshly created node id as its
  endpoint.
- Cover the mirrored direction (`(new)<-[:T]-(bound)`) and mixed bound/inline
  multi-hop patterns — or raise an explicit error for genuinely unsupported
  shapes; silent success is never acceptable.
- Extend `tests/executor/write_refresh_visibility_test.rs` (its MATCH…CREATE
  scenarios were deliberately narrowed to avoid this bug) or add a dedicated
  test module asserting node AND relationship visibility for the inline shape
  on both dispatch paths.

## Impact

- Affected specs: `docs/specs/cypher-subset.md` (CREATE clause semantics)
- Affected code: `crates/nexus-core/src/engine/query_pipeline.rs`,
  `crates/nexus-core/src/engine/match_exec.rs` (or wherever
  `execute_match_create_query` delegates), `crates/nexus-core/tests/executor/`
- Breaking change: NO — strictly fixes silent data loss; queries that worked
  keep working, queries that silently lost data start persisting it
- User benefit: `MATCH … CREATE` with an inline target node stops silently
  dropping relationships — data written is data stored, or an explicit error
