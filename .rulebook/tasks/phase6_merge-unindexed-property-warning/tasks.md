## 1. Notification model

- [ ] 1.1 Define `Notification { code, title, description, severity, category, position }` in `nexus-core` (Neo4j-compatible shape)
- [ ] 1.2 Add `notifications: Vec<Notification>` accumulator to `QueryResult` (or equivalent execution-context struct)
- [ ] 1.3 Define category enum `{ Performance, Hint, Deprecation, Generic }` and severity enum `{ Information, Warning }`

## 2. Planner integration

- [ ] 2.1 In the plan-build path, when a node selector is `(label, prop = literal/parameter)` and the catalog reports no covering index, push a `Notification` (`code = "Nexus.Performance.UnindexedPropertyAccess"`, `category = Performance`, `severity = Information`)
- [ ] 2.2 Cover both `MERGE` and `MATCH` selector forms (`{ prop: $v }` and `WHERE n.prop = $v`)
- [ ] 2.3 Cover compound selectors — emit one notification per offending `(label, prop)` pair, not per row
- [ ] 2.4 Suppress emission when an index is `POPULATING` (avoid noise during index build)

## 3. Logging hook

- [ ] 3.1 Rate-limit WARN log per `(label, property)` pair (default window 60s, configurable via `NEXUS_PLANNER_WARN_INTERVAL_SECS`)
- [ ] 3.2 Log line includes the suggested `CREATE INDEX` DDL verbatim so it can be copy-pasted

## 4. HTTP envelope

- [ ] 4.1 Serialize `notifications: [...]` into the `/cypher` JSON response in `crates/nexus-server/src/api/cypher/execute.rs`
- [ ] 4.2 Confirm field is omitted (not `null`, not `[]`) when there are no notifications, to keep the wire format compact for the hot path

## 5. Tail (mandatory — enforced by rulebook v5.3.0)

- [ ] 5.1 Update or create documentation covering the implementation — specifically `docs/performance/PERFORMANCE.md` with a "Recommended indexes for ingest workloads" section covering `Artifact.natural_key`, `Artifact.path`, `Turn.id`, `ToolCall.id`, plus the wire format of the new `notifications` field
- [ ] 5.2 Write tests covering the new behavior — planner unit tests asserting notification emission for `MERGE`/`MATCH` selector forms; integration test asserting envelope contains `notifications` for an unindexed `MERGE`; regression test asserting NO notification when the index exists
- [ ] 5.3 Run tests and confirm they pass — `cargo +nightly fmt --all`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace --verbose` all green
