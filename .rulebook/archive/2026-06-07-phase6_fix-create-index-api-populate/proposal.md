# Proposal: phase6_fix-create-index-api-populate

Source: GitHub issue #9 (https://github.com/hivellm/nexus/issues/9)

## Why
`CREATE INDEX FOR (n:Label) ON (n.prop)` issued through the executor /
REST / RPC path does NOT register the index in the typed `property_index`
that `has_index` / `find_exact` consult, and does not backfill existing
nodes. The executor handler `execute_create_index` (`crates/nexus-core/
src/executor/operators/admin.rs`) only interns the label/key in the
catalog for the `None | Some("property")` branch — no
`property_index.create_index`, no populate.

As a result, on the server (multi-database) path every consumer of that
index is silently defeated:
- read `NodeIndexSeek` (#8) — `has_index` is false, so `MATCH (n:Label
  {prop:val})` falls back to a full label scan and emits
  `Nexus.Performance.UnindexedPropertyAccess`.
- index-backed node MERGE existence (`engine/crud.rs` `has_index` guard)
  — falls back to the O(N) label scan, so the edge-upsert write-burst
  meltdown persists.

Reproduced empirically against the release server: `CREATE INDEX FOR
(n:Turn) ON (n.id)` returns the executor's single-column
`["Turn.id.property"]` shape; a following `MATCH (n:Turn {id:"T1"})`
still emits `UnindexedPropertyAccess`. The engine handler
(`engine/mod.rs` `execute_index_commands`, used by the standalone
single-engine path) is correct — it calls `property_index.create_index`
+ `populate_index` — but the server routes CREATE INDEX through the
executor handler instead, so the bug only surfaces via the API.

## What Changes
- Make `execute_create_index`'s `None | Some("property")` branch register
  the typed property index (`property_index.create_index(label_id,
  key_id)`) and backfill existing nodes that carry the property,
  mirroring `engine/mod.rs` `populate_index`. The executor already holds
  the Arc-shared `PropertyIndex` handle (threaded in #8), plus the label
  index and store, so it can register + populate directly.
- Handle `IF NOT EXISTS` / `OR REPLACE` correctly for property indexes
  (check `property_index.has_index`, not the spatial R-tree registry).
- Skip null/array/object values on backfill (null-key contract — null is
  never indexed); String/Integer/Float/Boolean only, matching the
  write-side `json_to_property_value` normalization so seeks never miss.

## Impact
- Affected specs: cypher-subset / index DDL, executor / admin
- Affected code: `crates/nexus-core/src/executor/operators/admin.rs`
  (`execute_create_index`)
- Breaking change: NO. Response shape unchanged; only the side effect
  (index registration + backfill) is added. Results unchanged; reads and
  MERGE existence become O(log N).
- User benefit: API-created indexes actually work — unblocks both the #8
  read seek and index-backed MERGE / edge-upsert throughput.

## Notes
- Companion to #8 (`phase6_fix-read-match-index-seek`) and the
  index-backed MERGE fix; this is the missing population step that both
  depend on when the index is created via the API.
