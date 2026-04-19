/**
 * SDK dotted-name → wire-command mapping.
 *
 * Every method the TypeScript client exposes (`executeCypher`,
 * `listDatabases`, `ping`, …) funnels through `mapCommand(dotted,
 * payload)`. The table must stay in sync with
 * `docs/specs/sdk-transport.md §6` and with the Rust SDK's
 * `sdks/rust/src/transport/command_map.rs` — cross-SDK parity is
 * enforced by eyeballing this mapping rather than by a shared runtime
 * package.
 */

import { NexusValue, nx } from './types';

export interface CommandMapping {
  /** Wire-level command name routed to the server dispatcher. */
  command: string;
  /** Positional argument list, already encoded as `NexusValue`. */
  args: NexusValue[];
}

/**
 * Translate an SDK dotted name into a wire command + argument vector.
 * Returns `null` for unknown names so the client can fall back to the
 * HTTP transport (if configured) or surface a clear error.
 *
 * `payload` carries the method-specific JSON shape the caller hands
 * in. Some commands ignore it (`graph.ping`, `graph.stats`), others
 * require specific fields (`graph.cypher` needs `query` + optional
 * `parameters`).
 */
export function mapCommand(dotted: string, payload: Record<string, unknown>): CommandMapping | null {
  switch (dotted) {
    // ── Admin ────────────────────────────────────────────────────────
    case 'graph.cypher': {
      const query = payload.query;
      if (typeof query !== 'string') return null;
      const args: NexusValue[] = [nx.Str(query)];
      const params = payload.parameters;
      if (params !== undefined && params !== null) {
        args.push(jsonToNexus(params));
      }
      return { command: 'CYPHER', args };
    }
    case 'graph.ping':
      return { command: 'PING', args: [] };
    case 'graph.hello':
      return { command: 'HELLO', args: [nx.Int(1)] };
    case 'graph.stats':
      return { command: 'STATS', args: [] };
    case 'graph.health':
      return { command: 'HEALTH', args: [] };
    case 'graph.quit':
      return { command: 'QUIT', args: [] };
    case 'auth.login': {
      const apiKey = payload.api_key;
      if (typeof apiKey === 'string' && apiKey.length > 0) {
        return { command: 'AUTH', args: [nx.Str(apiKey)] };
      }
      const user = payload.username;
      const pass = payload.password;
      if (typeof user !== 'string' || typeof pass !== 'string') return null;
      return { command: 'AUTH', args: [nx.Str(user), nx.Str(pass)] };
    }

    // ── Database management ─────────────────────────────────────────
    case 'db.list':
      return { command: 'DB_LIST', args: [] };
    case 'db.create':
    case 'db.drop':
    case 'db.use': {
      const name = payload.name;
      if (typeof name !== 'string') return null;
      const cmd = dotted === 'db.create' ? 'DB_CREATE' : dotted === 'db.drop' ? 'DB_DROP' : 'DB_USE';
      return { command: cmd, args: [nx.Str(name)] };
    }

    // ── Schema inspection ───────────────────────────────────────────
    case 'schema.labels':
      return { command: 'LABELS', args: [] };
    case 'schema.rel_types':
      return { command: 'REL_TYPES', args: [] };
    case 'schema.property_keys':
      return { command: 'PROPERTY_KEYS', args: [] };
    case 'schema.indexes':
      return { command: 'INDEXES', args: [] };

    // ── Data import/export ──────────────────────────────────────────
    case 'data.export': {
      const format = payload.format;
      if (typeof format !== 'string') return null;
      const args: NexusValue[] = [nx.Str(format)];
      const query = payload.query;
      if (typeof query === 'string') {
        args.push(nx.Str(query));
      }
      return { command: 'EXPORT', args };
    }
    case 'data.import': {
      const format = payload.format;
      const data = payload.data;
      if (typeof format !== 'string' || typeof data !== 'string') return null;
      return { command: 'IMPORT', args: [nx.Str(format), nx.Str(data)] };
    }

    default:
      return null;
  }
}

/**
 * JSON → NexusValue. Used by `graph.cypher` parameter translation and
 * the HTTP-fallback response decoder.
 */
export function jsonToNexus(v: unknown): NexusValue {
  if (v === null || v === undefined) return nx.Null();
  if (typeof v === 'boolean') return nx.Bool(v);
  if (typeof v === 'bigint') return nx.Int(v);
  if (typeof v === 'number') {
    return Number.isInteger(v) ? nx.Int(v) : nx.Float(v);
  }
  if (typeof v === 'string') return nx.Str(v);
  if (v instanceof Uint8Array) return nx.Bytes(v);
  if (Array.isArray(v)) return nx.Array(v.map(jsonToNexus));
  if (typeof v === 'object') {
    const entries = Object.entries(v as Record<string, unknown>);
    return nx.Map(entries.map(([k, val]) => [nx.Str(k), jsonToNexus(val)] as [NexusValue, NexusValue]));
  }
  return nx.Null();
}

/**
 * NexusValue → plain JS value for user-visible API surfaces. `Bytes`
 * is surfaced as `Uint8Array`, `Map` as a plain object (keys stringified
 * when necessary — matches the HTTP JSON response shape).
 */
export function nexusToJson(v: NexusValue): unknown {
  switch (v.kind) {
    case 'Null':
      return null;
    case 'Bool':
      return v.value;
    case 'Int':
      if (typeof v.value === 'bigint') {
        // Match JSON-number range: fold safe integers back to number.
        return v.value >= BigInt(Number.MIN_SAFE_INTEGER) && v.value <= BigInt(Number.MAX_SAFE_INTEGER)
          ? Number(v.value)
          : v.value;
      }
      return v.value;
    case 'Float':
      return v.value;
    case 'Bytes':
      return v.value;
    case 'Str':
      return v.value;
    case 'Array':
      return v.value.map(nexusToJson);
    case 'Map': {
      const obj: Record<string, unknown> = {};
      for (const [k, val] of v.value) {
        const key =
          k.kind === 'Str'
            ? k.value
            : k.kind === 'Int'
              ? String(k.value)
              : JSON.stringify(nexusToJson(k));
        obj[key] = nexusToJson(val);
      }
      return obj;
    }
  }
}
