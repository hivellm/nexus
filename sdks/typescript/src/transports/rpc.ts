/**
 * Native binary RPC transport — a thin wrapper around the Thunder client.
 *
 * Thunder (`@hivehub/thunder`) owns the wire (frame + MessagePack codec,
 * SPEC-001), the single-connection multiplexer, and the handshake /
 * reconnect / credential re-send. This file adapts the SDK's own value
 * model (`NexusValue`, capitalized kinds) to Thunder's (`Value`, lowercase
 * kinds) and preserves the `Transport` interface the rest of the SDK
 * depends on.
 *
 * Handshake: the Nexus server ignores `HELLO` arguments and gates the
 * connection on a separate `AUTH`, so Thunder's `auth_command` handshake
 * with the `arg_less` HELLO style (bare `HELLO []` then `AUTH [...]`)
 * matches it exactly — and, unlike a hand-rolled connect, re-runs on
 * reconnect so credentials are re-sent automatically.
 */

import {
  Client,
  Config,
  Value,
  type Credentials,
} from '@hivehub/thunder';
import {
  Transport,
  TransportCredentials,
  TransportRequest,
  TransportResponse,
  NexusValue,
  nx,
} from './types';
import { Endpoint, endpointToString } from './endpoint';

/** Translate the SDK's `NexusValue` into a Thunder `Value`. */
function nexusToThunder(v: NexusValue): Value {
  switch (v.kind) {
    case 'Null':
      return Value.null();
    case 'Bool':
      return Value.bool(v.value);
    case 'Int':
      return Value.int(v.value);
    case 'Float':
      return Value.float(v.value);
    case 'Bytes':
      return Value.bytes(v.value);
    case 'Str':
      return Value.str(v.value);
    case 'Array':
      return Value.array(v.value.map(nexusToThunder));
    case 'Map':
      return Value.map(
        v.value.map(([k, val]) => [nexusToThunder(k), nexusToThunder(val)] as [Value, Value]),
      );
  }
}

/** Translate a Thunder `Value` back into the SDK's `NexusValue`. */
function thunderToNexus(v: Value): NexusValue {
  switch (v.kind) {
    case 'null':
      return nx.Null();
    case 'bool':
      return nx.Bool(v.value);
    case 'int':
      return nx.Int(v.value);
    case 'float':
      return nx.Float(v.value);
    case 'bytes':
      return nx.Bytes(v.value);
    case 'str':
      return nx.Str(v.value);
    case 'array':
      return nx.Array(v.value.map(thunderToNexus));
    case 'map':
      return nx.Map(
        v.value.map(([k, val]) => [thunderToNexus(k), thunderToNexus(val)] as [NexusValue, NexusValue]),
      );
  }
}

export class RpcTransport implements Transport {
  private readonly endpoint: Endpoint;
  private readonly credentials: TransportCredentials;
  private readonly connectTimeoutMs: number;
  private client: Client | null = null;
  private connectPromise: Promise<Client> | null = null;

  constructor(endpoint: Endpoint, credentials: TransportCredentials, connectTimeoutMs = 5_000) {
    this.endpoint = endpoint;
    this.credentials = credentials;
    this.connectTimeoutMs = connectTimeoutMs;
  }

  async execute(req: TransportRequest): Promise<TransportResponse> {
    const client = await this.ensureConnected();
    // Thunder's `call` returns the `Ok` payload directly and throws a typed
    // `ThunderError` (Auth/Server/Connection/Timeout/Decode/…) on `Err`; the
    // SDK's higher layers surface those to the caller unchanged.
    const result = await client.call(req.command, req.args.map(nexusToThunder));
    return { value: thunderToNexus(result) };
  }

  describe(): string {
    return `${endpointToString(this.endpoint)} (RPC)`;
  }

  isRpc(): boolean {
    return true;
  }

  async close(): Promise<void> {
    const client = this.client;
    this.client = null;
    this.connectPromise = null;
    if (client) {
      await client.close();
    }
  }

  /** Thunder config matching the Nexus RPC listener. */
  private config(): Config {
    return Config.standard()
      .withScheme('nexus')
      .withPort(this.endpoint.port)
      .withHandshake('auth_command')
      .withHelloStyle('arg_less');
  }

  /** SDK credentials → Thunder credentials (api key wins, then user/pass). */
  private thunderCredentials(): Credentials | undefined {
    if (this.credentials.apiKey) {
      return { type: 'apiKey', apiKey: this.credentials.apiKey };
    }
    if (this.credentials.username && this.credentials.password) {
      return {
        type: 'userPass',
        user: this.credentials.username,
        pass: this.credentials.password,
      };
    }
    return undefined;
  }

  /** Lazy-connect a single Thunder client, reusing it across calls. */
  private async ensureConnected(): Promise<Client> {
    if (this.client && this.client.isAlive) {
      return this.client;
    }
    if (this.connectPromise) {
      return this.connectPromise;
    }
    // Bare `host:port` endpoint sidesteps Thunder's scheme matching.
    this.connectPromise = Client.connect(
      `${this.endpoint.host}:${this.endpoint.port}`,
      this.config(),
      {
        credentials: this.thunderCredentials(),
        connectTimeoutMs: this.connectTimeoutMs,
      },
    )
      .then((client) => {
        this.client = client;
        this.connectPromise = null;
        return client;
      })
      .catch((err) => {
        this.connectPromise = null;
        throw err;
      });
    return this.connectPromise;
  }
}
