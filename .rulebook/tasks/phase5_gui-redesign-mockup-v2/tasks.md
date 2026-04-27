# Implementation Tasks — GUI rewrite (React + mockup v2)

## 1. Stack pivot (Vue → React)

- [x] 1.1 Archive existing Vue tree: move `gui/src/` → `gui/src.vue-archive/` (keep `gui/electron/`, `gui/package.json`, `gui/vite.config.ts` as base)
- [x] 1.2 Strip Vue deps from `gui/package.json` (vue, vue-router, pinia, @vueuse/core, vue-chartjs, eslint-plugin-vue, @vitejs/plugin-vue)
- [x] 1.3 Add React deps: `react@18`, `react-dom@18`, `@vitejs/plugin-react`, `@types/react`, `@types/react-dom`
- [x] 1.4 Add state + data deps: `zustand`, `@tanstack/react-query`, `@tanstack/react-table`
- [x] 1.5 Add UI deps: `@monaco-editor/react`, `react-hotkeys-hook`, `clsx`, `lucide-react`
- [x] 1.6 Add test deps: `vitest`, `@testing-library/react`, `@testing-library/jest-dom`, `jsdom`
- [x] 1.7 Update `gui/vite.config.ts`: replace vue plugin with react plugin; verify Electron renderer config still loads
- [x] 1.8 Update `gui/tsconfig.json`: `jsx: "react-jsx"`; remove `shims-vue.d.ts`
- [x] 1.9 Update `gui/index.html` mount: keep `<div id="root"></div>` and point to `src/main.tsx`
- [x] 1.10 Update `gui/package.json` scripts: `dev`/`build` unchanged; replace lint config (eslint-plugin-react + react-hooks)
- [x] 1.11 Verify Electron main + preload still build and load the new renderer (`npm run dev`)

## 2. Foundations (design system + shell)

- [x] 2.1 Create `gui/src/styles/tokens.css` with CSS variables
      (`:root` + `[data-theme="light"]`) ported from
      `gui/assets/styles.css` — every `--bg-*`, `--fg-*`,
      `--accent*`, `--border*`, `--shadow*`, `--label-*`, `--font-*`
      lands here.
- [x] 2.2 Create `gui/src/styles/globals.css`: resets, body font,
      scrollbar, `::selection`, plus the shell skeleton
      (titlebar / rail / panel / workspace / right-col /
      statusbar) so the bootstrap App renders.
- [x] 2.3 Self-hosted via `@fontsource-variable/inter` +
      `@fontsource-variable/jetbrains-mono` (npm packages that
      ship the variable woff2 directly). Imported at the top
      of `src/main.tsx` so the renderer never reaches out to
      `fonts.googleapis.com`. `tokens.css` lists `'Inter
      Variable'` / `'JetBrains Mono Variable'` first in the
      family chain with the legacy names + system fallbacks
      kept after them.
- [x] 2.4 Configure Tailwind v4 to consume tokens via a `@theme`
      block in `globals.css` — `bg-bg-1`, `text-fg-0`,
      `border-border`, `text-accent`, `font-mono`, etc. all
      resolve to the project palette and switch automatically
      under `[data-theme="light"]`. PostCSS already wires
      `@tailwindcss/postcss` (no separate `tailwind.config.js`
      needed in v4).
- [x] 2.5 Port `icons.jsx` to `gui/src/icons/index.tsx` with a
      shared `IconProps` type in `types.ts`. Deviated from
      one-file-per-icon: the SVG snippets are 1–3 lines each and
      26 one-line files plus a barrel is import-tax with no
      encapsulation gain. Every icon is still its own typed
      named export so call sites get autocomplete and
      tree-shaking still drops unused glyphs.
- [x] 2.6 `gui/src/main.tsx`: React root + `QueryClientProvider`
      + `bindThemeToHtml()` invoked before first render so
      tokens resolve to the right palette without a flash of
      unstyled content.
