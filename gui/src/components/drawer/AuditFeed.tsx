/**
 * Right-drawer audit feed — recent activity from `useAuditLog()`.
 * Server entries are interleaved with the local query history
 * (`queryHistoryStore`) so a fresh user without a running audit
 * stream still sees something useful — their own queries.
 *
 * The local entries are tagged with a "self" user; the server
 * entries pass through as-is.
 */
import { useMemo } from 'react';
import { useAuditLog } from '../../services/queries';
import { useQueryHistoryStore } from '../../stores/queryHistoryStore';
import type { AuditEntry, AuditLevel } from '../../types/api';

function fmtTime(ts: string): string {
  const d = new Date(ts);
  if (Number.isNaN(d.getTime())) return ts.slice(11, 19);
  return d.toLocaleTimeString(undefined, { hour12: false });
}

function shortQuery(q: string): string {
  const oneLine = q.replace(/\s+/g, ' ').trim();
  return oneLine.length > 60 ? oneLine.slice(0, 60) + '…' : oneLine;
}

export function AuditFeed() {
  const audit = useAuditLog();
  const localHistory = useQueryHistoryStore((s) => s.entries);

  const merged = useMemo<AuditEntry[]>(() => {
    const localAsAudit: AuditEntry[] = localHistory.slice(0, 30).map((h) => ({
      timestamp: h.ts,
      level: h.ok ? 'info' : 'error',
      user: 'self',
      action: h.ok ? 'query.run' : 'query.failed',
      detail: `${h.ms}ms · ${h.rows} rows · ${shortQuery(h.query)}`,
    }));
    const server = audit.data?.entries ?? [];
    const all = [...localAsAudit, ...server];
    all.sort((a, b) => (a.timestamp < b.timestamp ? 1 : -1));
    return all.slice(0, 50);
  }, [localHistory, audit.data]);

  return (
    <section className="drawer-section drawer-grow">
      <header className="drawer-head">
        <span>Audit</span>
        <span className="drawer-sub">
          {audit.isError ? 'server log unreachable' : `${merged.length} recent`}
        </span>
      </header>
      <div className="audit-list">
        {merged.length === 0 ? (
          <div className="audit-empty">No activity yet. Run a query to see entries here.</div>
        ) : (
          merged.map((e, i) => (
            <div className={`audit-row level-${e.level satisfies AuditLevel}`} key={`${e.timestamp}-${i}`}>
              <span className={`level-dot ${e.level}`} />
              <span className="audit-time mono">{fmtTime(e.timestamp)}</span>
              <span className="audit-user mono">{e.user}</span>
              <span className="audit-action">{e.action}</span>
              {e.detail && <div className="audit-detail mono">{e.detail}</div>}
            </div>
          ))
        )}
      </div>
    </section>
  );
}
