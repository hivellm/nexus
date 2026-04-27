/**
 * Layout / chrome state for the React shell.
 *
 * Three concerns colocated because they all change together when the
 * user clicks a rail icon, opens a tab, or flips the theme:
 *
 * - **Activity rail selection** (`currentView`) — drives which left
 *   panel renders.
 * - **Editor tabs** (`editorTabs` + `activeTab`) — open Cypher tabs
 *   shown in the titlebar; survive reload via `localStorage`.
 * - **Theme + tweaks visibility** (`theme`, `tweaksVisible`) — a small
 *   floating panel exposes the dark/light segmented control. Theme
 *   choice persists to `nexus_tweaks` per item 2.9; the
 *   `data-theme` attribute on `<html>` is mirrored automatically by
 *   a one-line subscriber set up in `main.tsx`.
 */
import { create } from 'zustand';
import { persist } from 'zustand/middleware';

export type ViewKey =
  | 'connections'
  | 'schema'
  | 'knn'
  | 'replication'
  | 'audit';

export type Theme = 'dark' | 'light';

export interface EditorTab {
  /** Stable tab identifier (uuid-ish — generated client-side). */
  id: string;
  /** Display name in the titlebar tab strip. */
  title: string;
  /** Tab body content (Cypher source, default empty for new tabs). */
  body: string;
  /** True when the tab has unsaved edits since the last `Save`. */
  dirty: boolean;
}

interface LayoutState {
  currentView: ViewKey;
  editorTabs: EditorTab[];
  activeTab: string | null;
  tweaksVisible: boolean;
  theme: Theme;

  setView: (view: ViewKey) => void;
  toggleTweaks: () => void;
  setTheme: (theme: Theme) => void;

  openTab: (tab: Omit<EditorTab, 'dirty'>) => void;
  closeTab: (id: string) => void;
  selectTab: (id: string) => void;
  setTabBody: (id: string, body: string) => void;
  ensureDefaultTab: () => void;
}

export const SAMPLE_CYPHER = `// Welcome to Nexus. Try a starter query — ⌘↵ to run.
//
// Inspect the graph
MATCH (n)
RETURN n
LIMIT 25;

// Common patterns to try next:
// MATCH (n) RETURN labels(n) AS label, count(*) AS count
// MATCH (a)-[r]->(b) RETURN type(r), count(*) ORDER BY count(*) DESC
// CREATE (p:Person {name: 'Alice', age: 30}) RETURN p
`;

/**
 * Persistence partition: the chrome cares about user-visible
 * preferences (theme, tweaks panel toggle, editor tabs). Transient
 * runtime values (rail selection, dirty flags) reload to defaults
 * so a stale persisted view doesn't confuse the next session.
 */
const PERSIST_KEY = 'nexus_tweaks';

export const useLayoutStore = create<LayoutState>()(
  persist(
    (set) => ({
      currentView: 'connections',
      editorTabs: [],
      activeTab: null,
      tweaksVisible: false,
      theme: 'dark',

      setView: (view) => set({ currentView: view }),
      toggleTweaks: () => set((s) => ({ tweaksVisible: !s.tweaksVisible })),
      setTheme: (theme) => set({ theme }),

      openTab: (tab) =>
        set((s) => ({
          editorTabs: [...s.editorTabs, { ...tab, dirty: false }],
          activeTab: tab.id,
        })),
      closeTab: (id) =>
        set((s) => {
          const next = s.editorTabs.filter((t) => t.id !== id);
          const stillActive = s.activeTab === id ? null : s.activeTab;
          return {
            editorTabs: next,
            activeTab:
              stillActive ?? (next.length > 0 ? next[next.length - 1].id : null),
          };
        }),
      selectTab: (id) => set({ activeTab: id }),
      setTabBody: (id, body) =>
        set((s) => ({
          editorTabs: s.editorTabs.map((t) =>
            t.id === id ? { ...t, body, dirty: true } : t,
          ),
        })),
      ensureDefaultTab: () =>
        set((s) => {
          // If a tab already has content, leave it alone — only seed
          // the sample query when the workspace would otherwise be
          // an empty editor staring back at a new user.
          if (s.editorTabs.length > 0) {
            const allEmpty = s.editorTabs.every((t) => !t.body.trim());
            if (!allEmpty) {
              return s.activeTab
                ? s
                : { ...s, activeTab: s.editorTabs[0].id };
            }
            // All persisted tabs are blank — replace the active one
            // with the sample so the user sees something runnable.
            const id = s.activeTab ?? s.editorTabs[0].id;
            return {
              ...s,
              editorTabs: s.editorTabs.map((t) =>
                t.id === id ? { ...t, body: SAMPLE_CYPHER, dirty: false } : t,
              ),
              activeTab: id,
            };
          }
          const id = `tab-${Date.now().toString(36)}`;
          return {
            ...s,
            editorTabs: [
              { id, title: 'query-1.cypher', body: SAMPLE_CYPHER, dirty: false },
            ],
            activeTab: id,
          };
        }),
    }),
    {
      name: PERSIST_KEY,
      // Persist only the user-controlled preferences. Activity-rail
      // view + dirty flags reset on reload so a stale state does
      // not surface a dirty-but-empty tab the next session.
      partialize: (s) => ({
        theme: s.theme,
        tweaksVisible: s.tweaksVisible,
        editorTabs: s.editorTabs.map((t) => ({ ...t, dirty: false })),
        activeTab: s.activeTab,
      }),
    },
  ),
);

/**
 * Mirror the persisted theme choice onto `<html data-theme="...">`
 * so the CSS variables in `tokens.css` switch live. Called once
 * from `main.tsx`; the returned unsubscribe is wired into the
 * React 18 `StrictMode` cleanup path.
 */
export function bindThemeToHtml(): () => void {
  const apply = (theme: Theme) => {
    const root = document.documentElement;
    root.setAttribute('data-theme', theme);
  };
  apply(useLayoutStore.getState().theme);
  return useLayoutStore.subscribe((state, prev) => {
    if (state.theme !== prev.theme) {
      apply(state.theme);
    }
  });
}
