/**
 * `layoutStore` tests — the Zustand-backed shell state. Covers the
 * three concerns the store coordinates: rail view selection,
 * editor-tab lifecycle, theme persistence. Persistence itself is
 * exercised through the public mutators; localStorage is wiped at
 * the top of every test so each one starts from a known shape.
 */
import { describe, it, expect, beforeEach } from 'vitest';
import { SAMPLE_CYPHER, useLayoutStore } from './layoutStore';

beforeEach(() => {
  localStorage.clear();
  // Reset the store to its initial state. Zustand keeps a single
  // module-level store across tests; without the reset, mutations
  // from one test leak into the next.
  useLayoutStore.setState({
    currentView: 'connections',
    editorTabs: [],
    activeTab: null,
    tweaksVisible: false,
    theme: 'dark',
  });
});

describe('rail view selection', () => {
  it('starts on connections view', () => {
    expect(useLayoutStore.getState().currentView).toBe('connections');
  });

  it('setView writes the active rail target', () => {
    useLayoutStore.getState().setView('schema');
    expect(useLayoutStore.getState().currentView).toBe('schema');
  });
});

describe('editor tabs', () => {
  it('openTab appends and activates', () => {
    useLayoutStore.getState().openTab({
      id: 'tab-1',
      title: 'q.cypher',
      body: 'MATCH (n) RETURN n',
    });
    const s = useLayoutStore.getState();
    expect(s.editorTabs).toHaveLength(1);
    expect(s.activeTab).toBe('tab-1');
    expect(s.editorTabs[0].dirty).toBe(false);
  });

  it('setTabBody marks the tab dirty', () => {
    useLayoutStore.getState().openTab({
      id: 't',
      title: 'a.cypher',
      body: '',
    });
    useLayoutStore.getState().setTabBody('t', 'MATCH (n)');
    const tab = useLayoutStore.getState().editorTabs[0];
    expect(tab.body).toBe('MATCH (n)');
    expect(tab.dirty).toBe(true);
  });

  it('closeTab drops the tab and falls back to the previous tab', () => {
    const s = useLayoutStore.getState();
    s.openTab({ id: 'a', title: 'a', body: '' });
    s.openTab({ id: 'b', title: 'b', body: '' });
    s.closeTab('b');
    const next = useLayoutStore.getState();
    expect(next.editorTabs.map((t) => t.id)).toEqual(['a']);
    expect(next.activeTab).toBe('a');
  });

  it('ensureDefaultTab seeds SAMPLE_CYPHER on a fresh shell', () => {
    useLayoutStore.getState().ensureDefaultTab();
    const tabs = useLayoutStore.getState().editorTabs;
    expect(tabs).toHaveLength(1);
    expect(tabs[0].body).toBe(SAMPLE_CYPHER);
  });

  it('ensureDefaultTab leaves user-typed bodies alone', () => {
    useLayoutStore.getState().openTab({
      id: 't',
      title: 'q.cypher',
      body: 'CREATE (n:User)',
    });
    useLayoutStore.getState().ensureDefaultTab();
    expect(useLayoutStore.getState().editorTabs[0].body).toBe('CREATE (n:User)');
  });

  it('ensureDefaultTab replaces an all-blank persisted tab with the seed', () => {
    useLayoutStore.setState({
      editorTabs: [{ id: 't', title: 'q.cypher', body: '   \n', dirty: false }],
      activeTab: 't',
    });
    useLayoutStore.getState().ensureDefaultTab();
    expect(useLayoutStore.getState().editorTabs[0].body).toBe(SAMPLE_CYPHER);
  });
});

describe('theme + tweaks', () => {
  it('toggleTweaks flips visibility', () => {
    expect(useLayoutStore.getState().tweaksVisible).toBe(false);
    useLayoutStore.getState().toggleTweaks();
    expect(useLayoutStore.getState().tweaksVisible).toBe(true);
    useLayoutStore.getState().toggleTweaks();
    expect(useLayoutStore.getState().tweaksVisible).toBe(false);
  });

  it('setTheme persists the chosen theme', () => {
    useLayoutStore.getState().setTheme('light');
    expect(useLayoutStore.getState().theme).toBe('light');
    useLayoutStore.getState().setTheme('dark');
    expect(useLayoutStore.getState().theme).toBe('dark');
  });
});
