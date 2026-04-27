/**
 * Drives the metrics ringbuffers from `useStats()`. Each TanStack
 * Query refetch lands here as a `dataUpdatedAt` change; we push
 * one sample into each ring (and refresh the snapshot scalars).
 *
 * Lives in its own hook so the right-drawer mounts it once at the
 * top — the four sparkline cards subscribe to the rings instead of
 * to TanStack Query directly, so swapping the active connection
 * resets the rings via the `baseUrl` effect dependency.
 */
import { useEffect } from 'react';
import { useStats } from '../../services/queries';
import { useMetricsStore } from '../../stores/metricsStore';
import { useApiBase } from '../../services/queries';

export function useMetricsPump(): void {
  const stats = useStats();
  const baseUrl = useApiBase();
  const setSnapshot = useMetricsStore((s) => s.setSnapshot);
  const pushSample = useMetricsStore((s) => s.pushSample);

  // Reset the rings when the active connection changes — old samples
  // are not meaningful against a different server.
  useEffect(() => {
    useMetricsStore.setState({
      rings: { qps: [], pageCacheHitRate: [], p99LatencyMs: [], walSizeMb: [] },
    });
  }, [baseUrl]);

  useEffect(() => {
    if (!stats.data) return;
    const d = stats.data;
    const qps = d.qps ?? 0;
    const cache = d.page_cache_hit_rate ?? 0;
    const p99 = d.p99_latency_ms ?? 0;
    const wal = d.wal_size_bytes ? d.wal_size_bytes / 1_048_576 : 0;
    setSnapshot({
      qps,
      pageCacheHitRate: cache,
      p99LatencyMs: p99,
      walSizeMb: wal,
      nodes: d.catalog?.node_count ?? 0,
      edges: d.catalog?.rel_count ?? 0,
    });
    pushSample({
      qps,
      pageCacheHitRate: cache,
      p99LatencyMs: p99,
      walSizeMb: wal,
    });
  }, [stats.dataUpdatedAt, stats.data, setSnapshot, pushSample]);
}
