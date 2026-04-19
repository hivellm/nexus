import type {
  NexusConfig,
  QueryParams,
  QueryResult,
  Node,
  Relationship,
  NodeProperties,
  RelationshipProperties,
  SchemaInfo,
  QueryStatistics,
  BatchOperation,
  DatabaseInfo,
  ListDatabasesResponse,
  CreateDatabaseResponse,
  DropDatabaseResponse,
  SwitchDatabaseResponse,
} from './types';
import {
  NexusSDKError,
  ConnectionError,
  ValidationError,
} from './errors';
import {
  Endpoint,
  NexusValue,
  Transport,
  TransportCredentials,
  TransportMode,
  TransportRequest,
  buildTransport,
  endpointToString,
  nexusToJson,
  nx,
} from './transports';

/**
 * Nexus Graph Database client.
 *
 * Defaults to the native binary RPC transport on `nexus://127.0.0.1:15475`.
 * Callers can opt down to HTTP with a `transport: 'http'` option or by
 * passing an `http://` URL; see `docs/specs/sdk-transport.md` for the
 * full contract.
 *
 * @example
 * ```typescript
 * const client = new NexusClient(); // nexus://127.0.0.1:15475 (RPC)
 * const result = await client.executeCypher('RETURN 1 AS one');
 * ```
 *
 * @example
 * ```typescript
 * // HTTP fallback (browser, firewall, diagnostic)
 * const client = new NexusClient({
 *   baseUrl: 'http://localhost:15474',
 *   auth: { apiKey: 'nexus_sk_...' },
 * });
 * ```
 */
export class NexusClient {
  private readonly debug: boolean;
  private readonly transport: Transport;
  private readonly endpoint: Endpoint;
  private readonly mode: TransportMode;

  constructor(config: NexusConfig = {}) {
    this.debug = config.debug ?? false;

    const credentials: TransportCredentials = {
      apiKey: config.auth?.apiKey,
      username: config.auth?.username,
      password: config.auth?.password,
    };

    // Validation: API-key + basic-auth optional for local RPC (auth is
    // disabled on 127.0.0.1 by default); required when the user targets
    // a non-loopback host over HTTP.
    if (config.auth) {
      const hasKey = !!credentials.apiKey;
      const hasBasic = !!credentials.username && !!credentials.password;
      if (config.auth.apiKey === '' || (config.auth.username && !config.auth.password)) {
        throw new ValidationError(
          'auth: provide either a non-empty apiKey or username+password pair'
        );
      }
      if (!hasKey && !hasBasic && config.auth.username) {
        throw new ValidationError(
          'auth: username provided without password — basic auth needs both fields'
        );
      }
    }

    const envTransport = typeof process !== 'undefined' ? process.env?.NEXUS_SDK_TRANSPORT : undefined;
    const built = buildTransport({
      baseUrl: config.baseUrl,
      transport: config.transport,
      rpcPort: config.rpcPort,
      resp3Port: config.resp3Port,
      credentials,
      timeoutMs: config.timeout,
      retries: config.retries,
      envTransport,
    });
    this.transport = built.transport;
    this.endpoint = built.endpoint;
    this.mode = built.mode;

    if (this.debug) {
      console.log(`[nexus-sdk] transport: ${this.transport.describe()}`);
    }
  }

  /** Human-readable endpoint + transport label — handy for CLI verbose flags. */
  endpointDescription(): string {
    return this.transport.describe();
  }

  /** The raw endpoint used by this client. */
  getEndpoint(): Endpoint {
    return this.endpoint;
  }

  /** Active transport mode after the precedence chain was resolved. */
  getTransportMode(): TransportMode {
    return this.mode;
  }

  /** Close any persistent sockets (RPC transport owns a TCP connection). */
  async close(): Promise<void> {
    await this.transport.close();
  }

  // ── Cypher ────────────────────────────────────────────────────────────

  async executeCypher(cypher: string, params?: QueryParams): Promise<QueryResult> {
    const args: NexusValue[] = [nx.Str(cypher)];
    if (params && Object.keys(params).length > 0) {
      args.push(paramsToNexus(params));
    }
    const req: TransportRequest = { command: 'CYPHER', args };
    const resp = await this.transport.execute(req);
    return extractQueryResult(resp.value);
  }

  async createNode(labels: string[], properties: NodeProperties): Promise<Node> {
    const labelsStr = labels.map((l) => `:${l}`).join('');
    const cypher = `CREATE (n${labelsStr} $props) RETURN n`;
    const result = await this.executeCypher(cypher, { props: properties });
    if (result.rows.length === 0) throw new NexusSDKError('Failed to create node');
    return result.rows[0].n as Node;
  }

  async getNode(id: number): Promise<Node | null> {
    const result = await this.executeCypher('MATCH (n) WHERE id(n) = $id RETURN n', { id });
    return result.rows.length > 0 ? (result.rows[0].n as Node) : null;
  }

  async updateNode(id: number, properties: NodeProperties): Promise<Node> {
    const result = await this.executeCypher(
      'MATCH (n) WHERE id(n) = $id SET n += $props RETURN n',
      { id, props: properties }
    );
    if (result.rows.length === 0) throw new NexusSDKError('Node not found');
    return result.rows[0].n as Node;
  }

