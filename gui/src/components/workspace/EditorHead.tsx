/**
 * EditorHead — breadcrumb (db · graph · tab name) on the left,
 * action buttons (Format, Save, Share, History, Run) on the right.
 * Run is the only primary button; everything else is `btn` so the
 * Run path stands out at a glance.
 *
 * `onRun` and `onFormat` are passed in from the workspace so the
 * Cypher editor can also bind them to keyboard shortcuts (⌘↵, ⌘S,
 * ⌘/, …) without re-resolving the active tab here.
 */
import {
  useConnectionsStore,
  selectCurrentConnection,
} from '../../stores/connectionsStore';
import { useLayoutStore } from '../../stores/layoutStore';
import {
  ChevronRightIcon,
  DatabaseIcon,
  FormatIcon,
  HistoryIcon,
  PlayIcon,
  SaveIcon,
  ShareIcon,
} from '../../icons';

interface EditorHeadProps {
  onRun: () => void;
  onFormat?: () => void;
  onSave?: () => void;
  onShare?: () => void;
  onHistory?: () => void;
  isRunning?: boolean;
}

export function EditorHead({
  onRun,
  onFormat,
  onSave,
  onShare,
  onHistory,
  isRunning,
}: EditorHeadProps) {
  const conn = useConnectionsStore(selectCurrentConnection);
  const activeTab = useLayoutStore((s) => s.activeTab);
  const editorTabs = useLayoutStore((s) => s.editorTabs);
  const tabName = editorTabs.find((t) => t.id === activeTab)?.title ?? 'untitled';

  return (
    <div className="editor-head">
      <div className="breadcrumb">
        <DatabaseIcon style={{ color: 'var(--fg-3)' }} />
        <strong>{conn?.name ?? 'no connection'}</strong>
        <ChevronRightIcon style={{ color: 'var(--fg-4)' }} />
        <span>graph:default</span>
        <ChevronRightIcon style={{ color: 'var(--fg-4)' }} />
        <strong>{tabName}</strong>
      </div>
      <div className="grow" />
      <button className="btn ghost" type="button" title="Format" onClick={onFormat}>
        <FormatIcon /> Format
      </button>
      <button
        className="btn ghost"
        type="button"
        title="Save"
        aria-label="Save"
        onClick={onSave}
      >
        <SaveIcon />
      </button>
      <button
        className="btn ghost"
        type="button"
        title="Share"
        aria-label="Share"
        onClick={onShare}
      >
        <ShareIcon />
      </button>
      <button className="btn" type="button" onClick={onHistory}>
        <HistoryIcon /> History
      </button>
      <button
        className="btn primary"
        type="button"
        onClick={onRun}
        disabled={isRunning || !conn}
      >
        <PlayIcon /> {isRunning ? 'Running…' : 'Run'} <span className="kbd">⌘↵</span>
      </button>
    </div>
  );
}
