## 1. Implementation
- [ ] 1.1 Remove `all = "allow"` under `[lints.clippy]` from `nexus-server/Cargo.toml`, `nexus-core/Cargo.toml`, `nexus-cli/Cargo.toml`, `nexus-protocol/Cargo.toml`
- [ ] 1.2 Remove `unused = "allow"` and `dead_code = "allow"` from the `[lints.rust]` tables
- [ ] 1.3 Run `cargo clippy --workspace --all-targets -- -D warnings` and triage the new failures
- [ ] 1.4 For each triaged warning: fix it, or add `#[allow(...)]` with a short comment justifying the exception
- [ ] 1.5 Remove the `unused manifest key: example.1.num_cpus` warning from root `Cargo.toml`

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 2.1 Update `AGENTS.md` / contributor docs mentioning that clippy is now strict
- [ ] 2.2 Ensure CI step `cargo clippy --workspace -- -D warnings` still passes after the cleanup — regression test is CI itself
- [ ] 2.3 Run `cargo test --workspace` and confirm nothing breaks from the fallout cleanups
