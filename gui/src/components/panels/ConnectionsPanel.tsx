/**
 * ConnectionsPanel — list of saved connections with status dot,
 * name, URL, role badge, edit affordance, and a "+" button. Click
 * on an entry switches `currentConnectionId`; the pencil button
 * opens the dialog in edit mode. Every TanStack Query hook keyed
 * off `useApiBase()` invalidates and re-fetches against the new
 * server when the active connection changes.
 *
 * A lightweight per-row `/health` probe runs every 10 s so each
 * row's LED reflects reality without waiting for the user to flip
 * the active connection.
 */
import { useEffect, useState } from 'react';
import {
  useConnectionsStore,
  type Connection,
  type ConnectionStatus,
} from '../../stores/connectionsStore';
import { api, NexusApiError } from '../../services/api';
import { PlusIcon, SettingsIcon } from '../../icons';
import { ConnectionDialog } from './ConnectionDialog';

const PROBE_INTERVAL_MS = 10_000;
const PROBE_TIMEOUT_MS = 3_000;

async function probeOne(c: Connection): Promise<ConnectionStatus> {
  const ctrl = new AbortController();
  const t = setTimeout(() => ctrl.abort(), PROBE_TIMEOUT_MS);
  try {
    await api.health(c.url, { signal: ctrl.signal, apiKey: c.apiKey });
    return 'connected';
  } catch (err) {
    if (err instanceof NexusApiError) return 'error';
    return 'idle';
  } finally {
    clearTimeout(t);
  }
}

export function ConnectionsPanel() {
  const connections = useConnectionsStore((s) => s.connections);
  const currentId = useConnectionsStore((s) => s.currentConnectionId);
  const setCurrent = useConnectionsStore((s) => s.setCurrent);
  const setStatus = useConnectionsStore((s) => s.setStatus);

  const [dialog, setDialog] = useState<
    | { mode: 'create' }
    | { mode: 'edit'; conn: Connection }
    | null
  >(null);

  useEffect(() => {
    let cancelled = false;
    const tick = async () => {
      const snapshot = useConnectionsStore.getState().connections;
      await Promise.all(
        snapshot.map(async (c) => {
          const status = await probeOne(c);
          if (!cancelled) setStatus(c.id, status);
        }),
      );
    };
    tick();
    const id = setInterval(tick, PROBE_INTERVAL_MS);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, [setStatus]);

  return (
    <div className="panel">
      <div className="panel-head">
        <span>Connections</span>
        <span className="title-count">({connections.length})</span>
        <div className="grow" />
        <button
          className="hd-btn"
          type="button"
          title="New connection"
          aria-label="New connection"
          onClick={() => setDialog({ mode: 'create' })}
        >
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
            <button
              type="button"
              className="hd-btn conn-edit"
              title="Edit connection"
              aria-label="Edit connection"
              onClick={(e) => {
                e.stopPropagation();
                setDialog({ mode: 'edit', conn: c });
              }}
            >
              <SettingsIcon />
            </button>
          </div>
        ))}
      </div>

      {dialog?.mode === 'create' && (
        <ConnectionDialog mode="create" onClose={() => setDialog(null)} />
      )}
      {dialog?.mode === 'edit' && (
        <ConnectionDialog
          mode="edit"
          connection={dialog.conn}
          onClose={() => setDialog(null)}
        />
      )}
    </div>
  );
}
