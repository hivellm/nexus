/**
 * ConnectionsPanel — list of saved connections with status dot,
 * name, URL, role badge, and a "+" button. Click on an entry
 * switches `currentConnectionId`; every TanStack Query hook keyed
 * off `useApiBase()` invalidates and re-fetches against the new
 * server.
 */
import {
  useConnectionsStore,
  type Connection,
} from '../../stores/connectionsStore';
import { PlusIcon } from '../../icons';

export function ConnectionsPanel() {
  const connections = useConnectionsStore((s) => s.connections);
  const currentId = useConnectionsStore((s) => s.currentConnectionId);
  const setCurrent = useConnectionsStore((s) => s.setCurrent);

  return (
    <div className="panel">
      <div className="panel-head">
        <span>Connections</span>
        <span className="title-count">({connections.length})</span>
        <div className="grow" />
        <button className="hd-btn" type="button" title="New connection" aria-label="New connection">
          <PlusIcon />
        </button>
      </div>
      <div className="panel-body">
        {connections.map((c: Connection) => (
          <div
            key={c.id}
            className={`conn ${c.id === currentId ? 'active' : ''}`}
            onClick={() => setCurrent(c.id)}
            role="button"
            tabIndex={0}
            onKeyDown={(e) => {
              if (e.key === 'Enter' || e.key === ' ') {
                e.preventDefault();
                setCurrent(c.id);
              }
            }}
          >
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
