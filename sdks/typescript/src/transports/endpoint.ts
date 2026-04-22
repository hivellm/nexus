/**
 * SDK endpoint URL parsing — mirror of `nexus-cli/src/endpoint.rs`
 * and `sdks/rust/src/transport/endpoint.rs`.
 *
 * Accepts `nexus://`, `http://`, `https://`, `resp3://`, and the bare
 * `host[:port]` form (treated as RPC). Rejects `nexus-rpc://` and
 * `nexus+rpc://` so the single canonical token stays `nexus`.
 */

export type Scheme = 'nexus' | 'http' | 'https' | 'resp3';

export const RPC_DEFAULT_PORT = 15475;
export const HTTP_DEFAULT_PORT = 15474;
export const HTTPS_DEFAULT_PORT = 443;
export const RESP3_DEFAULT_PORT = 15476;

export interface Endpoint {
  readonly scheme: Scheme;
  readonly host: string;
  readonly port: number;
}

/** `nexus://127.0.0.1:15475` — the default the SDK uses when no URL is given. */
export function defaultLocalEndpoint(): Endpoint {
  return { scheme: 'nexus', host: '127.0.0.1', port: RPC_DEFAULT_PORT };
}

export function parseEndpoint(raw: string): Endpoint {
  const trimmed = raw.trim();
  if (trimmed.length === 0) {
    throw new Error('endpoint URL must not be empty');
  }

  const sep = trimmed.indexOf('://');
  if (sep !== -1) {
    const schemeRaw = trimmed.slice(0, sep).toLowerCase();
    const rest = trimmed.slice(sep + 3).replace(/\/+$/, '');
    let scheme: Scheme;
    let defaultPort: number;
    switch (schemeRaw) {
      case 'nexus':
        scheme = 'nexus';
        defaultPort = RPC_DEFAULT_PORT;
        break;
      case 'http':
        scheme = 'http';
        defaultPort = HTTP_DEFAULT_PORT;
        break;
      case 'https':
        scheme = 'https';
        defaultPort = HTTPS_DEFAULT_PORT;
        break;
      case 'resp3':
        scheme = 'resp3';
        defaultPort = RESP3_DEFAULT_PORT;
        break;
      default:
        throw new Error(
          `unsupported URL scheme '${schemeRaw}://' (expected 'nexus://', 'http://', 'https://', or 'resp3://')`
        );
    }
    const { host, port } = splitHostPort(rest);
    return { scheme, host, port: port ?? defaultPort };
  }

  // Bare form: host[:port] → treat as RPC.
  const { host, port } = splitHostPort(trimmed);
  return { scheme: 'nexus', host, port: port ?? RPC_DEFAULT_PORT };
}

export function endpointAuthority(ep: Endpoint): string {
  return `${ep.host}:${ep.port}`;
}

export function endpointToString(ep: Endpoint): string {
  return `${ep.scheme}://${endpointAuthority(ep)}`;
}

/** Render the endpoint as an HTTP URL — used by the HTTP transport when the
 *  user gave a `nexus://` or `resp3://` URL but asked for HTTP fallback. */
export function endpointAsHttpUrl(ep: Endpoint): string {
  switch (ep.scheme) {
    case 'http':
      return `http://${endpointAuthority(ep)}`;
    case 'https':
      return `https://${endpointAuthority(ep)}`;
    case 'nexus':
    case 'resp3':
      return `http://${ep.host}:${HTTP_DEFAULT_PORT}`;
  }
}

function splitHostPort(s: string): { host: string; port?: number } {
  if (s.length === 0) {
    throw new Error('missing host');
  }
  if (s.startsWith('[')) {
    const end = s.indexOf(']');
    if (end === -1) {
      throw new Error(`unterminated IPv6 literal in '${s}'`);
    }
    const host = s.slice(1, end);
    const tail = s.slice(end + 1);
    if (tail.length === 0) {
      return { host };
    }
    if (!tail.startsWith(':')) {
      throw new Error(`unexpected characters after IPv6 literal: '${tail}'`);
    }
    return { host, port: parsePort(tail.slice(1)) };
  }
  const colonIdx = s.lastIndexOf(':');
  if (colonIdx === -1) {
    return { host: s };
  }
  const host = s.slice(0, colonIdx);
  const portStr = s.slice(colonIdx + 1);
  if (host.length === 0) {
    throw new Error(`missing host in '${s}'`);
  }
  return { host, port: parsePort(portStr) };
}

function parsePort(s: string): number {
  const n = Number(s);
  if (!Number.isInteger(n) || n < 0 || n > 65535) {
    throw new Error(`invalid port '${s}': must be 0..=65535`);
  }
  return n;
}