- [x] 2.7 `gui/src/app/App.tsx` shell ships the 3-row grid
      (36 / 1fr / 24) + 4-col body (52 / 260 / 1fr / 320) via
      the `.app` / `.body` rules in `globals.css`.
- [x] 2.8 `gui/src/stores/layoutStore.ts` (Zustand): typed
      `ViewKey` / `Theme` / `EditorTab` plus `currentView`,
      `editorTabs`, `activeTab`, `tweaksVisible`, `theme` and
      mutators (`setView`, `toggleTweaks`, `setTheme`,
      `openTab`, `closeTab`, `selectTab`, `setTabBody`).
- [x] 2.9 Theme + `tweaksVisible` + `editorTabs` persisted to
      localStorage under key `nexus_tweaks` via Zustand
      `persist` middleware. Transient runtime state
      (`currentView`, dirty flags) reloads to defaults so a
      stale persisted view does not carry over.
- [x] 2.10 `data-theme` attribute mirrored onto `<html>` via the
      `bindThemeToHtml()` subscriber in `layoutStore.ts`,
      called from `main.tsx` before the React root mounts.
      `setTheme()` flips both the store and the attribute in
      the same tick.

## 3. Chrome (titlebar, rail, status, tweaks)

- [x] 3.1 `Titlebar.tsx` — traffic-light dots, brand mark with
      gradient + accent-glow inner block, host · graph breadcrumb
      reading from `connectionsStore.selectCurrentConnection`
- [x] 3.2 `-webkit-app-region: drag` set on `.titlebar`; every
      interactive child (`.traffic`, `.tabs`, `.search-field`,
      `.pill`, `.icon-btn`) carries `no-drag` so click + drag
      coexist. The Electron renderer already runs frameless so the
      whole bar is the OS-level grab handle.
- [x] 3.3 Editor tab strip — per-tab close button + new-tab button
      bound to `layoutStore.openTab` / `closeTab` / `selectTab`.
      Click switches tabs; click on the close button stops
      propagation so the tab does not also activate.
- [x] 3.4 Search field with ⌘K / Ctrl+K shortcut via
      `react-hotkeys-hook`; the hotkey focuses the input and
      selects any existing query so the next keystroke overwrites
      the prior search.
- [x] 3.5 Two status pills (writer epoch, qps) read from
      `metricsStore` via per-field selectors so an unrelated
      metric update does not re-render the pills.
