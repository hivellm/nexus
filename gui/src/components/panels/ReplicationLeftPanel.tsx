/**
 * ReplicationLeftPanel — SVG topology of master + replicas with
 * animated dashed wires (state colored by replica health), plus a
 * scrollable list of nodes with epoch / lag / ack metadata. Wires
 * straight to `useReplicationStatus()` (2 s polling).
 */
import { useReplicationStatus } from '../../services/queries';
import { RefreshIcon, ReplicationIcon } from '../../icons';
import type { ReplicaInfo } from '../../types/api';

const REPLICA_X = [50, 120, 190] as const;

function wireStroke(state: ReplicaInfo['state']): string {
  switch (state) {
    case 'connected':
      return 'url(#wire)';
    case 'lagging':
      return '#f59e0b';
    case 'disconnected':
      return '#ef4444';
  }
}

function markerColor(state: ReplicaInfo['state']): { fill: string; stroke: string } {
  switch (state) {
    case 'connected':
      return { fill: 'rgba(16,185,129,0.12)', stroke: '#10b981' };
    case 'lagging':
      return { fill: 'rgba(245,158,11,0.12)', stroke: '#f59e0b' };
    case 'disconnected':
      return { fill: 'rgba(239,68,68,0.12)', stroke: '#ef4444' };
  }
}

export function ReplicationLeftPanel() {
  const { data, isLoading, error, refetch } = useReplicationStatus();
  const replicas = data?.replicas ?? [];
  const master = data?.master;

  return (
    <div className="panel">
      <div className="panel-head">
        <ReplicationIcon />
        <span>Replication</span>
        <div className="grow" />
        <button
          className="hd-btn"
          type="button"
          onClick={() => refetch()}
          title="Refresh"
          aria-label="Refresh replication status"
        >
          <RefreshIcon />
        </button>
      </div>
      <div className="repl-topo">
        <svg width="240" height="200" viewBox="0 0 240 200" aria-label="Replication topology">
          <defs>
            <linearGradient id="wire" x1="0" x2="1">
              <stop offset="0%" stopColor="#00d4ff" stopOpacity="0.9" />
              <stop offset="100%" stopColor="#00d4ff" stopOpacity="0.25" />
            </linearGradient>
          </defs>
          {replicas.slice(0, REPLICA_X.length).map((rp, i) => (
            <line
              key={`wire-${i}`}
              x1="120"
              y1="40"
              x2={REPLICA_X[i]}
              y2="150"
              stroke={wireStroke(rp.state)}
              strokeWidth="1.5"
              strokeDasharray="3 3"
            >
              <animate
                attributeName="stroke-dashoffset"
                from="0"
                to="-12"
                dur="1s"
                repeatCount="indefinite"
              />
            </line>
          ))}
          <circle
            cx="120"
            cy="40"
            r="22"
            fill="rgba(0,212,255,0.12)"
            stroke="#00d4ff"
            strokeWidth="1.5"
          />
          <text
            x="120"
            y="44"
            textAnchor="middle"
            fill="#00d4ff"
            fontSize="11"
            fontWeight="700"
            fontFamily="JetBrains Mono"
          >
            M
          </text>
          <text
            x="120"
            y="78"
            textAnchor="middle"
            fill="#c8ced6"
            fontSize="10"
            fontFamily="JetBrains Mono"
          >
            master
          </text>
          {replicas.slice(0, REPLICA_X.length).map((rp, i) => {
            const colors = markerColor(rp.state);
            return (
              <g key={`replica-${i}`}>
                <circle
                  cx={REPLICA_X[i]}
                  cy="160"
                  r="16"
                  fill={colors.fill}
                  stroke={colors.stroke}
                  strokeWidth="1.3"
                />
                <text
                  x={REPLICA_X[i]}
                  y="164"
                  textAnchor="middle"
                  fill={colors.stroke}
                  fontSize="10"
                  fontWeight="700"
                  fontFamily="JetBrains Mono"
                >
                  r{i + 1}
                </text>
              </g>
            );
          })}
        </svg>
      </div>
      <div className="panel-head" style={{ borderTop: '1px solid var(--border)' }}>
        <span>Nodes</span>
        <span className="title-count">
          {isLoading
            ? 'loading…'
            : error
              ? 'error'
              : `1 master · ${replicas.length} replicas`}
        </span>
      </div>
      <div className="panel-body">
        {master && (
          <div className="repl-node">
            <span className="marker healthy" />
            <div>
              <div className="name">{master.host}</div>
              <div className="sub">epoch {master.epoch}</div>
            </div>
            <span className="role-badge master">master</span>
          </div>
        )}
        {replicas.map((rp, i) => (
          <div key={`${rp.host}-${i}`} className="repl-node">
            <span className={`marker ${rp.state === 'connected' ? 'healthy' : rp.state}`} />
            <div>
              <div className="name">{rp.host}</div>
              <div className="sub">
                epoch {rp.epoch} · lag {rp.lag_ms}ms · ack {rp.ack_ms}ms
              </div>
            </div>
            <span className="role-badge replica">replica</span>
          </div>
        ))}
      </div>
    </div>
  );
}
