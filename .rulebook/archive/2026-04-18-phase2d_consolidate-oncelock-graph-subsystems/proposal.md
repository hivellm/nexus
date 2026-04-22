# Proposal: phase2d_consolidate-oncelock-graph-subsystems

## Why

Fourth decomposition slice of the old `phase2_consolidate-oncelock-into-app-state`.
`api/graph_correlation.rs` has `GRAPH_MANAGER`, `api/graph_correlation_umicp.rs`
has `UMICP_HANDLER`, and `api/comparison.rs` has `GRAPH_A` + `GRAPH_B`. Like
phase2c, these are net-new fields on `NexusServer`.

## What Changes

- Extend `NexusServer` with `graph_correlation_manager:
  Arc<Mutex<GraphCorrelationManager>>`, `graph_umicp_handler:
  Arc<GraphUmicpHandler>`, `graph_a: Arc<Mutex<Graph>>`, `graph_b:
  Arc<Mutex<Graph>>`.
- Build those Arcs inside `main.rs::async_main` (today they are built in
  `init_graphs` / `init_manager`) and hand them to `NexusServer::new`.
- Migrate every handler in the three modules to read from the new fields
  via `State<Arc<NexusServer>>`.
- Delete `GRAPH_A`, `GRAPH_B`, `GRAPH_MANAGER`, `UMICP_HANDLER` and their
  `init_*` / `get_*` helpers.
- Drop `api::comparison::init_graphs` / `api::graph_correlation::init_manager`
  calls from `main.rs`.

## Impact

- Affected specs: none
- Affected code:
  - `nexus-server/src/lib.rs`
  - `nexus-server/src/api/graph_correlation.rs`
  - `nexus-server/src/api/graph_correlation_umicp.rs`
  - `nexus-server/src/api/comparison.rs`
  - `nexus-server/src/main.rs`
- Breaking change: NO (HTTP surface unchanged; `NexusServer::new` gains
  params — internal only).
- User benefit: graph-correlation subsystems now plug into the same
  state handle as everything else. Tests that exercise them no longer
  collide through the global `GRAPH_MANAGER`.
