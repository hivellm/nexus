# Tasks: phase18_de-flake-fulltext-ranking-tests

`fulltext_ranking_regression.rs` fails ~1 run in 5 on Windows with Tantivy
`PermissionDenied` (OS error 5) on `.fast`/`.fieldnorm` segment files. A different
test fails each time and the unwrap is at `:66` inside the shared `seeded_registry()`
helper — so the fault is in shared setup, not in any assertion.

## 1. Diagnose the real cause
- [ ] 1.1 Reproduce reliably: run the file in a loop (`for i in $(seq 20)`) and record which tests fail and at what rate, with `RUST_BACKTRACE=1` to get the full stack through `seeded_registry()` (`:66`)
- [ ] 1.2 Determine which of the three candidate causes holds: (a) tests sharing one index directory, (b) a `TempDir` dropped while a Tantivy writer still holds open handles (Windows refuses deletion/reopen of mapped files, unlike Linux — which is why this is OS-specific), or (c) genuine parallel-writer contention inside the helper. Confirm with evidence, do not pick by plausibility
- [ ] 1.3 Establish whether the same handle-release problem can affect production index rebuilds on Windows, or whether it is strictly a test-harness artifact — this decides if the fix belongs in the test or in the fulltext registry's writer lifecycle

## 2. Fix the cause
- [ ] 2.1 Apply the fix indicated by 1.2 — per-test isolated index directory, explicit writer commit+drop before the directory goes out of scope, or serialized access to the shared index, as the diagnosis dictates
- [ ] 2.2 If 1.3 found a production-side lifecycle defect, fix that too (or file it separately with justification if it is genuinely out of scope)
- [ ] 2.3 Do NOT paper over it with `--test-threads=1`, `#[ignore]`, or a retry loop; those hide the defect and, if the handle-release theory is right, hide a real Windows bug

## 3. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 3.1 Update or create documentation covering the implementation (a comment in the test file explaining the isolation requirement so it is not undone later; CHANGELOG entry only if a production-side lifecycle fix landed)
- [ ] 3.2 Write tests covering the new behavior (no new assertions needed — the deliverable is the existing 7 tests becoming deterministic; if a production lifecycle bug was found in 1.3, that needs its own test)
- [ ] 3.3 Run tests and confirm they pass (20 consecutive green runs of `cargo +nightly test -p nexus-core --test fulltext_ranking_regression`, plus a full `cargo +nightly test --workspace` green; a single green run does not close a 1-in-5 flake)
