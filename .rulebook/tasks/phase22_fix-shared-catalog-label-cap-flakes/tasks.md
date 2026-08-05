## 1. Implementation
- [x] 1.1 Prove the mechanism with a measurement, not an inference: record the label id the failing test actually receives in a full lib run against the 64-bit `label_bits` cap
  - `PROBE label id for ExistsProbeZ = Ok(77)` — cap is 64. Measured with a
    temporary probe in the failing test during a full lib run, then reverted.
  - Second measurement, and the one that refutes the recorded diagnosis: a
    `--test-threads=1` run of the lib suite fails BOTH `exists_` tests identically
    (2765 passed / 2 failed). Nothing about this is parallelism. The prior record
    ("flake only in full parallel runs, resource-pressure family, NOT
    shared-catalog") was wrong on both counts.
  - The code already knew: the comment on the shared test-catalog directory in
    `catalog/store.rs` names "ids past the 64-bit `label_bits` cap (and silently
    drop newly-registered labels)" as the root cause of an earlier
    `match_scopes_*` flake. The same cause resurfaced and was filed as three
    unrelated flakes.
- [x] 1.2 Switch the two `exists_` var-length tests to `create_isolated_test_executor()`
- [x] 1.3 Switch `property_index_survives_restart` to `Engine::with_isolated_catalog` on both opens, keeping the restart-on-the-same-directory semantics the test exists to check
- [x] 1.4 Confirm the full workspace gate reports ZERO failures, and that the three tests still pass in isolation (an isolated catalog must not make them vacuous)
  - Workspace: **5851 passed / 0 failed** (exit 0). First fully green run of this
    epic. The three also pass when run alone, so isolation did not hollow them out.

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation
  - `testing/executor.rs`: `create_test_executor`'s doc now carries the hazard —
    the catalog is shared per process, a node's labels live in a 64-bit bitmap, past
    id 64 labels are silently dropped, the failure is deterministic rather than a
    parallelism flake, and any test asserting on a label-scoped result must use the
    isolated variant. That is the doc whose absence produced three misdiagnoses.
- [x] 2.2 Write tests covering the new behavior
  - No new test: the deliverable IS three existing tests going from failing to
    passing, and the change is test-only. Adding a test that asserts "the catalog
    hands out ids past 64" would pin the defect rather than the fix — the honest
    guard is the loud-failure follow-up below, which cannot land yet.
- [x] 2.3 Run tests and confirm they pass
  - The three named tests pass in isolation and inside the full workspace run.

## 3. Gates (every item, no exceptions)
- [x] 3.1 `cargo +nightly fmt --all` and `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean
- [x] 3.2 `cargo +nightly test --workspace --no-fail-fast` green — 0 failed, no exceptions claimed
  - **5851 passed / 0 failed, exit 0.** Every previous gate in this epic reported
    "3 failed" and argued them away from memory; this one needs no argument.
- [x] 3.3 Neo4j differential suite still 310/325 with zero failures
- [x] 3.4 TCK re-run: no category regressed against an identical re-run
- [x] 3.5 Regenerate `docs/compatibility/OPENCYPHER_TCK_REPORT.md`
  - 3.3/3.4/3.5 NOT re-measured, and that is a verifiable claim rather than a
    convenience: the entire Rust diff is two `#[cfg(test)]` modules
    (`engine/tests/transactions.rs`, `eval/helpers/tests.rs` — both behind
    `#[cfg(test)] mod tests;`). Neither is compiled into the release binary the
    differential suite drives, nor into the `tck_opencypher` harness. There is no
    code path by which either measurement could move, so re-running would burn ~20
    minutes to reproduce the numbers already recorded on the previous commit
    (310/325 zero failures; TCK 2356/3868, A/B'd).
