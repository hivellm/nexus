# Proposal: phase4_tracing-hotpath-hygiene

## Why

`nexus-core/src/executor/mod.rs` has 113+ `tracing::info!` / `debug!` calls
inside the hot `execute()` path. Under an active `RUST_LOG=info` filter,
every query pays the cost of formatting the message even when the caller
doesn't capture it — string allocation, Arc cloning of the subscriber,
format-machinery overhead. Two concrete symptoms:

- In the `fix/memory-leak-v1` debugging session the server log was
  95 % `Hnsw max_nb_connection 16 ... entering PointIndexation drop`
  lines — active noise from `hnsw_rs` propagating through `tracing`.
- `CREATE` emits 8 `debug!` lines per statement (see `executor/mod.rs:545`
  region) regardless of depth, cluttering logs and wasting cycles.

## What Changes

- Audit every `tracing::info!` in `nexus-core/src/executor/` and downgrade
  hot-path ones to `trace!` (default off).
- For operators the user actually wants to debug, wrap in
  `#[tracing::instrument(skip_all, level = "debug")]` on the function and
  drop the manual log lines.
- Silence the `hnsw_rs` info spam via `RUST_LOG=...,hnsw_rs=warn` default
  in the server startup.
- Remove the few `println!` leftovers if any remain.

## Impact

- Affected specs: none
- Affected code: `nexus-core/src/executor/mod.rs`,
  `nexus-server/src/main.rs` (default EnvFilter), anything using
  `tracing::info!` in hot paths
- Breaking change: NO (only log verbosity)
- User benefit: production logs become actionable; per-request
  overhead drops; CPU spent on log formatting recovered
