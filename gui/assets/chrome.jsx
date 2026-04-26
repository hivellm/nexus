// Titlebar + Activity rail + Status bar

function Titlebar({ currentTab, onTabChange, currentConn }) {
  const tabs = [
    { id: 'cypher-1', name: 'query-1.cypher', icon: <Icon.Code /> },
    { id: 'cypher-2', name: 'knn-seed.cypher', icon: <Icon.Code /> },
    { id: 'scratch', name: 'scratch.cypher', icon: <Icon.Code /> },
  ];
  return (
    <div className="titlebar">
      <div className="traffic">
        <button className="dot close" aria-label="close" />
        <button className="dot min" aria-label="minimize" />
        <button className="dot max" aria-label="maximize" />
      </div>
      <div className="brand">
        <div className="brand-mark" />
        <span>Nexus</span>
      </div>
      <span className="sep">/</span>
      <div className="path">
        <span className="host">{currentConn.name}</span>
        <span className="sep"> · </span>
        <span>graph:code-dep</span>
      </div>

      <div className="tabs">
        {tabs.map(t => (
          <div
            key={t.id}
            className={`tab ${currentTab === t.id ? 'active' : ''}`}
            onClick={() => onTabChange(t.id)}
          >
            {t.icon}
            <span>{t.name}</span>
            <span className="close-x"><Icon.Close /></span>
          </div>
        ))}
        <div className="tab" style={{ padding: '0 10px', color: 'var(--fg-3)' }}>
          <Icon.Plus />
        </div>
      </div>

      <div className="grow" />

      <div className="search-field">
        <Icon.Search style={{ color: 'var(--fg-3)' }} />
        <input placeholder="Search nodes, relationships, queries…" />
        <span className="kbd">⌘K</span>
      </div>

      <div className="pill">
        <span className="led" />
        <span>epoch 8422</span>
      </div>
      <div className="pill" title="442 queries/sec">
        <span>442 q/s</span>
      </div>
      <button className="icon-btn" title="Notifications"><Icon.Bell /></button>
      <button className="icon-btn" title="Settings"><Icon.Settings /></button>
    </div>
  );
}

function ActivityRail({ currentView, onViewChange, onToggleTweaks }) {
  const items = [
    { id: 'query', icon: <Icon.Code />, label: 'Cypher' },
    { id: 'schema', icon: <Icon.Database />, label: 'Schema' },
    { id: 'knn', icon: <Icon.Vector />, label: 'KNN Search' },
    { id: 'replication', icon: <Icon.Replication />, label: 'Replication' },
    { id: 'audit', icon: <Icon.Audit />, label: 'Audit Log' },
  ];
  return (
    <div className="rail">
      {items.map(it => (
        <button
          key={it.id}
          className={`rail-btn ${currentView === it.id ? 'active' : ''}`}
          onClick={() => onViewChange(it.id)}
          title={it.label}
        >
          {it.icon}
        </button>
      ))}
      <div className="spacer" />
      <button className="rail-btn" onClick={onToggleTweaks} title="Tweaks"><Icon.Settings /></button>
    </div>
  );
}

function StatusBar({ result, graphStats, replLag }) {
  return (
    <div className="statusbar">
      <span className="sg"><span className="led" /> Connected</span>
      <span className="sep">·</span>
      <span className="sg mono">localhost:15474</span>
      <span className="sep">·</span>
      <span className="sg">writer <strong>epoch 8422</strong></span>
      <span className="sep">·</span>
      <span className="sg">replicas <strong>3/3</strong> (max lag {replLag}ms)</span>
      <div className="grow" />
      <span className="sg">page cache <strong>94.2%</strong></span>
      <span className="sep">·</span>
      <span className="sg">WAL <strong>128.4 MB</strong></span>
      <span className="sep">·</span>
      <span className="sg clickable">|V| <strong>{graphStats.nodes.toLocaleString()}</strong></span>
      <span className="sep">·</span>
      <span className="sg clickable">|E| <strong>{graphStats.edges.toLocaleString()}</strong></span>
      <span className="sep">·</span>
      <span className="sg">Ln <strong>5</strong>, Col <strong>18</strong> · Cypher · UTF-8</span>
    </div>
  );
}

Object.assign(window, { Titlebar, ActivityRail, StatusBar });