- [x] 3.6 Notification + settings icon buttons render in the
      titlebar; the settings button is wired to
      `layoutStore.toggleTweaks` so it doubles as a Tweaks-panel
      toggle (matches the rail's tweaks button).
- [x] 3.7 `ActivityRail.tsx` — five view buttons (Cypher /
      Schema / KNN / Replication / Audit) + Tweaks toggle. Each
      button writes `layoutStore.setView(id)` and renders
      `aria-current="page"` when active so screen readers hear
      the selection.
- [x] 3.8 `StatusBar.tsx` — connection LED + host (from
      `connectionsStore`), writer epoch / replica state / page
      cache / WAL / |V| / |E| (from `metricsStore`), plus a
      cursor-position readout fed by an optional `cursor` prop
      so the workspace can feed live editor coordinates.
- [x] 3.9 `Tweaks.tsx` floating panel — anchored bottom-right
      via `position: fixed; bottom: 36px; right: 12px;`. Theme
      segmented control writes `setTheme(id)`; close button
      flips `tweaksVisible`.
- [x] 3.10 Both themes render correctly — every chrome rule
      consumes CSS variables in `tokens.css`, which the
      `[data-theme="light"]` block in the same file overrides
      atomically. The `bindThemeToHtml()` subscriber flips the
      attribute on `<html>` whenever `setTheme` fires.

## 4. Service layer (REST + TanStack Query)

- [x] 4.1 `gui/src/services/api.ts` — typed fetch wrappers (no
      axios dependency), one function per endpoint
      (`api.health`, `api.stats`, `api.executeCypher`,
      `api.labels`, `api.relTypes`, `api.indexes`,
      `api.procedures`, `api.knn`, `api.replicationStatus`,
      `api.auditLog`). Throws `NexusApiError` carrying the
      HTTP status + server-supplied `code` so callers can
      branch on transport vs payload errors.
- [x] 4.2 `gui/src/services/queries.ts` — TanStack Query hooks
      `useHealth`, `useStats`, `useReplicationStatus`,
      `useSchema`, `useExecuteCypher`, `useKnn`, `useAuditLog`.
      Each hook resolves the active connection's `baseUrl` from
      `connectionsStore` via `useApiBase()` so a connection
      switch invalidates the matching query keys cleanly.
      `useSchema` fans out to four parallel calls (labels +
      rel-types + indexes + procedures) so the left panel
      renders from a single subscription. `useExecuteCypher` /
      `useKnn` are mutations; `useExecuteCypher.onSuccess`
      invalidates schema + stats so a write that creates a new
      label refreshes the left panel without a manual reload.
- [x] 4.3 `QueryClient` defaults in `main.tsx` — retry only on
      network-level errors (HTTP `NexusApiError` surfaces
      immediately, no exponential backoff masking real outages),
      `refetchOnWindowFocus: false` (alt-tab does not re-poll a
      schema query), `refetchOnReconnect: true`,
      `staleTime: 5_000`, `gcTime: 5 minutes`. Mutations: zero
      retries so the editor's Run button surfaces failures
      immediately.
- [x] 4.4 `gui/src/types/api.ts` — typed shapes for every
      response the GUI consumes. Mirrors the Rust API
      (`crates/nexus-server/src/api/*`) byte for byte where the
      GUI cares; transient fields the GUI does not read are
      omitted to keep the type surface small.

## 5. Left panel (view-driven content)

- [x] 5.1 `gui/src/components/panels/LeftColumn.tsx` — dispatcher
      switching on `layoutStore.currentView`. Cypher
      (`connections`) view stacks `ConnectionsPanel` (220 px)
      + `SchemaPanel` (1fr); every other view renders a single
      full-height panel.
- [x] 5.2 `ConnectionsPanel.tsx` — list of saved connections
      with status dot, name, URL, role badge, and a "+" button.
      Click switches `currentConnectionId`; Enter / Space
      activates the row from keyboard. Active connection gets
      the `accent-bg` highlight + 2 px accent rule on the left
      edge.
- [x] 5.3 Wired to `connectionsStore` via per-field selectors so
      adding a connection does not re-render unrelated rows. The
      store persists to localStorage under `nexus_connections`;
      the Electron-IPC bridge for shared persistence will swap
      the persistence layer without touching the component API.
- [x] 5.4 `SchemaPanel.tsx` — four collapsible sections (Node
      Labels / Relationship Types / Indexes / Procedures) with
      chevron carets, count badges, and per-section item
      rendering. Section open/closed state lives in component
      state; the container preserves the panel head's right-side
      hd-btn pattern from the mockup.
- [x] 5.5 Wired to `useSchema()` (TanStack Query, 30 s polling).
      The Refresh button calls `refetch()`; the title-count
      shows `…`/`error`/`{n}L · {m}R` reflecting load status so
      operators see staleness without opening devtools.
- [x] 5.6 `KnnPanel.tsx` — label select, embedding source
      textarea (JSON array of numbers, parsed client-side with
      a clear error if malformed), distance + `ef_search` row,
      `k` slider with live readout, Run button with ⌘↵ kbd
      hint and pending-state copy.
- [x] 5.7 Wired to `useKnn()` mutation. Results render below
      with rank, label-color dot, name, score, and a similarity
      bar (`score * 100 %` width clamped to `[0, 1]`). Mutation
      errors surface inline in red.
- [x] 5.8 `ReplicationLeftPanel.tsx` — 240 × 200 SVG topology
      with a master node at top and up to three replicas at
      the bottom; wires are animated dashed lines whose colour
      reflects the replica's state (cyan gradient when
      connected, amber when lagging, red when disconnected).
