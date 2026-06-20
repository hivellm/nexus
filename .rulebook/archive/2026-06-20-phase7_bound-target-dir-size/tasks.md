## 1. Implementation
- [x] 1.1 Cargo.toml `[profile.dev]`: switch `debug = true` to `debug = "line-tables-only"` (release `strip = true` already present) — done; workspace `cargo check` clean under the new profile
- [x] 1.2 Add `scripts/sweep-target.sh` and `scripts/sweep-target.ps1` wrapping cargo-sweep (auto-install if missing, default 14-day retention, `--clean` for full reclaim) — both parse clean (`bash -n` / PowerShell Parser); .sh marked +x; `--help`/`-DryRun`/`--clean`/unknown-arg paths verified
- [x] 1.3 Add `CARGO_INCREMENTAL: 0` to the Rust CI workflow env blocks (rust-test, rust-lint, rust-bench) — all three YAMLs validated; rust-test already carried `CARGO_PROFILE_DEV_DEBUG=0`/`CARGO_PROFILE_TEST_DEBUG=0` from the prior CI fix

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 2.1 Update or create documentation covering the implementation — new `docs/development/rust-target-hygiene.md` covers the profile lever, both sweep scripts, the CI flag, and cron/Task-Scheduler automation; CHANGELOG `[Unreleased]` entry added (post-2.3.3 tooling, not part of the released 2.3.3 section)
- [x] 2.2 Write tests covering the new behavior — this is a build-config/tooling change with no Rust source, so behavior is verified rather than unit-tested: `rustc -C debuginfo=line-tables-only` on a panicking program shows `at .\bt_check.rs:1` / `:2` (file:line preserved); `bash -n` + PowerShell Parser validate both sweep scripts; the `--help`/`--dry-run`/`--clean`/unknown-arg paths were exercised; all three workflow YAMLs parse with `CARGO_INCREMENTAL=0`
- [x] 2.3 Run tests and confirm they pass — `cargo check --workspace` clean under the new dev profile; `nexus-protocol` full build clean; no Rust source changed so the test suite is unaffected (last full run 4716/0)
