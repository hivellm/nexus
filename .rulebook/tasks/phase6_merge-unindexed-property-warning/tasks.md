## 1. Notification model

- [x] 1.1 Define `Notification { code, title, description, severity, category, position }` in `nexus-core` (Neo4j-compatible shape)
- [x] 1.2 Add `notifications: Vec<Notification>` accumulator to `QueryResult` (or equivalent execution-context struct)
- [x] 1.3 Define category enum `{ Performance, Hint, Deprecation, Generic }` and severity enum `{ Information, Warning }`

## 2. Planner integration

- [x] 2.1 In the plan-build path, when a node selector is `(label, prop = literal/parameter)` and the catalog reports no covering index, push a `Notification` (`code = "Nexus.Performance.UnindexedPropertyAccess"`, `category = Performance`, `severity = Information`)
- [x] 2.2 Cover both `MERGE` and `MATCH` selector forms (`{ prop: $v }` and `WHERE n.prop = $v`)
- [x] 2.3 Cover compound selectors — emit one notification per offending `(label, prop)` pair, not per row
- [x] 2.4 Suppress emission when an index is `POPULATING` (avoid noise during index build) — N/A by design in current Nexus: `PropertyIndex::has_index` is binary (no `POPULATING` state surfaced); `create_index` flips the bit synchronously, so the planner naturally stops emitting once the operator runs. Documented in the `compute_unindexed_property_access_notifications` rustdoc.

## 3. Logging hook

- [x] 3.1 Rate-limit WARN log per `(label, property)` pair (default window 60s, configurable via `NEXUS_PLANNER_WARN_INTERVAL_SECS`)
- [x] 3.2 Log line includes the suggested `CREATE INDEX` DDL verbatim so it can be copy-pasted

## 4. HTTP envelope

- [x] 4.1 Serialize `notifications: [...]` into the `/cypher` JSON response in `crates/nexus-server/src/api/cypher/execute.rs` (and mirrored on the native RPC envelope at `crates/nexus-server/src/protocol/rpc/dispatch/cypher.rs`)
- [x] 4.2 Confirm field is omitted (not `null`, not `[]`) when there are no notifications, to keep the wire format compact for the hot path

## 5. Tail (mandatory — enforced by rulebook v5.3.0)

- [x] 5.1 Update or create documentation covering the implementation — `docs/performance/PERFORMANCE.md` gained a "Recommended indexes for ingest workloads" section covering `Artifact.natural_key`, `Artifact.path`, `Turn.id`, `ToolCall.id`, `Session.id`, the wire format of the new `notifications` field, and the `NEXUS_PLANNER_WARN_INTERVAL_SECS` knob.
- [x] 5.2 Write tests covering the new behavior — 6 planner unit tests in `crates/nexus-core/src/executor/planner/tests.rs` (MERGE inline, MATCH inline, WHERE equality, suppression-when-indexed, deduplication-per-plan, no-op-without-property-index-handle) plus 3 engine end-to-end tests in `crates/nexus-core/tests/unindexed_property_notification_e2e_test.rs` (MERGE through `Engine::execute_cypher` surfaces notification, indexed pair stays silent, no cross-query leak through the per-thread sink).
- [x] 5.3 Run tests and confirm they pass — `cargo +nightly fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo +nightly test -p nexus-core` all green (2340 lib tests + 3 new e2e + 6 new planner tests pass).