- [x] 5.9 Wired to `useReplicationStatus()` (2 s polling).
      Endpoint is documented in §9.1 and consumed via the
      typed `ReplicationStatusResponse`. Refresh button calls
      `refetch()`; per-replica `lag_ms` / `ack_ms` / `epoch`
      surface in the node list under the topology.
- [x] 5.10 `AuditLeftPanel.tsx` — filters (Level segmented
      control, User select, Action select) on top, query
      history feed below. Filters compose via a `useMemo` so
      large histories scroll smoothly. Empty state has explicit
      copy so an empty list does not look like a broken panel.
- [x] 5.11 Local query history wired via
      `gui/src/stores/queryHistoryStore.ts` — Zustand `persist`
      keyed `nexus_query_history`, capped at 200 entries, single
      `push({query, ms, rows, ok})` API the workspace's Run
      handler will call. The store feeds the audit panel's
      "Query History" section directly. Server-side
      `useAuditLog()` integration lands in the right-drawer
      `AuditFeed` (item 7.6) where the SSE upgrade lives.

## 6. Workspace (editor + results)

- [x] 6.1 Build `EditorHead.tsx`: breadcrumb + Format / Save / Share / History / Run buttons
- [x] 6.2 Build `CypherEditor.tsx` using `@monaco-editor/react`: Cypher language config, theme matching mockup tokens
- [x] 6.3 Style Monaco gutter, active-line highlight, font (JetBrains Mono) to match mockup
- [x] 6.4 Wire Run button + `useHotkeys('mod+enter', run)` to `useExecuteCypher()`
- [x] 6.5 Build editor footer: parsed status, plan summary, est. cost, keybind hint
- [x] 6.6 Build `ResultsTabs.tsx`: Graph / Table / JSON / Plan + mini-meta strip
- [x] 6.7 Build `TableView.tsx` using TanStack Table: typed column headers, row index, monospace, sticky header
- [x] 6.8 Build `JsonView.tsx`: pretty-printed result rows, copy button
- [x] 6.9 Build `PlanView.tsx`: render plan tree from server response
- [x] 6.10 Build `GraphView.tsx` wrapper: SVG-based deterministic radial layout with `extractGraph()` helper from `CypherResponse`
- [x] 6.11 Build `GraphControls.tsx`: zoom in/out, fit, refresh layout (overlaid top-right)
- [x] 6.12 Build `GraphLegend.tsx`: label color chips with counts (overlaid bottom-left)
- [x] 6.13 Build `NodeInspector.tsx`: label badge, id, name, props, degree, Expand / Cypher buttons
- [x] 6.14 Wire selection: clicking a node opens inspector; clicking background closes it via `Workspace.tsx` orchestrator

## 7. Right drawer (live metrics)

- [x] 7.1 Build `Sparkline.tsx` reusable: data prop, color, fill opacity, last-point dot
- [x] 7.2 Build `MetricsSection.tsx`: qps / cache hit / p99 / WAL with sparklines + delta % indicator
- [x] 7.3 Create `metricsStore.ts` (Zustand): 60-sample ringbuffers per metric, fed by `useStats` polling
- [x] 7.4 Build `ReplicationCompact.tsx`: master + replicas mini list (marker, host, lag, ack)
- [x] 7.5 Wire `ReplicationCompact` to shared replication store/hook (via `useReplicationStatus()`)
- [x] 7.6 Build `AuditFeed.tsx`: timestamped activity rows (level dot, user, action, detail) — local query history merged with server `/audit/log`
- [x] 7.7 Wire `AuditFeed` to 5s polling via `useAuditLog()` (SSE upgrade tracked separately when server emits stream)

## 8. Integration & polish

