/**
 * TanStack Query hooks. Each hook resolves the current connection's
 * `baseUrl` from `connectionsStore` and threads it into the matching
 * `api.*` call. Polling intervals match what each panel needs:
 *
 * - `useHealth` / `useStats` / `useReplicationStatus` — 2 s, fast
 *   enough for the live pills and statusbar without hammering the
 *   server.
 * - `useSchema` / `useProcedures` — 30 s, schema rarely changes.
 * - `useAuditLog` — 5 s, reasonable for a tail-style feed (the
 *   SSE upgrade in §7 replaces this).
 *
 * Mutations (`useExecuteCypher`, `useKnn`) intentionally do not
 * auto-fire; the editor's Run button calls `mutate(query)` and the
 * `onSuccess` invalidates the schema query so newly created
 * labels show up in the left panel immediately.
 */
import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from '@tanstack/react-query';
import { useMemo } from 'react';
import { api, NexusApiError } from './api';
import {
  selectCurrentConnection,
  useConnectionsStore,
} from '../stores/connectionsStore';
import type {
  AuditLogResponse,
  CypherRequest,
  CypherResponse,
  HealthResponse,
  IndexesResponse,
  KnnRequest,
  KnnResponse,
  LabelsResponse,
  ProceduresResponse,
  RelTypesResponse,
  ReplicationStatusResponse,
  StatsResponse,
} from '../types/api';

/**
 * Resolve the active connection's base URL. Returned as a stable
 * string so query keys keyed off it are stable too. Returns `null`
 * when no connection is configured — every hook short-circuits to
 * `enabled: false` in that case.
 */
export function useApiBase(): string | null {
  const conn = useConnectionsStore(selectCurrentConnection);
  return useMemo(() => conn?.url ?? null, [conn?.url]);
}

const POLL_FAST = 2_000;
const POLL_SLOW = 30_000;
const POLL_AUDIT = 5_000;

export function useHealth(): UseQueryResult<HealthResponse, NexusApiError> {
  const baseUrl = useApiBase();
  return useQuery<HealthResponse, NexusApiError>({
    queryKey: ['health', baseUrl],
    queryFn: ({ signal }) => api.health(baseUrl!, { signal }),
    enabled: baseUrl !== null,
    refetchInterval: POLL_FAST,
    staleTime: POLL_FAST,
  });
}

export function useStats(): UseQueryResult<StatsResponse, NexusApiError> {
  const baseUrl = useApiBase();
  return useQuery<StatsResponse, NexusApiError>({
    queryKey: ['stats', baseUrl],
    queryFn: ({ signal }) => api.stats(baseUrl!, { signal }),
    enabled: baseUrl !== null,
    refetchInterval: POLL_FAST,
    staleTime: POLL_FAST,
  });
}

export function useReplicationStatus(): UseQueryResult<
  ReplicationStatusResponse,
  NexusApiError
> {
  const baseUrl = useApiBase();
  return useQuery<ReplicationStatusResponse, NexusApiError>({
    queryKey: ['replication-status', baseUrl],
    queryFn: ({ signal }) => api.replicationStatus(baseUrl!, { signal }),
    enabled: baseUrl !== null,
    refetchInterval: POLL_FAST,
    staleTime: POLL_FAST,
  });
}

export interface SchemaSnapshot {
  labels: LabelsResponse;
  relTypes: RelTypesResponse;
  indexes: IndexesResponse;
  procedures: ProceduresResponse;
}

export function useSchema(): UseQueryResult<SchemaSnapshot, NexusApiError> {
  const baseUrl = useApiBase();
  return useQuery<SchemaSnapshot, NexusApiError>({
    queryKey: ['schema', baseUrl],
    queryFn: async ({ signal }) => {
      const url = baseUrl!;
      const [labels, relTypes, indexes, procedures] = await Promise.all([
        api.labels(url, { signal }),
        api.relTypes(url, { signal }),
        api.indexes(url, { signal }),
        api.procedures(url, { signal }),
      ]);
      return { labels, relTypes, indexes, procedures };
    },
    enabled: baseUrl !== null,
    refetchInterval: POLL_SLOW,
    staleTime: POLL_SLOW,
  });
}

export function useExecuteCypher(): UseMutationResult<
  CypherResponse,
  NexusApiError,
  CypherRequest
> {
  const baseUrl = useApiBase();
  const qc = useQueryClient();
  return useMutation<CypherResponse, NexusApiError, CypherRequest>({
    mutationFn: (req) => {
      if (baseUrl === null) {
        throw new NexusApiError(0, 'No active connection');
      }
      return api.executeCypher(baseUrl, req);
    },
    onSuccess: () => {
      // Schema may have changed if the query created labels / rel
      // types / indexes — invalidate so the left panel refreshes.
      qc.invalidateQueries({ queryKey: ['schema', baseUrl] });
      qc.invalidateQueries({ queryKey: ['stats', baseUrl] });
    },
  });
}

export function useKnn(): UseMutationResult<KnnResponse, NexusApiError, KnnRequest> {
  const baseUrl = useApiBase();
  return useMutation<KnnResponse, NexusApiError, KnnRequest>({
    mutationFn: (req) => {
      if (baseUrl === null) {
        throw new NexusApiError(0, 'No active connection');
      }
      return api.knn(baseUrl, req);
    },
  });
}

export function useAuditLog(
  cursor?: string,
): UseQueryResult<AuditLogResponse, NexusApiError> {
  const baseUrl = useApiBase();
  return useQuery<AuditLogResponse, NexusApiError>({
    queryKey: ['audit-log', baseUrl, cursor],
    queryFn: ({ signal }) => api.auditLog(baseUrl!, cursor, { signal }),
    enabled: baseUrl !== null,
    refetchInterval: POLL_AUDIT,
    staleTime: POLL_AUDIT,
  });
}
