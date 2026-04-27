/**
 * Live server-metrics state — drives titlebar pills, statusbar
 * pills, and the right-drawer sparklines.
 *
 * Keeps two layers:
 *
 * 1. **Latest snapshot** — scalar fields (`qps`, `pageCacheHitRate`,
 *    …). Used by chrome that just needs the current value.
 * 2. **Ringbuffers** — 60-sample histories per metric, fed by the
 *    polling pump (`useMetricsPump`). Used by `Sparkline.tsx` to
 *    render a 60-second mini chart.
 *
 * The store stays a dumb cache so subtrees subscribed to a single
 * field don't re-render when an unrelated field updates. The pump
 * lives in a hook (not a setInterval inside the store) so React's
 * lifecycle owns the cancellation path.
 */
import { create } from 'zustand';

export interface MetricsSnapshot {
  epoch: number;
  qps: number;
  replicaState: string;
  replLagMs: number;
  pageCacheHitRate: number;
  walSizeMb: number;
  p99LatencyMs: number;
  nodes: number;
  edges: number;
  labelCount: number;
  relTypeCount: number;
}

export const RING_SIZE = 60;

export type RingKey = 'qps' | 'pageCacheHitRate' | 'p99LatencyMs' | 'walSizeMb';

type Rings = Record<RingKey, number[]>;

/** A ring sample is `null` when the server didn't provide that
 * metric on the latest poll — components that read the ring
 * should treat consecutive nulls as "no data" and render a dash
 * instead of a flat-zero sparkline. */
export type MaybeSample = Partial<Record<RingKey, number | null>>;

interface MetricsState extends MetricsSnapshot {
  rings: Rings;
  setSnapshot: (snap: Partial<MetricsSnapshot>) => void;
  pushSample: (sample: MaybeSample) => void;
}

const INITIAL: MetricsSnapshot = {
  epoch: 0,
  qps: 0,
  replicaState: '0/0',
  replLagMs: 0,
  pageCacheHitRate: 0,
  walSizeMb: 0,
  p99LatencyMs: 0,
  nodes: 0,
  edges: 0,
  labelCount: 0,
  relTypeCount: 0,
};

const EMPTY_RINGS: Rings = {
  qps: [],
  pageCacheHitRate: [],
  p99LatencyMs: [],
  walSizeMb: [],
};

function appendCapped(arr: number[], v: number): number[] {
  const next = arr.length >= RING_SIZE ? arr.slice(arr.length - RING_SIZE + 1) : arr.slice();
  next.push(v);
  return next;
}

export const useMetricsStore = create<MetricsState>()((set) => ({
  ...INITIAL,
  rings: EMPTY_RINGS,
  setSnapshot: (snap) => set(snap),
  pushSample: (sample) =>
    set((s) => ({
      rings: {
        qps: typeof sample.qps === 'number' ? appendCapped(s.rings.qps, sample.qps) : s.rings.qps,
        pageCacheHitRate:
          typeof sample.pageCacheHitRate === 'number'
            ? appendCapped(s.rings.pageCacheHitRate, sample.pageCacheHitRate)
            : s.rings.pageCacheHitRate,
        p99LatencyMs:
          typeof sample.p99LatencyMs === 'number'
            ? appendCapped(s.rings.p99LatencyMs, sample.p99LatencyMs)
            : s.rings.p99LatencyMs,
        walSizeMb:
          typeof sample.walSizeMb === 'number'
            ? appendCapped(s.rings.walSizeMb, sample.walSizeMb)
            : s.rings.walSizeMb,
      },
    })),
}));
