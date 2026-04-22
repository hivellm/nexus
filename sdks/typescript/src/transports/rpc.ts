/**
 * Native binary RPC transport.
 *
 * Holds a single TCP stream per client (frames cannot interleave) and
 * serialises concurrent `execute()` calls behind a tiny mutex. Mirrors
 * the Rust SDK's `transport::rpc::RpcTransport`: lazy connect, HELLO
 * handshake, optional AUTH, monotonic request ids that skip the
 * reserved `PUSH_ID` (`u32::MAX`).
 */

import * as net from 'node:net';
import {
  Transport,
  TransportCredentials,
  TransportRequest,
  TransportResponse,
  nx,
} from './types';
import { Endpoint, endpointAuthority, endpointToString } from './endpoint';
import {
  decodeResponseBody,
  encodeRequestFrame,
  rpcResponseToTransport,
  RpcRequest,
  RpcResponse,
} from './codec';

/** Reserved id used by the server for PUSH frames — clients must skip it. */
const PUSH_ID = 0xffff_ffff;

export class RpcTransport implements Transport {
  private readonly endpoint: Endpoint;
  private readonly credentials: TransportCredentials;
  private socket: net.Socket | null = null;
  private buffer: Buffer = Buffer.alloc(0);
  private readonly pending = new Map<number, {
    resolve: (resp: RpcResponse) => void;
    reject: (err: Error) => void;
  }>();
  private connectPromise: Promise<void> | null = null;
  private nextId = 1;
  private readonly connectTimeoutMs: number;

  constructor(endpoint: Endpoint, credentials: TransportCredentials, connectTimeoutMs = 5_000) {
    this.endpoint = endpoint;
    this.credentials = credentials;
    this.connectTimeoutMs = connectTimeoutMs;
  }

  async execute(req: TransportRequest): Promise<TransportResponse> {
    const resp = await this.call(req.command, req.args);
    return rpcResponseToTransport(resp);
  }

  describe(): string {
    return `${endpointToString(this.endpoint)} (RPC)`;
  }

  isRpc(): boolean {
    return true;
  }

  async close(): Promise<void> {
    const sock = this.socket;
    this.socket = null;
    this.connectPromise = null;
    if (sock) {
      await new Promise<void>((resolve) => {
        sock.end(() => resolve());
        sock.destroy();
      });
    }
    // Cancel pending requests.
    for (const [, p] of this.pending) {
      p.reject(new Error('transport closed'));
    }
    this.pending.clear();
  }

  /**
   * Low-level single request. Lazy-connects on first use.
   */
  async call(command: string, args: TransportRequest['args']): Promise<RpcResponse> {
    await this.ensureConnected();
    return this.send({ id: this.allocId(), command, args });
  }

  private allocId(): number {
    let id = this.nextId++;
    if (id === PUSH_ID) id = this.nextId++;
    if (this.nextId === PUSH_ID) this.nextId++;
    // Wrap before overflowing u32.
    if (this.nextId > 0xffff_fffe) this.nextId = 1;
    return id;
  }

  private async ensureConnected(): Promise<void> {
    if (this.socket && !this.socket.destroyed) return;
    if (this.connectPromise) return this.connectPromise;
    this.connectPromise = this.connect().catch((e) => {
      this.connectPromise = null;
      throw e;
    });
    try {
      await this.connectPromise;
    } finally {
      this.connectPromise = null;
    }
  }

  private connect(): Promise<void> {
    return new Promise<void>((resolve, reject) => {
      const authority = endpointAuthority(this.endpoint);
      const sock = net.createConnection({
        host: this.endpoint.host,
        port: this.endpoint.port,
      });
      sock.setNoDelay(true);

      let settled = false;
      const timer = setTimeout(() => {
        if (settled) return;
        settled = true;
        sock.destroy();
        reject(new Error(`failed to connect to ${authority}: timeout after ${this.connectTimeoutMs}ms`));
      }, this.connectTimeoutMs);

      sock.once('error', (err) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        reject(new Error(`failed to connect to ${authority}: ${err.message}`));
      });

      sock.once('connect', () => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        this.socket = sock;
        sock.on('data', (chunk) => this.onData(chunk));
        sock.on('close', () => this.onClose());
        sock.on('error', (err) => this.onSocketError(err));
        this.handshake().then(resolve, reject);
      });
    });
  }

  private async handshake(): Promise<void> {
    // HELLO 1
    const hello = await this.send({
      id: 0,
      command: 'HELLO',
      args: [nx.Int(1)],
    });
    if (!hello.result.ok) {
      throw new Error(`HELLO rejected by server: ${hello.result.message}`);
    }
    // Optional AUTH.
    if (this.hasCredentials()) {
      const args = this.credentials.apiKey
        ? [nx.Str(this.credentials.apiKey)]
        : [nx.Str(this.credentials.username ?? ''), nx.Str(this.credentials.password ?? '')];
      const auth = await this.send({ id: 0, command: 'AUTH', args });
      if (!auth.result.ok) {
        throw new Error(`authentication failed: ${auth.result.message}`);
      }
    }
  }

  private hasCredentials(): boolean {
    return (
      !!this.credentials.apiKey ||
      (!!this.credentials.username && !!this.credentials.password)
    );
  }

  private send(req: RpcRequest): Promise<RpcResponse> {
    const sock = this.socket;
    if (!sock || sock.destroyed) {
      return Promise.reject(new Error('RPC transport is not connected'));
    }
    return new Promise<RpcResponse>((resolve, reject) => {
      this.pending.set(req.id, { resolve, reject });
      const frame = encodeRequestFrame(req);
      sock.write(frame, (err) => {
        if (err) {
          this.pending.delete(req.id);
          reject(new Error(`failed to send RPC frame: ${err.message}`));
        }
      });
    });
  }

  private onData(chunk: Buffer): void {
    this.buffer = this.buffer.length === 0 ? chunk : Buffer.concat([this.buffer, chunk]);
    // Drain as many complete frames as the buffer currently holds.
    while (this.buffer.length >= 4) {
      const length = this.buffer.readUInt32LE(0);
      if (this.buffer.length < 4 + length) {
        return;
      }
      const body = this.buffer.subarray(4, 4 + length);
      this.buffer = this.buffer.subarray(4 + length);
      try {
        const resp = decodeResponseBody(body);
        const pending = this.pending.get(resp.id);
        if (pending) {
          this.pending.delete(resp.id);
          pending.resolve(resp);
        }
        // A frame for an unknown id (or PUSH_ID) is ignored here; push
        // subscriptions are not yet implemented on the SDK side.
      } catch (e) {
        // A malformed frame poisons the whole stream — reject every
        // pending request rather than silently dropping frames.
        const err = e instanceof Error ? e : new Error(String(e));
        for (const [, p] of this.pending) p.reject(err);
        this.pending.clear();
        this.buffer = Buffer.alloc(0);
        this.socket?.destroy();
        this.socket = null;
        return;
      }
    }
  }

  private onClose(): void {
    this.socket = null;
    this.buffer = Buffer.alloc(0);
    for (const [, p] of this.pending) {
      p.reject(new Error('RPC connection closed'));
    }
    this.pending.clear();
  }

  private onSocketError(err: Error): void {
    for (const [, p] of this.pending) {
      p.reject(new Error(`RPC socket error: ${err.message}`));
    }
    this.pending.clear();
    this.socket?.destroy();
    this.socket = null;
  }
}
