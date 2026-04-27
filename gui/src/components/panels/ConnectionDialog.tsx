/**
 * Modal dialog for creating / editing a Nexus connection. Used by
 * `ConnectionsPanel` from both the "+" header button (new) and the
 * per-row pencil affordance (edit). Saves through
 * `connectionsStore.upsert`; delete is exposed in edit mode only.
 *
 * URL validation is intentionally permissive — the field accepts
 * anything `URL()` can parse so non-standard ports / hostnames /
 * protocols work without a custom whitelist.
 */
import { useEffect, useState } from 'react';
import {
  useConnectionsStore,
  type Connection,
  type ConnectionRole,
} from '../../stores/connectionsStore';
import { CloseIcon } from '../../icons';

interface ConnectionDialogProps {
  mode: 'create' | 'edit';
  connection?: Connection;
  onClose: () => void;
}

const ROLES: ReadonlyArray<{ id: ConnectionRole; label: string }> = [
  { id: 'standalone', label: 'Standalone' },
  { id: 'master', label: 'Master' },
  { id: 'replica', label: 'Replica' },
];

function genId(name: string): string {
  const slug = name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '')
    .slice(0, 24);
  return `${slug || 'conn'}-${Math.random().toString(36).slice(2, 6)}`;
}

function isValidUrl(s: string): boolean {
  try {
    const u = new URL(s);
    return u.protocol === 'http:' || u.protocol === 'https:';
  } catch {
    return false;
  }
}

export function ConnectionDialog({ mode, connection, onClose }: ConnectionDialogProps) {
  const upsert = useConnectionsStore((s) => s.upsert);
  const remove = useConnectionsStore((s) => s.remove);
  const setCurrent = useConnectionsStore((s) => s.setCurrent);

  const [name, setName] = useState(connection?.name ?? '');
  const [url, setUrl] = useState(connection?.url ?? 'http://localhost:15474');
  const [role, setRole] = useState<ConnectionRole>(connection?.role ?? 'standalone');
  const [apiKey, setApiKey] = useState(connection?.apiKey ?? '');
  const [showKey, setShowKey] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);

  const handleSave = () => {
    const trimmedName = name.trim();
    const trimmedUrl = url.trim().replace(/\/+$/, '');
    if (!trimmedName) {
      setError('Name is required');
      return;
    }
    if (!isValidUrl(trimmedUrl)) {
      setError('URL must be http(s)://host:port');
      return;
    }
    const next: Connection = {
      id: connection?.id ?? genId(trimmedName),
      name: trimmedName,
      url: trimmedUrl,
      role,
      status: connection?.status ?? 'idle',
      apiKey: apiKey.trim() || undefined,
    };
    upsert(next);
    if (mode === 'create') setCurrent(next.id);
    onClose();
  };

  const handleDelete = () => {
    if (!connection) return;
    remove(connection.id);
    onClose();
  };

  return (
    <div
      className="conn-dialog-backdrop"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="conn-dialog" role="dialog" aria-label="Connection">
        <div className="cd-head">
          <span className="grow">{mode === 'create' ? 'New connection' : 'Edit connection'}</span>
          <button type="button" className="hd-btn" aria-label="Close" onClick={onClose}>
            <CloseIcon />
          </button>
        </div>
        <div className="cd-body">
          <label className="cd-row">
            <span>Name</span>
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              autoFocus
            />
          </label>
          <label className="cd-row">
            <span>URL</span>
            <input
              type="text"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              spellCheck={false}
            />
          </label>
          <label className="cd-row">
            <span>Role</span>
            <select value={role} onChange={(e) => setRole(e.target.value as ConnectionRole)}>
              {ROLES.map((r) => (
                <option key={r.id} value={r.id}>{r.label}</option>
              ))}
            </select>
          </label>
          <label className="cd-row">
            <span>API key</span>
            <div className="cd-secret">
              <input
                type={showKey ? 'text' : 'password'}
                value={apiKey}
                onChange={(e) => setApiKey(e.target.value)}
                spellCheck={false}
                autoComplete="off"
              />
              <button
                type="button"
                className="btn ghost"
                onClick={() => setShowKey((v) => !v)}
              >
                {showKey ? 'Hide' : 'Show'}
              </button>
            </div>
          </label>
          {error && <div className="cd-error">{error}</div>}
        </div>
        <div className="cd-foot">
          {mode === 'edit' && (
            <button type="button" className="btn danger" onClick={handleDelete}>
              Delete
            </button>
          )}
          <div className="grow" />
          <button type="button" className="btn ghost" onClick={onClose}>
            Cancel
          </button>
          <button type="button" className="btn primary" onClick={handleSave}>
            {mode === 'create' ? 'Add' : 'Save'}
          </button>
        </div>
      </div>
    </div>
  );
}
