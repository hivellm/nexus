# Server env-var override for an `ExecutorConfig` knob

**Category:** rust / server-binary-boundary
**Tags:** nexus-server, nexus-core, executor-config, env-var, binary-boundary-error-handling

## Description

`Executor::default()` bakes in `ExecutorConfig::default()`. To let an
operator override a single config knob (e.g.
`cartesian_product_max_bytes`) without exposing the whole
`ExecutorConfig` struct or plumbing a YAML/config-file path, add:

1. A narrow `pub fn set_<knob>(&mut self, value: T)` mutator on
   `Executor` in `crates/nexus-core/src/executor/engine.rs`, placed as
   a sibling right after the existing `set_columnar_threshold`
   pattern. Doc comment explains *why* it's public (server env-var
   override) and links the task.
2. Read the env var in the ONE canonical executor-construction site —
   `build_executor()` in `crates/nexus-server/src/api/cypher/mod.rs`
   — not at every `Executor::default()` call site (tests/benches keep
   defaults).
3. `std::env::var(..)` returns `Result<String, VarError>` — match all
   three outcomes explicitly (`Ok`, `Err(NotPresent)`,
   `Err(NotUnicode)`), then `raw.parse::<usize>()` inside the `Ok`
   arm with its own match on `Ok(n) if n > 0`, `Ok(0)`, `Err(_)`. Every
   invalid/absent branch either does nothing (absent) or
   `tracing::warn!`s and keeps the default — never `.unwrap()`,
   never `let _ = ...`, never return `Err` for a bad env value. This
   is required by the "Binary-Boundary Error Handling" rule in
   CLAUDE.md: server binaries must not panic on operator input, and
   `build_executor()`'s `anyhow::Result` is reserved for real startup
   failures (e.g. cache init), not env-var typos.

## Example

```rust
match std::env::var("NEXUS_CARTESIAN_PRODUCT_MAX_BYTES") {
    Ok(raw) => match raw.parse::<usize>() {
        Ok(max_bytes) if max_bytes > 0 => {
            executor.set_cartesian_product_max_bytes(max_bytes);
            tracing::info!("... applied: {} bytes", max_bytes);
        }
        Ok(_) => tracing::warn!("... is zero; keeping default"),
        Err(e) => tracing::warn!("... not a valid usize ({e}); keeping default"),
    },
    Err(std::env::VarError::NotPresent) => {}
    Err(std::env::VarError::NotUnicode(raw)) => {
        tracing::warn!("... not valid unicode: {:?}", raw);
    }
}
```

## When to use

Any future `ExecutorConfig` knob that needs an operator-facing env-var
override without a full config-file story. Reuse `build_executor()` as
the single injection point.

## When not to use

Knobs that should be per-request or per-query (those belong in the
Cypher request payload or session state, not process env), or knobs
that already have a config-file/YAML path — don't add a second
override mechanism for the same value.
