# Proposal: phase14_fix-external-id-write-path

Closes: https://github.com/hivellm/nexus/issues/29
**Priority: HIGH — live regression on 2.5.0 breaking a downstream consumer (Cortex).**
Should be executed before the Thunder migration (phase10–13).

## Why

On 2.5.0, `MATCH (n:Turn) RETURN n._id` projects `null` for every node of every
label, and `WHERE n._id = '...'` matches nothing. It worked on 2.3.4. Cortex built
its dashboard graph view, communities surface, and path/compare endpoints on the
reserved `_id` slot (their decision 004) and had to work around it with
`coalesce(n._id, n.id, n.natural_key, n.path)`.

**Root cause — CONFIRMED, and it is a write-side data bug, not a read-side
projection bug.** The read path is intact: `evaluate_projection_expression`
(`crates/nexus-core/src/executor/eval/projection/core.rs:86-105`) still special-cases
`_id` and looks it up in the catalog's external-id index. It returns `Null` because
the nodes genuinely have no external id recorded. Both symptoms share this one cause,
because projection and WHERE go through the same evaluator (`core.rs:3-5`).

The parser strips `_id` out of the property map and hoists it into
`CreateClause.external_id_expr` / `MergeClause.external_id_expr`
(`executor/parser/clauses/write.rs:14` and `:87`, `executor/types.rs:344-349`). In
2.3.4 the HTTP write path consumed that field via the server-side fork
`crates/nexus-server/src/api/cypher/execute/write_ops.rs`, which resolved it and
called `create_node_with_external_id`. Commit **`99660fb4`**
(`feat(server)!: route HTTP CREATE/MERGE through the engine write path`) removed that
dispatch, and follow-up **`2362681d`** deleted the fork entirely. The receiving path,
`crates/nexus-core/src/engine/write_exec.rs`, **never reads `external_id_expr`**
(`grep -n external_id` on that file returns zero hits) — it only walks
`node.properties`, from which the parser has already removed `_id`. The value is
silently discarded, so it lands in neither the index nor the property bag.

**Which queries break** (routing order at `engine/query_pipeline.rs:659-666` — the
write-path branch is tested first):
- Broken: `MERGE`, any `CREATE … SET`, `CREATE … REMOVE`, `FOREACH`, and **all
  `UNWIND $rows … MERGE/SET` batch ingest** → `write_exec.rs` → `_id` dropped.
- Still working: bare standalone `CREATE (n {_id: …})` with no SET/MERGE, which
  routes to the executor (`executor/dispatch.rs:279-290`, `:594-605`).

**Why CI missed it — a structural test gap.** `crates/nexus-core/tests/cypher_external_id.rs`
has 3 tests that all use bare standalone `CREATE` — exactly the one path still
working. The `RETURN n._id` projection assertion was deliberately **removed** from
that suite (`cypher_external_id.rs:48-53`) as flaky against the process-wide shared
catalog and downgraded to "manual validation". The parser tests
(`executor/parser/tests/external_ids.rs`, 6 tests) only prove extraction into
`external_id_expr` — nobody tests that a consumer uses it. The only end-to-end
coverage (`sdks/rust/tests/external_id_live.rs:271,292`) requires a live server and
is not in CI. **No test in the default CI suite covers `_id` projection or filtering.**

## What Changes

- **Open question to resolve first (blocks the fix design):** the issue's example
  shows `n.id = "01KQR4RM6HKH2YM15NCH2EEBWP"`, a bare ULID, but `ExternalId::from_str`
  requires a `sha256:` / `sha512:` / `uuid:` / `str:` prefix
  (`docs/specs/cypher-subset.md:435-444`, `executor/operators/create.rs:64`).
  HYPOTHESIS: if Cortex writes unprefixed ids, the repaired path would reject them
  with `invalid _id` rather than succeed — meaning 2.3.4 may have been rejecting
  those writes too and the working `_id` values came from a different route. This
  determines whether the fix is "restore the plumbing" or "restore the plumbing AND
  reconcile the id format", so it is checklist item 1.1, not an assumption.
- **The fix**: thread `external_id_expr` + `conflict_policy` through
  `engine/write_exec.rs` for all three write arms (CREATE at `:61-78`,
  `process_merge_clause` at `:447`, and the UNWIND path via
  `execute_unwind_write_query` at `:36-42`), routing to
  `Engine::create_node_with_external_id` (`engine/crud/nodes.rs:47`) — that helper is
  intact and still used by the REST `/data/nodes` endpoint, so this is reconnecting
  existing plumbing, not new machinery.
- **Close the test gap** so this cannot regress silently again: restore automated
  `_id` projection + WHERE-filter coverage in a form that does not collide with the
  shared catalog (isolated per-test database rather than deletion of the assertion),
  and cover the MERGE / CREATE+SET / UNWIND-batch paths that were never tested.
- **Backfill**: nodes written under 2.5.0 have no external id recoverable from the
  graph (the parser stripped `_id` before it reached the property store). Deliver a
  documented backfill path for affected deployments. Cortex's `coalesce` workaround
  suggests the source values survive in `n.id` / `natural_key`, which would make
  backfill feasible — to be confirmed, not assumed.

## Impact

- Affected specs: `docs/specs/cypher-subset.md` (§ Reserved `_id` Property — clarify
  which write forms honour it, and the prefix requirement)
- Affected code: `crates/nexus-core/src/engine/write_exec.rs` (primary),
  `crates/nexus-core/tests/cypher_external_id.rs`, possibly
  `crates/nexus-core/src/engine/query_pipeline.rs` (routing), backfill script under
  `scripts/`
- Breaking change: NO — restores documented 2.3.4 behaviour. (If item 1.1 concludes
  the id format must also be reconciled, any format change IS user-visible and must
  be called out separately.)
- User benefit: `_id` works again on every write form; downstream consumers can drop
  their `coalesce` workarounds; the reserved slot regains automated CI protection.

## References

- Issue: https://github.com/hivellm/nexus/issues/29
- Suspect commits: `99660fb4` (routed HTTP writes through the engine path),
  `2362681d` (deleted the working `write_ops.rs` fork)
- Read path (correct, do not change): `executor/eval/projection/core.rs:86-105`
- Parser hoist: `executor/parser/clauses/write.rs:14,:87`, `executor/types.rs:344-349`
- Intact helper to route to: `engine/crud/nodes.rs:47`
- Catalog index: `crates/nexus-core/src/catalog/extensions.rs:125`