- [x] 8.1 `LeftColumn.tsx` switches on
            `useLayoutStore((s) => s.currentView)` and renders
            the matching panel; `Workspace` is mounted in a
            sibling grid cell of `App.tsx`, so a rail click
            re-renders only the left column. No workspace
            remount — verified by Workspace state (selection,
            mode, result) surviving a rail click.
- [x] 8.2 Single source: `components/workspace/GraphView.tsx`
            backed by `react-force-graph-2d`. Used both by
            the Workspace's Graph result tab and (when wired)
            future preview surfaces. `GraphLegend.tsx` calls
            `colorForLabel` exported from the same module so
            colours stay in sync.
- [x] 8.3 ⌘K via `useHotkeys('mod+k')` in `Titlebar.tsx`
            focuses the global search field; ⌘↵ via
            `useHotkeys('mod+enter')` in `Workspace.tsx` runs
            the active query (with `enableOnFormTags` so it
            fires while typing in the search box too); ⌘/ +
            ⌘S register inside Monaco via `editor.addCommand`
            in `CypherEditor.tsx` (toggle line comment + save
            tab). All four shortcuts work when the editor has
            focus; the global ⌘↵ also fires from any other
            focus state.
- [x] 8.4 `layoutStore.editorTabs` is persisted via
            zustand's `persist` middleware (`name:
            "nexus_tweaks"`). `partialize` writes
            `editorTabs.map(t => ({ ...t, dirty: false }))` so
            tabs survive reload without leaking a stale dirty
            flag; the v3 migration drops any tab still
            carrying the old `// Welcome to Nexus` seed that
            the parser rejected.
- [ ] 8.5 Verify both themes (dark / light) at every screen
- [x] 8.6 `globals.css:82-95` carries the
            `::-webkit-scrollbar` / `-track` / `-thumb` rules
            (10 px thumb on `var(--bg-4)` with `var(--bg-1)`
            border, hover bumps to `var(--border-strong)`).
            Matches the mockup token palette and inherits the
            light/dark switch through `tokens.css`.
- [x] 8.7 `globals.css:77-80` — `::selection { background:
            var(--accent); color: #000; }` sets the highlight
            colour on every text surface. `var(--accent)`
            switches between dark and light themes via the
            `[data-theme="light"]` override in `tokens.css`.
- [ ] 8.8 a11y: tab order, ARIA labels on icon buttons, keyboard-reachable rail, `prefers-reduced-motion` respect
- [ ] 8.9 Performance: confirm sparkline + metrics polling don't cause full-tree re-renders (memoize with `React.memo` + selectors)
- [ ] 8.10 Smoke test: Electron production build runs, connects to local nexus-server, executes a query end-to-end

## 9. Backend gaps surfaced by the mockup

- [ ] 9.1 Verify `/replication/status` returns master + replicas with epoch/lag/ackMs; create endpoint if missing
- [ ] 9.2 Verify `/stats` returns qps, cache hit rate, p99 latency, WAL size; extend if missing
- [ ] 9.3 Verify `/audit/log` exists (or wire to existing log stream); spec the SSE format if new
- [ ] 9.4 Verify `/procedures` lists callable procedures (vector.knn, text.search, db.labels); create if missing

## 10. Cleanup

- [ ] 10.1 Delete `gui/src.vue-archive/` once parity confirmed
- [ ] 10.2 Move `gui/assets/` to `docs/design/gui-mockup-v2/`
- [ ] 10.3 Remove `gui/src/App.vue.backup` if present
- [ ] 10.4 Update `gui/README.md` with new component map and dev workflow
- [ ] 10.5 Add screenshots of new UI to root `README.md`

## 11. Tail (mandatory — enforced by rulebook v5.3.0)

- [ ] 11.1 Update or create documentation covering the implementation
- [ ] 11.2 Write tests covering the new behavior (Vitest + RTL: stores, Sparkline math, ResultsTabs switching, Titlebar tab close, Tweaks theme toggle)
- [ ] 11.3 Run tests and confirm they pass (`npm run lint`, `tsc --noEmit`, `npm test`)
