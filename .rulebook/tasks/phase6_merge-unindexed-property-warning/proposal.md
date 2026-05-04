# Proposal: phase6_merge-unindexed-property-warning

## Why

A live Cortex deployment (`hivehub/nexus:2.1.0`) saturated 100% CPU and stopped responding to `/stats` and `MATCH ... count(*)` (5–10s timeouts) while the ingestion pipeline ran a steady stream of `MERGE (n:Artifact { natural_key: "..." }) SET ...`. `CALL db.indexes()` returned only label-lookup indexes — no property index existed on `Artifact.natural_key` or `Artifact.path`, so every `MERGE` degraded to a full label scan + property comparison, and every `WHERE a.path CONTAINS $q OR a.natural_key CONTAINS $q` did the same scan again.

Two cumulative facts make this a recurring foot-gun, not an operator mistake:

1. The Cypher planner currently accepts `MERGE (n:Label { prop: $v })` against an unindexed property without any signal — neither a runtime warning, a planner notice in the response envelope, nor a startup log line. Operators only discover the missing index after the database is already wedged.
2. The same pathology applies to `MATCH (n:Label { prop: $v })` and to equality predicates in `WHERE`. The planner already knows the label and the property selector at parse time, and it already enumerates indexes when it builds the plan — emitting a notice is a small additional step on a path that has all the information it needs.

Without this signal, every fresh deployment that ingests at scale will re-discover the same outage shape.

## What Changes

- The Cypher planner emits a structured `Notification` (Neo4j `INFORMATION` severity, `Performance` category) whenever a `MERGE` or `MATCH` selects nodes by `(label, property)` and no `BTREE`/`RANGE`/`TEXT` index covers that pair.
- The notification is included in the existing `/cypher` response envelope under a new `notifications: [...]` field (Neo4j-compatible shape: `code`, `title`, `description`, `severity`, `position`).
- A planner-level rate-limited WARN log (one entry per `(label, property)` pair per `info` window, default 60s) so the same pathology surfaces in `docker logs` when the response envelope is not inspected (e.g. fire-and-forget ingestion).
- A short section in `docs/performance/PERFORMANCE.md` documenting the recommended indexes for the common high-volume MERGE patterns (`Artifact.natural_key`, `Artifact.path`, `Turn.id`, `ToolCall.id`) and showing the `CREATE INDEX` syntax that Nexus accepts today.

Out of scope:
- Auto-creating indexes (deferred — single-writer model means index build can stall ingestion; needs its own design).
- Full-text recommendation for `CONTAINS` (separate task).

## Impact

- Affected specs: `crates/nexus-core/src/executor/planner` (notification emission), `crates/nexus-server/src/api/cypher/execute.rs` (envelope field).
- Affected code:
  - `crates/nexus-core/src/executor/planner/*` — notification hook on plan-build path.
  - `crates/nexus-core/src/executor/result.rs` (or equivalent) — `Notification` struct + accumulator on `QueryResult`.
  - `crates/nexus-server/src/api/cypher/execute.rs` — serialize `notifications` into the JSON envelope.
  - `docs/performance/PERFORMANCE.md` — recommended-index section.
- Breaking change: NO. New optional field in response envelope; SDKs that ignore unknown fields are unaffected. Neo4j-compatible field name (`notifications`) so first-party SDKs that already model Neo4j responses can surface it without further work.
- User benefit: ingest pipelines surface the missing-index pathology BEFORE the database wedges. Operators receive a one-line "create this index" hint per offending query pair, not a postmortem.
