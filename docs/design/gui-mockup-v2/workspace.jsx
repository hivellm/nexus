// Cypher editor + Results (Graph + Table toggle) + Graph viz

function CypherEditor({ onRun }) {
  // Syntax-highlighted representation of the default query
  // MATCH (m:Module)-[r:DEPENDS_ON]->(d:Module)
  // WHERE m.layer > 1
  // RETURN m.name, d.name, r
  // ORDER BY m.layer DESC
  // LIMIT 100
  return (
    <div className="cypher-editor">
      <div className="cypher-code">
        <div className="cypher-gutter">
          <div>1</div><div>2</div><div className="active-line">3</div><div>4</div><div>5</div><div>6</div><div>7</div>
        </div>
        <div className="cypher-lines">
          <div className="ln">
            <span className="cmt">-- Find modules and their dependencies, ordered by layer</span>
          </div>
          <div className="ln">
            <span className="kw">MATCH</span>{' '}
            <span className="pun">(</span><span className="var">m</span><span className="op">:</span><span className="lbl">Module</span><span className="pun">)</span>
            <span className="op">-[</span><span className="var">r</span><span className="op">:</span><span className="str">DEPENDS_ON</span><span className="op">]-&gt;</span>
            <span className="pun">(</span><span className="var">d</span><span className="op">:</span><span className="lbl">Module</span><span className="pun">)</span>
          </div>
          <div className="ln active">
            <span className="kw">WHERE</span>{' '}
            <span className="var">m</span><span className="pun">.</span><span className="var">layer</span>{' '}
            <span className="op">&gt;</span>{' '}
            <span className="num">1</span>{' '}
            <span className="kw">AND</span>{' '}
            <span className="var">d</span><span className="pun">.</span><span className="var">name</span>{' '}
            <span className="op">=~</span>{' '}
            <span className="str">'.*store.*'</span>
            <span className="cypher-cursor" />
          </div>
          <div className="ln">
            <span className="kw">RETURN</span>{' '}
            <span className="var">m</span><span className="pun">.</span><span className="var">name</span><span className="pun">,</span>{' '}
            <span className="var">m</span><span className="pun">.</span><span className="var">layer</span><span className="pun">,</span>{' '}
            <span className="var">m</span><span className="pun">.</span><span className="var">path</span><span className="pun">,</span>{' '}
            <span className="fn">count</span><span className="pun">(</span><span className="var">d</span><span className="pun">)</span>{' '}
            <span className="kw">AS</span>{' '}
            <span className="var">deps</span>
          </div>
          <div className="ln">
            <span className="kw">ORDER BY</span>{' '}
            <span className="var">m</span><span className="pun">.</span><span className="var">layer</span>{' '}
            <span className="kw">ASC</span><span className="pun">,</span>{' '}
            <span className="var">deps</span>{' '}
            <span className="kw">DESC</span>
          </div>
          <div className="ln">
            <span className="kw">LIMIT</span> <span className="num">100</span>
          </div>
          <div className="ln" />
        </div>
      </div>
      <div className="editor-footer">
        <span className="stat">Parsed <strong>OK</strong></span>
        <span className="stat">Plan: <strong>Scan(:Module) → Expand[DEPENDS_ON] → Filter → Aggregate</strong></span>
        <span className="stat">Est. cost <strong>142</strong></span>
        <div className="grow" />
        <span className="stat">⌘+↵ to run · ⌘+/ to comment</span>
      </div>
    </div>
  );
}

function EditorHead({ currentConn, resultMode, onResultMode }) {
  return (
    <div className="editor-head">
      <div className="breadcrumb">
        <Icon.Database style={{ color: 'var(--fg-3)' }} />
        <strong>{currentConn.name}</strong>
        <Icon.ChevronRight style={{ color: 'var(--fg-4)' }} />
        <span>graph:code-dep</span>
        <Icon.ChevronRight style={{ color: 'var(--fg-4)' }} />
        <strong>query-1.cypher</strong>
      </div>
      <div className="grow" />
      <button className="btn ghost" title="Format"><Icon.Format /> Format</button>
      <button className="btn ghost" title="Save"><Icon.Save /></button>
      <button className="btn ghost" title="Share"><Icon.Share /></button>
      <button className="btn"><Icon.History /> History</button>
      <button className="btn primary">
        <Icon.Play /> Run <span className="kbd">⌘↵</span>
      </button>
    </div>
  );
}

