# Implementation Tasks - V1 Desktop GUI

**Status**: 🟡 IN PROGRESS (~49% — core shell, editor, graph view, metrics done)
**Priority**: Medium (after MVP and Authentication)
**Estimated**: Q1 2026

> Stack note: React 18 (not Vue 3), Monaco (not CodeMirror), `react-force-graph-2d` (not Cytoscape.js), inline SVG sparklines (not Chart.js). Functional parity preserved; checklist labels kept verbatim for traceability.

---

## 1. Electron Setup

- [x] 1.1 Initialize Electron project
- [x] 1.2 Setup Vue 3 + Vite (delivered as React 18 + Vite)
- [x] 1.3 Setup TailwindCSS
- [x] 1.4 Configure IPC communication
- [ ] 1.5 Setup auto-updater
- [x] 1.6 Configure build scripts (Windows, macOS, Linux)

## 2. Graph Visualization

- [x] 2.1 Setup Cytoscape.js (delivered as react-force-graph-2d)
- [x] 2.2 Implement force-directed layout
- [x] 2.3 Implement node rendering (styled by label)
- [x] 2.4 Implement relationship rendering (styled by type)
- [x] 2.5 Add zoom/pan controls
- [x] 2.6 Add node selection and property inspector
- [ ] 2.7 Add filtering (by label, type)
- [ ] 2.8 Add tests

## 3. Query Editor

- [x] 3.1 Setup CodeMirror with Cypher syntax (delivered as Monaco)
- [x] 3.2 Implement query execution
- [x] 3.3 Implement result table view
- [x] 3.4 Implement result graph view (toggle)
- [x] 3.5 Add query history
- [ ] 3.6 Add saved queries
- [ ] 3.7 Add export (JSON, CSV)
- [x] 3.8 Add tests

## 4. KNN Search Interface

- [x] 4.1 Add text input for queries
- [ ] 4.2 Integrate with Vectorizer for embedding generation
- [x] 4.3 Display similarity results visually
- [ ] 4.4 Add hybrid query builder (KNN + patterns)
- [ ] 4.5 Add vector index management UI
- [ ] 4.6 Add tests

## 5. Monitoring Dashboard

- [x] 5.1 Setup Chart.js (delivered as inline SVG sparklines)
- [x] 5.2 Add query throughput chart
- [x] 5.3 Add page cache hit rate chart
- [x] 5.4 Add WAL size chart
- [ ] 5.5 Add replication lag chart (if enabled)
- [ ] 5.6 Add real-time updates (WebSocket)
- [ ] 5.7 Add tests

## 6. Management Tools

- [x] 6.1 Schema browser (labels, types, properties)
- [ ] 6.2 Index management UI (create, rebuild, delete)
- [ ] 6.3 Backup/restore UI
- [ ] 6.4 Replication monitoring and control
- [ ] 6.5 Configuration editor
- [ ] 6.6 Log viewer
- [ ] 6.7 Add tests

## 7. Build & Package

- [x] 7.1 Build Windows MSI installer
- [x] 7.2 Build macOS DMG
- [x] 7.3 Build Linux AppImage/DEB
- [ ] 7.4 Test installers on all platforms
- [ ] 7.5 Setup auto-update mechanism

## 8. Documentation & Quality

- [ ] 8.1 Update docs/ROADMAP.md
- [ ] 8.2 Add GUI screenshots to README
- [ ] 8.3 Create GUI user guide
- [ ] 8.4 Update CHANGELOG.md with v0.7.0
- [ ] 8.5 Run all quality checks

## 9. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 9.1 Update or create documentation covering the implementation
- [ ] 9.2 Write tests covering the new behavior
- [ ] 9.3 Run tests and confirm they pass