  async deleteNode(id: number, detach = false): Promise<void> {
    const cypher = detach
      ? 'MATCH (n) WHERE id(n) = $id DETACH DELETE n'
      : 'MATCH (n) WHERE id(n) = $id DELETE n';
    await this.executeCypher(cypher, { id });
  }

  async findNodes(
    label: string,
    properties?: NodeProperties,
    limit?: number
  ): Promise<Node[]> {
    let cypher = `MATCH (n:${label})`;
    if (properties && Object.keys(properties).length > 0) {
      cypher +=
        ' WHERE ' +
        Object.keys(properties)
          .map((key) => `n.${key} = $props.${key}`)
          .join(' AND ');
    }
    cypher += ' RETURN n';
    if (limit) cypher += ` LIMIT ${limit}`;
    const result = await this.executeCypher(
      cypher,
      properties ? { props: properties } : undefined
    );
    return result.rows.map((row) => row.n as Node);
  }

  async createRelationship(
    startNodeId: number,
    endNodeId: number,
    type: string,
    properties?: RelationshipProperties
  ): Promise<Relationship> {
    const cypher = properties
      ? `MATCH (a), (b) WHERE id(a) = $startId AND id(b) = $endId CREATE (a)-[r:${type} $props]->(b) RETURN r`
      : `MATCH (a), (b) WHERE id(a) = $startId AND id(b) = $endId CREATE (a)-[r:${type}]->(b) RETURN r`;
    const params: Record<string, unknown> = { startId: startNodeId, endId: endNodeId };
    if (properties) params.props = properties;
    const result = await this.executeCypher(cypher, params);
    if (result.rows.length === 0) throw new NexusSDKError('Failed to create relationship');
    return result.rows[0].r as Relationship;
  }

  async getRelationship(id: number): Promise<Relationship | null> {
    const result = await this.executeCypher(
      'MATCH ()-[r]->() WHERE id(r) = $id RETURN r',
      { id }
    );
    return result.rows.length > 0 ? (result.rows[0].r as Relationship) : null;
  }

  async deleteRelationship(id: number): Promise<void> {
    await this.executeCypher('MATCH ()-[r]->() WHERE id(r) = $id DELETE r', { id });
  }

  async getLabels(): Promise<string[]> {
    const resp = await this.transport.execute({ command: 'LABELS', args: [] });
    const json = nexusToJson(resp.value);
    return asStringArray(json, 'labels');
  }

  async getRelationshipTypes(): Promise<string[]> {
    const resp = await this.transport.execute({ command: 'REL_TYPES', args: [] });
    const json = nexusToJson(resp.value);
    return asStringArray(json, 'types');
  }

  async getSchema(): Promise<SchemaInfo> {
    const [labels, relationshipTypes] = await Promise.all([
      this.getLabels(),
      this.getRelationshipTypes(),
    ]);
    return { labels, relationshipTypes, indexes: [] };
  }

  async executeBatch(operations: BatchOperation[]): Promise<QueryResult[]> {
    // Each operation is dispatched serially through the single persistent
    // RPC socket so frames cannot interleave on the wire.
    const out: QueryResult[] = [];
    for (const op of operations) {
      out.push(await this.executeCypher(op.cypher, op.params));
    }
    return out;
  }

  async testConnection(): Promise<boolean> {
    try {
      await this.transport.execute({ command: 'PING', args: [] });
      return true;
    } catch {
      throw new ConnectionError(`Failed to connect to ${endpointToString(this.endpoint)}`);
    }
  }

  async ping(): Promise<boolean> {
    return this.testConnection();
  }

  async getStatistics(): Promise<QueryStatistics> {
    const resp = await this.transport.execute({ command: 'STATS', args: [] });
    const json = nexusToJson(resp.value);
    return extractStats(json);
  }

  // ── Database management ───────────────────────────────────────────────

  async listDatabases(): Promise<ListDatabasesResponse> {
    const resp = await this.transport.execute({ command: 'DB_LIST', args: [] });
    const json = nexusToJson(resp.value);
    if (typeof json !== 'object' || json === null) {
      throw new NexusSDKError('DB_LIST: expected object response');
    }
    const obj = json as Record<string, unknown>;
    const databases = Array.isArray(obj.databases) ? (obj.databases as DatabaseInfo[]) : [];
    const defaultDatabase =
      typeof obj.defaultDatabase === 'string'
        ? obj.defaultDatabase
        : typeof obj.default === 'string'
          ? obj.default
          : 'default';
    return { databases, defaultDatabase };
  }

  async createDatabase(name: string): Promise<CreateDatabaseResponse> {
    const resp = await this.transport.execute({
      command: 'DB_CREATE',
      args: [nx.Str(name)],
    });
    return asSuccessMessage(nexusToJson(resp.value), name);
  }

