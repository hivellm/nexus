/**
 * `metricsStore` tests. Covers ringbuffer push semantics, the
 * 60-sample cap, and the no-op behaviour when the server-supplied
 * field is absent (the pump passes `null` so the ring stays
 * empty rather than filling with misleading zeros).
 */
import { describe, it, expect, beforeEach } from 'vitest';
import { RING_SIZE, useMetricsStore } from './metricsStore';

beforeEach(() => {
  useMetricsStore.setState({
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
    rings: { qps: [], pageCacheHitRate: [], p99LatencyMs: [], walSizeMb: [] },
  });
});

describe('metricsStore', () => {
  it('pushSample appends numeric samples to the matching ring', () => {
    useMetricsStore.getState().pushSample({
      qps: 12.5,
      pageCacheHitRate: 0.92,
      p99LatencyMs: 4.1,
      walSizeMb: 16.0,
    });
    const r = useMetricsStore.getState().rings;
    expect(r.qps).toEqual([12.5]);
    expect(r.pageCacheHitRate).toEqual([0.92]);
    expect(r.p99LatencyMs).toEqual([4.1]);
    expect(r.walSizeMb).toEqual([16.0]);
  });

  it('pushSample skips fields the server did not provide', () => {
    useMetricsStore.getState().pushSample({
      qps: 1,
      pageCacheHitRate: null,
      p99LatencyMs: null,
      walSizeMb: null,
    });
    const r = useMetricsStore.getState().rings;
    expect(r.qps).toEqual([1]);
    expect(r.pageCacheHitRate).toEqual([]);
    expect(r.p99LatencyMs).toEqual([]);
    expect(r.walSizeMb).toEqual([]);
  });

  it('ring caps at RING_SIZE (60) — oldest sample falls off', () => {
    const push = useMetricsStore.getState().pushSample;
    for (let i = 0; i < RING_SIZE + 5; i++) {
      push({ qps: i, pageCacheHitRate: null, p99LatencyMs: null, walSizeMb: null });
    }
    const r = useMetricsStore.getState().rings.qps;
    expect(r).toHaveLength(RING_SIZE);
    expect(r[0]).toBe(5); // 5..64 after the first five fell off
    expect(r[r.length - 1]).toBe(RING_SIZE + 4);
  });

  it('setSnapshot writes scalar fields independently of the rings', () => {
    useMetricsStore.getState().setSnapshot({
      qps: 99,
      labelCount: 14,
      relTypeCount: 10,
      nodes: 3778,
      edges: 2060,
    });
    const s = useMetricsStore.getState();
    expect(s.qps).toBe(99);
    expect(s.labelCount).toBe(14);
    expect(s.relTypeCount).toBe(10);
    expect(s.nodes).toBe(3778);
    expect(s.edges).toBe(2060);
    // Rings stay empty — setSnapshot is scalar-only.
    expect(s.rings.qps).toEqual([]);
  });
});
