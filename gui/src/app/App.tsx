export function App() {
  return (
    <div className="app" data-screen-label="Nexus Studio — bootstrap">
      <header className="titlebar">
        <span className="brand">Nexus</span>
        <span className="sep">/</span>
        <span style={{ color: 'var(--fg-3)' }}>Graph Database Studio</span>
      </header>
      <div className="body">
        <aside className="rail" aria-label="Activity rail" />
        <aside className="panel" aria-label="Left panel" />
        <main className="workspace" />
        <aside className="right-col" aria-label="Right drawer" />
      </div>
      <footer className="statusbar">
        <span className="sg">
          <span className="led" /> bootstrap
        </span>
      </footer>
    </div>
  );
}
