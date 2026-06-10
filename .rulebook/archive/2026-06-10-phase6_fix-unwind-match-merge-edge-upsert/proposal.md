# Proposal: phase6_fix-unwind-match-merge-edge-upsert

Source: GitHub issue #14 (https://github.com/hivellm/nexus/issues/14)

## Why
Following the #13 fix, `UNWIND [...] AS row MERGE (n:L {k:row.k}) SET ...`
(node upsert) persists every row. But the **edge** upsert shape — `UNWIND`
followed by `MATCH ... MERGE (a)-[r]->(b)` — is rejected:

```
UNWIND [{fk:"za",tk:"zb",w:5}] AS row
  MATCH (a:ZT {id:row.fk}), (b:ZT {id:row.tk})
  MERGE (a)-[r:ZREL]->(b) ON CREATE SET r.w=row.w ON MATCH SET r.w=row.w
  RETURN count(r) AS c
-> Execution error: "Unsupported clause after UNWIND in write query"
```

The post-UNWIND clause loop added in #13 (`execute_unwind_write_query`)
only allows MERGE/SET/REMOVE/FOREACH and explicitly rejects `MATCH` (and
WHERE/WITH/…) after the UNWIND. So clients can batch node writes but must
still issue one request per edge — on a graph with ~as many edges as
nodes, edge ingestion stays O(N) HTTP round-trips and is the backfill
bottleneck. The first-party Rust SDK serializes requests over one
transport, so per-edge writes can't be parallelized client-side either —
server-side UNWIND-edge support is the only path to fast edge ingestion.

## What Changes
- Allow a per-row `MATCH` (and the subsequent relationship `MERGE` +
  `ON CREATE`/`ON MATCH SET`) inside the post-UNWIND write loop in
  `execute_unwind_write_query`, so `UNWIND rows AS row
  MATCH (a:..{row.fk}),(b:..{row.tk}) MERGE (a)-[r:T]->(b)
  ON CREATE/ON MATCH SET ...` resolves the endpoints per row (binding
  `row.fk`/`row.tk` via the existing unwind_bindings lane) and upserts the
  edge for every row — the edge analogue of the #13 node fix.
- Per-row `MATCH` must run against a fresh per-row context (like the
  existing per-row write context) so each row's endpoint resolution and
  edge MERGE/SET only touch that row's nodes; `RETURN count(r)` reflects
  every row.
- Investigation first: confirm `find_nodes_by_node_pattern` /
  `process_match_clause_multi` resolve `{id: row.fk}` against the unwind
  binding, and that relationship MERGE (`process_merge_relationship`) +
  `ON CREATE`/`ON MATCH SET` apply per row in the UNWIND loop.

## Impact
- Affected specs: cypher-subset / UNWIND + MATCH + relationship MERGE,
  executor / write path
- Affected code: `crates/nexus-core/src/engine/mod.rs`
  (`execute_unwind_write_query` post-UNWIND clause handling — allow MATCH;
  per-row relationship MERGE + ON CREATE/ON MATCH SET); possibly
  `process_merge_relationship` to apply ON CREATE/ON MATCH SET on rels
- Breaking change: NO (currently errors; the fix makes a valid pattern
  persist — no API/response-format change)
- User benefit: one-request-per-batch **edge** upserts (not one per edge);
  fast single-pass backfill of both nodes and edges.

## Notes
- Direct follow-up to #13 (`phase6_fix-unwind-write-persists`): same
  UNWIND-write path, extended from node-only to the MATCH+edge-MERGE shape.
- Watch the relationship-MERGE `ON CREATE`/`ON MATCH SET` path: the #13
  node loop handled SET on nodes; relationship MERGE SET may need the same
  per-row binding treatment.
