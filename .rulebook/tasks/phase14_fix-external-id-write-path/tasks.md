# Tasks: phase14_fix-external-id-write-path

Fix issue #29: `_id` (reserved external-id slot) is silently dropped by the engine
write path, so `RETURN n._id` projects null and `WHERE n._id = …` never matches.
Root cause confirmed: `engine/write_exec.rs` never reads `external_id_expr` after
commit `99660fb4` moved HTTP writes onto it. Read path is correct — do not change it.

## 1. Confirm scope before changing code (research-first)
- [x] 1.1 Resolve the id-format question — **ANSWERED, and it reframes the issue. Cortex never writes `_id` at all.** Its live writer (`cortex-workers/src/graph/nexus_client.rs:368-405`) explicitly ignores the template registry (the parameter is literally named `_templates`, see the comment at `:378-379`) and emits `UNWIND [...] AS row MERGE (n:Label { id: row._k }) SET ...` (`render_node_unwind`, `:575-608`) — a plain `id` property, never `_id`. `git log -S "CREATE (n:"` on that file returns **zero commits**: the live writer has never contained a `_id`-bearing form, on any version. The 11 `cypher/node_*.cypher` templates that do use `_id` are dead code, pinned only by a test that greps the files on disk (`graph/cypher.rs:326-374`) and never executes them.
  **Consequences:** (a) the "regression vs 2.3.4" framing in issue #29 is a misdiagnosis — `_id` projected null on 2.3.4 too, because nobody wrote it; the bare ULID in the report is the ordinary `id` property. (b) The Nexus bug is nonetheless **real and independently confirmed** (§2 below) — `write_exec.rs` does silently discard `external_id_expr`; it simply is not what Cortex hit. (c) Fixing it will not un-break Cortex, and conversely strict `ExternalId::from_str` validation cannot regress Cortex today. (d) Cortex ADR-004 (status **`proposed`**, never accepted) explicitly chose **unprefixed** passthrough values (`identity.rs:68-71` returns `natural_key` verbatim), which `ExternalId::from_str` rejects — so before Cortex ever activates those templates, the format must be reconciled at `external_id_for_node`, their single chokepoint. Report this back on issue #29 rather than letting the regression framing stand.
- [x] 1.2 Reproduction written — delivered as the permanent regression suite rather than a throwaway script: `crates/nexus-core/tests/cypher_external_id_write_paths.rs` (11 tests) covers `MERGE`, `CREATE … SET`, and `UNWIND … CREATE`, each asserting a non-null `_id` via three independent routes (catalog index, `RETURN n._id`, `WHERE n._id = …`). The bare standalone `CREATE` path is pinned separately by the pre-existing 3 tests in `cypher_external_id.rs`, which still pass unchanged — proving the routing split at `engine/query_pipeline.rs:659-666`
- [x] 1.3 Pre/post-upgrade data split — **moot, resolved by §1.1.** There was no regression: `_id` was never populated on these paths on any version, so there is no "before 2.5.0 it worked" cohort to compare against. Nothing to localize and nothing to size

## 2. Fix the engine write path

Two design corrections found while mapping the fix, both confirmed:
**(a)** The deleted `write_ops.rs` MERGE arm called plain `create_node` too
(`v2.3.4:.../write_ops.rs:345-347`) — so MERGE `_id` is **new logic, not a
restoration**; only CREATE is restored. **(b)** Copy the in-tree, still-working
`Executor::resolve_external_id` (`executor/operators/create.rs:35-66`) and
`ast_conflict_policy_to_storage` (`create.rs:20-28`) rather than the deleted server
helpers — the in-tree pair already returns `crate::Error` instead of `String` and
lives in nexus-core; the server versions took `&request.params`, which no longer
exists (params are on `self.current_params`, `engine/mod.rs:204`).

