/**
 * Shared transport-layer primitives.
 *
 * Every `NexusClient` in 1.0.0 delegates its wire format to a
 * `Transport` picked at construction time. Three modes are recognised:
 *
 * - `nexus` — native binary RPC (length-prefixed MessagePack on port
 *   15475). **Default** on Node.
 * - `http` / `https` — JSON over REST (port 15474 / 443). Legacy /
 *   browser-friendly.
 * - `resp3` — reserved for a future RESP3 implementation. Currently
 *   throws a clear configuration error.
 *
 * Precedence when picking the transport:
 *
 * 1. URL scheme in `NexusConfig.baseUrl` (`nexus://` → RPC, `http://` → HTTP, ...)
 * 2. `NEXUS_SDK_TRANSPORT` env var (Node only)
 * 3. `NexusConfig.transport` field
 * 4. Default: `'nexus'` (RPC)
 *
 * See `docs/specs/sdk-transport.md` for the cross-SDK contract.
 */

/**
 * Transport selector. Values match the URL-scheme tokens and the
 * `NEXUS_SDK_TRANSPORT` env-var strings so a single token lines up
 * everywhere.
 */
export type TransportMode = 'nexus' | 'resp3' | 'http' | 'https';

/**
 * Dynamically-typed value carried by RPC requests and responses.
 *
 * Mirrors `nexus_protocol::rpc::types::NexusValue` — a tagged union
 * rather than an `unknown`, so SDKs can map every wire variant to a
 * native JS value with a flat switch. The RPC codec serialises this
 * using rmp-serde's externally-tagged representation (`{"Str": "hi"}`,
 * the literal string `"Null"` for the unit variant, etc.).
 */
export type NexusValue =
  | { kind: 'Null' }
  | { kind: 'Bool'; value: boolean }
  | { kind: 'Int'; value: bigint | number }
  | { kind: 'Float'; value: number }
  | { kind: 'Bytes'; value: Uint8Array }
  | { kind: 'Str'; value: string }
  | { kind: 'Array'; value: NexusValue[] }
  | { kind: 'Map'; value: Array<[NexusValue, NexusValue]> };

/** Helper constructors — shorter at call sites than writing the literal. */
export const nx = {
  Null: (): NexusValue => ({ kind: 'Null' }),
  Bool: (value: boolean): NexusValue => ({ kind: 'Bool', value }),
  Int: (value: number | bigint): NexusValue => ({ kind: 'Int', value }),
  Float: (value: number): NexusValue => ({ kind: 'Float', value }),
  Bytes: (value: Uint8Array): NexusValue => ({ kind: 'Bytes', value }),
  Str: (value: string): NexusValue => ({ kind: 'Str', value }),
  Array: (value: NexusValue[]): NexusValue => ({ kind: 'Array', value }),
  Map: (value: Array<[NexusValue, NexusValue]>): NexusValue => ({ kind: 'Map', value }),
};

/** A single request against the active transport. */
export interface TransportRequest {
  /** Wire command name (`CYPHER`, `PING`, `STATS`, ...). */
  command: string;
  /** Positional arguments as already-encoded `NexusValue` entries. */
  args: NexusValue[];
}

/** A single response from the active transport. */
export interface TransportResponse {
  value: NexusValue;
}

/** Credentials carried by a transport. Both paths may be set; apiKey wins. */
export interface TransportCredentials {
  apiKey?: string;
  username?: string;
  password?: string;
}

/** Generic transport interface — one method per request/response pair. */
export interface Transport {
  execute(req: TransportRequest): Promise<TransportResponse>;
  describe(): string;
  isRpc(): boolean;
  close(): Promise<void>;
}
