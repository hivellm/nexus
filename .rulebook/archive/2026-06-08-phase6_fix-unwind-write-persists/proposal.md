# Proposal: phase6_fix-unwind-write-persists

Source: GitHub issue #13 (https://github.com/hivellm/nexus/issues/13)

## Why
A write that ranges over an `UNWIND` row list silently persists nothing:
the request returns HTTP 200 with `count = 0` and no error, but no nodes
are created/updated. Reads over `UNWIND` work; only the write
(MERGE/CREATE/SET) path inside `UNWIND` is dropped.

Reproduction (2.3.1, inlined literals, REST `/cypher`):
```
UNWIND [{id:"unw1",nm:"A"},{id:"unw2",nm:"B"}] AS row
  MERGE (n:ZZUnw {id: row.id}) SET n.name = row.nm
  RETURN count(n) AS c
-> 200, rows: [[0]]   (expected [[2]])

MATCH (n:ZZUnw) RETURN n.id, n.name
-> []                  (nothing persisted)
```
The same MERGE as a standalone statement persists correctly. Long-standing
(a Cortex writer workaround comment dates it to Nexus 1.15). Still present
in 2.3.1.

Impact: batched writes are impossible — every MERGE becomes its own
request, capping write throughput at ~1-2 writes/sec; bulk
backfill/ingestion is ~100x slower than it should be. The sustained
one-statement-per-write churn is the likely load source behind the #12
busy-loop stall.

## What Changes
- Make the write executor iterate `UNWIND` rows for the write clauses
  (MERGE / CREATE / SET, and by extension DELETE/REMOVE driven by a row
  variable) the same way the read path already does, binding the `UNWIND`
  row variable per iteration so `UNWIND [...] AS row MERGE (n:L {k:row.k})
  SET n.x = row.y` persists every row in a single statement/transaction.
- `RETURN count(n)` (and other aggregates/returns) must reflect the rows
  actually written.
- Investigation first: determine where the write-path dispatch
  (`execute_write_query` / `execute_cypher_dispatch`) handles `UNWIND` and
  why the row list is not driving the per-row write; define the correct
  binding + iteration contract and whether the whole batch runs in one
  transaction.

## Impact
- Affected specs: cypher-subset / UNWIND + write clauses, executor / write path
- Affected code: `crates/nexus-core/src/engine/` write-query path
  (UNWIND row binding for MERGE/CREATE/SET); possibly the executor UNWIND
  operator interaction with write clauses
- Breaking change: NO (currently a silent no-op; the fix makes it persist
  as documented — no API/response-format change)
- User benefit: one-request-per-batch writes and a fast,
  single-transaction backfill instead of ~1-2 writes/sec; removes the
  per-statement churn implicated in #12.

## Notes
- Companion to #12 (sustained-write busy-loop): fixing batched UNWIND
  writes removes the one-statement-per-write load pattern that appears to
  trigger #12.
