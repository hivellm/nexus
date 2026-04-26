# Proposal: GUI rewrite — React + mockup v2 ("Graph Database Studio")

## Why

The existing Vue 3 GUI in `gui/src/` is a generic dashboard that does not match
the dev-tool aesthetic we want for v1. The user produced a new mockup in
`gui/assets/` (React + Babel HTML, self-contained: `Nexus GUI.html` +
`chrome.jsx`, `panels.jsx`, `workspace.jsx`, `right.jsx`, `icons.jsx`,
`data.js`, `styles.css`) defining the target look-and-feel: 4-column workspace
(rail · left · editor+results · metrics drawer), Cypher tabs in titlebar,
syntax-highlighted editor with plan footer, SVG graph view with inspector,
sparkline metrics, replication topology, audit feed.

**Decision: rewrite in React, not port to Vue.** Reasons:

1. The mockup is already React 18 — JSX, `React.useState`, `React.useEffect`,
   functional components, hook patterns. A direct port is faster than a Vue
   translation.
2. React has stronger ecosystem fit for the components we need: Monaco
   (`@monaco-editor/react`), vis-network React wrappers, Cytoscape React,
   sparkline libs, table libs (TanStack Table).
3. Existing Vue surface (10 views + 5 components) is small enough that a
   rewrite is cheaper than retrofitting a Vue port to the new IA.
4. Aligns the GUI with the broader hivellm tooling that uses React.

The existing `gui/src/` Vue tree will be archived (moved to
`gui/src.vue-archive/`) and replaced by a fresh React codebase under
`gui/src/`. Service layer (REST client) ports cleanly — no Vue-specific
bindings there.

`gui/assets/` is reference-only and gets moved to `docs/design/gui-mockup-v2/`
once parity is reached.

## What Changes

### Stack

- **React 18** + **TypeScript** (strict mode)
- **Vite** (kept — `vite.config.ts` already targets Electron renderer)
- **Electron** (kept — main process unchanged, renderer rewritten)
- **Zustand** for state (lightweight, hooks-native; replaces Pinia)
- **TanStack Query** for server state (REST polling, cache, retries)
- **TanStack Table** for the result table view
- **TailwindCSS v4** (kept — tokens bridged to mockup CSS variables)
- **`@monaco-editor/react`** for the Cypher editor
- **vis-network** wrapped in a custom React component for production graphs;
  pure-SVG renderer (from mockup) for the empty/sample state
- **react-hotkeys-hook** for keyboard shortcuts (⌘K, ⌘↵, ⌘/, ⌘S)
- **Lucide React** or inline-SVG icon set ported from `icons.jsx`
- **Vitest** + **React Testing Library** for tests

Removed deps: `vue`, `vue-router`, `pinia`, `@vueuse/core`, `vue-chartjs`,
`eslint-plugin-vue`, `@vitejs/plugin-vue`. Kept where useful: `axios`,
`monaco-editor`, `vis-network`, `vis-data`, `chart.js` (for heavier dashboards
later).

### Visual / structural

- 3-row × 4-column app shell: titlebar (36px) / body / statusbar (24px),
  body = `52px rail | 260px left | 1fr workspace | 320px right`.
- Mac-style traffic-light controls + brand mark + breadcrumb path
  (host · graph) + tabbed editor strip + ⌘K search + epoch/qps pills.
- Activity rail (Cypher / Schema / KNN / Replication / Audit + Tweaks) drives
  left panel content.
- Cypher view left panel: Connections + Schema browser (Node Labels /
  Relationship Types / Indexes / Procedures, collapsible).
- KNN view: form + scored results with similarity bar.
- Replication view: animated SVG topology + node list with lag/ack/epoch +
  Resync/Promote actions.
- Audit view: filters + query history.
- Workspace: editor head (breadcrumb + Format/Save/Share/History/Run with ⌘↵),
  Cypher editor (Monaco, themed to match mockup gutter/active-line/footer),
  plan/cost footer.
