# Proposal: phase7_bound-target-dir-size

Source: GitHub issue #24 (https://github.com/hivellm/nexus/issues/24)

## Why
Cargo never garbage-collects `target/`: stale object files, incremental
caches, and rlibs from old dependency versions accumulate indefinitely.
Combined with the dev profile's default full debuginfo for every
workspace crate AND every dependency, `target/` grows without bound — the
sibling `hivellm/cortex` repo reached 500+ GB before anyone noticed. This
repo has the same exposure. The biggest single size lever is debuginfo:
`debug = "line-tables-only"` keeps file:line in panics/backtraces while
slashing debuginfo size and speeding incremental rebuilds (Rust perf-team,
Kobzol 2025).

## What Changes
- `Cargo.toml` `[profile.dev]`: `debug = "line-tables-only"` (was
  `debug = true`). `[profile.release]` already has `strip = true`.
- `scripts/sweep-target.sh` + `scripts/sweep-target.ps1`: wrap
  `cargo-sweep` to remove artifacts not accessed in N days (default 14)
  without breaking incrementality (hot set stays). Auto-install
  cargo-sweep if missing; `--clean` flag for a full `cargo clean`.
- CI: `CARGO_INCREMENTAL=0` on the Rust workflows (rust-test, rust-lint,
  rust-bench) — CI starts cold, so incremental only adds artifacts and
  slows the build. (Workflow-file edits require a `workflow`-scoped push.)
- `docs/development/rust-target-hygiene.md`: short note explaining the
  levers, the sweep script, and a scheduled-job suggestion.

## Impact
- Affected specs: build / tooling (no runtime behavior)
- Affected code: `Cargo.toml`, `scripts/sweep-target.*`,
  `.github/workflows/rust-*.yml`, `docs/development/`
- Breaking change: NO (debuginfo still present as line tables — panics and
  backtraces keep file:line)
- User benefit: `target/` stays bounded; faster incremental rebuilds;
  smaller release binaries (already stripped); no manual disk babysitting.
