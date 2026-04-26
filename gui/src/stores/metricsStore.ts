/**
 * Live server-metrics state (titlebar pills, statusbar pills,
 * right-drawer sparklines).
 *
 * Slice-1 scope: just the scalar values shown in chrome
 * (`epoch`, `qps`, `replicaState`, `pageCacheHitRate`,
 * `walSizeMb`, `nodes`, `edges`, `replLagMs`). Sparklines (60-sample
 * ringbuffers) land in §7 alongside `MetricsSection.tsx`.
 *
 * Mutated externally by the polling layer (`useStats` + friends in
 * `services/api.ts`); the store stays a dumb cache so React subtrees
 * subscribed to a single field don't re-render when an unrelated
 * field updates.
 */
import { create } from 'zustand';

export interface MetricsSnapshot {
  epoch: number;
  qps: number;
  replicaState: string;
  replLagMs: number;
  pageCacheHitRate: number;
  walSizeMb: number;
  nodes: number;
  edges: number;
}

interface MetricsState extends MetricsSnapshot {
  setSnapshot: (snap: Partial<MetricsSnapshot>) => void;
}

const INITIAL: MetricsSnapshot = {
  epoch: 0,
  qps: 0,
  replicaState: '0/0',
  replLagMs: 0,
  pageCacheHitRate: 0,
  walSizeMb: 0,
  nodes: 0,
  edges: 0,
};

export const useMetricsStore = create<MetricsState>()((set) => ({
  ...INITIAL,
  setSnapshot: (snap) => set(snap),
}));
