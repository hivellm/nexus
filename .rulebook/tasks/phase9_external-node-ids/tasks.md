## 1. Catalog: ExternalId index
- [ ] 1.1 Define `ExternalId` enum in `crates/nexus-core/src/catalog/external_id.rs` with variants `Hash(HashKind, [u8; N])`, `Uuid([u8; 16])`, `String(SmolStr<256>)`, `Bytes(BoundedVec<64>)` and a 1-byte discriminator on the wire
- [ ] 1.2 Add `HashKind { Blake3, Sha256, Sha512 }` and parser/printer (`sha256:hex…`, `blake3:hex…`, `uuid:…`, `str:…`, `bytes:hex…`) with round-trip tests
- [ ] 1.3 Implement length validation and reject zero-length / over-length values with a typed error
- [ ] 1.4 Create LMDB sub-database `external_ids` in `crates/nexus-core/src/catalog/mod.rs` (key = encoded `ExternalId`, value = `u64` internal id, little-endian)
- [ ] 1.5 Implement `ExternalIdIndex::put_if_absent`, `get`, `delete`, `iter` with transactional semantics matching the rest of the catalog
- [ ] 1.6 Add reverse map `internal_ids` sub-database (key = `u64`, value = encoded `ExternalId`) for `n._id` projection without scanning
- [ ] 1.7 Wire catalog open/close/recovery to load both sub-databases and add a startup integrity check (forward and reverse maps agree)
- [ ] 1.8 Unit tests: insert / lookup / duplicate-rejection / delete / reopen-and-reload / forward-reverse consistency

## 2. Storage: node creation paths
- [ ] 2.1 Add `RecordStore::create_node_with_external_id` accepting `Option<ExternalId>` and a `ConflictPolicy { Error, Match, Replace }`
- [ ] 2.2 Refactor `create_node` and `create_node_with_label_bits` (`crates/nexus-core/src/storage/mod.rs:585-680`) to delegate to the new function with `None`
- [ ] 2.3 On `Match`: return the existing internal id without writing a record
- [ ] 2.4 On `Replace`: reuse the internal id, overwrite properties through the property store, leave label bits as-is unless the caller passes new ones
- [ ] 2.5 On `Error`: surface a typed `ExternalIdConflict { existing_internal_id, attempted_external_id }` error
- [ ] 2.6 Update `delete_node` to remove both the forward and reverse external-id entries atomically
- [ ] 2.7 Mirror the new path in `crates/nexus-core/src/storage/graph_engine/engine.rs:119` and `crates/nexus-core/src/graph/core.rs:352`
- [ ] 2.8 Unit tests: each conflict policy, delete-then-recreate, concurrent insert race (single-writer model — verify ordering)

## 3. Engine + transaction integration
- [ ] 3.1 Extend `engine::crud::create_node` (`crates/nexus-core/src/engine/crud.rs:356`) and `create_node_with_transaction` (`:366`) with `external_id: Option<ExternalId>` and `policy: ConflictPolicy`
- [ ] 3.2 Ensure WAL records the external-id assignment so recovery rebuilds the catalog index
- [ ] 3.3 Verify MVCC: a reader at epoch E sees the external-id mapping iff it sees the node record (snapshot consistency)
- [ ] 3.4 Add transaction-rollback path that removes the external-id entry if the node creation aborts

## 4. Cypher executor
- [ ] 4.1 Reserve `_id` as a magic property in `crates/nexus-core/src/executor/parser/` and forbid user-defined `_id` properties unless `compat.allow_user_underscore_id = true`
- [ ] 4.2 Parse `_id` value as one of: quoted prefixed string (`'sha256:abc…'`), parameter (`$external_id`), or function call (`hash('blake3', $bytes)`) — emit a clear error otherwise
- [ ] 4.3 Add an optional clause modifier `CREATE (n:Label {_id: $x}) ON CONFLICT MATCH|REPLACE|ERROR` (default `ERROR`)
- [ ] 4.4 Implement the `CREATE` operator branch in `crates/nexus-core/src/executor/operators/create.rs` to call the new storage path
- [ ] 4.5 Implement `MERGE` fast-path in `crates/nexus-core/src/executor/operators/merge.rs`: when the only match key is `_id`, bypass the pattern-match scan and use the index
- [ ] 4.6 Implement `MATCH` resolution in the planner: predicate `n._id = 'sha256:…'` becomes an index seek, not a label scan
- [ ] 4.7 Project `_id` correctly: `RETURN n._id` returns the original prefixed string for the node, or `null` if no external id was set
- [ ] 4.8 Compatibility tests: re-run `scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1` — must stay 300/300 (Neo4j has no `_id` semantics, so existing tests must not regress)

## 5. REST + RPC + SDK
- [ ] 5.1 Extend `POST /cypher` to accept and round-trip `_id` through parameters; document in `docs/specs/api-protocols.md`
- [ ] 5.2 Add convenience endpoint `GET /nodes/by-external-id/{id}` returning the node (404 when absent)
- [ ] 5.3 Add `POST /nodes` (and the equivalent RPC op) accepting `{labels, properties, external_id, conflict_policy}` for callers that don't want to write Cypher
- [ ] 5.4 Update `crates/nexus-protocol/` RPC schema (MessagePack) with the new fields, version-bumping the protocol minor number
- [ ] 5.5 Update each SDK (`sdks/{rust,python,typescript,go,csharp,php}/`) to expose `externalId` / `external_id` on create + a `getByExternalId` helper
- [ ] 5.6 Update SDK comprehensive tests (one new test per SDK covering create-with-external-id and re-create-with-conflict-policy)

## 6. Documentation
- [ ] 6.1 Update `docs/specs/storage-format.md` with the catalog sub-databases and the wire encoding for `ExternalId`
- [ ] 6.2 Update `docs/specs/cypher-subset.md` with the reserved `_id` property and the `ON CONFLICT` clause
- [ ] 6.3 Update `docs/specs/api-protocols.md` with the new REST and RPC payloads
- [ ] 6.4 Update `docs/ARCHITECTURE.md` with an "External identity" subsection
- [ ] 6.5 Add `docs/guides/EXTERNAL_IDS.md` with motivating examples (file-hash ingestion, deterministic re-import, cross-system joins)
- [ ] 6.6 Update `CHANGELOG.md` under "Added" using conventional-commit style

## 7. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 7.1 Update or create documentation covering the implementation
- [ ] 7.2 Write tests covering the new behavior
- [ ] 7.3 Run tests and confirm they pass
