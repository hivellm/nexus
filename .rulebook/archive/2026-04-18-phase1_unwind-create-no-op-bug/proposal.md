# Proposal: phase1_unwind-create-no-op-bug

## Why

`UNWIND range(0, 999) AS id CREATE (n:Item {id: id})` silently creates **zero**
nodes. The server logs `WARN CREATE operator: existing_rows is empty, skipping
CREATE. result_set.rows=1000, variables=[]` and returns `execution_time_ms=0`
with rows=1000 — client thinks it succeeded. Observed directly during the
memtest run for `fix/memory-leak-v1`. This breaks the most natural bulk-ingest
pattern in Cypher and silently produces empty databases.

## What Changes

- Trace the CREATE operator path when rows come from the result_set with an
  empty `variables` map (`nexus-core/src/executor/mod.rs` near line 609).
- The fix is almost certainly that the CREATE planner expected the UNWIND
  row to be materialised in `context.variables`, but UNWIND populates
  `context.result_set.rows` instead — the CREATE stage has to walk the
  result set rows, not the empty variables map.
- Replace the silent `WARN + return Ok` with either a correct pass over
  the result_set rows or, if that truly is an unsupported shape, a hard
  `Error::CypherExecution("CREATE after UNWIND not yet supported, …")` so
  the caller knows the query did nothing.

## Impact

- Affected specs: `nexus-core/src/executor/mod.rs`, `docs/specs/cypher-subset.md`
- Affected code:
  - `nexus-core/src/executor/mod.rs:609` (and the CREATE branch feeding it)
  - Cypher compatibility tests — some currently-passing tests may depend on
    the buggy behaviour; those need the fixtures rewritten
- Breaking change: YES for callers that silently depended on CREATE being a
  no-op (they were already broken)
- User benefit: bulk ingestion via UNWIND actually works — the standard
  Neo4j idiom works end-to-end
