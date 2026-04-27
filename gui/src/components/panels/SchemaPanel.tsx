/**
 * SchemaPanel — collapsible Node Labels / Relationship Types /
 * Indexes / Procedures sections. Reads from `useSchema()`, which
 * fans out to four parallel REST calls and refreshes on a 30 s
 * cadence (or immediately after a Cypher mutation invalidates the
 * `schema` query key).
 */
import { useState } from 'react';
import { useSchema } from '../../services/queries';
import { ChevronDownIcon, ChevronRightIcon, FilterIcon, RefreshIcon } from '../../icons';

interface SectionState {
  nodes: boolean;
  rels: boolean;
  idx: boolean;
  fns: boolean;
}

const LABEL_PALETTE = [
  'var(--label-module)',
  'var(--label-function)',
  'var(--label-struct)',
  'var(--label-trait)',
  'var(--label-crate)',
] as const;

function colorForId(id: number): string {
  return LABEL_PALETTE[id % LABEL_PALETTE.length];
}

export function SchemaPanel() {
  const { data, isLoading, error, refetch } = useSchema();
  const [open, setOpen] = useState<SectionState>({
    nodes: true,
    rels: true,
    idx: true,
    fns: false,
  });
  const toggle = (k: keyof SectionState) => setOpen((s) => ({ ...s, [k]: !s[k] }));

  const labels = data?.labels.labels ?? [];
  const relTypes = data?.relTypes.types ?? [];
  const indexes = data?.indexes.indexes ?? [];
  const procedures = data?.procedures.procedures ?? [];

  return (
    <div className="panel" style={{ borderTop: '1px solid var(--border)' }}>
      <div className="panel-head">
        <span>Schema</span>
        <span className="title-count">
          {isLoading ? '…' : error ? 'error' : `${labels.length}L · ${relTypes.length}R`}
        </span>
        <div className="grow" />
        <button
          className="hd-btn"
          type="button"
          title="Refresh"
          aria-label="Refresh schema"
          onClick={() => refetch()}
        >
          <RefreshIcon />
        </button>
        <button className="hd-btn" type="button" title="Filter" aria-label="Filter schema">
          <FilterIcon />
        </button>
      </div>
      <div className="panel-body">
        <div className="schema-section">
          <div
            className="schema-group"
            onClick={() => toggle('nodes')}
            role="button"
            tabIndex={0}
            onKeyDown={(e) => {
              if (e.key === 'Enter' || e.key === ' ') toggle('nodes');
            }}
          >
            {open.nodes ? <ChevronDownIcon className="caret" /> : <ChevronRightIcon className="caret" />}
            <span>Node Labels</span>
            <span className="group-count">{labels.length}</span>
          </div>
          {open.nodes &&
            labels.map((l) => (
              <div key={l.id} className="schema-item" title={`:${l.name}`}>
                <span className="chip" style={{ background: colorForId(l.id) }} />
                <span className="name mono">:{l.name}</span>
                <span className="count">id={l.id}</span>
              </div>
            ))}
        </div>

        <div className="schema-section">
          <div
            className="schema-group"
            onClick={() => toggle('rels')}
            role="button"
            tabIndex={0}
            onKeyDown={(e) => {
              if (e.key === 'Enter' || e.key === ' ') toggle('rels');
            }}
          >
            {open.rels ? <ChevronDownIcon className="caret" /> : <ChevronRightIcon className="caret" />}
            <span>Relationship Types</span>
            <span className="group-count">{relTypes.length}</span>
          </div>
          {open.rels &&
            relTypes.map((r) => (
              <div key={r.id} className="schema-item">
                <span className="chip ring" style={{ borderColor: 'var(--fg-2)' }} />
                <span className="name mono">[:{r.name}]</span>
                <span className="count">id={r.id}</span>
              </div>
            ))}
        </div>

        <div className="schema-section">
          <div
            className="schema-group"
            onClick={() => toggle('idx')}
            role="button"
            tabIndex={0}
            onKeyDown={(e) => {
              if (e.key === 'Enter' || e.key === ' ') toggle('idx');
            }}
          >
            {open.idx ? <ChevronDownIcon className="caret" /> : <ChevronRightIcon className="caret" />}
            <span>Indexes</span>
            <span className="group-count">{indexes.length}</span>
          </div>
          {open.idx &&
            indexes.map((ix) => (
              <div key={ix.name} className="schema-item">
                <span className="chip sq" style={{ background: 'var(--accent)' }} />
                <div
                  className="name"
                  style={{ display: 'flex', flexDirection: 'column', lineHeight: 1.3 }}
                >
                  <span className="mono" style={{ fontSize: 12 }}>
                    {ix.name}
                  </span>
                  <span style={{ fontSize: 10.5, color: 'var(--fg-3)' }}>
                    {ix.type} · {ix.state}
                  </span>
                </div>
                <span className="count">{ix.properties.length}</span>
              </div>
            ))}
        </div>

        <div className="schema-section">
          <div
            className="schema-group"
            onClick={() => toggle('fns')}
            role="button"
            tabIndex={0}
            onKeyDown={(e) => {
              if (e.key === 'Enter' || e.key === ' ') toggle('fns');
            }}
          >
            {open.fns ? <ChevronDownIcon className="caret" /> : <ChevronRightIcon className="caret" />}
            <span>Procedures</span>
            <span className="group-count">{procedures.length}</span>
          </div>
          {open.fns &&
            procedures.map((p) => (
              <div key={p.name} className="schema-item" title={p.signature}>
                <span className="chip" style={{ background: 'var(--label-function)' }} />
                <span className="name mono" style={{ fontSize: 12 }}>
                  {p.name}()
                </span>
              </div>
            ))}
        </div>
      </div>
    </div>
  );
}
