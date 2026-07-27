# Proposal: phase18_de-flake-fulltext-ranking-tests

**Found while running the phase14 quality gate. Pre-existing, unrelated to that fix.**

## Why

`crates/nexus-core/tests/fulltext_ranking_regression.rs` fails intermittently on
Windows — measured at roughly 1 failure in 5 full-suite runs, with a *different* test
failing each time. Observed failures include `phrase_query_pins_exact_match`,
`vector_family_dominates_vector_query`, and `limit_respected_on_dense_matches`.

Root cause is not ranking logic — it is Tantivy index file locking:

```
panicked at crates\nexus-core\tests\fulltext_ranking_regression.rs:66:57:
called `Result::unwrap()` on an `Err` value: Tantivy(OpenWriteError(IoError {
  io_error: Os { code: 5, kind: PermissionDenied, message: "Acesso negado." },
  filepath: "39f9adfdc0f04beebf93d98bae8560f7.fast" }))
```

Windows error 5 (`PermissionDenied`) on `.fast` / `.fieldnorm` segment files means
concurrent test threads are contending for Tantivy index files, or a writer is not
released before the next test opens the same path. The varying test name and the
`:66` unwrap in the shared `seeded_registry()` helper both point at the shared setup,
not at any individual assertion.

This matters beyond the annoyance: phase12 and phase13 both gate on a fully green
suite (interop matrix and the release train's `gate` job). A test that fails 20-40% of
the time will randomly block releases and train people to re-run until green, which is
how genuine regressions get waved through.

The repo has precedent for treating this seriously — commit `e12b8590`
*"test(core): de-flake row-lock concurrency tests"*.

## What Changes

Diagnose whether the contention is (a) tests sharing an index directory, (b) a
`TempDir` being dropped while a Tantivy writer still holds handles, or (c) genuine
parallel-writer contention within `seeded_registry()`. Then fix the cause rather than
the symptom.

Do **not** paper over it with `--test-threads=1`, `#[ignore]`, or a retry loop. Those
hide the defect, and if the underlying handle-release problem is real it may affect
production index rebuilds on Windows too — which is worth determining explicitly.

## Impact

- Affected specs: none
- Affected code: `crates/nexus-core/tests/fulltext_ranking_regression.rs` (the
  `seeded_registry()` helper at ~`:66`); possibly the fulltext registry's writer
  lifecycle if the handle-release theory holds
- Breaking change: NO
- User benefit: a trustworthy test suite; the phase12/phase13 release gates stop
  failing at random.

## References

- Observed 2026-07-19 on branch `release/2.6.0`, Windows 10, `cargo +nightly test --workspace`
- Failure detail quoted above; the file's only commit is `f619e1f5`
- Precedent for de-flaking work: `e12b8590`
