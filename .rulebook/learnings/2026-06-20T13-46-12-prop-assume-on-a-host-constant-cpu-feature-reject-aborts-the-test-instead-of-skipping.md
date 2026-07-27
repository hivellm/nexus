# prop_assume! on a host-constant (CPU feature) reject-aborts the test instead of skipping
**Source**: manual
**Date**: 2026-06-20
**Related Task**: phase7_bound-target-dir-size
**Tags**: proptest, prop_assume, ci, simd, cpu-features, avx, flaky, reject-abort, test-skip
CI's Rust Tests failed on ~38 SIMD parity tests with `Test aborted: Too many global rejects (global rejects: 1024)`. Root cause: the feature-gated proptests used a PER-CASE `prop_assume!(cpu().avx2)` / `prop_assume!(cpu().avx512f)` etc. On a runner whose CPU lacks the feature (GitHub ubuntu-latest exposes no AVX2/AVX-512), the assume condition is false for EVERY generated case, so proptest rejects all of them and aborts the whole test as a FAILURE — not a skip. Locally it passed only because the dev CPU has the features.

Key insight: `prop_assume!` is for filtering RANDOM inputs that happen to be invalid (a small fraction). It is the WRONG tool for a condition that is CONSTANT across the whole run (host CPU features, env presence, OS). A constant-false assume rejects 100% of cases → abort.

Fix: gate once, not per case. Replace `prop_assume!(cpu().X);` with `if !cpu().X { return Ok(()); }` inside the proptest body — the case passes trivially (clean skip) when the host lacks the feature, and runs the real assertions when present. (Even cleaner: hoist the check to a wrapping `#[test] fn` that early-returns before invoking `proptest!`, so you don't generate 128 throwaway cases.)

This bug was latent for months because the nextest list-phase hang (--all-targets pulling in harness=false criterion benches) meant ZERO tests ever ran on CI. Fixing the hang surfaced every CPU-feature-sensitive test at once. General lesson: when a CI test job has been red/hung for a long time, fixing the blocker often reveals a backlog of latent failures the blocker was masking — budget for a cascade, not a single fix.