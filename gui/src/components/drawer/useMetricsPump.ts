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
    setSnapshot({
      qps: d.qps ?? 0,
      pageCacheHitRate: d.page_cache_hit_rate ?? 0,
      p99LatencyMs: d.p99_latency_ms ?? 0,
      walSizeMb: d.wal_size_bytes ? d.wal_size_bytes / 1_048_576 : 0,
      nodes: d.catalog?.node_count ?? 0,
      edges: d.catalog?.rel_count ?? 0,
      labelCount: d.catalog?.label_count ?? 0,
      relTypeCount: d.catalog?.rel_type_count ?? 0,
    });
    // Only push samples for metrics the server actually emits;
    // missing fields stay out of the ring so the sparkline can
    // render a "—" no-data state instead of a misleading flat-zero.
    pushSample({
      qps: typeof d.qps === 'number' ? d.qps : null,
      pageCacheHitRate:
        typeof d.page_cache_hit_rate === 'number' ? d.page_cache_hit_rate : null,
      p99LatencyMs:
        typeof d.p99_latency_ms === 'number' ? d.p99_latency_ms : null,
      walSizeMb:
        typeof d.wal_size_bytes === 'number' ? d.wal_size_bytes / 1_048_576 : null,
    });
  }, [stats.dataUpdatedAt, stats.data, setSnapshot, pushSample]);
}
