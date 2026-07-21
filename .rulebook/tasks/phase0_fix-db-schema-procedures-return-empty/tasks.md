# Tasks: phase0_fix-db-schema-procedures-return-empty

P1: `CALL db.labels()` / `db.relationshipTypes()` / `db.propertyKeys()` return
zero rows on a live `nexus-server` while the same catalog data resolves
correctly via `labels(n)` / `type(r)` on the same server. Full evidence and
suspect map in `proposal.md`. Key facts: reproduces on both a long-lived and a
freshly created database; does NOT reproduce in a standalone binary using the
identical `Catalog` API; harness-confirmed vs Neo4j (5 labels / 1 type vs 0).

## 1. Root cause (research-first — no fix until the divergence is located)
- [ ] 1.1 Reproduce minimally against a spawned server: seed one label + one
      typed relationship over HTTP `/cypher`, then call the three procedures
      over HTTP and over RPC; record which transports are affected
- [ ] 1.2 Trace which `Catalog` instance the procedure operator reads at call
      time (path + env identity) vs the engine's write-path catalog; check the
      executor clone installed by `refresh_executor` and the per-database
      routing (default `neo4j` served by `server.engine` vs manager map).
      Deliverable: "server does X at file:line, standalone does Y, difference
      causes empty iteration" — not a hypothesis
- [ ] 1.3 Determine regression window: `git log` on `db_schema.rs`, executor
      shared-state wiring, and server dispatch to find when the procedures
      last returned data on a live server (the serial sweep on 2026-07-13
      recorded non-zero latencies AND correct content for these scenarios —
      that run is the last known-good)

## 2. Fix
- [ ] 2.1 Fix the wiring so the procedures read the live catalog on every
      database and both transports; no behavior change to procedure output
      format
- [ ] 2.2 Verify the serial-benchmark schema-procedure scenarios pass the
      harness content check again (nexus content == neo4j content), then
      re-measure the <=1.2x latency gate that is currently blocked
      (phase9_store-lock-read-concurrency §4.1)

## 3. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 3.1 Update or create documentation covering the implementation
- [ ] 3.2 Write tests covering the new behavior (server-level integration:
      non-empty correct results for all three procedures after writes, on the
      default and a freshly created database)
- [ ] 3.3 Run tests and confirm they pass
