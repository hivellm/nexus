## 1. Implementation
- [ ] 1.1 Enumerate every `tracing::info!` / `debug!` call in `nexus-core/src/executor/` (use `rg -n 'tracing::(info|debug)!'` and tag each as hotpath vs. setup vs. useful-per-query)
- [ ] 1.2 Downgrade hotpath lines to `trace!`; consolidate series-of-lines (e.g. 8 CREATE messages) into one structured event
- [ ] 1.3 Add `#[tracing::instrument(skip_all, level = "debug")]` to `execute`, `execute_create`, `execute_expand` so callers can turn detail on without per-line clutter
- [ ] 1.4 Update the default `EnvFilter` in `nexus-server/src/main.rs` to suppress `hnsw_rs=info` (keep `warn` and above)
- [ ] 1.5 Grep for leftover `println!` in non-test code and remove/convert

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 2.1 Update `docs/users/operations/LOGS.md` describing the new default log levels and how to re-enable deep tracing
- [ ] 2.2 Add a smoke test asserting that the server boots with `RUST_LOG` unset and doesn't emit more than N log lines per successful query
- [ ] 2.3 Run `cargo test --workspace` and confirm pass
