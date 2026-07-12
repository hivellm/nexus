# Proposal: phase1_http-merge-rel-and-set-rel-parity

## Why

The HTTP `/cypher` write path (`crates/nexus-server/src/api/cypher/execute/write_ops.rs`)
is a **separate implementation** from the `nexus-core` engine write path
(`crates/nexus-core/src/engine/write_exec.rs`). The issue #25 fix for MERGE
relationship properties and `SET` on relationship variables landed in the
core engine and passes its unit tests, but it is **never exercised over
HTTP** — the server dispatch calls `engine.create_node` /
`engine.create_relationship` directly and reimplements MERGE/SET itself.

As a result, live testing of `hivehub/nexus:2.4.0` (and the previously
published 2.3.4 — this is pre-existing, not a regression) shows three
relationship gaps over the HTTP API:

1. **`MERGE (a)-[r:T]->(b)` does not create the relationship.** Confirmed:
   after `CREATE (a:QQ {id:1}) CREATE (b:QQ {id:2}) MERGE (a)-[r:REL2]->(b)`,
   `MATCH ()-[r:REL2]->() RETURN count(r)` returns `0`.
2. **`SET` on a relationship variable is dropped.** The server logs
   `WARN Variable r not found in context` for
   `MATCH (a)-[r:T]->(b) SET r.k = v` and `MERGE (...)-[r]->(...) SET r.k = v`.
   The write-path MATCH/MERGE never binds relationship variables into its
   context, so the SET target cannot be resolved.
3. **`CREATE ... RETURN r.prop` cannot project a relationship-variable
   property in the same statement.** The property is stored correctly (a
   separate `MATCH` reads it back), but the CREATE/MERGE RETURN projection
   returns `null` for relationship-variable property access.

`CREATE`-relationship with inline properties (literal and `$param`) works
correctly and persists — verified in 2.4.0 (`CREATE (a)-[r:E {w:$w}]->(b)`
with `{w:7}` reads back `7`). Only MERGE-rel, SET-rel, and same-statement
rel-property projection are affected.

## What Changes

Bring the HTTP `/cypher` write path to parity with the core engine's
relationship semantics. Preferred approach: route the server's MERGE/SET
relationship handling through the already-fixed `nexus-core`
`write_exec.rs` logic (or bind relationship variables into the server's
`variable_context` the same way node variables are bound), so that:

- `MERGE (a)-[r:T {..}]->(b)` creates the edge idempotently with its inline
  properties, honouring direction and matched endpoints.
- `MATCH/MERGE (a)-[r:T]->(b) SET r.k = v` / `SET r += {..}` resolve the
  relationship variable and persist the property (null value removes key).
- `CREATE/MERGE (...)-[r:T {..}]->(...) RETURN r.k` projects the relationship
  property from the same statement.

## Impact

- Affected specs: `specs/http-write-path/spec.md` (this task)
- Affected code: `crates/nexus-server/src/api/cypher/execute/write_ops.rs`,
  `crates/nexus-server/src/api/cypher/mod.rs`; possibly reuse of
  `crates/nexus-core/src/engine/write_exec.rs`.
- Breaking change: NO (fills a gap; existing working queries unaffected).
- User benefit: openCypher MERGE-relationship upserts and relationship
  `SET` work over the HTTP API and every first-party SDK, closing the
  real-world manifestation of issue #25.
