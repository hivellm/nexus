// Right column — Live metrics, Replication status compact, Audit feed

function Sparkline({ data, color = '#00d4ff', height = 34, width = 120, fillOpacity = 0.2 }) {
  const max = Math.max(...data), min = Math.min(...data);
  const range = max - min || 1;
  const step = width / (data.length - 1);
  const pts = data.map((v, i) => `${(i*step).toFixed(1)},${(height - ((v - min) / range) * height).toFixed(1)}`).join(' ');
  const area = `0,${height} ${pts} ${width},${height}`;
  return (
    <svg className="spark" width={width} height={height} viewBox={`0 0 ${width} ${height}`}>
      <polygon points={area} fill={color} fillOpacity={fillOpacity} />
      <polyline points={pts} fill="none" stroke={color} strokeWidth="1.3" />
      <circle cx={(data.length-1)*step} cy={height - ((data[data.length-1]-min)/range)*height} r="2" fill={color} />
    </svg>
  );
}

function MetricsSection() {
  const m = window.METRICS;
  const last = (arr) => arr[arr.length - 1];
  const prev = (arr) => arr[arr.length - 2];
  const delta = (arr) => {
    const p = prev(arr);
    if (!p || !isFinite(p)) return 0;
    return ((last(arr) - p) / p) * 100;
  };
  return (
    <div className="right-section">
      <div className="panel-head">
        <span>Live Metrics</span>
        <div className="grow" />
        <span style={{ fontSize: 10, color: 'var(--fg-3)', textTransform: 'none', letterSpacing: 0, fontWeight: 400 }}>
          <span style={{ display: 'inline-block', width: 6, height: 6, borderRadius: '50%', background: 'var(--ok)', marginRight: 4, boxShadow: '0 0 4px var(--ok)' }}/>
          streaming • 1s
        </span>
      </div>
      <div style={{ flex: 1, overflow: 'auto' }}>
        <div className="metric">
          <div>
            <div className="m-label">queries / sec</div>
            <div className="m-val">{Math.round(last(m.qps))}<span className="unit">q/s</span></div>
            <div className={`m-delta ${delta(m.qps) < 0 ? 'down' : ''}`}>{delta(m.qps) >= 0 ? '▲' : '▼'} {Math.abs(delta(m.qps)).toFixed(1)}%</div>
          </div>
          <Sparkline data={m.qps} color="#00d4ff" />
        </div>
        <div className="metric">
          <div>
            <div className="m-label">cache hit rate</div>
            <div className="m-val">{last(m.cacheHit).toFixed(1)}<span className="unit">%</span></div>
            <div className="m-delta">▲ 0.3%</div>
          </div>
          <Sparkline data={m.cacheHit} color="#10b981" />
        </div>
        <div className="metric">
          <div>
            <div className="m-label">p99 latency</div>
            <div className="m-val">{last(m.p99Latency).toFixed(1)}<span className="unit">ms</span></div>
            <div className={`m-delta ${delta(m.p99Latency) > 0 ? 'down' : ''}`}>{delta(m.p99Latency) > 0 ? '▲' : '▼'} {Math.abs(delta(m.p99Latency)).toFixed(1)}%</div>
          </div>
          <Sparkline data={m.p99Latency} color="#f59e0b" />
        </div>
        <div className="metric">
          <div>
            <div className="m-label">WAL size</div>
            <div className="m-val">{last(m.walSize).toFixed(1)}<span className="unit">MB</span></div>
            <div className="m-delta down">▲ 0.1%</div>
          </div>
          <Sparkline data={m.walSize} color="#a78bfa" />
        </div>
      </div>
    </div>
  );
}

function ReplicationCompact() {
  const r = window.REPLICATION;
  return (
    <div className="right-section">
      <div className="panel-head">
        <Icon.Replication /> <span>Replication</span>
        <div className="grow" />
        <span style={{ fontSize: 10, color: 'var(--fg-3)', textTransform: 'none', letterSpacing: 0, fontWeight: 400 }}>async · 3/3 acking</span>
      </div>
      <div style={{ padding: 0, flex: 1, overflow: 'auto' }}>
        <div className="repl-node">
          <span className="marker healthy" />
          <div>
            <div className="name">{r.master.host}</div>
            <div className="sub">epoch {r.master.epoch} · {r.master.wal}</div>
          </div>
          <span className="role-badge master">master</span>
        </div>
        {r.replicas.map((rp, i) => (
          <div key={i} className="repl-node">
            <span className={`marker ${rp.status}`} />
            <div>
              <div className="name">{rp.host.replace('nexus-','').replace('.prod','')}</div>
              <div className="sub">lag {rp.lag}ms · ack {rp.ackMs}ms</div>
            </div>
            <span className="role-badge replica">replica</span>
          </div>
        ))}
      </div>
    </div>
  );
}

function AuditFeed() {
  return (
    <div className="right-section">
      <div className="panel-head">
        <Icon.Audit /> <span>Activity</span>
        <div className="grow" />
        <button className="hd-btn"><Icon.Filter /></button>
        <button className="hd-btn"><Icon.More /></button>
      </div>
      <div style={{ flex: 1, overflow: 'auto' }}>
        {window.AUDIT_LOG.map((a, i) => (
          <div key={i} className="audit-row">
            <span className="ts">{a.ts}</span>
            <span className={`dot ${a.level}`} />
            <span className="msg">
              <span className="usr">{a.user}</span>{' '}
              <span className="act">{a.action}</span>{' '}
              <span className="det">{a.detail}</span>
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

function RightColumn() {
  return (
    <div className="right-col">
      <MetricsSection />
      <ReplicationCompact />
      <AuditFeed />
    </div>
  );
}

Object.assign(window, { RightColumn, Sparkline, MetricsSection, ReplicationCompact, AuditFeed });
