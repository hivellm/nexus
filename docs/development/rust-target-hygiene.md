# Rust `target/` hygiene

Cargo never garbage-collects `target/`. Stale object files, incremental
caches, and compiled rlibs from **old dependency versions** accumulate
indefinitely until something deletes them. Combined with the dev profile's
default **full debuginfo** for every workspace crate *and* every
dependency, `target/` grows without bound — the sibling `hivellm/cortex`
repo reached **500+ GB** (half a 1 TB SSD) before anyone noticed. This page
documents how we keep `target/` bounded here (GH #24).

## 1. Profile settings (the biggest size lever)

`Cargo.toml`:

```toml
[profile.dev]
debug = "line-tables-only"   # file:line in panics/backtraces, no bulky
                             # variable/type debuginfo; ~30-40% faster
                             # incremental rebuilds

[profile.release]
strip = true                 # drop the residual symbol table
```

`line-tables-only` keeps exactly the debuginfo a panic backtrace needs
(file and line numbers) and discards the per-variable/per-type DWARF that
dominates `target/` size. Backtraces still point at the right source line.

## 2. Sweeping stale artifacts

[`cargo-sweep`](https://github.com/holmgr/cargo-sweep) removes artifacts not
**accessed** in the last N days, so the hot set you're actively rebuilding
survives and incrementality is preserved.

```bash
# Linux / macOS / Git Bash
scripts/sweep-target.sh              # sweep artifacts older than 14 days
scripts/sweep-target.sh --time 30    # custom retention window
scripts/sweep-target.sh --dry-run    # preview, delete nothing
scripts/sweep-target.sh --clean      # full `cargo clean` (cold next build)
```

```powershell
# Windows PowerShell
scripts\sweep-target.ps1
scripts\sweep-target.ps1 -Days 30
scripts\sweep-target.ps1 -DryRun
scripts\sweep-target.ps1 -Clean
```

Both scripts auto-install `cargo-sweep` (`cargo install cargo-sweep`) on
first run if it isn't on `PATH`.

## 3. CI: no incremental compilation

CI runners start from a cold cache, so incremental compilation only writes
extra artifacts and slows the build. The Rust workflows
(`rust-test`, `rust-lint`, `rust-bench`) set `CARGO_INCREMENTAL: 0`; the
test workflow additionally drops dev/test debuginfo
(`CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`) so the hosted
runner's linker doesn't exhaust memory.

## 4. Keeping it bounded automatically (optional)

Run the sweep on a schedule so `target/` never balloons:

- **Linux / macOS** — cron, e.g. weekly:
  ```cron
  0 3 * * 1 cd /path/to/nexus && scripts/sweep-target.sh >/dev/null 2>&1
  ```
- **Windows** — Task Scheduler running `pwsh -File scripts\sweep-target.ps1`
  on a weekly trigger.

## When to reach for `cargo clean`

Use `--clean` (full `cargo clean`) only when you want to reclaim
*everything* and accept a cold rebuild — e.g. after a toolchain bump or when
disk is critically low. For routine maintenance, the time-based sweep is
strictly better because it keeps the incremental hot set.

## References

- [Disable/limit debuginfo to shrink `target/`](https://kobzol.github.io/rust/rustc/2025/05/20/disable-debuginfo-to-improve-rust-compile-times.html) (Rust perf-team, Kobzol 2025)
- [`cargo-sweep`](https://github.com/holmgr/cargo-sweep)
- [Cargo profiles reference](https://doc.rust-lang.org/cargo/reference/profiles.html)
