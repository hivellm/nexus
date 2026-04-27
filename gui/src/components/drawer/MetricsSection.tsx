/**
 * Right-drawer "Live metrics" card — four metric rows (qps, page
 * cache hit, p99 latency, WAL size) each with a Sparkline + the
 * scalar value + a simple delta-since-first-sample percentage.
 *
 * Reads from `metricsStore.rings` so re-renders are bounded by
 * ringbuffer mutations from `useMetricsPump`.
 */
import { useMetricsStore, type RingKey } from '../../stores/metricsStore';
import { Sparkline } from './Sparkline';

interface MetricRow {
  key: RingKey;
  label: string;
  format: (v: number) => string;
  color: string;
}

const ROWS: ReadonlyArray<MetricRow> = [
  {
    key: 'qps',
    label: 'Queries / s',
    format: (v) => v.toFixed(1),
    color: 'var(--accent)',
  },
  {
    key: 'pageCacheHitRate',
    label: 'Page cache hit',
    format: (v) => `${(v * 100).toFixed(1)}%`,
    color: 'var(--label-function)',
  },
  {
    key: 'p99LatencyMs',
    label: 'p99 latency',
    format: (v) => `${v.toFixed(1)} ms`,
    color: 'var(--warn)',
  },
  {
    key: 'walSizeMb',
    label: 'WAL size',
    format: (v) => `${v.toFixed(1)} MB`,
    color: 'var(--ok)',
  },
];

function deltaPct(series: number[]): number | null {
  if (series.length < 2) return null;
  const first = series[0];
  const last = series[series.length - 1];
  if (first === 0) return last === 0 ? 0 : 100;
  return ((last - first) / Math.abs(first)) * 100;
}

function fmt(n: number): string {
  if (n >= 1e9) return (n / 1e9).toFixed(1) + 'B';
  if (n >= 1e6) return (n / 1e6).toFixed(1) + 'M';
  if (n >= 1e3) return (n / 1e3).toFixed(1) + 'k';
  return String(n);
}

export function MetricsSection() {
  const rings = useMetricsStore((s) => s.rings);
  const nodes = useMetricsStore((s) => s.nodes);
  const edges = useMetricsStore((s) => s.edges);
  const labelCount = useMetricsStore((s) => s.labelCount);
  const relTypeCount = useMetricsStore((s) => s.relTypeCount);

  return (
    <section className="drawer-section">
      <header className="drawer-head">
        <span>Live metrics</span>
        <span className="drawer-sub">last 60 samples</span>
      </header>
      <div className="catalog-grid">
        <div className="catalog-cell">
          <span className="cv">{fmt(nodes)}</span>
          <span className="ck">nodes</span>
        </div>
        <div className="catalog-cell">
          <span className="cv">{fmt(edges)}</span>
          <span className="ck">edges</span>
        </div>
        <div className="catalog-cell">
          <span className="cv">{labelCount}</span>
          <span className="ck">labels</span>
        </div>
        <div className="catalog-cell">
          <span className="cv">{relTypeCount}</span>
          <span className="ck">rel types</span>
        </div>
      </div>
      <div className="metric-list">
        {ROWS.map((row) => {
          const series = rings[row.key];
          const hasData = series.length > 0;
          const last = hasData ? series[series.length - 1] : 0;
          const delta = deltaPct(series);
          return (
            <div key={row.key} className="metric-row">
              <div className="metric-head">
                <span className="metric-label">{row.label}</span>
                {delta !== null && (
                  <span
                    className={`metric-delta ${delta > 0 ? 'up' : delta < 0 ? 'down' : 'flat'}`}
                  >
                    {delta > 0 ? '+' : ''}{delta.toFixed(0)}%
                  </span>
                )}
                {!hasData && <span className="metric-delta flat">no data</span>}
              </div>
              <div className="metric-body">
                <span className="metric-value">
                  {hasData ? row.format(last) : '—'}
                </span>
                <Sparkline data={series} color={row.color} width={140} height={28} />
              </div>
            </div>
          );
        })}
      </div>
    </section>
  );
}
