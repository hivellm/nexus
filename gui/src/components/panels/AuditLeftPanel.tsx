/**
 * AuditLeftPanel — filters (level / user / action) on top, query
 * history feed below. The local query history is stored in
 * `useQueryHistoryStore`; the server's `/audit/log` feed renders
 * in the right drawer's `AuditFeed` (item 7.6).
 */
import { useMemo, useState } from 'react';
import { useQueryHistoryStore } from '../../stores/queryHistoryStore';
import { AuditIcon } from '../../icons';
import type { AuditLevel } from '../../types/api';

const LEVEL_FILTERS: ReadonlyArray<{ id: 'all' | AuditLevel; label: string }> = [
  { id: 'all', label: 'All' },
  { id: 'info', label: 'Info' },
  { id: 'warn', label: 'Warn' },
  { id: 'error', label: 'Error' },
];

export function AuditLeftPanel() {
  const entries = useQueryHistoryStore((s) => s.entries);
  const [level, setLevel] = useState<'all' | AuditLevel>('all');
  const [user, setUser] = useState('all');
  const [action, setAction] = useState('all');

  const filtered = useMemo(() => {
    return entries.filter((e) => {
      if (level !== 'all') {
        const entryLevel: AuditLevel = e.ok ? 'info' : 'error';
        if (entryLevel !== level) return false;
      }
      // User + action filters apply against query metadata; with
      // the current local-history shape every entry is the active
      // GUI user running Cypher, so non-`all` selectors fall
      // through to "no match" which keeps the UI honest.
      if (user !== 'all') return false;
      if (action !== 'all' && !e.query.toLowerCase().includes(action.replace('.*', ''))) {
        return false;
      }
      return true;
    });
  }, [entries, level, user, action]);

  const labelStyle = {
    fontSize: 10.5,
    textTransform: 'uppercase' as const,
    letterSpacing: '0.06em',
    color: 'var(--fg-3)',
    fontWeight: 600,
  };
  const selectStyle = {
    width: '100%',
    marginTop: 4,
    background: 'var(--bg-2)',
    border: '1px solid var(--border)',
    borderRadius: 4,
    padding: '6px 8px',
    color: 'var(--fg-0)',
    fontFamily: 'var(--font-ui)',
    fontSize: 12.5,
  };

  return (
    <div className="panel">
      <div className="panel-head">
        <AuditIcon />
        <span>Filters</span>
      </div>
      <div style={{ padding: 12, display: 'flex', flexDirection: 'column', gap: 10 }}>
        <div>
          <label style={labelStyle}>Level</label>
          <div className="seg" role="group" aria-label="Level filter" style={{ marginTop: 4 }}>
            {LEVEL_FILTERS.map((f) => (
              <button
                key={f.id}
                type="button"
                className={level === f.id ? 'on' : ''}
                onClick={() => setLevel(f.id)}
                aria-pressed={level === f.id}
              >
                {f.label}
              </button>
            ))}
          </div>
        </div>
        <div>
          <label style={labelStyle} htmlFor="audit-user">
            User
          </label>
          <select
            id="audit-user"
            style={selectStyle}
            value={user}
            onChange={(e) => setUser(e.target.value)}
          >
            <option value="all">all</option>
            <option value="admin">admin</option>
            <option value="system">system</option>
          </select>
        </div>
        <div>
          <label style={labelStyle} htmlFor="audit-action">
            Action
          </label>
          <select
            id="audit-action"
            style={selectStyle}
            value={action}
            onChange={(e) => setAction(e.target.value)}
          >
            <option value="all">all</option>
            <option value="match">match.*</option>
            <option value="create">create.*</option>
            <option value="merge">merge.*</option>
            <option value="delete">delete.*</option>
          </select>
        </div>
      </div>
      <div className="panel-head" style={{ borderTop: '1px solid var(--border)' }}>
        <span>Query History</span>
        <span className="title-count">{filtered.length}</span>
      </div>
      <div className="panel-body">
        {filtered.length === 0 ? (
          <div style={{ padding: '12px', color: 'var(--fg-3)', fontSize: 12 }}>
            No queries match the current filters.
          </div>
        ) : (
          filtered.map((q) => (
            <div
              key={q.id}
              style={{
                padding: '8px 12px',
                borderBottom: '1px solid var(--border)',
                fontFamily: 'var(--font-mono)',
                fontSize: 11.5,
                cursor: 'pointer',
                color: 'var(--fg-1)',
              }}
            >
              <div
                style={{
                  display: 'flex',
                  justifyContent: 'space-between',
                  color: 'var(--fg-3)',
                  fontSize: 10.5,
                  marginBottom: 3,
                }}
              >
                <span>{new Date(q.ts).toLocaleTimeString()}</span>
                <span>
                  {q.ms}ms · {q.rows} rows{q.ok ? '' : ' · err'}
                </span>
              </div>
              <div
                style={{
                  whiteSpace: 'nowrap',
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                }}
              >
                {q.query}
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
