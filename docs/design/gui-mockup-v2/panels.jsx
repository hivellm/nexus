// Left side panels — Connections + Schema browser + KNN panel + Audit + Replication

function ConnectionsPanel({ connections, onSelect }) {
  return (
    <div className="panel">
      <div className="panel-head">
        <span>Connections</span>
        <span className="title-count">({connections.length})</span>
        <div className="grow" />
        <button className="hd-btn" title="New connection"><Icon.Plus /></button>
      </div>
      <div className="panel-body">
        {connections.map((c, i) => (
          <div key={i} className={`conn ${c.current ? 'active' : ''}`} onClick={() => onSelect && onSelect(c)}>
            <span className={`st ${c.status}`} />
            <div className="conn-meta">
              <div className="conn-name">{c.name}</div>
              <div className="conn-url">{c.url}</div>
            </div>
            <span className={`role ${c.role}`}>{c.role}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

function SchemaPanel({ labels, reltypes }) {
  const [open, setOpen] = React.useState({ nodes: true, rels: true, idx: true, fns: false });
  const toggle = (k) => setOpen(s => ({ ...s, [k]: !s[k] }));
  return (
    <div className="panel" style={{ borderTop: '1px solid var(--border)' }}>
      <div className="panel-head">
        <span>Schema</span>
        <span className="title-count">localhost:dev</span>
        <div className="grow" />
        <button className="hd-btn" title="Refresh"><Icon.Refresh /></button>
        <button className="hd-btn" title="Filter"><Icon.Filter /></button>
      </div>
      <div className="panel-body">

        <div className="schema-section">
          <div className="schema-group" onClick={() => toggle('nodes')}>
            {open.nodes ? <Icon.ChevronDown className="caret" /> : <Icon.ChevronRight className="caret" />}
            <span>Node Labels</span>
            <span className="group-count">{labels.length}</span>
          </div>
          {open.nodes && labels.map((l) => (
            <div key={l.id} className="schema-item" title={`CALL db.labels.${l.name}`}>
              <span className="chip" style={{ background: l.color }} />
              <span className="name mono">:{l.name}</span>
              <span className="count">{l.count.toLocaleString()}</span>
            </div>
          ))}
        </div>

        <div className="schema-section">
          <div className="schema-group" onClick={() => toggle('rels')}>
            {open.rels ? <Icon.ChevronDown className="caret" /> : <Icon.ChevronRight className="caret" />}
            <span>Relationship Types</span>
            <span className="group-count">{reltypes.length}</span>
          </div>
          {open.rels && reltypes.map((r) => (
            <div key={r.id} className="schema-item">
              <span className="chip ring" style={{ borderColor: 'var(--fg-2)' }} />
              <span className="name mono">[:{r.name}]</span>
              <span className="count">{r.count.toLocaleString()}</span>
            </div>
          ))}
        </div>

        <div className="schema-section">
          <div className="schema-group" onClick={() => toggle('idx')}>
            {open.idx ? <Icon.ChevronDown className="caret" /> : <Icon.ChevronRight className="caret" />}
            <span>Indexes</span>
            <span className="group-count">5</span>
          </div>
          {open.idx && [
            { n: 'label.bitmap', t: 'roaring', c: '5 labels' },
            { n: 'Module.name', t: 'btree • unique', c: '14' },
            { n: 'Function.body', t: 'tantivy · fulltext', c: '142' },
            { n: 'Function.embedding', t: 'hnsw · M=16', c: '142' },
            { n: 'Struct.name', t: 'btree', c: '38' },
          ].map((ix, i) => (
            <div key={i} className="schema-item">
              <span className="chip sq" style={{ background: 'var(--accent)' }} />
              <div className="name" style={{ display: 'flex', flexDirection: 'column', lineHeight: 1.3 }}>
                <span className="mono" style={{ fontSize: 12 }}>{ix.n}</span>
                <span style={{ fontSize: 10.5, color: 'var(--fg-3)' }}>{ix.t}</span>
              </div>
              <span className="count">{ix.c}</span>
            </div>
          ))}
        </div>

        <div className="schema-section">
          <div className="schema-group" onClick={() => toggle('fns')}>
            {open.fns ? <Icon.ChevronDown className="caret" /> : <Icon.ChevronRight className="caret" />}
            <span>Procedures</span>
            <span className="group-count">7</span>
          </div>
          {open.fns && [
            'vector.knn', 'text.search', 'db.labels',
            'db.stats', 'replication.status', 'replication.promote', 'graph.pattern_match',
          ].map((fn, i) => (
            <div key={i} className="schema-item">
              <span className="chip" style={{ background: 'var(--label-function)' }} />
              <span className="name mono" style={{ fontSize: 12 }}>{fn}()</span>
            </div>
          ))}
        </div>

      </div>
    </div>
  );
}

function LeftColumn({ connections, labels, reltypes, currentView }) {
  if (currentView === 'knn') return <KnnPanel />;
  if (currentView === 'replication') return <ReplicationLeftPanel />;
  if (currentView === 'audit') return <AuditLeftPanel />;
  return (
    <div className="panel" style={{ display: 'grid', gridTemplateRows: '220px 1fr' }}>
      <ConnectionsPanel connections={connections} />
      <SchemaPanel labels={labels} reltypes={reltypes} />
    </div>
  );
}

function KnnPanel() {
  const [k, setK] = React.useState(10);
  const results = [
    { name: 'parse_query', label: 'Function', sim: 0.942 },
    { name: 'plan', label: 'Function', sim: 0.891 },
    { name: 'optimize', label: 'Function', sim: 0.876 },
    { name: 'execute', label: 'Function', sim: 0.832 },
    { name: 'handle_cypher', label: 'Function', sim: 0.814 },
    { name: 'Engine', label: 'Struct', sim: 0.772 },
    { name: 'mcp_query', label: 'Function', sim: 0.754 },
    { name: 'NexusServer', label: 'Struct', sim: 0.711 },
    { name: 'handle_knn', label: 'Function', sim: 0.695 },
    { name: 'read_node', label: 'Function', sim: 0.641 },
  ].slice(0, k);
  const labelColor = (l) => ({
    Function: 'var(--label-function)', Struct: 'var(--label-struct)',
    Module: 'var(--label-module)', Trait: 'var(--label-trait)', Crate: 'var(--label-crate)',
  }[l]);
  return (
    <div className="panel">
      <div className="panel-head">
        <Icon.Vector style={{ color: 'var(--accent)' }} /> <span>KNN Vector Search</span>
      </div>
      <div className="knn-form">
        <div>
          <label>Label</label>
          <select defaultValue="Function">
            <option>Function</option><option>Struct</option><option>Module</option><option>Trait</option>
          </select>
        </div>
        <div>
          <label>Embedding source</label>
          <textarea defaultValue="// query plan execution with cost-based optimization&#10;// prefer physical operators over logical"/>
        </div>
        <div className="row-inline">
          <div>
            <label>Distance</label>
            <select defaultValue="cosine"><option>cosine</option><option>euclidean</option></select>
          </div>
          <div>
            <label>ef_search</label>
            <input defaultValue="64" />
          </div>
        </div>
        <div>
          <label>k = <output>{k}</output></label>
          <div className="k-range">
            <input type="range" min="1" max="50" value={k} onChange={e => setK(+e.target.value)} />
          </div>
        </div>
        <button className="btn primary" style={{ justifyContent: 'center' }}>
          <Icon.Play /> Run KNN <span className="kbd">⌘↵</span>
        </button>
      </div>
      <div className="panel-head" style={{ borderTop: '1px solid var(--border)' }}>
        <span>Results</span>
        <span className="title-count">{results.length} • 11.4 ms</span>
      </div>
      <div className="knn-results">
        {results.map((r, i) => (
          <div key={i} className="knn-row">
            <span className="rank">#{i + 1}</span>
            <div className="main">
              <span className="dt" style={{ background: labelColor(r.label) }} />
              <span className="nm">{r.name}</span>
            </div>
            <span className="sim">{r.sim.toFixed(3)}</span>
            <div className="knn-bar"><span style={{ width: `${r.sim * 100}%` }} /></div>
          </div>
        ))}
      </div>
    </div>
  );
}

function ReplicationLeftPanel() {
  const r = window.REPLICATION;
  return (
    <div className="panel">
      <div className="panel-head"><Icon.Replication /> <span>Replication</span></div>
      <div className="repl-topo">
        <svg width="240" height="200" viewBox="0 0 240 200">
          <defs>
            <linearGradient id="wire" x1="0" x2="1">
              <stop offset="0%" stopColor="#00d4ff" stopOpacity="0.9"/>
              <stop offset="100%" stopColor="#00d4ff" stopOpacity="0.25"/>
            </linearGradient>
          </defs>
          <line x1="120" y1="40" x2="50" y2="150" stroke="url(#wire)" strokeWidth="1.5" strokeDasharray="3 3"><animate attributeName="stroke-dashoffset" from="0" to="-12" dur="1s" repeatCount="indefinite"/></line>
          <line x1="120" y1="40" x2="120" y2="150" stroke="#f59e0b" strokeWidth="1.5" strokeDasharray="3 3"><animate attributeName="stroke-dashoffset" from="0" to="-12" dur="1s" repeatCount="indefinite"/></line>
          <line x1="120" y1="40" x2="190" y2="150" stroke="url(#wire)" strokeWidth="1.5" strokeDasharray="3 3"><animate attributeName="stroke-dashoffset" from="0" to="-12" dur="1s" repeatCount="indefinite"/></line>
          {/* master */}
          <circle cx="120" cy="40" r="22" fill="rgba(0,212,255,0.12)" stroke="#00d4ff" strokeWidth="1.5"/>
          <text x="120" y="44" textAnchor="middle" fill="#00d4ff" fontSize="11" fontWeight="700" fontFamily="JetBrains Mono">M</text>
          <text x="120" y="78" textAnchor="middle" fill="#c8ced6" fontSize="10" fontFamily="JetBrains Mono">master</text>
          {/* replicas */}
          {[{x:50,ok:true,n:'r1'},{x:120,ok:false,n:'r2'},{x:190,ok:true,n:'r3'}].map((rp,i)=>(
            <g key={i}>
              <circle cx={rp.x} cy="160" r="16" fill={rp.ok?'rgba(16,185,129,0.12)':'rgba(245,158,11,0.12)'} stroke={rp.ok?'#10b981':'#f59e0b'} strokeWidth="1.3"/>
              <text x={rp.x} y="164" textAnchor="middle" fill={rp.ok?'#10b981':'#f59e0b'} fontSize="10" fontWeight="700" fontFamily="JetBrains Mono">{rp.n}</text>
            </g>
          ))}
        </svg>
      </div>
      <div className="panel-head" style={{ borderTop: '1px solid var(--border)' }}>
        <span>Nodes</span><span className="title-count">1 master · 3 replicas</span>
      </div>
      <div className="panel-body">
        <div className="repl-node">
          <span className="marker healthy" />
          <div>
            <div className="name">{r.master.host}</div>
            <div className="sub">epoch {r.master.epoch} · WAL {r.master.wal}</div>
          </div>
          <span className="role-badge master">master</span>
        </div>
        {r.replicas.map((rp, i) => (
          <div key={i} className="repl-node">
            <span className={`marker ${rp.status}`} />
            <div>
              <div className="name">{rp.host}</div>
              <div className="sub">epoch {rp.epoch} · lag {rp.lag}ms · ack {rp.ackMs}ms</div>
            </div>
            <span className="role-badge replica">replica</span>
          </div>
        ))}
        <div style={{ padding: 12, display: 'flex', gap: 8 }}>
          <button className="btn" style={{ flex: 1 }}><Icon.Refresh /> Resync</button>
          <button className="btn" style={{ flex: 1 }}>Promote…</button>
        </div>
      </div>
    </div>
  );
}

function AuditLeftPanel() {
  return (
    <div className="panel">
      <div className="panel-head">
        <Icon.Audit /> <span>Filters</span>
      </div>
      <div style={{ padding: 12, display: 'flex', flexDirection: 'column', gap: 10 }}>
        <div>
          <label style={{ fontSize: 10.5, textTransform: 'uppercase', letterSpacing: '0.06em', color: 'var(--fg-3)', fontWeight: 600 }}>Level</label>
          <div className="tweaks seg" style={{ marginTop: 4 }}>
            <button className="on">All</button><button>Info</button><button>Warn</button><button>Error</button>
          </div>
        </div>
        <div>
          <label style={{ fontSize: 10.5, textTransform: 'uppercase', letterSpacing: '0.06em', color: 'var(--fg-3)', fontWeight: 600 }}>User</label>
          <select style={{ width: '100%', marginTop: 4, background: 'var(--bg-2)', border: '1px solid var(--border)', borderRadius: 4, padding: '6px 8px', color: 'var(--fg-0)', fontFamily: 'var(--font-ui)', fontSize: 12.5 }}>
            <option>all</option><option>admin</option><option>system</option><option>ingest-bot</option>
          </select>
        </div>
        <div>
          <label style={{ fontSize: 10.5, textTransform: 'uppercase', letterSpacing: '0.06em', color: 'var(--fg-3)', fontWeight: 600 }}>Action</label>
          <select style={{ width: '100%', marginTop: 4, background: 'var(--bg-2)', border: '1px solid var(--border)', borderRadius: 4, padding: '6px 8px', color: 'var(--fg-0)', fontFamily: 'var(--font-ui)', fontSize: 12.5 }}>
            <option>all</option><option>query.*</option><option>wal.*</option><option>replication.*</option>
          </select>
        </div>
      </div>
      <div className="panel-head" style={{ borderTop: '1px solid var(--border)' }}>
        <span>Query History</span><span className="title-count">6</span>
      </div>
      <div className="panel-body">
        {window.QUERY_HISTORY.map(q => (
          <div key={q.id} style={{ padding: '8px 12px', borderBottom: '1px solid var(--border)', fontFamily: 'var(--font-mono)', fontSize: 11.5, cursor: 'pointer', color: 'var(--fg-1)' }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', color: 'var(--fg-3)', fontSize: 10.5, marginBottom: 3 }}>
              <span>{q.ts}</span>
              <span>{q.ms}ms · {q.rows} rows</span>
            </div>
            <div style={{ whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{q.query}</div>
          </div>
        ))}
      </div>
    </div>
  );
}

Object.assign(window, { LeftColumn, ConnectionsPanel, SchemaPanel, KnnPanel, ReplicationLeftPanel, AuditLeftPanel });
