import { describe, expect, it } from 'vitest';
import {
  defaultLocalEndpoint,
  endpointAsHttpUrl,
  endpointAuthority,
  endpointToString,
  parseEndpoint,
} from '../src/transports/endpoint';
import {
  decodeResponseBody,
  encodeRequestFrame,
  fromWireValue,
  rpcResponseToTransport,
  toWireValue,
} from '../src/transports/codec';
import { mapCommand, jsonToNexus, nexusToJson } from '../src/transports/command-map';
import { buildTransport, parseTransportMode } from '../src/transports/index';
import { nx } from '../src/transports/types';
import { pack } from 'msgpackr';

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

describe('wire codec — NexusValue', () => {
  it('encodes Null as the literal string "Null"', () => {
    expect(toWireValue(nx.Null())).toBe('Null');
  });

  it('encodes Str as { Str: "…" }', () => {
    expect(toWireValue(nx.Str('hi'))).toEqual({ Str: 'hi' });
  });

  it('encodes Int as { Int: bigint }', () => {
    const wire = toWireValue(nx.Int(42)) as { Int: bigint };
    expect(typeof wire.Int).toBe('bigint');
    expect(wire.Int).toBe(42n);
  });

  it('encodes Bool / Float / Bytes / Array / Map', () => {
    expect(toWireValue(nx.Bool(true))).toEqual({ Bool: true });
    expect(toWireValue(nx.Float(1.5))).toEqual({ Float: 1.5 });
    const bytesWire = toWireValue(nx.Bytes(new Uint8Array([1, 2, 3]))) as { Bytes: Uint8Array };
    expect(bytesWire.Bytes).toBeInstanceOf(Uint8Array);
    expect([...bytesWire.Bytes]).toEqual([1, 2, 3]);
    const arrWire = toWireValue(nx.Array([nx.Int(1), nx.Str('two')])) as { Array: unknown[] };
    expect(arrWire.Array.length).toBe(2);
    const mapWire = toWireValue(nx.Map([[nx.Str('k'), nx.Int(99)]])) as { Map: unknown[][] };
    expect(mapWire.Map.length).toBe(1);
  });

  it('roundtrips every primitive variant through fromWireValue', () => {
    // `Int` is normalised to bigint after the round trip because
    // MessagePack carries 64-bit integers as bigint; `nexusToJson`
    // folds safe integers back to number on the user-facing boundary.
    const cases: Array<[ReturnType<typeof nx[keyof typeof nx]>, ReturnType<typeof nx[keyof typeof nx]>]> = [
      [nx.Null(), nx.Null()],
      [nx.Bool(false), nx.Bool(false)],
      [nx.Bool(true), nx.Bool(true)],
      [nx.Int(0), nx.Int(0n)],
      [nx.Str(''), nx.Str('')],
      [nx.Str('hello'), nx.Str('hello')],
      [nx.Float(3.14), nx.Float(3.14)],
      [nx.Bytes(new Uint8Array([0, 255])), nx.Bytes(new Uint8Array([0, 255]))],
    ];
    for (const [input, expected] of cases) {
      const back = fromWireValue(toWireValue(input));
      expect(back).toEqual(expected);
    }
  });

  it('roundtrips nested Array + Map', () => {
    const v = nx.Map([
      [nx.Str('labels'), nx.Array([nx.Str('Person')])],
      [nx.Str('age'), nx.Int(30n)],
    ]);
    const back = fromWireValue(toWireValue(v));
    expect(back).toEqual(v);
  });

  it('rejects a multi-key tagged value', () => {
    expect(() => fromWireValue({ Str: 'a', Int: 1n })).toThrow(/single-key/);
  });

  it('rejects an unknown tag', () => {
    expect(() => fromWireValue({ Widget: 'x' })).toThrow(/unknown NexusValue tag/);
  });
});

describe('wire codec — Request frame', () => {
  it('produces u32 LE length prefix + msgpack body', () => {
    const frame = encodeRequestFrame({ id: 7, command: 'PING', args: [] });
    const length = new DataView(frame.buffer, frame.byteOffset, frame.byteLength).getUint32(0, true);
    expect(length).toBe(frame.length - 4);
    expect(length).toBeGreaterThan(0);
  });

  it('decodes an Ok response', () => {
    const body = pack({ id: 9, result: { Ok: { Str: 'OK' } } });
    const resp = decodeResponseBody(body);
    expect(resp.id).toBe(9);
    expect(resp.result.ok).toBe(true);
    if (resp.result.ok) expect(resp.result.value).toEqual(nx.Str('OK'));
    const transport = rpcResponseToTransport(resp);
    expect(transport.value).toEqual(nx.Str('OK'));
  });

  it('decodes an Err response and rpcResponseToTransport throws', () => {
    const body = pack({ id: 3, result: { Err: 'boom' } });
    const resp = decodeResponseBody(body);
    expect(resp.result.ok).toBe(false);
    if (!resp.result.ok) expect(resp.result.message).toBe('boom');
    expect(() => rpcResponseToTransport(resp)).toThrow(/boom/);
  });
});

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
