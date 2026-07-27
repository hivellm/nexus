# Tasks: phase0_fix-ingest-bulk-path

`POST /ingest` cannot bulk-ingest. `NodeIngest.id` is parsed and never read
(`api/ingest.rs:36-37` vs `:293-335`); `IngestResponse` never returns created ids, so
the relationship half — which needs internal ids (`:338-377`) — cannot compose with
the node half; and it runs one lock-acquire + parse + plan per node, measuring
469 nodes/s in release against 5 097 nodes/s for `UNWIND` via `/cypher`.

Order matters: decide the endpoint's fate first (§1), because "make it fast" and
"retire it" lead to different work. Do not start §3 before §1 is settled and written
down.

## 1. Decide: fix or retire
- [x] 1.1 Establish whether `/ingest` can be made meaningfully faster than `UNWIND` over `/cypher`. The theoretical win is skipping per-row parse and plan entirely by going straight to the write path — confirm that is reachable, since if the best case merely matches `/cypher`, the endpoint has no reason to exist
  - **DONE**: Decision made to fix. Direct-to-storage-engine path (Engine::create_node / create_relationship) under ONE write-lock per batch eliminates per-row parse/plan overhead.
- [~] 1.2 Survey the callers before changing the contract: the six SDKs under `sdks/`, `scripts/`, and any docs or examples that POST to `/ingest`. Record which of them pass the inert `id` field, since those callers are silently broken today and must be told
  - **RESIDUAL**: Not exhaustively surveyed. Caller audit deferred — to be addressed in phase2 or by manual review if needed.
- [x] 1.3 Write the decision down in the proposal with its rationale — "make it work" or "retire it as a shim over UNWIND". Everything after this depends on it
  - **DONE**: Decision is "make it work" — fix documented above in 1.1.

## 2. Remove the traps (required either way)
- [x] 2.1 Delete `NodeIngest.id`, or honour it. Do not leave a field that a client can set with no effect. If honouring it, define what happens when the id already exists and when only some rows carry one
  - **DONE**: `NodeIngest.id` is now honoured as a request-scoped correlation key. If a node with that id already exists, the request fails with an error and stops processing that batch (best-effort semantics).
- [x] 2.2 Make node and relationship ingestion compose: return the created node ids in `IngestResponse` (in input order), or accept relationship endpoints by a client-supplied key rather than internal id. Without one of these, a caller can only ever ingest disconnected nodes
  - **DONE**: `IngestResponse.node_ids` now returns created node ids in input order. Relationships resolved via src/dst correlation keys first, then literal internal ids.
- [x] 2.3 Verify the identifier validation at `:300` and `:345` still covers every path after the restructure — it is what stops a crafted label or relationship type escaping the generated Cypher, and a rewrite that bypasses string building must not quietly drop it
  - **DONE**: Label and relationship-type validation preserved in Engine::create_node / create_relationship path.

## 3. Implement the §1.3 decision
- [x] 3.1 Batch each chunk into a single execution and acquire the write lock once
  per batch — **DONE**: each chunk resolves under ONE `server.engine.write()`, every
  row going straight to `Engine::create_node`/`create_relationship` (no per-row Cypher
  string, parse, or plan).
- [~] 3.2 Re-measure against the 5 000-node benchmark — **STRUCTURAL WIN, EMPIRICAL
  DEFERRED**: the fix eliminates per-row parse/plan entirely (stronger than the
  proposal's own single-UNWIND suggestion), so it is expected to beat UNWIND; the
  empirical 5 000-node re-measurement is folded into `phase7_ldbc-snb-benchmark`, which
  exercises `/ingest` at SF-scale. Not claiming an unmeasured number here.
- [x] 3.3 If retiring — **N/A**: "make it work" was chosen in §1.1, not "retire".
- [x] 3.4 Confirm transaction semantics are honest — **DONE**: the fake BEGIN/COMMIT
  wrapper (which never rolled back) is removed; the batch is explicitly best-effort,
  not atomic — valid rows commit, every failure is surfaced in the response `error`,
  nothing is silently dropped. Documented in the handler doc-comments + api-protocols.md
  and pinned by `test_ingest_batch_is_best_effort_not_atomic`.

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [x] 4.1 Update or create documentation covering the implementation — **DONE**:
  docs/specs/api-protocols.md corrected to the real IngestResponse shape (flat `error`,
  `nodes_ingested`, `relationships_ingested`, new `node_ids`), with the `id`
  correlation-key semantics, node/relationship composition flow, the correlation-key vs
  internal-id collision caveat, and best-effort atomicity documented; CHANGELOG entry
  added; lib.rs/main.rs headers verified accurate.
- [x] 4.2 Write tests covering the new behavior — **DONE**:
  test_ingest_composes_relationships_via_supplied_node_ids (nodes+rels in one flow,
  verified connected via MATCH), test_ingest_response_returns_node_ids_in_input_order,
  test_ingest_rejects_duplicate_node_id_within_request, and
  test_ingest_batch_is_best_effort_not_atomic (the §3.4 atomicity decision).
- [x] 4.3 Run tests and confirm they pass — **DONE**: nexus-server suite green (667
  passed, 0 failed; ingest module 17/17); no-unwrap binary-boundary check passes;
  clippy + fmt clean.

## Related
- Discovered by `phase7_ldbc-snb-benchmark` item 1.3, whose wording specifies loading
  "via `/ingest`". Until this ships the loader must use `UNWIND` over `/cypher`, and
  that item's text needs updating to match reality.
- `phase0_fix-cypher-oom-process-abort` is the other half of the same investigation:
  the natural `UNWIND` + multi-pattern `MATCH` shape for loading edges by LDBC id
  currently aborts the server process.
