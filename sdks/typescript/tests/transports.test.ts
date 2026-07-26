import { describe, expect, it } from 'vitest';
import {
  defaultLocalEndpoint,
  endpointAsHttpUrl,
  endpointAuthority,
  endpointToString,
  parseEndpoint,
} from '../src/transports/endpoint';
import { mapCommand, jsonToNexus, nexusToJson } from '../src/transports/command-map';
import { buildTransport, parseTransportMode } from '../src/transports/index';
import { nx } from '../src/transports/types';

describe('endpoint parser', () => {
  it('defaults to nexus://127.0.0.1:15475', () => {
    const ep = defaultLocalEndpoint();
    expect(ep.scheme).toBe('nexus');
    expect(ep.host).toBe('127.0.0.1');
    expect(ep.port).toBe(15475);
    expect(endpointToString(ep)).toBe('nexus://127.0.0.1:15475');
  });

  it('parses nexus:// with explicit port', () => {
    const ep = parseEndpoint('nexus://example.com:17000');
    expect(ep.scheme).toBe('nexus');
    expect(ep.port).toBe(17000);
  });

  it('parses http:// with default port', () => {
    const ep = parseEndpoint('http://localhost');
    expect(ep.scheme).toBe('http');
    expect(ep.port).toBe(15474);
  });

  it('parses https:// with default port', () => {
    const ep = parseEndpoint('https://nexus.example.com');
    expect(ep.scheme).toBe('https');
    expect(ep.port).toBe(443);
  });

  it('treats bare host:port as RPC', () => {
    const ep = parseEndpoint('10.0.0.5:15600');
    expect(ep.scheme).toBe('nexus');
    expect(ep.port).toBe(15600);
  });

  it('rejects nexus-rpc:// scheme', () => {
    expect(() => parseEndpoint('nexus-rpc://host')).toThrow(/unsupported URL scheme/);
  });

  it('rejects empty input', () => {
    expect(() => parseEndpoint('')).toThrow();
    expect(() => parseEndpoint('   ')).toThrow();
  });

  it('parses IPv6 literal with port', () => {
    const ep = parseEndpoint('nexus://[::1]:15475');
    expect(ep.host).toBe('::1');
    expect(ep.port).toBe(15475);
  });

  it('nexus:// maps to sibling HTTP port for HTTP fallback URL', () => {
    const ep = parseEndpoint('nexus://host:17000');
    expect(endpointAsHttpUrl(ep)).toBe('http://host:15474');
  });

  it('authority is host:port', () => {
    expect(endpointAuthority({ scheme: 'nexus', host: 'db', port: 15475 })).toBe('db:15475');
  });
});

// The wire codec (NexusValue ↔ MessagePack framing) now lives in
// `@hivehub/thunder`; the SDK's RPC transport wraps Thunder's client
// instead of owning the codec, so the former `wire codec` describe blocks
// were removed with `src/transports/codec.ts`. Thunder ships its own
// codec conformance tests. The endpoint / command-map / transport-select
// behaviour below is the SDK's own and stays here.

describe('command map', () => {
  it('maps graph.cypher with query only', () => {
    const m = mapCommand('graph.cypher', { query: 'RETURN 1' });
    expect(m?.command).toBe('CYPHER');
    expect(m?.args.length).toBe(1);
    expect(m?.args[0]).toEqual(nx.Str('RETURN 1'));
  });

  it('maps graph.cypher with params appended', () => {
    const m = mapCommand('graph.cypher', {
      query: 'MATCH (n {name:$n}) RETURN n',
      parameters: { n: 'Alice' },
    });
    expect(m?.args.length).toBe(2);
    expect(m?.args[1].kind).toBe('Map');
  });

  it('maps graph.ping / stats / health / quit with no args', () => {
    for (const name of ['graph.ping', 'graph.stats', 'graph.health', 'graph.quit']) {
      const m = mapCommand(name, {});
      expect(m).not.toBeNull();
      expect(m?.args.length).toBe(0);
    }
  });

  it('auth.login: api_key wins over user/pass', () => {
    const m = mapCommand('auth.login', { api_key: 'nx_1', username: 'u', password: 'p' });
    expect(m?.command).toBe('AUTH');
    expect(m?.args.length).toBe(1);
    expect(m?.args[0]).toEqual(nx.Str('nx_1'));
  });

  it('auth.login falls back to user+pass', () => {
    const m = mapCommand('auth.login', { username: 'u', password: 'p' });
    expect(m?.args.length).toBe(2);
  });

  it('db.create requires name', () => {
    expect(mapCommand('db.create', {})).toBeNull();
    const m = mapCommand('db.create', { name: 'mydb' });
    expect(m?.command).toBe('DB_CREATE');
  });

  it('data.export with and without query', () => {
    const m1 = mapCommand('data.export', { format: 'json' });
    expect(m1?.args.length).toBe(1);
    const m2 = mapCommand('data.export', { format: 'csv', query: 'MATCH (n) RETURN n' });
    expect(m2?.args.length).toBe(2);
  });

  it('data.import requires both format and data', () => {
    expect(mapCommand('data.import', { format: 'json' })).toBeNull();
    expect(mapCommand('data.import', { data: '[]' })).toBeNull();
    const m = mapCommand('data.import', { format: 'json', data: '[]' });
    expect(m?.args.length).toBe(2);
  });

  it('unknown dotted name returns null', () => {
    expect(mapCommand('graph.nonsense', {})).toBeNull();
  });

  it('jsonToNexus handles nested objects', () => {
    const v = jsonToNexus({ labels: ['Person'], properties: { name: 'Alice', age: 30 } });
    expect(v.kind).toBe('Map');
  });

  it('nexusToJson round-trips a Map variant back to a plain object', () => {
    const v = nx.Map([
      [nx.Str('name'), nx.Str('Alice')],
      [nx.Str('age'), nx.Int(30)],
    ]);
    expect(nexusToJson(v)).toEqual({ name: 'Alice', age: 30 });
  });
});

describe('buildTransport — precedence', () => {
  it('defaults to RPC when nothing is provided', () => {
    const { mode, endpoint } = buildTransport({ credentials: {} });
    expect(mode).toBe('nexus');
    expect(endpoint.port).toBe(15475);
  });

  it('URL scheme wins over env var', () => {
    const { mode } = buildTransport({
      baseUrl: 'http://host:15474',
      credentials: {},
      envTransport: 'nexus',
    });
    expect(mode).toBe('http');
  });

  it('env var overrides bare host:port', () => {
    const { mode } = buildTransport({
      baseUrl: 'host:15474',
      credentials: {},
      envTransport: 'http',
    });
    expect(mode).toBe('http');
  });

  it('config.transport honoured when URL is bare and env unset', () => {
    const { mode } = buildTransport({
      baseUrl: 'host:15474',
      transport: 'http',
      credentials: {},
    });
    expect(mode).toBe('http');
  });

  it('resp3 transport throws a clear configuration error', () => {
    expect(() =>
      buildTransport({ transport: 'resp3', credentials: {} })
    ).toThrow(/resp3 transport is not yet shipped/);
  });

  it('parseTransportMode aliases', () => {
    expect(parseTransportMode('nexus')).toBe('nexus');
    expect(parseTransportMode('rpc')).toBe('nexus');
    expect(parseTransportMode('NexusRpc')).toBe('nexus');
    expect(parseTransportMode('http')).toBe('http');
    expect(parseTransportMode('auto')).toBeNull();
    expect(parseTransportMode('widget')).toBeNull();
  });
});
