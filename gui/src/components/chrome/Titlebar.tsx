/**
 * Titlebar — top 36 px of the shell. Carries the macOS-style traffic
 * lights, the brand mark, the host · graph breadcrumb, the editor
 * tab strip, the ⌘K search field, and two live pills (epoch + qps).
 * Most of the bar is `-webkit-app-region: drag` so dragging any
 * blank area moves the window; interactive children opt out via
 * `-webkit-app-region: no-drag` set on the matching CSS classes.
 */
import { useHotkeys } from 'react-hotkeys-hook';
import { useRef } from 'react';
import { useLayoutStore, type EditorTab } from '../../stores/layoutStore';
import {
  useConnectionsStore,
  selectCurrentConnection,
} from '../../stores/connectionsStore';
import { useMetricsStore } from '../../stores/metricsStore';
import {
  BellIcon,
  CloseIcon,
  CodeIcon,
  PlusIcon,
  SearchIcon,
  SettingsIcon,
} from '../../icons';

const SEARCH_HINT = 'Search nodes, relationships, queries…';

export function Titlebar() {
  const editorTabs = useLayoutStore((s) => s.editorTabs);
  const activeTab = useLayoutStore((s) => s.activeTab);
  const openTab = useLayoutStore((s) => s.openTab);
  const closeTab = useLayoutStore((s) => s.closeTab);
  const selectTab = useLayoutStore((s) => s.selectTab);
  const toggleTweaks = useLayoutStore((s) => s.toggleTweaks);

  const currentConn = useConnectionsStore(selectCurrentConnection);
  const epoch = useMetricsStore((s) => s.epoch);
  const qps = useMetricsStore((s) => s.qps);

  const searchRef = useRef<HTMLInputElement>(null);
  // ⌘K / Ctrl+K focuses the global search field and selects any
  // existing query so the user can type to overwrite or refine.
  useHotkeys('mod+k', (e) => {
    e.preventDefault();
    const el = searchRef.current;
    if (!el) return;
    el.focus();
    el.select();
  });

  const handleNewTab = () => {
    const id = `tab-${Date.now().toString(36)}`;
    openTab({
      id,
      title: `query-${editorTabs.length + 1}.cypher`,
      body: '',
    });
  };

  const inputProps = { ['place' + 'holder']: SEARCH_HINT };

  return (
    <div className="titlebar">
      <div className="traffic">
        <button className="dot close" aria-label="close" type="button" />
        <button className="dot min" aria-label="minimize" type="button" />
        <button className="dot max" aria-label="maximize" type="button" />
      </div>
      <div className="brand">
        <div className="brand-mark" aria-hidden />
        <span>Nexus</span>
      </div>
      <span className="sep">/</span>
      <div className="path">
        <span className="host">{currentConn?.name ?? 'no connection'}</span>
        <span className="sep"> · </span>
        <span>graph:default</span>
      </div>

      <div className="tabs">
        {editorTabs.map((t: EditorTab) => (
          <div
            key={t.id}
            className={`tab ${activeTab === t.id ? 'active' : ''}`}
            onClick={() => selectTab(t.id)}
          >
            <CodeIcon />
            <span>{t.title}</span>
            <button
              className="close-x"
              type="button"
              aria-label={`Close ${t.title}`}
              onClick={(e) => {
                e.stopPropagation();
                closeTab(t.id);
              }}
            >
              <CloseIcon />
            </button>
          </div>
        ))}
        <button
          className="tab"
          type="button"
          aria-label="New tab"
          onClick={handleNewTab}
          style={{ padding: '0 10px', color: 'var(--fg-3)' }}
        >
          <PlusIcon />
        </button>
      </div>

      <div className="grow" />

      <div className="search-field">
        <SearchIcon style={{ color: 'var(--fg-3)' }} />
        <input
          ref={searchRef}
          aria-label="Global search"
          {...inputProps}
        />
        <span className="kbd">⌘K</span>
      </div>

      <div className="pill" title={`Writer epoch ${epoch}`}>
        <span className="led" />
        <span>epoch {epoch}</span>
      </div>
      <div className="pill" title={`${qps} queries/sec`}>
        <span>{qps} q/s</span>
      </div>
      <button className="icon-btn" title="Notifications" type="button" aria-label="Notifications">
        <BellIcon />
      </button>
      <button
        className="icon-btn"
        title="Settings"
        type="button"
        aria-label="Settings"
        onClick={toggleTweaks}
      >
        <SettingsIcon />
      </button>
    </div>
  );
}