- [x] 2.1 `crates/nexus-core/src/engine/write_exec.rs` CREATE arm (`:71`): read `create_clause.external_id_expr` + `.conflict_policy`, resolve via a local copy of `resolve_external_id` (literal or `$param` through `self.current_params`) and route to `Engine::create_node_with_external_id` (`engine/crud/nodes.rs:47`). The relationship-target node at `:96` must NOT re-consume the `_id` — mirror the `ext_id_consumed` guard at `operators/create.rs:110`
- [x] 2.2 `process_merge_clause` (`write_exec.rs:447-513`): `MergeClause` has **no** `conflict_policy` field (`parser/ast.rs:212-226`), so use `ConflictPolicy::Match` — correct for find-or-create and it closes the TOCTOU window. Match side: when `external_id_expr` is set, look up `catalog.external_id_index().get_internal(&txn, &ext)` (`catalog/external_id_index.rs:138`) BEFORE `find_nodes_by_node_pattern` (`:485`) — external id is a stronger key than the property filter, and the parser has already stripped `_id` from `node_pattern.properties`, so the existing match path is blind to it. Create side (`:499`) routes through `create_node_with_external_id`
- [x] 2.3 UNWIND+CREATE arm (`write_exec.rs:354`): same treatment as 2.1. Note UNWIND+MERGE delegates to `process_merge_clause`, so 2.2 fixes it automatically. **Gotcha:** every early return in this loop must run `self.unwind_bindings.clear()` — a new `?`-propagating resolution inside the loop body would skip it and leak the binding; resolve before entering the loop
- [x] 2.4 Verify error propagation: an invalid/unparseable `_id` must surface the `invalid _id` error (`executor/operators/create.rs:65`) rather than being silently dropped — silent-drop is what made this bug invisible
- [ ] 2.5 **Scope decision, do NOT implement unilaterally:** per-row `_id` from an unwound row (`UNWIND $rows AS row CREATE (n {_id: row.id})`) is currently a **parse error** — `extract_underscore_id_from_pattern` (`parser/clauses/mod.rs:38-45`) accepts only `Literal(String)` or `Parameter`, rejecting `PropertyAccess`. So batch ingest can today only carry one constant `_id` for every row, which collides on row 2. Widening the parser guard to resolve through `self.unwind_bindings` is a feature expansion beyond this bugfix; decide with the maintainer before doing it, and note that no known consumer needs it (Cortex does not write `_id` at all — §1.1)
- [ ] 2.6 `merge_single_node` (`write_exec.rs:526-563`, called from `process_merge_relationship:703`) takes only a `NodePattern` and so cannot see `external_id_expr`. Decide whether relationship-MERGE endpoints need `_id` support; if yes the signature must widen, if no document the limitation explicitly rather than leaving it silent

## 3. Close the test gap (this is why CI missed it)
- [x] 3.1 Restore automated `RETURN n._id` projection coverage that was deleted from `crates/nexus-core/tests/cypher_external_id.rs:48-53` — use an isolated per-test database instead of the process-wide shared catalog that made it flaky, so it stays in CI rather than being downgraded to manual validation again
- [x] 3.2 Add coverage for the write forms that were never tested: MERGE, CREATE+SET, and UNWIND batch ingest — the existing 3 tests all use bare standalone CREATE, the one path that never broke
- [x] 3.3 Add `WHERE n._id = …` filter coverage (same evaluator as projection, but assert it explicitly since it was a reported symptom)
- [ ] 3.4 Make the non-CI end-to-end coverage reachable: either wire `sdks/rust/tests/external_id_live.rs` into a CI job with a spawned server, or document explicitly why it stays manual

## 4. Backfill — premise weakened by §1.1, re-scope before building

§1.1 established there was no regression and that the one reported consumer (Cortex)
never wrote `_id` on any version. So the original premise — "deployments upgraded to
2.5.0 and silently lost ids they previously had" — is false as stated. Do not build a
backfill script on that assumption; establish first whether any real data is affected.

- [ ] 4.1 Determine whether ANY deployment actually wrote `_id` through the broken paths (MERGE / CREATE+SET / UNWIND). If none did, there is nothing to backfill and §4.2/§4.3 should be closed as not-applicable with that finding recorded — do not ship an unused script
- [ ] 4.2 Only if 4.1 finds affected data: ship a backfill under `scripts/` populating the external-id index from a caller-specified source property, with dry-run mode and a report of skipped/ambiguous nodes. Note it must synthesize the required prefix (`str:` etc.) since raw identity values are unprefixed and `ExternalId::from_str` rejects them
- [ ] 4.3 Only if 4.1 finds affected data: document the remediation procedure (who runs it, how to detect affected nodes)

## 5. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 5.1 Update or create documentation covering the implementation (`docs/specs/cypher-subset.md` § Reserved `_id`: state explicitly which write forms honour `_id` and the prefix requirement resolved in 1.1; CHANGELOG entry referencing issue #29)
- [ ] 5.2 Write tests covering the new behavior (sections 1.2 and 3 — projection, filtering, and all write forms, in CI without live-server dependency)
- [ ] 5.3 Run tests and confirm they pass (`cargo +nightly fmt --all`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo +nightly test --workspace`; confirm the issue #29 reproduction from 1.2 now passes and report the result on the GitHub issue)
