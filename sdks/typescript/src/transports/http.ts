/**
 * HTTP fallback transport.
 *
 * Wraps `axios` behind the same `Transport` interface the RPC path
 * uses, so `NexusClient` can pick a transport once at construction
 * and stop branching on scheme for every method call. The translation
 * from wire-level command names to HTTP routes is a thin hard-coded
 * table — every HTTP route the legacy TypeScript client used has a
 * mapping here. Commands without a mapping surface as a structured
 * error.
 */

import axios, { AxiosInstance } from 'axios';
import axiosRetry from 'axios-retry';
import {
  NexusValue,
  Transport,
  TransportCredentials,
  TransportRequest,
  TransportResponse,
} from './types';
import { Endpoint, endpointAsHttpUrl, endpointToString } from './endpoint';
import { jsonToNexus } from './command-map';

export interface HttpTransportOptions {
  timeoutMs?: number;
  retries?: number;
}

export class HttpTransport implements Transport {
  private readonly endpoint: Endpoint;
  private readonly client: AxiosInstance;
  private readonly baseUrl: string;

  constructor(
    endpoint: Endpoint,
    credentials: TransportCredentials,
    options: HttpTransportOptions = {}
  ) {
    this.endpoint = endpoint;
    this.baseUrl = endpointAsHttpUrl(endpoint);
    this.client = axios.create({
      baseURL: this.baseUrl,
      timeout: options.timeoutMs ?? 30_000,
      headers: { 'Content-Type': 'application/json' },
    });

    if (credentials.apiKey) {
      this.client.defaults.headers.common['X-API-Key'] = credentials.apiKey;
    } else if (credentials.username && credentials.password) {
      const token = Buffer.from(`${credentials.username}:${credentials.password}`).toString('base64');
      this.client.defaults.headers.common['Authorization'] = `Basic ${token}`;
    }

    axiosRetry(this.client, {
      retries: options.retries ?? 3,
      retryDelay: axiosRetry.exponentialDelay,
      retryCondition: (err) =>
        axiosRetry.isNetworkOrIdempotentRequestError(err) ||
        (err.response?.status !== undefined && err.response.status >= 500),
    });
  }

  async execute(req: TransportRequest): Promise<TransportResponse> {
    const value = await this.dispatch(req.command, req.args);
    return { value };
  }

  describe(): string {
    const tag = this.endpoint.scheme === 'https' ? 'HTTPS' : 'HTTP';
    return `${endpointToString(this.endpoint)} (${tag})`;
  }

  isRpc(): boolean {
    return false;
  }

  async close(): Promise<void> {
    /* axios has no persistent socket to close. */
  }

  private async dispatch(cmd: string, args: NexusValue[]): Promise<NexusValue> {
    switch (cmd) {
      case 'CYPHER': {
        const query = asString(args[0], 'CYPHER', 0);
        const params = args[1] ? nexusToPlainJson(args[1]) : null;
        const body = { query, parameters: params ?? null };
        const { data } = await this.client.post('/cypher', body);
        return jsonToNexus(data);
      }
      case 'PING':
      case 'HEALTH': {
        const { data } = await this.client.get('/health');
        return jsonToNexus(data);
      }
      case 'STATS': {
        const { data } = await this.client.get('/stats');
        return jsonToNexus(data);
      }
      case 'EXPORT': {
        const fmt = asString(args[0], 'EXPORT', 0);
        const { data } = await this.client.get(`/export?format=${encodeURIComponent(fmt)}`);
        return jsonToNexus({
          format: fmt,
          data: typeof data === 'string' ? data : JSON.stringify(data),
        });
      }
      case 'IMPORT': {
        const fmt = asString(args[0], 'IMPORT', 0);
        const payload = asString(args[1], 'IMPORT', 1);
        const { data } = await this.client.post(
          `/import?format=${encodeURIComponent(fmt)}`,
          payload,
          { headers: { 'Content-Type': 'text/plain' } }
        );
        return jsonToNexus(data);
      }
      // Phase9 §5.5 — external-id node operations.
      case 'NODE_CREATE_EXT': {
        // args[0] = JSON-encoded request body (Str)
        const bodyStr = asString(args[0], 'NODE_CREATE_EXT', 0);
        const body = JSON.parse(bodyStr) as Record<string, unknown>;
        const { data } = await this.client.post('/data/nodes', body);
        return jsonToNexus(data);
      }
      case 'NODE_GET_BY_EXT_ID': {
        // args[0] = external_id value (Str)
        const extId = asString(args[0], 'NODE_GET_BY_EXT_ID', 0);
        const { data } = await this.client.get(
          `/data/nodes/by-external-id?external_id=${encodeURIComponent(extId)}`
        );
        return jsonToNexus(data);
      }
      default:
        throw new Error(
          `HTTP fallback does not know how to route '${cmd}' — add an entry to sdks/typescript/src/transports/http.ts`
        );
    }
  }
}

function asString(v: NexusValue | undefined, cmd: string, idx: number): string {
  if (v && v.kind === 'Str') return v.value;
  throw new Error(`HTTP fallback: '${cmd}' argument ${idx} must be a string`);
}

function nexusToPlainJson(v: NexusValue): unknown {
  switch (v.kind) {
    case 'Null':
      return null;
    case 'Bool':
      return v.value;
    case 'Int':
      return typeof v.value === 'bigint'
        ? v.value >= BigInt(Number.MIN_SAFE_INTEGER) && v.value <= BigInt(Number.MAX_SAFE_INTEGER)
          ? Number(v.value)
          : v.value.toString()
        : v.value;
    case 'Float':
      return v.value;
    case 'Bytes':
      return Array.from(v.value);
    case 'Str':
      return v.value;
    case 'Array':
      return v.value.map(nexusToPlainJson);
    case 'Map': {
      const obj: Record<string, unknown> = {};
      for (const [k, val] of v.value) {
        const key =
          k.kind === 'Str'
            ? k.value
            : k.kind === 'Int'
              ? String(k.value)
              : JSON.stringify(nexusToPlainJson(k));
        obj[key] = nexusToPlainJson(val);
      }
      return obj;
    }
  }
}
