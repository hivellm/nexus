# Nexus GUI — desktop client

React 18 + Vite + TypeScript + Tailwind v4. Embeds Monaco for the
Cypher editor and `react-force-graph-2d` for the result graph.
Optionally bundled as an Electron desktop app via
`vite-plugin-electron`.

## Dev workflow

```bash
# Install deps (first time only)
npm install

# Run the renderer-only Vite dev server (recommended for UI work).
# The Electron plugin is currently broken in the bundled build —
# the renderer-only path works against any running nexus-server.
NEXUS_NO_ELECTRON=1 npx vite

# Open http://localhost:15475/ — the dev server hot-reloads on
# every file change.
```

The dev server expects a Nexus server reachable from the GUI's
configured connections. Defaults: `http://localhost:15474` (the
`localhost` entry the connections store seeds) and
`http://localhost:15002` (the `cortex` Nexus, also seeded). The
Tweaks panel + the `+` button on the connections list let users
add their own.

## Component map

```
gui/src/
├── app/App.tsx                    — shell grid (titlebar, rail, columns, statusbar, tweaks)
├── main.tsx                       — bootstrap (font import, Monaco theme prime, QueryClient)
├── components/
│   ├── chrome/
│   │   ├── Titlebar.tsx           — traffic lights, brand, breadcrumb, tab strip, ⌘K search
│   │   ├── ActivityRail.tsx       — 5 view buttons + Tweaks toggle
│   │   ├── StatusBar.tsx          — connection LED, host, epoch, replicas, page cache, WAL, |V|/|E|, cursor pos
│   │   └── Tweaks.tsx             — floating panel (theme toggle + future knobs)
│   ├── panels/
│   │   ├── LeftColumn.tsx         — dispatcher on `currentView`
│   │   ├── ConnectionsPanel.tsx   — list + edit dialog + per-row health probe
│   │   ├── ConnectionDialog.tsx   — modal form (name/url/role/api-key)
│   │   ├── SchemaPanel.tsx        — labels / rel types / indexes / procedures
│   │   ├── KnnPanel.tsx           — vector search form + results
│   │   ├── ReplicationLeftPanel.tsx
│   │   └── AuditLeftPanel.tsx
│   ├── workspace/
│   │   ├── Workspace.tsx          — orchestrator (editor + results pane)
│   │   ├── EditorHead.tsx         — breadcrumb + Format/Save/Share/History/Run
│   │   ├── CypherEditor.tsx       — Monaco with custom Cypher grammar + nexus-dark/light themes
│   │   ├── ResultsTabs.tsx        — Graph / Table / JSON / Plan + mini-meta
│   │   ├── TableView.tsx          — TanStack Table renderer
│   │   ├── JsonView.tsx           — pretty-printed JSON + copy button
│   │   ├── PlanView.tsx           — indented plan tree
│   │   ├── GraphView.tsx          — react-force-graph-2d backed canvas
│   │   ├── GraphControls.tsx      — zoom/fit/refresh overlay
│   │   ├── GraphLegend.tsx        — per-label colour chips
│   │   └── NodeInspector.tsx      — selected-node detail card
│   └── drawer/
│       ├── RightDrawer.tsx        — composes the three sections + mounts the metrics pump
│       ├── MetricsSection.tsx     — qps / cache hit / p99 / WAL with sparklines + delta %
│       ├── Sparkline.tsx          — pure SVG mini chart (memoised)
│       ├── ReplicationCompact.tsx — master + replicas / standalone
│       ├── AuditFeed.tsx          — server `/audit/log` + local `queryHistoryStore`
│       └── useMetricsPump.ts      — hook that drives the ringbuffer off `useStats`
├── services/
│   ├── api.ts                     — typed fetch wrappers + NexusApiError
│   ├── queries.ts                 — TanStack Query hooks (useHealth/useStats/useSchema/…)
│   └── cypher.ts                  — sanitizeCypher() — strips comments before /cypher
├── stores/
│   ├── layoutStore.ts             — currentView, editorTabs, theme, tweaksVisible
│   ├── connectionsStore.ts        — saved connections + per-row status
│   ├── metricsStore.ts            — snapshot + ringbuffers
│   └── queryHistoryStore.ts       — local 200-entry persisted ring
├── types/api.ts                   — typed REST shapes (HealthResponse, StatsResponse, …)
├── icons/                         — SVG icon components + types
└── styles/
    ├── tokens.css                 — design tokens + light-theme override
    ├── globals.css                — chrome + workspace + drawer + dialog rules
    └── monaco-themes.ts           — primes Monaco with nexus-dark/light via loader.init()
```

## Conventions

- Theme switching happens via `[data-theme="light"]` on `<html>` —
  `bindThemeToHtml()` in `stores/layoutStore.ts` mirrors the persisted
  choice on bootstrap.
- Every API call goes through `services/api.ts`. The hooks in
  `services/queries.ts` thread the active connection's
  `baseUrl` + `apiKey` so handlers don't reach into the
  connections store directly.
- Editor content lives in `layoutStore.editorTabs[activeTab]`.
  Run path: `Workspace.handleRun` calls
  `sanitizeCypher(tab.body)` (strips `// — — /* */` comments
  the Nexus parser rejects) and feeds the result to
  `useExecuteCypher.mutate`.

## Mockup reference

Source mockup files for this redesign live at
[`docs/design/gui-mockup-v2/`](../docs/design/gui-mockup-v2/).
The `Nexus GUI.html` + sibling `.jsx` files are the original
v2 spec; the React rewrite mirrors the layout token-by-token.

## Build

```bash
# Renderer-only production build (Vite static output)
NEXUS_NO_ELECTRON=1 npm run build

# Electron desktop bundle (currently broken on the bundling
# side; the renderer-only path is the supported flow)
npm run build:electron
```
