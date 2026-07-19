# Proposal: phase0_fix-ingest-bulk-path

**Priority: HIGH — the documented bulk-ingestion endpoint cannot bulk-ingest.**
Found while implementing `phase7_ldbc-snb-benchmark` item 1.3 (LDBC SNB bulk loader);
not previously reported and not tracked by any GitHub issue.

## Why

`POST /ingest` is documented as the bulk data ingestion endpoint
(`nexus-server/src/lib.rs:6`, `main.rs:6`). It is unusable as one, for three
independent reasons — two correctness, one performance.

### 1. `NodeIngest.id` is silently ignored

`NodeIngest` accepts an `id: Option<u64>` (`api/ingest.rs:36-37`), but
`create_node_in_batch` (`:293-335`) never reads it — the field carries
`#[allow(dead_code)]`, which is the compiler confirming it. A client supplying
stable ids gets them silently discarded.

Verified: ingesting three nodes with `id` 933 / 4139 / 8796 produced internal ids
0 / 1 / 2. (Properties *are* stored, so a caller can pass the id inside
`properties` instead — but the top-level field is a trap.)

### 2. Relationships are unusable, because ids are never returned

`RelIngest.src` / `.dst` are internal node ids — `create_relationship_in_batch`
(`:338-377`) builds `MATCH (a), (b) WHERE id(a) = {src} AND id(b) = {dst}`. But
`IngestResponse` (`:64-80`) returns only counts, never the ids of the nodes it just
created. There is therefore **no way to correlate an input node with its internal
id** through this endpoint, so a client that ingests nodes cannot then ingest the
relationships between them. The two halves of the endpoint do not compose.

### 3. It is an order of magnitude slower than the plain Cypher path

The endpoint's "batching" wraps a `BEGIN TRANSACTION`, then for every node builds a
Cypher string and calls `engine.execute_cypher` while holding `server.engine.write()`
— one lock acquisition, one parse and one plan per node (`:191-210`, `:328-333`).

Measured, same machine, 5 000 nodes:

| Path | Debug | Release |
|---|---:|---:|
| `POST /ingest` | 464 nodes/s | 469 nodes/s |
| `UNWIND $rows AS r CREATE (...)` via `/cypher` | 736 nodes/s | 5 097 nodes/s |

`/ingest` does not improve at all with optimizations, because its cost is per-node
lock and parse overhead rather than compute — so in a release build, the endpoint
built for bulk loading is **~11x slower than the ordinary query path**. At 469
nodes/s, SF0.1's 327 588 nodes take ~12 minutes and SF1 takes over 2 hours, before
a single edge is written.

## What Changes

Decide the endpoint's fate deliberately rather than leaving a documented API that
does not work. Two coherent options; §1 picks one on evidence:

- **Make it work**: batch each chunk into a single parameterized `UNWIND` execution
  instead of one query per row, acquire the write lock once per batch, honour the
  `id` field (or remove it), and return the created ids so relationships can
  reference them. This is what would make `/ingest` genuinely faster than `/cypher`,
  since it can skip re-parsing entirely.
- **Retire it**: document `UNWIND` over `/cypher` as the supported bulk path, keep
  `/ingest` as a thin compatibility shim over that, and delete the dead `id` field.

Either way the dead `id` field and the non-composing relationship path must go.

## Impact

- Affected specs: `docs/specs/api-protocols.md`; `CLAUDE.md` and `lib.rs`/`main.rs`
  headers describe `/ingest` as the bulk path
- Affected code: `crates/nexus-server/src/api/ingest.rs`
- Breaking change: **possibly** — removing `NodeIngest.id` changes the request
  schema, though the field is inert today so nothing can depend on its behaviour.
  Returning created ids is additive.
- User benefit: a bulk-loading endpoint that actually loads in bulk, and can load a
  connected graph rather than only disconnected nodes
- Blocks: `phase7_ldbc-snb-benchmark` item 1.3 specifies loading "via `/ingest`".
  That is not currently achievable — the loader must use `UNWIND` over `/cypher`
  and the benchmark task's wording needs updating to match.
