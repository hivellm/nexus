# Proposal: phase7_complete-v1-gui

Continuation of `phase5_implement-v1-gui` (archived ~49% complete — core
shell, graph view, query editor, and metrics dashboard shipped on React 18
+ Vite + Monaco + react-force-graph-2d). This task carries the remaining
unchecked GUI items forward for the 2.3.0 line.

## Why
The V1 desktop GUI shipped its core surface but stopped at ~49%. The
remaining features — vector/KNN integration, advanced query-editor
ergonomics, the management toolset, live monitoring, packaging hardening,
and the documentation/test tail — are required for a feature-complete,
shippable desktop client. Leaving them untracked after archiving the
parent would orphan real product work, so they are materialized here.

## What Changes
- Auto-update: implement the Electron auto-updater wiring (was 1.5 / 7.5).
- Graph view: label/type filtering + tests.
- Query editor: saved queries, result export (JSON, CSV).
- KNN interface: Vectorizer embedding integration, hybrid query builder
  (KNN + patterns), vector index management UI + tests.
- Monitoring: replication-lag chart, real-time WebSocket updates + tests.
- Management tools: index management (create/rebuild/delete), backup/restore
  UI, replication monitoring/control, configuration editor, log viewer + tests.
- Packaging: cross-platform installer testing.
- Documentation: ROADMAP, README screenshots, GUI user guide, CHANGELOG.

## Impact
- Affected specs: gui / desktop-client
- Affected code: `gui/` desktop app (React renderer + Electron main),
  IPC bridge, packaging config; server endpoints consumed by mgmt/monitoring
  (read-only — no server response-format changes)
- Breaking change: NO
- User benefit: feature-complete desktop client (vector search, management,
  live monitoring, signed auto-updating installers)
