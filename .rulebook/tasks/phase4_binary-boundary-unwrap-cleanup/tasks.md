## 1. Implementation
- [ ] 1.1 Run `rg -n '\.unwrap\(\)|\.expect\(' nexus-cli/src nexus-server/src` and catalogue each site with a verdict (fix / keep-with-comment / remove)
- [ ] 1.2 Replace actionable `.unwrap()` in `nexus-cli/src/commands/mod.rs:31` with `?` + `anyhow::Context`
- [ ] 1.3 Fix `let _ = std::fs::create_dir_all(parent)` at query.rs:33 to log or propagate on failure
- [ ] 1.4 Fix `let _ = rl.load_history(...)` at query.rs:307 to `tracing::debug!` on absence, `warn!` on read error
- [ ] 1.5 Replace obvious `.unwrap()` on `SystemTime` arithmetic with `.unwrap_or(Duration::ZERO)` + comment (clock going backwards is the only failure mode)

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 2.1 Update `AGENTS.md` section on error handling with the pattern used
- [ ] 2.2 Add a `scripts/ci/check_no_unwrap_in_bin.sh` that fails CI if new `.unwrap()` appears in `*/src/main.rs` or `*/src/commands/`
- [ ] 2.3 Run `cargo test --workspace` + `cargo clippy --workspace -- -D warnings` and confirm clean
