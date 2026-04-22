/**
 * Transport factory + re-exports.
 *
 * Resolves the effective `TransportMode` from the precedence chain
 * defined in `docs/specs/sdk-transport.md`:
 *
 *   URL scheme  >  NEXUS_SDK_TRANSPORT env var  >  config field  >  default (nexus)
 *
 * Returns a concrete `Transport` instance the `NexusClient` drives.
 */

import {
  Transport,
  TransportCredentials,
  TransportMode,
  nx,
} from './types';
import {
  Endpoint,
  defaultLocalEndpoint,
  parseEndpoint,
} from './endpoint';
import { RpcTransport } from './rpc';
import { HttpTransport } from './http';
import { jsonToNexus, mapCommand, nexusToJson } from './command-map';

export { RpcTransport } from './rpc';
export { HttpTransport } from './http';
export type {
  NexusValue,
  Transport,
  TransportCredentials,
  TransportMode,
  TransportRequest,
  TransportResponse,
} from './types';
export { nx, jsonToNexus, mapCommand, nexusToJson };
export {
  Endpoint,
  defaultLocalEndpoint,
  endpointToString,
  parseEndpoint,
} from './endpoint';

export interface BuildTransportOptions {
  baseUrl?: string;
  transport?: TransportMode;
  rpcPort?: number;
  resp3Port?: number;
  credentials: TransportCredentials;
  timeoutMs?: number;
  retries?: number;
  envTransport?: string | undefined;
}

/**
 * Build the `Transport` the client should use, applying the full
 * precedence chain.
 *
 * @returns a `{ transport, endpoint, mode }` triple — the endpoint is
 *          returned so the client can surface a nice description in
 *          `--verbose` output without a second parse.
 */
export function buildTransport(opts: BuildTransportOptions): {
  transport: Transport;
  endpoint: Endpoint;
  mode: TransportMode;
} {
  let endpoint = opts.baseUrl ? parseEndpoint(opts.baseUrl) : defaultLocalEndpoint();

  // 1. URL scheme wins. Translate to TransportMode.
  let mode = schemeToMode(endpoint.scheme);

  // 2. Env var overrides a URL that was *bare* (no scheme → defaulted to 'nexus')
  //    AND also overrides a config field; but it must NOT override an explicit
  //    non-default URL scheme. Align with the Rust SDK.
  const envMode = opts.envTransport ? parseTransportMode(opts.envTransport) : null;
  const explicitUrl = opts.baseUrl !== undefined && opts.baseUrl.includes('://');
  if (envMode && !explicitUrl) {
    mode = envMode;
    endpoint = realignEndpointPort(endpoint, mode, opts);
  }

  // 3. Config field — honoured only if URL didn't explicitly pick one.
  if (opts.transport && !explicitUrl && !envMode) {
    mode = opts.transport;
    endpoint = realignEndpointPort(endpoint, mode, opts);
  }

  // Build the actual transport.
  switch (mode) {
    case 'nexus':
      return {
        transport: new RpcTransport(endpoint, opts.credentials),
        endpoint,
        mode,
      };
    case 'http':
    case 'https':
      return {
        transport: new HttpTransport(endpoint, opts.credentials, {
          timeoutMs: opts.timeoutMs,
          retries: opts.retries,
        }),
        endpoint,
        mode,
      };
    case 'resp3':
      throw new Error(
        `resp3 transport is not yet shipped in the TypeScript SDK — use 'nexus' (RPC) or 'http' for now`
      );
  }
}

function schemeToMode(scheme: Endpoint['scheme']): TransportMode {
  return scheme === 'nexus' ? 'nexus' : scheme;
}

export function parseTransportMode(raw: string): TransportMode | null {
  const v = raw.trim().toLowerCase();
  switch (v) {
    case 'nexus':
    case 'rpc':
    case 'nexusrpc':
      return 'nexus';
    case 'resp3':
      return 'resp3';
    case 'http':
      return 'http';
    case 'https':
      return 'https';
    case '':
    case 'auto':
      return null;
    default:
      return null;
  }
}

function realignEndpointPort(
  ep: Endpoint,
  mode: TransportMode,
  opts: BuildTransportOptions
): Endpoint {
  // When the user asked for HTTP but gave a nexus:// URL (or vice versa),
  // retarget the port to the right default for the chosen transport —
  // but only if the URL didn't explicitly set a port matching the old
  // scheme's default (we can't tell the difference, so we assume they
  // want the right port for the selected mode).
  const desiredScheme: Endpoint['scheme'] =
    mode === 'nexus' ? 'nexus' : mode === 'https' ? 'https' : mode === 'resp3' ? 'resp3' : 'http';
  const targetPort =
    mode === 'nexus'
      ? (opts.rpcPort ?? 15475)
      : mode === 'resp3'
        ? (opts.resp3Port ?? 15476)
        : mode === 'https'
          ? 443
          : 15474;
  return { scheme: desiredScheme, host: ep.host, port: targetPort };
}
