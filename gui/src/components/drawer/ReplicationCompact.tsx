/**
 * Right-drawer compact replication card — master row plus a list of
 * replicas with state marker, host, lag (ms), and ack (ms).
 *
 * Reads `useReplicationStatus()` directly so the data lifecycle
 * stays cache-managed by TanStack Query. Standalone deployments
 * (no replicas) render a single "standalone" row.
 */
import { useReplicationStatus } from '../../services/queries';

function fmtMs(ms: number): string {
  if (ms < 1) return '<1 ms';
  if (ms < 1000) return `${ms.toFixed(0)} ms`;
  return `${(ms / 1000).toFixed(1)} s`;
}

export function ReplicationCompact() {
  const repl = useReplicationStatus();

  if (repl.isError) {
    return (
      <section className="drawer-section">
        <header className="drawer-head">
          <span>Replication</span>
          <span className="drawer-sub error">unreachable</span>
        </header>
      </section>
    );
  }

  const data = repl.data;
  const isStandalone = !data || data.replicas.length === 0;

  return (
    <section className="drawer-section">
      <header className="drawer-head">
        <span>Replication</span>
        <span className="drawer-sub">
          {isStandalone ? 'standalone' : `${data!.replicas.length} replica(s)`}
        </span>
      </header>
      <div className="repl-list">
        {data && (
          <div className="repl-line">
            <span className="repl-marker healthy" />
            <span className="repl-host mono">{data.master.host}</span>
            <span className="role-badge master">MASTER</span>
            <span className="repl-meta mono">epoch {data.master.epoch}</span>
          </div>
        )}
        {data?.replicas.map((r) => (
          <div className="repl-line" key={r.host}>
            <span className={`repl-marker ${r.state === 'connected' ? 'healthy' : r.state === 'lagging' ? 'lagging' : 'disconnected'}`} />
            <span className="repl-host mono">{r.host}</span>
            <span className="role-badge replica">REPLICA</span>
            <span className="repl-meta mono">lag {fmtMs(r.lag_ms)} · ack {fmtMs(r.ack_ms)}</span>
          </div>
        ))}
        {!data && (
          <div className="repl-line">
            <span className="repl-marker healthy" />
            <span className="repl-host mono">localhost</span>
            <span className="role-badge standalone">STANDALONE</span>
          </div>
        )}
      </div>
    </section>
  );
}
