/**
 * Wire codec for the Nexus RPC protocol.
 *
 * The server-side types live in `nexus-protocol::rpc::types`. rmp-serde
 * encodes Rust enums using the externally-tagged representation by
 * default, which means:
 *
 * - Unit variants (`NexusValue::Null`) → the string `"Null"`.
 * - Data-bearing variants (`NexusValue::Str("hi")`) → a single-key map
 *   `{"Str": "hi"}`.
 * - `Result<T, E>` follows the same rule: `{"Ok": v}` / `{"Err": s}`.
 *
 * `toWireValue` / `fromWireValue` translate between the tagged
 * TypeScript type and that on-wire shape. `encodeRequest` and
 * `decodeResponse` handle the `u32 LE length prefix + MessagePack`
 * frame format documented in `docs/specs/rpc-wire-format.md`.
 */

import { pack, unpack } from 'msgpackr';
import { NexusValue, nx, TransportResponse } from './types';

/** Request frame serialised onto the wire. */
export interface RpcRequest {
  id: number;
  command: string;
  args: NexusValue[];
}

/** Decoded response — either a value or an error string. */
export type RpcResponseResult =
  | { ok: true; value: NexusValue }
  | { ok: false; message: string };

export interface RpcResponse {
  id: number;
  result: RpcResponseResult;
}

/**
 * Encode a `NexusValue` into its on-wire (pre-MessagePack) JS shape.
 *
 * Returns the literal string `'Null'` for unit variants and a single-key
 * object `{ VariantName: payload }` for every data-bearing variant.
 * Bytes are encoded as `Uint8Array` — msgpackr preserves them verbatim.
 */
export function toWireValue(v: NexusValue): unknown {
  switch (v.kind) {
    case 'Null':
      return 'Null';
    case 'Bool':
      return { Bool: v.value };
    case 'Int':
      // msgpackr encodes both number and bigint as integer types —
      // bigint is required when the magnitude exceeds 2^53-1.
      return { Int: typeof v.value === 'bigint' ? v.value : BigInt(v.value) };
    case 'Float':
      return { Float: v.value };
    case 'Bytes':
      return { Bytes: v.value };
    case 'Str':
      return { Str: v.value };
    case 'Array':
      return { Array: v.value.map(toWireValue) };
    case 'Map':
      // rmp-serde encodes `Vec<(K, V)>` as an array of 2-tuples — NOT a
      // msgpack map, since keys can be non-string NexusValues.
      return { Map: v.value.map(([k, val]) => [toWireValue(k), toWireValue(val)]) };
  }
}

/** Decode the wire-level JS shape back into a tagged `NexusValue`. */
export function fromWireValue(raw: unknown): NexusValue {
  if (raw === 'Null' || raw === null || raw === undefined) {
    return nx.Null();
  }
  if (typeof raw !== 'object') {
    // msgpackr can surface primitive variants as primitives when the
    // server sends a pre-tagged value through a different codepath.
    // Fall back to the most specific tag.
    if (typeof raw === 'boolean') return nx.Bool(raw);
    if (typeof raw === 'string') return nx.Str(raw);
    if (typeof raw === 'bigint') return nx.Int(raw);
    if (typeof raw === 'number') {
      return Number.isInteger(raw) ? nx.Int(raw) : nx.Float(raw);
    }
    return nx.Null();
  }

  // Bytes surface as Uint8Array (or Buffer on Node). Check before the
  // plain-object branch.
  if (raw instanceof Uint8Array) {
    return nx.Bytes(raw);
  }

  if (Array.isArray(raw)) {
    // Untagged array — interpret as `Array` variant.
    return nx.Array(raw.map(fromWireValue));
  }

  const entries = Object.entries(raw as Record<string, unknown>);
  if (entries.length !== 1) {
    throw new Error(`decode: expected single-key tagged NexusValue, got ${entries.length} keys`);
  }
  const [tag, payload] = entries[0];
  switch (tag) {
    case 'Null':
      return nx.Null();
    case 'Bool':
      return nx.Bool(Boolean(payload));
    case 'Int': {
      if (typeof payload === 'bigint') return nx.Int(payload);
      if (typeof payload === 'number') return nx.Int(payload);
      throw new Error(`decode: Int payload must be number or bigint`);
    }
    case 'Float': {
      if (typeof payload === 'number') return nx.Float(payload);
      if (typeof payload === 'bigint') return nx.Float(Number(payload));
      throw new Error(`decode: Float payload must be numeric`);
    }
    case 'Bytes': {
      if (payload instanceof Uint8Array) return nx.Bytes(payload);
      if (Array.isArray(payload)) return nx.Bytes(Uint8Array.from(payload as number[]));
      throw new Error(`decode: Bytes payload must be bytes`);
    }
    case 'Str':
      return nx.Str(String(payload));
    case 'Array': {
      if (!Array.isArray(payload)) throw new Error(`decode: Array payload must be array`);
      return nx.Array(payload.map(fromWireValue));
    }
    case 'Map': {
      if (!Array.isArray(payload)) throw new Error(`decode: Map payload must be array`);
      return nx.Map(
        payload.map((pair) => {
          if (!Array.isArray(pair) || pair.length !== 2) {
            throw new Error(`decode: Map entry must be [key, value] pair`);
          }
          return [fromWireValue(pair[0]), fromWireValue(pair[1])] as [NexusValue, NexusValue];
        })
      );
    }
    default:
      throw new Error(`decode: unknown NexusValue tag '${tag}'`);
  }
}

/**
 * Encode a request into a length-prefixed MessagePack frame.
 *
 * Wire layout: `u32_le(body_len) ++ msgpack(body)`.
 */
export function encodeRequestFrame(req: RpcRequest): Uint8Array {
  const body = pack({
    id: req.id,
    command: req.command,
    args: req.args.map(toWireValue),
  });
  const frame = new Uint8Array(4 + body.length);
  const view = new DataView(frame.buffer, frame.byteOffset, frame.byteLength);
  view.setUint32(0, body.length, true);
  frame.set(body, 4);
  return frame;
}

/**
 * Decode a response body (the MessagePack-encoded bytes **after** the
 * length prefix). The caller is responsible for reading exactly
 * `length` bytes off the wire before handing them here.
 */
export function decodeResponseBody(body: Uint8Array): RpcResponse {
  const raw = unpack(body) as { id: number | bigint; result: unknown };
  const id = typeof raw.id === 'bigint' ? Number(raw.id) : Number(raw.id);
  const result = decodeResultEnvelope(raw.result);
  return { id, result };
}

function decodeResultEnvelope(raw: unknown): RpcResponseResult {
  if (raw === null || typeof raw !== 'object') {
    throw new Error(`decode: Result must be a tagged map, got ${typeof raw}`);
  }
  const entries = Object.entries(raw as Record<string, unknown>);
  if (entries.length !== 1) {
    throw new Error(`decode: Result must have exactly one tagged key`);
  }
  const [tag, payload] = entries[0];
  switch (tag) {
    case 'Ok':
      return { ok: true, value: fromWireValue(payload) };
    case 'Err':
      return { ok: false, message: String(payload) };
    default:
      throw new Error(`decode: Result must be 'Ok' or 'Err', got '${tag}'`);
  }
}

/** Build a `TransportResponse` from the server-side `Response` frame. */
export function rpcResponseToTransport(resp: RpcResponse): TransportResponse {
  if (!resp.result.ok) {
    throw new Error(`server: ${resp.result.message}`);
  }
  return { value: resp.result.value };
}
