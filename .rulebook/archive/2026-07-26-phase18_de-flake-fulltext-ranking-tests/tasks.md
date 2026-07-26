# Tasks: phase18_de-flake-fulltext-ranking-tests

`fulltext_ranking_regression.rs` fails ~1 run in 5 on Windows with Tantivy
`PermissionDenied` (OS error 5) on `.fast`/`.fieldnorm` segment files. A different
test fails each time and the unwrap is at `:66` inside the shared `seeded_registry()`
helper — so the fault is in shared setup, not in any assertion.

## 1. Diagnose the real cause
- [x] 1.1 Reproduced: ≈1-in-5 (measured ~17% / 5-in-30) on Windows, a different test failing each run, `PermissionDenied` (OS error 5) on `.fast`/`.fieldnorm`/`.pos` segment files through `seeded_registry()`
- [x] 1.2 Cause identified with evidence — NOT (a): each `seeded_registry()` call already gets its OWN `TempDir` (no shared directory). It is (b)-adjacent: a Tantivy writer/merge lifecycle race. Each `FullTextIndex` write opens a fresh `IndexWriter`; `commit()` can schedule a background merge that `IndexWriter::drop` does NOT wait for, so a surviving merge thread races a later call's `garbage_collect_files()` on segment files Tantivy's `MmapDirectory` opened without `FILE_SHARE_DELETE` — Windows-specific by construction
- [x] 1.3 Production-side: YES — the same race hits real Windows fulltext index writes/rebuilds, not just tests. The fix belongs in the registry's writer lifecycle (`index/fulltext.rs`), which is where it landed

## 1b. A second, unrelated flake (found 2026-07-19 during the phase7 gate)
- [x] 1b.1 Fixed: `hub::client::tests::from_env_disabled_when_url_missing` (+ the sibling `from_env_missing_key_is_an_error`, which mutates the same `HIVEHUB_*` vars) are now `#[serial]` (serial_test, already a dev-dep of nexus-server). 5/5 consecutive green. No retry paper-over

## 2. Fix the cause
- [x] 2.1 Applied: `index_writer.wait_merging_threads()?` after each commit in `FullTextIndex` (so a generation's merges settle before its writer drops) + a bounded exponential-backoff retry (5 attempts, 10 ms→160 ms, mirroring `TempDirGuard::drop`) that absorbs the residual async Windows mmap-handle-release window
- [x] 2.2 Production lifecycle defect (from 1.3) fixed in the same place (`index/fulltext.rs`) — benefits real Windows index writes/rebuilds, not just the tests
- [x] 2.3 No paper-over: no `--test-threads=1`, no `#[ignore]`, no blind retry loop. The retry is scoped to ONLY the Windows-lock error class (`PermissionDenied` inside a Tantivy `Open{Read,Write}Error`) with a clear comment + precedent citation — a genuine error still propagates on the first attempt

## 3. Tail (docs + tests — check or waive with tailWaiver)
- [x] 3.1 Update or create documentation covering the implementation — CHANGELOG `[3.0.0]` `Fixed` entry (the Windows Tantivy merge/GC lifecycle race + the wait_merging_threads/retry-backoff fix + the `#[serial]` env-test fix, flagged as a production robustness fix); plus a comment on `seeded_registry()` recording that the historical flake was a production lifecycle bug, not a test-isolation issue, so it isn't "re-fixed" wrongly later
- [x] 3.2 Write tests covering the new behavior — no new assertions needed; the deliverable is the existing fulltext regression suite (ranking/crash-recovery/async-writer) becoming deterministic. The production lifecycle fix is exercised by those same tests now passing reliably under parallelism
- [x] 3.3 Run tests and confirm they pass — 25 consecutive green runs of the compiled `fulltext` group binary (built once, looped — was ≈1-in-5), full `cargo +nightly test --workspace` green (all binaries 0 failed), `cargo +nightly clippy --workspace --all-targets --all-features -- -D warnings` clean, `cargo +nightly fmt --all -- --check` clean
