## 1. Investigation
- [x] 1.1 Confirm the single-transaction materialization at mod.rs:4998 (`_batch_size` unused, `all_results.extend` before one commit) and pick a sensible result-count cap / config knob — confirmed (code now lives in `engine/ddl.rs` after the phase5 split); cap = 1M rows with `NEXUS_CALL_IN_TX_MAX_ROWS` env knob
- [x] 1.2 Define the structured error (`ERR_CALL_IN_TX_RESULT_TOO_LARGE`) and where to surface it — defined in `check_call_in_tx_result_cap` (engine/ddl.rs), surfaced as `Error::CypherExecution`

## 2. Implementation
- [x] 2.1 Cap the materialized subquery result count and return the structured error past the cap (no silent OOM); document `OF n ROWS` granularity as not-yet-implemented — cap shipped in f42a2ae6, hardened here (env knob + extracted testable check). NOTE: the "not-yet-implemented" premise was stale — `OF n ROWS` per-batch commit IS implemented for top-level queries via the executor operator (`run_call_subquery_in_transactions`, landed 2026-04-26); only the legacy engine path (internally dispatched ASTs) materializes in one transaction, which is what the cap guards. Documented truthfully in CHANGELOG.
- [x] 2.2 Ensure the error is a clean rollback (no partial commit) and is returned over REST as an execution error — call site aborts the wrapper write transaction before returning; `Error::CypherExecution` maps to a standard REST execution error

## 3. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 3.1 Update or create documentation (CHANGELOG / GH #22) — CHANGELOG [Unreleased] entry with the corrected `OF n ROWS` status
- [x] 3.2 Write tests: a CALL IN TRANSACTIONS subquery exceeding the cap returns the structured error and commits nothing; under-cap still works — `call_in_tx_result_cap_returns_structured_error_past_cap` (default cap boundary, env-knob override, structured error); under-cap top-level behavior covered by `call_in_transactions_terminates`. The legacy path cannot be driven end-to-end from the public API (its inner read-subquery execution is broken upstream of the cap by the `query_to_string` Debug-format reconstruction — pre-existing, out of scope), so the cap check is unit-tested at its seam and the abort is enforced at the call site.
- [x] 3.3 Run tests and confirm they pass — `cargo test -p nexus-core --lib engine::tests::transactions` 7/7; clippy clean; fmt applied
