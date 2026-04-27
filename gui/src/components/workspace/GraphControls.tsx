/**
 * GraphControls — overlaid top-right of the graph pane: zoom in /
 * zoom out / fit / refresh layout. Currently the GraphView uses a
 * deterministic radial layout so refresh is a no-op for now;
 * the buttons stay visible to keep the chrome stable across layout
 * algorithm swaps.
 */
import { FitIcon, MinusIcon, PlusIcon, RefreshIcon } from '../../icons';

interface GraphControlsProps {
  onZoomIn: () => void;
  onZoomOut: () => void;
  onFit: () => void;
  onRefreshLayout: () => void;
}

export function GraphControls({
  onZoomIn,
  onZoomOut,
  onFit,
  onRefreshLayout,
}: GraphControlsProps) {
  return (
    <div className="graph-controls" role="toolbar" aria-label="Graph controls">
      <button className="hd-btn" type="button" title="Zoom in" onClick={onZoomIn}>
        <PlusIcon />
      </button>
      <button className="hd-btn" type="button" title="Zoom out" onClick={onZoomOut}>
        <MinusIcon />
      </button>
      <button className="hd-btn" type="button" title="Fit" onClick={onFit}>
        <FitIcon />
      </button>
      <button
        className="hd-btn"
        type="button"
        title="Refresh layout"
        onClick={onRefreshLayout}
      >
        <RefreshIcon />
      </button>
    </div>
  );
}
