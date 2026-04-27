/**
 * ResultsTabs — segmented tab strip across the top of the results
 * pane. Drives the `ResultMode` type that the parent workspace
 * uses to swap between Graph / Table / JSON / Plan views. Right
 * side shows the mini-meta strip with planner / execution time /
 * record count / rows-per-second + a download button.
 */
import { CodeIcon, DownloadIcon, GraphIcon, TableIcon } from '../../icons';

export type ResultMode = 'graph' | 'table' | 'json' | 'plan';

interface ResultsTabsProps {
  mode: ResultMode;
  onMode: (mode: ResultMode) => void;
  rowCount: number;
  nodeCount: number;
  ms: number;
  planner?: string;
  onDownload?: () => void;
}

export function ResultsTabs({
  mode,
  onMode,
  rowCount,
  nodeCount,
  ms,
  planner,
  onDownload,
}: ResultsTabsProps) {
  const rowsPerSec = ms > 0 ? Math.round((rowCount * 1000) / ms) : 0;

  return (
    <div className="results-tabs" role="tablist">
      <button
        type="button"
        className={`result-tab ${mode === 'graph' ? 'active' : ''}`}
        role="tab"
        aria-selected={mode === 'graph'}
        onClick={() => onMode('graph')}
      >
        <GraphIcon /> Graph <span className="badge">{nodeCount}</span>
      </button>
      <button
        type="button"
        className={`result-tab ${mode === 'table' ? 'active' : ''}`}
        role="tab"
        aria-selected={mode === 'table'}
        onClick={() => onMode('table')}
      >
        <TableIcon /> Table <span className="badge">{rowCount}</span>
      </button>
      <button
        type="button"
        className={`result-tab ${mode === 'json' ? 'active' : ''}`}
        role="tab"
        aria-selected={mode === 'json'}
        onClick={() => onMode('json')}
      >
        <CodeIcon /> JSON
      </button>
      <button
        type="button"
        className={`result-tab ${mode === 'plan' ? 'active' : ''}`}
        role="tab"
        aria-selected={mode === 'plan'}
        onClick={() => onMode('plan')}
      >
        Plan
      </button>
      <div className="grow" />
      <div className="mini-meta">
        <span>
          planner <strong>{planner ?? '—'}</strong>
        </span>
        <span>
          execution <strong>{ms}ms</strong>
        </span>
        <span>
          records <strong>{rowCount.toLocaleString()}</strong>
        </span>
        <span>
          rows/s <strong>{rowsPerSec.toLocaleString()}</strong>
        </span>
        <button
          className="btn ghost"
          type="button"
          style={{ height: 24, padding: '0 8px' }}
          onClick={onDownload}
          aria-label="Download results"
        >
          <DownloadIcon />
        </button>
      </div>
    </div>
  );
}
