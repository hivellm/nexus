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
- [ ] 1.1 Establish whether `/ingest` can be made meaningfully faster than `UNWIND` over `/cypher`. The theoretical win is skipping per-row parse and plan entirely by going straight to the write path — confirm that is reachable, since if the best case merely matches `/cypher`, the endpoint has no reason to exist
- [ ] 1.2 Survey the callers before changing the contract: the six SDKs under `sdks/`, `scripts/`, and any docs or examples that POST to `/ingest`. Record which of them pass the inert `id` field, since those callers are silently broken today and must be told
- [ ] 1.3 Write the decision down in the proposal with its rationale — "make it work" or "retire it as a shim over UNWIND". Everything after this depends on it

## 2. Remove the traps (required either way)
- [ ] 2.1 Delete `NodeIngest.id`, or honour it. Do not leave a field that a client can set with no effect. If honouring it, define what happens when the id already exists and when only some rows carry one
- [ ] 2.2 Make node and relationship ingestion compose: return the created node ids in `IngestResponse` (in input order), or accept relationship endpoints by a client-supplied key rather than internal id. Without one of these, a caller can only ever ingest disconnected nodes
- [ ] 2.3 Verify the identifier validation at `:300` and `:345` still covers every path after the restructure — it is what stops a crafted label or relationship type escaping the generated Cypher, and a rewrite that bypasses string building must not quietly drop it

## 3. Implement the §1.3 decision
- [ ] 3.1 If fixing: batch each chunk into a single parameterized execution instead of one query per row, and acquire `server.engine.write()` once per batch rather than once per row (`:191-210`, `:328-333`)
- [ ] 3.2 If fixing: re-measure against the same 5 000-node benchmark and record the numbers next to the 469 / 5 097 nodes/s baseline. A fix that does not beat `UNWIND` means §1.1 was answered wrong — say so rather than shipping it
- [ ] 3.3 If retiring: reimplement `/ingest` as a thin shim over the `UNWIND` path so existing callers keep working, and mark it deprecated in the response and the docs
- [ ] 3.4 Confirm the transaction semantics are honest either way — today a batch issues `BEGIN TRANSACTION` and then continues past per-row failures (`:205-209`), so a partial batch can commit. Define and document whether a batch is atomic

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 4.1 Update or create documentation covering the implementation (`docs/specs/api-protocols.md`; the endpoint descriptions in `nexus-server/src/lib.rs:6` and `main.rs:6`; the bulk-loading guidance in `CLAUDE.md`; CHANGELOG entry covering the removed `id` field and the new response shape)
- [ ] 4.2 Write tests covering the new behavior: nodes and their relationships ingested in one flow and verified connected (the case that is impossible today), plus a test asserting the batch atomicity decided in §3.4
- [ ] 4.3 Run tests and confirm they pass (`cargo +nightly fmt --all`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo +nightly test --workspace` green)

## Related
- Discovered by `phase7_ldbc-snb-benchmark` item 1.3, whose wording specifies loading
  "via `/ingest`". Until this ships the loader must use `UNWIND` over `/cypher`, and
  that item's text needs updating to match reality.
- `phase0_fix-cypher-oom-process-abort` is the other half of the same investigation:
  the natural `UNWIND` + multi-pattern `MATCH` shape for loading edges by LDBC id
  currently aborts the server process.
