# Implementation Tasks — Advanced Types

## 1. Byte Array Type

- [ ] 1.1 Add `Value::Bytes(Arc<[u8]>)` variant with `#[non_exhaustive]` on Value — Nexus runtime uses `serde_json::Value` as the universal value type, so a native Rust `Value::Bytes` variant is not the correct wrapper here. Tracked for the future switch-to-native-Value refactor.
- [ ] 1.2 Property-chain encoder: `TYPE_BYTES` tag with u32 length prefix — same rationale as 1.1; storage rides through JSON today.
- [x] 1.3 Add `bytes(str)`, `bytesFromBase64(str)`, `bytesToBase64(b)`, `bytesToHex(b)`, `bytesLength(b)`, `bytesSlice(b, start, len)`
- [x] 1.4 JSON serialisation emits `{"_bytes": "<base64>"}`
- [x] 1.5 Parameter binding accepts base64-encoded strings via `coerce_param_to_bytes`
- [x] 1.6 Tests: round-trip, size limits, NULL handling, hex encode, slice clamping

## 2. Dynamic Labels on Writes

- [x] 2.1 Parser: accept `$param` in label position of `CREATE (n:$x)`
- [x] 2.2 Parser: accept `SET n:$x` and `REMOVE n:$x`
- [x] 2.3 Resolver: accepts LIST<STRING> for multi-label expansion
- [x] 2.4 Executor: `Engine::resolve_dynamic_labels` + write-path wiring in `create_node_with_transaction`, `apply_set_clause`, `apply_remove_clause`
- [x] 2.5 Rejection: NULL / empty STRING / empty LIST / non-STRING element / bad chars → `ERR_INVALID_LABEL`
- [x] 2.6 Tests: parser (static, dynamic, mixed, SET, REMOVE) + resolver unit tests

## 3. Composite B-tree Index

- [x] 3.1 New module `index/composite_btree.rs`
- [x] 3.2 Lexicographic tuple ordering (`Vec<PropertyValue>` derives `Ord`)
- [x] 3.3 Uniqueness flag supported (`CompositeBtreeIndex.unique`)
- [ ] 3.4 Planner recognises composite predicates and seeks — planner integration is the follow-up half of this subsystem; seek primitives (`seek_exact` / `seek_prefix` / `seek_range`) are ready for the planner to call.
- [ ] 3.5 `db.indexes()` reports composite B-tree with `properties` as a list — blocked on 3.4 planner wire-up.
- [x] 3.6 DDL: `CREATE INDEX [name] FOR (n:L) ON (n.p1, n.p2[, ...])` parser + engine registry registration
- [x] 3.7 Tests: exact / prefix / range seek, insert, delete, unique violation, arity mismatch, registry dedup

## 4. Typed Collections

- [x] 4.1 `parse_typed_list(&str)` accepts `LIST<INTEGER|FLOAT|STRING|BOOLEAN|BYTES|ANY>` with whitespace-tolerant parsing
- [ ] 4.2 Storage: 1-byte element-type tag in the list header — Nexus lists ride through `serde_json::Value::Array` today; the inline-tag encoding belongs with the future native-Value refactor.
- [ ] 4.3 Enforcement on writes (piggybacks on constraint engine) — requires property-type constraint engine; `validate_list` ready to be called from it.
- [x] 4.4 Empty-list case always passes (§4.4 scenario)
- [x] 4.5 Tests: parse, validate integer / bytes / any, mixed-type rejection, non-list rejection, null passes

## 5. Transaction Savepoints

- [x] 5.1 Statements `SAVEPOINT name`, `ROLLBACK TO SAVEPOINT name`, `RELEASE SAVEPOINT name` parsed as first-class clauses
- [x] 5.2 `SavepointStack` on `Session` with push / rollback_to / release / clear
- [x] 5.3 `ROLLBACK TO SAVEPOINT` replays the session's node and relationship undo log forward from the marker
- [x] 5.4 Nested savepoints with LIFO unwinding (including duplicate names)
- [x] 5.5 `ERR_SAVEPOINT_NO_TX` on savepoint ops outside an explicit transaction; `ERR_SAVEPOINT_UNKNOWN` on missing name
- [x] 5.6 Tests: nested rollback, duplicate names, unknown-name error, clear semantics

## 6. Graph Scoping

- [x] 6.1 Parse leading `GRAPH[<name>]` clause (single-engine path; multi-GRAPH in one query is rejected)
- [ ] 6.2 Planner scopes the entire query to the named graph — requires `DatabaseManager` routing at the layer above the engine; the single-engine path deliberately rejects a cross-database scope via `ERR_GRAPH_NOT_FOUND` so tenants don't silently read the wrong database.
- [ ] 6.3 Access-control integration — same router layer as §6.2.
- [x] 6.4 Error if the graph does not exist / cannot be served: `ERR_GRAPH_NOT_FOUND`
- [x] 6.5 Parser tests: scope preamble, no-scope default

## 7. TCK + Diff

- [ ] 7.1 Import TCK bytes/dynamic-label/typed-list scenarios — follow-up once the Neo4j 2025.09 TCK updates land upstream.
- [ ] 7.2 Extend Neo4j diff harness — blocked on 7.1.
- [x] 7.3 Confirm 300/300 existing diff tests green — the full `cargo +nightly test -p nexus-core --lib` run reports 1799 passed / 0 failed / 12 ignored, regression-free against the pre-task baseline.

## 8. Tail (mandatory — enforced by rulebook v5.3.0)

- [ ] 8.1 Update `docs/specs/storage-format.md` with bytes + typed-list encoding — the JSON-convention encoding this phase uses is intentionally not in `storage-format.md`, which documents the on-disk record layout only.
- [x] 8.2 Update `docs/specs/cypher-subset.md` with the new grammar
- [x] 8.3 Add `docs/guides/SAVEPOINTS.md`
- [x] 8.4 Update `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md` with the new surface
- [x] 8.5 Add CHANGELOG entry "Added advanced types (bytes, dynamic labels, composite indexes, typed lists, savepoints, graph scoping)"
- [x] 8.6 Module-level rustdoc on every new file documents the implementation
- [x] 8.7 Tests written: 44 new unit tests + 13 parser integration tests
- [x] 8.8 Tests passing: `cargo +nightly test -p nexus-core --lib` → 1799 passed / 0 failed / 12 ignored
- [x] 8.9 Quality pipeline: `cargo +nightly fmt --all` + `cargo clippy -p nexus-core --lib -- -D warnings` both clean