function ResultsTabs({ mode, onMode, rowCount, ms }) {
  return (
    <div className="results-tabs">
      <button className={`result-tab ${mode==='graph'?'active':''}`} onClick={()=>onMode('graph')}>
        <Icon.Graph /> Graph <span className="badge">{window.NODES.length}</span>
      </button>
      <button className={`result-tab ${mode==='table'?'active':''}`} onClick={()=>onMode('table')}>
        <Icon.Table /> Table <span className="badge">{rowCount}</span>
      </button>
      <button className="result-tab">
        <Icon.Code /> JSON
      </button>
      <button className="result-tab">
        Plan
      </button>
      <div className="grow" />
      <div className="mini-meta">
        <span>planner <strong>heuristic</strong></span>
        <span>execution <strong>{ms}ms</strong></span>
        <span>records <strong>{rowCount.toLocaleString()}</strong></span>
        <span>rows/s <strong>{Math.round(rowCount*1000/ms).toLocaleString()}</strong></span>
        <button className="btn ghost" style={{ height: 24, padding: '0 8px' }}><Icon.Download /></button>
      </div>
    </div>
  );
}

function GraphView({ selected, setSelected }) {
  const [zoom, setZoom] = React.useState(1);
  const [pan, setPan] = React.useState({ x: 0, y: 0 });
  const dragRef = React.useRef(null);

  const labelColor = {
    Module: 'var(--label-module)', Function: 'var(--label-function)',
    Struct: 'var(--label-struct)', Trait: 'var(--label-trait)', Crate: 'var(--label-crate)',
  };
  const rawColor = {
    Module: '#00d4ff', Function: '#a78bfa', Struct: '#10b981', Trait: '#f59e0b', Crate: '#ff4d8f',
  };
  const nodeById = Object.fromEntries(window.NODES.map(n => [n.id, n]));

  // Viewbox & interaction
  const minX = 50, minY = -20, w = 880, h = 720;

  const onWheel = (e) => {
    e.preventDefault();
    setZoom(z => Math.min(3, Math.max(0.3, z - e.deltaY * 0.001)));
  };
  const onMouseDown = (e) => {
    if (e.target.closest('[data-node]')) return;
    dragRef.current = { x: e.clientX, y: e.clientY, pan: { ...pan } };
  };
  const onMouseMove = (e) => {
    if (!dragRef.current) return;
    setPan({
      x: dragRef.current.pan.x + (e.clientX - dragRef.current.x),
      y: dragRef.current.pan.y + (e.clientY - dragRef.current.y),
    });
  };
  const onMouseUp = () => { dragRef.current = null; };

  const nodeRadius = (n) => {
    if (n.label === 'Crate') return 22;
    if (n.label === 'Module') return 16;
    if (n.label === 'Struct') return 13;
    if (n.label === 'Trait') return 13;
    return 11;
  };

  return (
    <div className="graph-wrap" onWheel={onWheel} onMouseDown={onMouseDown} onMouseMove={onMouseMove} onMouseUp={onMouseUp} onMouseLeave={onMouseUp}>
      <svg viewBox={`${minX} ${minY} ${w} ${h}`} preserveAspectRatio="xMidYMid meet">
        <defs>
          <marker id="arr" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
            <path d="M0,0 L10,5 L0,10 z" fill="#3b4049" />
          </marker>
          <marker id="arr-hi" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
            <path d="M0,0 L10,5 L0,10 z" fill="#00d4ff" />
          </marker>
          <radialGradient id="nodeglow" cx="50%" cy="50%" r="50%">
            <stop offset="0%" stopColor="rgba(255,255,255,0.3)" />
            <stop offset="100%" stopColor="rgba(255,255,255,0)" />
          </radialGradient>
          <pattern id="grid" width="40" height="40" patternUnits="userSpaceOnUse">
            <path d="M 40 0 L 0 0 0 40" fill="none" stroke="rgba(255,255,255,0.025)" strokeWidth="0.5"/>
          </pattern>
        </defs>
        <rect x={minX} y={minY} width={w} height={h} fill="url(#grid)" />
        <g transform={`translate(${pan.x}, ${pan.y}) scale(${zoom})`} style={{ transformOrigin: '0 0' }}>
          {/* Edges */}
          {window.EDGES.map((e, i) => {
            const s = nodeById[e.s], d = nodeById[e.d];
            if (!s || !d) return null;
            const highlighted = selected && (selected.id === s.id || selected.id === d.id);
            // Curve: for pairs with multiple edges, vary slightly
            const dx = d.x - s.x, dy = d.y - s.y;
            const dist = Math.sqrt(dx*dx + dy*dy);
            const offset = (e.dup ? 8 : 0);
            const mx = (s.x + d.x) / 2 + (dy / dist) * offset;
            const my = (s.y + d.y) / 2 - (dx / dist) * offset;
            // Trim endpoints for arrow
            const rs = nodeRadius(s) + 2, rd = nodeRadius(d) + 4;
            const sx = s.x + (dx / dist) * rs;
            const sy = s.y + (dy / dist) * rs;
            const ex = d.x - (dx / dist) * rd;
            const ey = d.y - (dy / dist) * rd;

            const stroke = highlighted ? '#00d4ff' : (e.t === 'CALLS' ? '#3b4049' : e.t === 'DEPENDS_ON' ? '#4a5260' : '#2a313c');
            return (
              <g key={i} style={{ opacity: selected && !highlighted ? 0.22 : 1 }}>
                <path
                  d={`M ${sx} ${sy} Q ${mx} ${my} ${ex} ${ey}`}
                  fill="none"
                  stroke={stroke}
                  strokeWidth={highlighted ? 1.6 : 1}
                  markerEnd={highlighted ? 'url(#arr-hi)' : 'url(#arr)'}
                />
                {highlighted && (
                  <text x={mx} y={my - 3} className="edge-label" textAnchor="middle" fill="#00d4ff">
                    {e.t}
                  </text>
                )}
              </g>
            );
          })}

          {/* Nodes */}
          {window.NODES.map((n) => {
            const r = nodeRadius(n);
            const isSelected = selected && selected.id === n.id;
            const isHL = selected && window.EDGES.some(e =>
              (e.s === selected.id && e.d === n.id) || (e.d === selected.id && e.s === n.id)
            );
            const dim = selected && !isSelected && !isHL;
            return (
              <g
                key={n.id}
                data-node={n.id}
                transform={`translate(${n.x}, ${n.y})`}
                style={{ cursor: 'pointer', opacity: dim ? 0.28 : 1, transition: 'opacity 0.15s' }}
                onClick={(e) => { e.stopPropagation(); setSelected(n); }}
              >
                {isSelected && (
                  <circle r={r + 6} fill="none" stroke="#00d4ff" strokeWidth="1.5">
                    <animate attributeName="r" from={r+4} to={r+10} dur="1.4s" repeatCount="indefinite"/>
                    <animate attributeName="opacity" from="0.8" to="0" dur="1.4s" repeatCount="indefinite"/>
                  </circle>
                )}
                <circle r={r} fill={rawColor[n.label]} fillOpacity={isSelected ? 1 : 0.85}
                  stroke={isSelected ? '#fff' : 'rgba(0,0,0,0.4)'} strokeWidth={isSelected ? 2 : 1}
                  style={isHL || isSelected ? { filter: `drop-shadow(0 0 8px ${rawColor[n.label]})` } : {}}
                />
                <circle r={r - 3} fill="url(#nodeglow)" opacity="0.5" />
                <text
                  textAnchor="middle" y={r + 12}
                  fontSize={n.label === 'Crate' ? '11' : '10'}
                  fontFamily="JetBrains Mono"
                  fill={isSelected ? '#fff' : '#c8ced6'}
                  fontWeight={isSelected ? 600 : 500}
                  style={{ userSelect: 'none', pointerEvents: 'none' }}
                >
                  {n.name}
                </text>
              </g>
            );
          })}
        </g>
      </svg>

      {/* Controls */}
      <div className="graph-controls">
        <button onClick={() => setZoom(z => Math.min(3, z + 0.15))} title="Zoom in"><Icon.Plus /></button>
        <button onClick={() => setZoom(z => Math.max(0.3, z - 0.15))} title="Zoom out"><Icon.Minus /></button>
        <hr/>
        <button onClick={() => { setZoom(1); setPan({x:0,y:0}); }} title="Fit"><Icon.Fit /></button>
        <button title="Refresh layout"><Icon.Refresh /></button>
      </div>

      {/* Legend */}
      <div className="graph-legend">
        {window.NEXUS_LABELS.map(l => (
          <div key={l.id} className="lgd-row">
            <span className="chip" style={{ background: l.color }} /> :{l.name}
            <span style={{ color: 'var(--fg-4)', marginLeft: 'auto' }}>{l.count}</span>
          </div>
        ))}
        <div style={{ borderTop: '1px solid var(--border)', marginTop: 4, paddingTop: 4, color: 'var(--fg-3)', fontSize: 10 }}>
          zoom {zoom.toFixed(2)}× · drag to pan
        </div>
      </div>

      {/* Inspector */}
      {selected && (
        <div className="node-inspector">
          <div className="insp-head">
            <span className="nd" style={{ background: rawColor[selected.label] }} />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ display: 'flex', gap: 6, alignItems: 'center' }}>
                <span className="lbl">:{selected.label}</span>
                <span style={{ fontSize: 10.5, color: 'var(--fg-3)', fontFamily: 'var(--font-mono)' }}>#{selected.id}</span>
              </div>
              <div className="nm">{selected.name}</div>
            </div>
            <button className="hd-btn" onClick={() => setSelected(null)}><Icon.Close /></button>
          </div>
          <div className="insp-body">
            <dl>
              <dt>name</dt><dd>"{selected.name}"</dd>
              {Object.entries(selected.props).map(([k, v]) => (
                <React.Fragment key={k}>
                  <dt>{k}</dt><dd>{typeof v === 'string' ? `"${v}"` : String(v)}</dd>
                </React.Fragment>
              ))}
              <dt>degree</dt>
              <dd>
                {window.EDGES.filter(e => e.s === selected.id).length} out ·{' '}
                {window.EDGES.filter(e => e.d === selected.id).length} in
              </dd>
            </dl>
            <div style={{ display: 'flex', gap: 6, marginTop: 12 }}>
              <button className="btn" style={{ flex: 1, height: 26, fontSize: 11 }}>Expand</button>
              <button className="btn ghost" style={{ height: 26, fontSize: 11 }}>Cypher</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function TableView() {
  const rows = window.RESULT_ROWS;
  return (
    <div className="tbl-wrap">
      <table className="tbl">
        <thead>
          <tr>
            <th style={{ width: 40 }}>#</th>
            <th><span className="th-inner">m.name <span className="ty">String</span></span></th>
            <th><span className="th-inner">m.layer <span className="ty">Int</span></span></th>
            <th><span className="th-inner">m.path <span className="ty">String</span></span></th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r, i) => (
            <tr key={i}>
              <td><span className="row-idx">{i+1}</span></td>
              <td style={{ color: 'var(--label-module)' }}>"{r['m.name']}"</td>
              <td className="num">{r['m.layer']}</td>
              <td style={{ color: 'var(--fg-1)' }}>"{r['m.path']}"</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

Object.assign(window, { CypherEditor, EditorHead, ResultsTabs, GraphView, TableView });
