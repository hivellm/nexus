/**
 * REST client for the Nexus server. Pulled together as a small set
 * of typed fetch wrappers (no axios dependency) so React components
 * can call them through TanStack Query without an intermediate
 * class instance — every call is pure (`baseUrl` + body in,
 * typed response out, throw on non-2xx).
 *
 * `baseUrl` resolves from the current connection in
 * `connectionsStore`; the `useApiBase()` selector below threads it
 * into every hook in `services/queries.ts`.
 */
import type {
  AuditLogResponse,
  CypherRequest,
  CypherResponse,
  ErrorResponse,
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

export class NexusApiError extends Error {
  status: number;
  code?: string;
  constructor(status: number, message: string, code?: string) {
    super(message);
    this.name = 'NexusApiError';
    this.status = status;
    this.code = code;
  }
}

interface RequestOptions {
  signal?: AbortSignal;
  apiKey?: string;
}

async function request<T>(
  baseUrl: string,
  path: string,
  init: RequestInit,
  opts?: RequestOptions,
): Promise<T> {
  const headers = new Headers(init.headers);
  if (!headers.has('Content-Type') && init.body) {
    headers.set('Content-Type', 'application/json');
  }
  if (opts?.apiKey) headers.set('X-API-Key', opts.apiKey);

  const url = `${baseUrl.replace(/\/+$/, '')}${path}`;
  const res = await fetch(url, { ...init, headers, signal: opts?.signal });

  // Try to surface server-side error payload before falling back
  // to status-text. The server uses `{error, code}` for all 4xx/5xx
  // responses on the JSON surface.
  if (!res.ok) {
    let detail: ErrorResponse | undefined;
    try {
      detail = (await res.json()) as ErrorResponse;
    } catch {
      // body wasn't JSON; fall through to status-only error
    }
    throw new NexusApiError(
      res.status,
      detail?.error ?? `${res.status} ${res.statusText}`,
      detail?.code,
    );
  }

  // 204 / empty bodies — return undefined cast to T so callers that
  // type the response as `void` work without a special case.
  if (res.status === 204) return undefined as unknown as T;
  return (await res.json()) as T;
}

export const api = {
  health(baseUrl: string, opts?: RequestOptions): Promise<HealthResponse> {
    return request<HealthResponse>(baseUrl, '/health', { method: 'GET' }, opts);
  },

  stats(baseUrl: string, opts?: RequestOptions): Promise<StatsResponse> {
    return request<StatsResponse>(baseUrl, '/stats', { method: 'GET' }, opts);
  },

  executeCypher(
    baseUrl: string,
    body: CypherRequest,
    opts?: RequestOptions,
  ): Promise<CypherResponse> {
    return request<CypherResponse>(
      baseUrl,
      '/cypher',
      { method: 'POST', body: JSON.stringify(body) },
      opts,
    );
  },

  labels(baseUrl: string, opts?: RequestOptions): Promise<LabelsResponse> {
    return request<LabelsResponse>(baseUrl, '/schema/labels', { method: 'GET' }, opts);
  },

  relTypes(baseUrl: string, opts?: RequestOptions): Promise<RelTypesResponse> {
    return request<RelTypesResponse>(baseUrl, '/schema/rel_types', { method: 'GET' }, opts);
  },

  indexes(baseUrl: string, opts?: RequestOptions): Promise<IndexesResponse> {
    return request<IndexesResponse>(baseUrl, '/schema/indexes', { method: 'GET' }, opts);
  },

  procedures(baseUrl: string, opts?: RequestOptions): Promise<ProceduresResponse> {
    return request<ProceduresResponse>(baseUrl, '/procedures', { method: 'GET' }, opts);
  },

  knn(baseUrl: string, body: KnnRequest, opts?: RequestOptions): Promise<KnnResponse> {
    return request<KnnResponse>(
      baseUrl,
      '/knn_traverse',
      { method: 'POST', body: JSON.stringify(body) },
      opts,
    );
  },

  replicationStatus(
    baseUrl: string,
    opts?: RequestOptions,
  ): Promise<ReplicationStatusResponse> {
    return request<ReplicationStatusResponse>(
      baseUrl,
      '/replication/status',
      { method: 'GET' },
      opts,
    );
  },

  auditLog(
    baseUrl: string,
    cursor: string | undefined,
    opts?: RequestOptions,
  ): Promise<AuditLogResponse> {
    const qs = cursor ? `?cursor=${encodeURIComponent(cursor)}` : '';
    return request<AuditLogResponse>(baseUrl, `/audit/log${qs}`, { method: 'GET' }, opts);
  },
};
