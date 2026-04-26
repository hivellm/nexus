/**
 * StatusBar — bottom 24 px row. Shows the connection LED + host,
 * writer epoch, replica state, page-cache hit rate, WAL size, |V|
 * and |E| counters, and a cursor-position readout. Pulls scalars
 * from `metricsStore` and `connectionsStore`; sparklines belong in
 * the right drawer (item 7).
 */
import {
  useConnectionsStore,
  selectCurrentConnection,
} from '../../stores/connectionsStore';
import { useMetricsStore } from '../../stores/metricsStore';

function formatNumber(n: number): string {
  return n.toLocaleString('en-US');
}

interface CursorPos {
  line: number;
  column: number;
}

interface StatusBarProps {
  cursor?: CursorPos;
}

export function StatusBar({ cursor }: StatusBarProps) {
  const currentConn = useConnectionsStore(selectCurrentConnection);
  const epoch = useMetricsStore((s) => s.epoch);
  const replicaState = useMetricsStore((s) => s.replicaState);
  const replLagMs = useMetricsStore((s) => s.replLagMs);
  const pageCacheHitRate = useMetricsStore((s) => s.pageCacheHitRate);
  const walSizeMb = useMetricsStore((s) => s.walSizeMb);
  const nodes = useMetricsStore((s) => s.nodes);
  const edges = useMetricsStore((s) => s.edges);

  const connected = currentConn?.status === 'connected';
  const ledClass = connected ? 'led' : 'led idle';
  const url = currentConn?.url ?? 'no connection';

  return (
    <footer className="statusbar" role="status" aria-live="polite">
      <span className="sg">
        <span className={ledClass} /> {connected ? 'Connected' : 'Idle'}
      </span>
      <span className="sep">·</span>
      <span className="sg mono">{url}</span>
      <span className="sep">·</span>
      <span className="sg">
        writer <strong>epoch {epoch}</strong>
      </span>
      <span className="sep">·</span>
      <span className="sg">
        replicas <strong>{replicaState}</strong> (max lag {replLagMs}ms)
      </span>
      <div className="grow" />
      <span className="sg">
        page cache <strong>{(pageCacheHitRate * 100).toFixed(1)}%</strong>
      </span>
      <span className="sep">·</span>
      <span className="sg">
        WAL <strong>{walSizeMb.toFixed(1)} MB</strong>
      </span>
      <span className="sep">·</span>
      <span className="sg clickable">
        |V| <strong>{formatNumber(nodes)}</strong>
      </span>
      <span className="sep">·</span>
      <span className="sg clickable">
        |E| <strong>{formatNumber(edges)}</strong>
      </span>
      <span className="sep">·</span>
      <span className="sg">
        Ln <strong>{cursor?.line ?? 1}</strong>, Col{' '}
        <strong>{cursor?.column ?? 1}</strong> · Cypher · UTF-8
      </span>
    </footer>
  );
}
