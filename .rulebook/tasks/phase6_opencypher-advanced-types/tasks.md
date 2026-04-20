# Implementation Tasks — Advanced Types

## 1. Byte Array Type

- [ ] 1.1 Add `Value::Bytes(Arc<[u8]>)` variant with `#[non_exhaustive]` on Value
- [ ] 1.2 Property-chain encoder: `TYPE_BYTES` tag with u32 length prefix
- [ ] 1.3 Add `bytes(str)`, `bytes.fromBase64(str)`, `bytes.toHex(b)`, `bytes.length(b)`
- [ ] 1.4 JSON serialisation emits `{"_bytes": "<base64>"}`
- [ ] 1.5 Parameter binding accepts base64-encoded strings under a `bytes` tag
- [ ] 1.6 Tests: round-trip, size limits, NULL handling

## 2. Dynamic Labels on Writes

- [ ] 2.1 Parser: accept `$param` in label position of `CREATE (n:$x)`
- [ ] 2.2 Parser: accept `SET n:$x` and `REMOVE n:$x`
- [ ] 2.3 Parser: accept LIST<STRING> for multi-label `$[labels]`
- [ ] 2.4 Executor: resolve parameter at runtime; lookup label IDs
- [ ] 2.5 Rejection: NULL or empty list, non-STRING element → `ERR_INVALID_LABEL`
- [ ] 2.6 Tests covering every write form

## 3. Composite B-tree Index

- [ ] 3.1 New module `index/composite_btree.rs`
- [ ] 3.2 Encoded key: `(label_bits, tuple_bytes)` with lexicographic ordering
- [ ] 3.3 Uniqueness flag supported (needed by NODE KEY)
- [ ] 3.4 Planner recognises composite predicates and seeks
- [ ] 3.5 `db.indexes()` reports composite B-tree with `properties` as a list
- [ ] 3.6 DDL: `CREATE INDEX FOR (n:L) ON (n.p1, n.p2[, ...])`
- [ ] 3.7 Tests: seek, insert, delete, MVCC replay

## 4. Typed Collections

- [ ] 4.1 Extend property-type constraint grammar for `LIST<T>`
- [ ] 4.2 Storage: 1-byte element-type tag in the list header; inline scalars
- [ ] 4.3 Enforcement on writes (piggybacks on constraint engine)
- [ ] 4.4 Empty-list case defaults to untyped
- [ ] 4.5 Tests covering ingest + enforcement + round-trip

## 5. Transaction Savepoints

- [ ] 5.1 New SQL-ish statements `SAVEPOINT name`, `ROLLBACK TO SAVEPOINT name`, `RELEASE SAVEPOINT name`
- [ ] 5.2 MVCC journal stack: per-tx push/pop of named markers
- [ ] 5.3 Rollback replays undo log to the named marker
- [ ] 5.4 Nested savepoints with proper unwinding
- [ ] 5.5 Reject savepoint ops outside a transaction
- [ ] 5.6 Tests including error inside a savepoint

## 6. Graph Scoping

- [ ] 6.1 Parse leading `GRAPH[<name>]` clause
- [ ] 6.2 Planner scopes the entire query to the named graph
- [ ] 6.3 Error if the caller has no read access to the graph
- [ ] 6.4 Error if the graph does not exist
- [ ] 6.5 Tests

## 7. TCK + Diff

- [ ] 7.1 Import TCK bytes/dynamic-label/typed-list scenarios
- [ ] 7.2 Extend Neo4j diff harness where Neo4j behaviours match
- [ ] 7.3 Confirm 300/300 existing diff tests green

## 8. Tail (mandatory — enforced by rulebook v5.3.0)

- [ ] 8.1 Update `docs/specs/storage-format.md` with bytes + typed-list encoding
- [ ] 8.2 Update `docs/specs/cypher-subset.md` with new grammar
- [ ] 8.3 Add `docs/guides/SAVEPOINTS.md`
- [ ] 8.4 Update `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md`
- [ ] 8.5 Add CHANGELOG entry "Added advanced types (bytes, dynamic labels, composite indexes, typed lists, savepoints)"
- [ ] 8.6 Update or create documentation covering the implementation
- [ ] 8.7 Write tests covering the new behavior
- [ ] 8.8 Run tests and confirm they pass
- [ ] 8.9 Quality pipeline: fmt + clippy + ≥95% coverage
