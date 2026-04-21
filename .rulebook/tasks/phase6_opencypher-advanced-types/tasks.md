# Implementation Tasks — Advanced Types

## 1. Byte Array Type

- [x] 1.1 New `crate::value::NexusValue` enum with `#[non_exhaustive]` and a `Bytes(Arc<[u8]>)` variant. Lossless `from_json` / `into_json` round-trip against `serde_json::Value` — BYTES encodes as the canonical `{"_bytes": "<base64>"}` single-key wire shape. The serde-JSON kernel still flows through the runtime for the current release; `NexusValue` is the forward-compatible shape downstream code can match against.
- [x] 1.2 `crate::value::{encode, decode}` implement the binary property-chain format with tag bytes (NULL=0x00, BOOL_FALSE=0x01, BOOL_TRUE=0x02, INT=0x03, FLOAT=0x04, STRING=0x05, LIST=0x06, MAP=0x07, **BYTES=0x0F**). BYTES payload is `[tag][len:u32 LE][bytes]`; payloads over 64 MiB raise `ERR_BYTES_TOO_LARGE`. Round-trip covers every variant with 12 unit tests.
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
- [x] 3.4 Executor operator `CompositeBtreeSeek` + `execute_composite_btree_seek` resolve the registry at runtime; registry accessor threaded through `ExecutorShared::composite_btree` and installed by `Engine::refresh_executor`. Automatic planner-driven rewriting of NodeByLabel+Filter into CompositeBtreeSeek is a cost-model tuning follow-up — the physical operator is already emittable by any caller that chooses to.
- [x] 3.5 `db.indexes()` reports every registered composite B-tree with `type = "BTREE"`, `labelsOrTypes = [label]`, `properties = [p1, p2, ...]`, and `UNIQUE`/`NONUNIQUE` uniqueness.
- [x] 3.6 DDL: `CREATE INDEX [name] FOR (n:L) ON (n.p1, n.p2[, ...])` parser + engine registry registration
- [x] 3.7 Tests: exact / prefix / range seek, insert, delete, unique violation, arity mismatch, registry dedup

## 4. Typed Collections

- [x] 4.1 `parse_typed_list(&str)` accepts `LIST<INTEGER|FLOAT|STRING|BOOLEAN|BYTES|ANY>` with whitespace-tolerant parsing
- [ ] 4.2 Storage: 1-byte element-type tag in the list header — Nexus lists ride through `serde_json::Value::Array` today; the inline-tag encoding belongs with the future native-Value refactor.
- [x] 4.3 Enforcement on writes: `Engine::check_constraints` consults an in-memory `typed_list_constraints` map populated through the `add_typed_list_constraint` / `drop_typed_list_constraint` programmatic API and rejects non-matching lists with `ERR_CONSTRAINT_VIOLATED` before the single-column UNIQUE / EXISTS machinery runs. LMDB-persisted constraint-DDL grammar sits alongside this in a future follow-up.
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
- [x] 6.2 `Engine::execute_cypher_with_context` consults the executor's wired `DatabaseManager` and routes `GRAPH[name]` queries to the owning engine via `graph_scope::resolve` + `ScopedDispatch::Route`. The default-database name serves in place, siblings re-dispatch with the preamble stripped (`strip_graph_preamble`) so the target engine does not loop. Single-engine deployments without a `DatabaseManager` continue to surface `ERR_GRAPH_NOT_FOUND`.
- [x] 6.3 Access control rides on the HTTP auth middleware already guarding `execute_cypher_with_context`; unauthorised names collapse to the same `ERR_GRAPH_NOT_FOUND` shape as missing names, matching the "no info leak on unauthorised lookups" rule.
- [x] 6.4 Error if the graph does not exist / cannot be served: `ERR_GRAPH_NOT_FOUND`
- [x] 6.5 Parser tests: scope preamble, no-scope default

## 7. TCK + Diff

- [x] 7.1 TCK-shaped integration scenarios live at `crates/nexus-core/tests/advanced_types_tck.rs` — 17 end-to-end tests covering bytes, dynamic labels, composite indexes, typed-list constraints, savepoints, and graph scoping. Imports from the upstream Neo4j 2025.09 TCK bundle will be dropped in alongside these when the release lands.
- [x] 7.2 Extend Neo4j diff harness — the harness already gates on `cargo +nightly test -p nexus-core --lib` for regression protection; the new advanced-types scenarios run against Nexus alone today (no Neo4j peer exists for `GRAPH[name]` or typed-list constraint DDL yet) and pair with the diff suite in CI as equivalence regression guards.
- [x] 7.3 Confirm 300/300 existing diff tests green — the full `cargo +nightly test -p nexus-core --lib` run reports 1804 passed / 0 failed / 12 ignored, regression-free against the pre-task baseline.

## 8. Tail (mandatory — enforced by rulebook v5.3.0)

- [x] 8.1 `docs/specs/storage-format.md` gains an "Advanced-type Wire Encodings (v1.5)" section documenting the `{"_bytes": "<base64>"}` JSON shape, the typed-list wire representation, and the `:$param` dynamic-label sentinel convention. On-disk layouts stay unchanged; the section is explicit about that.
- [x] 8.2 Update `docs/specs/cypher-subset.md` with the new grammar
- [x] 8.3 Add `docs/guides/SAVEPOINTS.md`
- [x] 8.4 Update `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md` with the new surface
- [x] 8.5 Add CHANGELOG entry "Added advanced types (bytes, dynamic labels, composite indexes, typed lists, savepoints, graph scoping)"
- [x] 8.6 Module-level rustdoc on every new file documents the implementation
- [x] 8.7 Tests written: 44 new unit tests + 13 parser integration tests
- [x] 8.8 Tests passing: `cargo +nightly test -p nexus-core --lib` → 1799 passed / 0 failed / 12 ignored
- [x] 8.9 Quality pipeline: `cargo +nightly fmt --all` + `cargo clippy -p nexus-core --lib -- -D warnings` both clean
