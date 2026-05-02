# Proposal: phase9_external-node-ids

## Why

Today Nexus generates an internal `u64` node id via `RecordStore::allocate_node_id` (monotonic `AtomicU64`) and never accepts a caller-supplied id. External systems that already key their domain entities by a stable hash (file content hash, document SHA-256, UUID, deterministic snowflake, etc.) cannot import data without losing that key — they have to maintain a side table mapping `external_id → nexus_internal_id`, which:

- Forces every cross-system join to round-trip through the side table, breaking idempotency on re-ingest (the same file produces a different node each run).
- Makes deduplication impossible at insert time — `CREATE` always allocates a fresh internal id, even when a node with the same logical identity already exists.
- Causes ordering to drift between systems: ingest order in Nexus does not match the external system's canonical order, so any downstream traversal that assumes "older = lower id" produces inconsistent results.
- Blocks deterministic re-imports for testing and disaster recovery — restoring from an external source can't reproduce the original Nexus ids.

Concrete pain points reported: re-ingesting the same set of files yields duplicates because Nexus has no notion that "this file's hash already exists"; downstream graphs built on Nexus ids cannot be rebuilt from source-of-truth without lossy remapping.

## What Changes

Add first-class support for **external node identity** alongside the existing internal `u64`:

1. **Two id namespaces, both indexed**:
   - `internal_id: u64` — preserved as the physical record offset (unchanged on disk; `NodeRecord` stays 32 bytes).
   - `external_id: ExternalId` — optional, caller-supplied, unique per database. Stored in a new catalog/index, not in the record itself.

2. **`ExternalId` typed enum** (caller picks one per node, but the database accepts a mix):
   - `Hash(BLAKE3 | SHA-256 | SHA-512)` — fixed-width binary, stored verbatim.
   - `Uuid` — 16-byte canonical form.
   - `String(bounded ≤ 256 bytes, UTF-8)` — for arbitrary natural keys (DOI, URN, path).
   - `Bytes(bounded ≤ 64 bytes)` — opaque binary.
   The variant is encoded as a 1-byte discriminator in the index value so we can extend it without rewriting the index.

3. **Cypher syntax**: `CREATE (n:Label {_id: 'sha256:abc…', name: 'foo'})` — the reserved property `_id` (configurable name, default `_id`) sets the external id at create time. `MATCH (n {_id: 'sha256:abc…'})` resolves through the external-id index in O(log n) (or O(1) with a hash lookup).

4. **Conflict policy on duplicate external id**: configurable per request — `error` (default), `match` (return the existing node, no create), `replace` (overwrite properties, keep the same internal id). Mirrors `MERGE` semantics but with an explicit caller-controlled mode that doesn't require a full `MERGE` pattern match.

5. **REST + RPC + SDK surface**: expose `external_id` in node creation payloads, in returned rows (when projected with `n._id`), and in `MATCH`/`MERGE` lookups.

6. **Storage**: a new `external_id_index` (LMDB sub-database in the catalog) maps `external_id_bytes → internal_id`. Reverse lookup (`internal_id → external_id`) lives next to the property store as an optional sidecar so projecting `n._id` doesn't require scanning the catalog.

7. **Backwards compatibility**: nodes without an external id behave exactly as today. Existing data files do not need migration; the external-id index simply starts empty. Internal ids remain the canonical traversal key.

## Impact

- **Affected specs**:
  - `docs/specs/storage-format.md` — add `external_id_index` LMDB sub-database layout and the reverse-mapping sidecar.
  - `docs/specs/cypher-subset.md` — document the reserved `_id` property and the conflict-policy modifier.
  - `docs/specs/api-protocols.md` — REST/RPC schema additions (`external_id` field on create, `_id` projection in rows).
  - `docs/ARCHITECTURE.md` — new "External identity" subsection under the catalog layer.

- **Affected code**:
  - `crates/nexus-core/src/catalog/` — new `ExternalIdIndex` (LMDB sub-database) with `put_if_absent`, `get`, `delete`.
  - `crates/nexus-core/src/storage/mod.rs` — `RecordStore::create_node_with_external_id`, reverse-mapping sidecar load/save, `delete_node` cleanup.
  - `crates/nexus-core/src/storage/graph_engine/engine.rs` — analogous path through the graph-engine surface.
  - `crates/nexus-core/src/engine/crud.rs` — accept `external_id` on `create_node*` paths.
  - `crates/nexus-core/src/executor/parser/` — recognise the reserved `_id` property in `CREATE`/`MERGE`/`MATCH` patterns.
  - `crates/nexus-core/src/executor/operators/create.rs` — allocate via external-id index when `_id` is provided, applying the conflict policy.
  - `crates/nexus-core/src/executor/operators/merge.rs` — fast-path lookup when the only constraining property is `_id`.
  - `crates/nexus-server/src/api/` — REST schema (`POST /cypher` parameters, `GET /nodes/by-external-id/{id}` convenience endpoint).
  - `crates/nexus-protocol/src/` — RPC message additions for the external-id field.
  - `sdks/{rust,python,typescript,go,csharp,php}/` — client surface to set/read `_id`.
  - `tests/cross-compatibility/` — new section asserting Neo4j-compatible behaviour for nodes without `_id` (no regression on the 300/300 diff).

- **Breaking change**: NO. The `_id` property name is reserved going forward; if a database already stores user properties named `_id`, ingestion remains read-compatible — a migration mode (`--rename-conflicting-id-props`) handles the rename at import time. Existing query results and SDKs are unaffected unless callers opt in.

- **User benefit**:
  - Deterministic, idempotent ingestion keyed by external hash — re-running the same import produces the same graph, no duplicates.
  - Cross-system joins without a side-table.
  - Pre-known ids let callers issue `MATCH` and `CREATE` in the same batch without round-trips to discover ids.
  - Stable ids across backup/restore and across replicas (since the external id is the source of truth).

## Source

- `crates/nexus-core/src/storage/mod.rs:289-290` (`allocate_node_id`)
- `crates/nexus-core/src/storage/mod.rs:585-623` (`create_node`)
- `crates/nexus-core/src/storage/mod.rs:625-680` (`create_node_with_label_bits`)
- `crates/nexus-core/src/storage/graph_engine/engine.rs:119-121` (`create_node`)
- `crates/nexus-core/src/engine/crud.rs:356-400` (high-level `create_node`)
- `crates/nexus-core/src/executor/operators/create.rs` (Cypher `CREATE` path)