- Results: Graph / Table / JSON / Plan tabs + mini-meta strip.
- Graph view: zoom/pan, edge curvature, animated selection ring,
  label-colored nodes with glow, controls cluster, legend, floating inspector.
- Right drawer: live metrics with custom SVG sparklines (qps, cache hit, p99,
  WAL), compact replication card, audit feed.
- Statusbar: connection LED, host, writer epoch, replica state, page cache,
  WAL, |V|/|E|, cursor pos.
- Floating Tweaks panel (theme dark/light), persisted to localStorage.
- Two themes (`[data-theme="dark"]` / `[data-theme="light"]`) via CSS
  variables.

### Architecture

- `gui/src/main.tsx` — React 18 root.
- `gui/src/app/App.tsx` — shell.
- `gui/src/components/` — chrome (Titlebar, ActivityRail, StatusBar, Tweaks),
  panels (Connections, Schema, Knn, ReplicationLeft, AuditLeft),
  workspace (EditorHead, CypherEditor, ResultsTabs, TableView, JsonView,
  PlanView, GraphView, GraphControls, GraphLegend, NodeInspector),
  right (Sparkline, MetricsSection, ReplicationCompact, AuditFeed),
  ui (Button, IconButton, Pill, Kbd, Segmented).
- `gui/src/icons/` — SVG icon components (ported from `icons.jsx`).
- `gui/src/stores/` — Zustand stores: `layoutStore`, `connectionStore`,
  `editorStore`, `metricsStore`, `replicationStore`, `auditStore`.
- `gui/src/services/` — REST client + TanStack Query hooks
  (`useStats`, `useReplicationStatus`, `useSchema`, `useExecuteCypher`,
  `useKnn`, `useAuditLog`).
- `gui/src/styles/` — `tokens.css` (CSS variables), `globals.css` (resets +
  scrollbar + selection).
- `gui/electron/` — kept; only `preload.ts` IPC channel signatures may need
  updating.

### Data wiring

- `/health` polled 1s for statusbar LED + writer epoch.
- `/stats` polled 1s for metrics (qps / cache / p99 / WAL); ringbuffer 60
  samples per metric.
- `/databases`, `/labels`, `/relationship-types`, `/indexes`, `/procedures`
  for schema browser; refresh button + auto-refresh on connection switch.
- `/cypher` for query exec; plan/cost from response `stats`.
- `/knn_traverse` for KNN form.
- `/replication/status` for replication views.
- `/audit/log` (SSE) or 2s polling fallback for audit feed.

## Impact

- **Affected specs**: `desktop-gui` (extends the v1 capability with concrete
  visual contract + React stack decision).
- **Affected code**:
  - `gui/src/` — full rewrite (Vue tree archived to `gui/src.vue-archive/`)
  - `gui/package.json` — deps swap (Vue stack → React stack)
  - `gui/vite.config.ts` — replace `@vitejs/plugin-vue` with
    `@vitejs/plugin-react`
  - `gui/tsconfig.json` — `jsx: "react-jsx"`
  - `gui/index.html` — root mount point unchanged
  - `gui/electron/` — main + preload unchanged (renderer-agnostic)
- **Breaking change**: YES. The whole renderer is rewritten. No API/schema
  change. Existing user settings (theme, last connection) preserved via
  localStorage keys reused (`nexus_tweaks`, connection list keys).
- **User benefit**: Dev-tool-grade ergonomics matching Neo4j Browser /
  DataGrip / VS Code feel. Single-window multi-tasking. Keyboard-first.
  Faster iteration on the GUI (mockup = source of truth, React port is
  near-1:1).

## Source / reference

- Mockup (source of truth): `gui/assets/Nexus GUI.html` + `*.jsx` +
  `styles.css` + `data.js`.
- Existing GUI (to archive): `gui/src/`.
- Server contracts: `crates/nexus-server/src/api/`.