  async getDatabase(name: string): Promise<DatabaseInfo> {
    // No dedicated RPC verb — fold through a Cypher `SHOW DATABASE $name`.
    const result = await this.executeCypher('SHOW DATABASE $name', { name });
    if (result.rows.length === 0) {
      throw new NexusSDKError(`Database '${name}' not found`);
    }
    return result.rows[0] as unknown as DatabaseInfo;
  }

  async dropDatabase(name: string): Promise<DropDatabaseResponse> {
    const resp = await this.transport.execute({
      command: 'DB_DROP',
      args: [nx.Str(name)],
    });
    const json = asSuccessMessage(nexusToJson(resp.value), name);
    return { success: json.success, message: json.message };
  }

  async getCurrentDatabase(): Promise<string> {
    const resp = await this.transport.execute({ command: 'DB_CURRENT', args: [] });
    const json = nexusToJson(resp.value);
    if (typeof json === 'string') return json;
    if (typeof json === 'object' && json !== null) {
      const obj = json as Record<string, unknown>;
      if (typeof obj.database === 'string') return obj.database;
      if (typeof obj.name === 'string') return obj.name;
    }
    throw new NexusSDKError(`DB_CURRENT: unexpected response shape`);
  }

  async switchDatabase(name: string): Promise<SwitchDatabaseResponse> {
    const resp = await this.transport.execute({
      command: 'DB_USE',
      args: [nx.Str(name)],
    });
    const json = asSuccessMessage(nexusToJson(resp.value), name);
    return { success: json.success, message: json.message };
  }
}

// ── Helpers ────────────────────────────────────────────────────────────

function paramsToNexus(params: QueryParams): NexusValue {
  const pairs: Array<[NexusValue, NexusValue]> = [];
  for (const [k, v] of Object.entries(params)) {
    pairs.push([nx.Str(k), jsValueToNexus(v)]);
  }
  return nx.Map(pairs);
}

function jsValueToNexus(v: unknown): NexusValue {
  if (v === null || v === undefined) return nx.Null();
  if (typeof v === 'boolean') return nx.Bool(v);
  if (typeof v === 'bigint') return nx.Int(v);
  if (typeof v === 'number') {
    return Number.isInteger(v) ? nx.Int(v) : nx.Float(v);
  }
  if (typeof v === 'string') return nx.Str(v);
  if (v instanceof Uint8Array) return nx.Bytes(v);
  if (Array.isArray(v)) return nx.Array(v.map(jsValueToNexus));
  if (typeof v === 'object') {
    const pairs: Array<[NexusValue, NexusValue]> = [];
    for (const [k, val] of Object.entries(v as Record<string, unknown>)) {
      pairs.push([nx.Str(k), jsValueToNexus(val)]);
    }
    return nx.Map(pairs);
  }
  return nx.Null();
}

function extractQueryResult(value: NexusValue): QueryResult {
  const json = nexusToJson(value);
  if (typeof json !== 'object' || json === null) {
    throw new NexusSDKError('CYPHER: expected object response');
  }
  const obj = json as Record<string, unknown>;
  const columns = Array.isArray(obj.columns)
    ? obj.columns.map((c) => String(c))
    : [];
  const rowsRaw = Array.isArray(obj.rows) ? obj.rows : [];
  const rows = rowsRaw.map((row) => normalizeRow(row, columns));
  return { columns, rows };
}

function normalizeRow(row: unknown, columns: string[]): Record<string, unknown> {
  if (Array.isArray(row)) {
    const obj: Record<string, unknown> = {};
    columns.forEach((col, idx) => {
      obj[col] = row[idx];
    });
    return obj;
  }
  if (typeof row === 'object' && row !== null) {
    return row as Record<string, unknown>;
  }
  return { value: row };
}

function asStringArray(json: unknown, field: string): string[] {
  if (Array.isArray(json)) return json.map(String);
  if (typeof json === 'object' && json !== null) {
    const obj = json as Record<string, unknown>;
    if (Array.isArray(obj[field])) return (obj[field] as unknown[]).map(String);
  }
  return [];
}

function asSuccessMessage(
  json: unknown,
  fallbackName: string
): { success: boolean; message: string; name: string } {
  if (typeof json !== 'object' || json === null) {
    return { success: true, message: '', name: fallbackName };
  }
  const obj = json as Record<string, unknown>;
  return {
    success: typeof obj.success === 'boolean' ? obj.success : true,
    message: typeof obj.message === 'string' ? obj.message : '',
    name: typeof obj.name === 'string' ? obj.name : fallbackName,
  };
}

function extractStats(json: unknown): QueryStatistics {
  // Synthesize zeros — STATS returns server-wide counters, not per-query
  // deltas. The QueryStatistics shape exists for API stability; a future
  // iteration will surface per-query stats when the executor emits them.
  const out: QueryStatistics = {
    nodesCreated: 0,
    nodesDeleted: 0,
    relationshipsCreated: 0,
    relationshipsDeleted: 0,
    propertiesSet: 0,
    labelsAdded: 0,
    labelsRemoved: 0,
    executionTime: 0,
  };
  if (typeof json !== 'object' || json === null) return out;
  const obj = json as Record<string, unknown>;
  if (typeof obj.execution_time_ms === 'number') out.executionTime = obj.execution_time_ms;
  return out;
}
