/**
 * KnnPanel — vector-similarity search form. Renders the form for
 * label / embedding source / distance / `ef_search` / k slider, and
 * a Run button bound to `useKnn()`. Results render below with a
 * similarity bar per hit.
 *
 * Embedding source is parsed as a JSON array of numbers; if the
 * user types prose instead the form rejects with a clear message
 * (the GUI does not embed text client-side; that flow is the
 * `vector.knn` procedure on the server).
 */
import { useState } from 'react';
import { useKnn } from '../../services/queries';
import { PlayIcon, VectorIcon } from '../../icons';
import type { KnnHit } from '../../types/api';

const LABEL_PALETTE: Record<string, string> = {
  Function: 'var(--label-function)',
  Struct: 'var(--label-struct)',
  Module: 'var(--label-module)',
  Trait: 'var(--label-trait)',
  Crate: 'var(--label-crate)',
};

function colorFor(label: string): string {
  return LABEL_PALETTE[label] ?? 'var(--accent)';
}

function parseVector(raw: string): number[] | null {
  const trimmed = raw.trim();
  if (!trimmed) return null;
  try {
    const parsed = JSON.parse(trimmed);
    if (!Array.isArray(parsed)) return null;
    if (!parsed.every((n) => typeof n === 'number' && Number.isFinite(n))) return null;
    return parsed;
  } catch {
    return null;
  }
}

export function KnnPanel() {
  const [label, setLabel] = useState('Function');
  const [source, setSource] = useState('[0.1, 0.2, 0.3]');
  const [distance, setDistance] = useState<'cosine' | 'euclidean' | 'dot'>('cosine');
  const [efSearch, setEfSearch] = useState(64);
  const [k, setK] = useState(10);
  const [parseError, setParseError] = useState<string | null>(null);
  const knn = useKnn();

  const handleRun = () => {
    const vector = parseVector(source);
    if (vector === null) {
      setParseError('Embedding must be a JSON array of finite numbers');
      return;
    }
    setParseError(null);
    knn.mutate({ label, vector, k, ef_search: efSearch, distance });
  };

  const hits: KnnHit[] = knn.data?.hits ?? [];

  return (
    <div className="panel">
      <div className="panel-head">
        <VectorIcon style={{ color: 'var(--accent)' }} />
        <span>KNN Vector Search</span>
      </div>
      <div className="knn-form">
        <div>
          <label htmlFor="knn-label">Label</label>
          <select id="knn-label" value={label} onChange={(e) => setLabel(e.target.value)}>
            <option>Function</option>
            <option>Struct</option>
            <option>Module</option>
            <option>Trait</option>
          </select>
        </div>
        <div>
          <label htmlFor="knn-source">Embedding (JSON array)</label>
          <textarea
            id="knn-source"
            value={source}
            onChange={(e) => setSource(e.target.value)}
            spellCheck={false}
          />
          {parseError && (
            <div style={{ color: 'var(--err)', fontSize: 11, marginTop: 4 }}>{parseError}</div>
          )}
        </div>
        <div className="row-inline">
          <div>
            <label htmlFor="knn-distance">Distance</label>
            <select
              id="knn-distance"
              value={distance}
              onChange={(e) => setDistance(e.target.value as typeof distance)}
            >
              <option value="cosine">cosine</option>
              <option value="euclidean">euclidean</option>
              <option value="dot">dot</option>
            </select>
          </div>
          <div>
            <label htmlFor="knn-ef">ef_search</label>
            <input
              id="knn-ef"
              type="number"
              min={1}
              value={efSearch}
              onChange={(e) => setEfSearch(Math.max(1, Number(e.target.value) || 1))}
            />
          </div>
        </div>
        <div>
          <label htmlFor="knn-k">
            k = <output>{k}</output>
          </label>
          <div className="k-range">
            <input
              id="knn-k"
              type="range"
              min={1}
              max={50}
              value={k}
              onChange={(e) => setK(Number(e.target.value))}
            />
          </div>
        </div>
        <button
          className="btn primary"
          type="button"
          onClick={handleRun}
          disabled={knn.isPending}
          style={{ justifyContent: 'center' }}
        >
          <PlayIcon /> {knn.isPending ? 'Running…' : 'Run KNN'}{' '}
          <span className="kbd">⌘↵</span>
        </button>
        {knn.error && (
          <div style={{ color: 'var(--err)', fontSize: 11 }}>
            {knn.error.message}
          </div>
        )}
      </div>
      <div className="panel-head" style={{ borderTop: '1px solid var(--border)' }}>
        <span>Results</span>
        <span className="title-count">
          {hits.length}
          {knn.data ? ` • ${knn.data.execution_time_ms.toFixed(1)} ms` : ''}
        </span>
      </div>
      <div className="knn-results">
        {hits.map((hit, i) => {
          const name =
            (hit.properties?.name as string | undefined) ?? `#${hit.node_id}`;
          return (
            <div key={`${hit.node_id}-${i}`} className="knn-row">
              <span className="rank">#{i + 1}</span>
              <div className="main">
                <span className="dt" style={{ background: colorFor(label) }} />
                <span className="nm">{name}</span>
              </div>
              <span className="sim">{hit.score.toFixed(3)}</span>
              <div className="knn-bar">
                <span style={{ width: `${Math.max(0, Math.min(1, hit.score)) * 100}%` }} />
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
