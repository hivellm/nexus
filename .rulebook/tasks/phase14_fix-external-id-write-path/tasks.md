# Tasks: phase14_fix-external-id-write-path

Fix issue #29: `_id` (reserved external-id slot) is silently dropped by the engine
write path, so `RETURN n._id` projects null and `WHERE n._id = …` never matches.
Root cause confirmed: `engine/write_exec.rs` never reads `external_id_expr` after
commit `99660fb4` moved HTTP writes onto it. Read path is correct — do not change it.

## 1. Confirm scope before changing code (research-first)
- [ ] 1.1 Resolve the id-format question: determine what Cortex actually writes as `_id` (bare ULID vs `str:`-prefixed). `ExternalId::from_str` requires a `sha256:`/`sha512:`/`uuid:`/`str:` prefix (`docs/specs/cypher-subset.md:435-444`, `executor/operators/create.rs:64`), so a bare ULID would be REJECTED even by the repaired path. Decide and record: restore plumbing only, or plumbing + id-format reconciliation (and whether unprefixed ids should be accepted as implicit `str:`)
- [ ] 1.2 Write a failing reproduction covering each broken form against a clean database: `MERGE (n {_id: …})`, `CREATE (n {_id: …}) SET …`, and `UNWIND $rows … MERGE … SET`, each asserting `RETURN n._id` is non-null and `WHERE n._id = …` matches. Confirm bare standalone `CREATE (n {_id: …})` still passes (proves the routing split at `engine/query_pipeline.rs:659-666`)
- [ ] 1.3 Confirm the pre-upgrade/post-upgrade data split: verify nodes written before 2.5.0 still project `_id` correctly while nodes written after do not — this localizes the regression and sizes the backfill

## 2. Fix the engine write path
- [ ] 2.1 `crates/nexus-core/src/engine/write_exec.rs` CREATE arm (`:61-78`): read `create_clause.external_id_expr` and `conflict_policy`, resolve the expression (literal or parameter) and route through `Engine::create_node_with_external_id` (`engine/crud/nodes.rs:47`) when set, plain `create_node` otherwise. Mirror the resolution logic the deleted `write_ops.rs` fork used (recoverable via `git show v2.3.4:crates/nexus-server/src/api/cypher/execute/write_ops.rs`)
- [ ] 2.2 `process_merge_clause` (`write_exec.rs:447`): same threading for `MergeClause.external_id_expr` + conflict policy, including the match-existing-by-external-id semantics MERGE requires
- [ ] 2.3 UNWIND write path (`write_exec.rs:36-42` → `execute_unwind_write_query`): ensure per-row `_id` resolution works when the value comes from the unwound row parameter, not a literal
- [ ] 2.4 Verify error propagation: an invalid/unparseable `_id` must surface the `invalid _id` error (`executor/operators/create.rs:65`) rather than being silently dropped — silent-drop is what made this regression invisible

## 3. Close the test gap (this is why CI missed it)
- [ ] 3.1 Restore automated `RETURN n._id` projection coverage that was deleted from `crates/nexus-core/tests/cypher_external_id.rs:48-53` — use an isolated per-test database instead of the process-wide shared catalog that made it flaky, so it stays in CI rather than being downgraded to manual validation again
- [ ] 3.2 Add coverage for the write forms that were never tested: MERGE, CREATE+SET, and UNWIND batch ingest — the existing 3 tests all use bare standalone CREATE, the one path that never broke
- [ ] 3.3 Add `WHERE n._id = …` filter coverage (same evaluator as projection, but assert it explicitly since it was a reported symptom)
- [ ] 3.4 Make the non-CI end-to-end coverage reachable: either wire `sdks/rust/tests/external_id_live.rs` into a CI job with a spawned server, or document explicitly why it stays manual

## 4. Backfill for data written under 2.5.0
- [ ] 4.1 Confirm whether the source values are recoverable from existing properties (`n.id` / `natural_key` / `path` — Cortex's coalesce workaround suggests yes) and document what is NOT recoverable
- [ ] 4.2 Ship a backfill script under `scripts/` that populates the external-id index for affected nodes from a caller-specified source property, with a dry-run mode and a report of skipped/ambiguous nodes
- [ ] 4.3 Document the upgrade/remediation procedure for affected deployments (who needs to run it, how to detect affected nodes)

## 5. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 5.1 Update or create documentation covering the implementation (`docs/specs/cypher-subset.md` § Reserved `_id`: state explicitly which write forms honour `_id` and the prefix requirement resolved in 1.1; CHANGELOG entry referencing issue #29)
- [ ] 5.2 Write tests covering the new behavior (sections 1.2 and 3 — projection, filtering, and all write forms, in CI without live-server dependency)
- [ ] 5.3 Run tests and confirm they pass (`cargo +nightly fmt --all`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo +nightly test --workspace`; confirm the issue #29 reproduction from 1.2 now passes and report the result on the GitHub issue)
