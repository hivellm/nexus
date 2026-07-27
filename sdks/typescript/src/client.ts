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
  CreateNodeResponse,
  GetNodeByExternalIdResponse,
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
  /**
   * Client-side session database. The server is stateless (the
   * `/session/database` routes were removed), so `switchDatabase` updates
   * this and every `executeCypher` stamps it for per-database routing.
   */
  private currentDatabase: string;

  constructor(config: NexusConfig = {}) {
    this.debug = config.debug ?? false;
    this.currentDatabase = config.database ?? 'neo4j';

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
    const req: TransportRequest = {
      command: 'CYPHER',
      args,
      database: this.currentDatabase,
    };
    const resp = await this.transport.execute(req);
    return extractQueryResult(resp.value);
  }

  async createNode(labels: string[], properties: NodeProperties): Promise<Node> {
    const labelsStr = labels.map((l) => `:${l}`).join('');
    const { clause, params } = inlineProps(properties as Record<string, unknown>, 'p');
    const cypher = `CREATE (n${labelsStr}${clause}) RETURN n`;
    const result = await this.executeCypher(cypher, params);
    if (result.rows.length === 0) throw new NexusSDKError('Failed to create node');
    return rawToNode(result.rows[0].n);
  }

  /**
   * Create a node with a caller-supplied external id (Phase9 §5.5).
   *
   * Issues `POST /data/nodes` with `external_id` and optional
   * `conflict_policy` in the JSON body. Both fields use snake_case to
   * match the server contract.
   *
   * @param labels - Node labels
   * @param properties - Node properties
   * @param externalId - Prefixed external id, e.g. `uuid:…`, `str:…`,
   *   `sha256:…`, `blake3:…`, `sha512:…`, `bytes:…`
   * @param conflictPolicy - `"error"` (default) | `"match"` | `"replace"`
   */
  async createNodeWithExternalId(
    labels: string[],
    properties: NodeProperties,
    externalId: string,
    conflictPolicy?: string,
  ): Promise<CreateNodeResponse> {
    const body: Record<string, unknown> = {
      labels,
      properties,
      external_id: externalId,
    };
    if (conflictPolicy !== undefined) {
      body.conflict_policy = conflictPolicy;
    }
    const resp = await this.transport.execute({
      command: 'NODE_CREATE_EXT',
      args: [nx.Str(JSON.stringify(body))],
    });
    const json = nexusToJson(resp.value) as Record<string, unknown>;
    return {
      node_id: json.node_id as number,
      message: typeof json.message === 'string' ? json.message : '',
      error: typeof json.error === 'string' ? json.error : undefined,
    };
  }

  /**
   * Resolve a node by its external id (Phase9 §5.5).
   *
   * Issues `GET /data/nodes/by-external-id?external_id=<urlencoded>`.
   * Returns `node: null` when the external id is not registered.
   *
   * @param externalId - The prefixed external id to look up
   */
  async getNodeByExternalId(externalId: string): Promise<GetNodeByExternalIdResponse> {
    const resp = await this.transport.execute({
      command: 'NODE_GET_BY_EXT_ID',
      args: [nx.Str(externalId)],
    });
    const json = nexusToJson(resp.value) as Record<string, unknown>;
    return {
      node: (json.node ?? null) as Node | null,
      message: typeof json.message === 'string' ? json.message : '',
      error: typeof json.error === 'string' ? json.error : undefined,
    };
  }

  async getNode(id: number): Promise<Node | null> {
    const result = await this.executeCypher('MATCH (n) WHERE id(n) = $id RETURN n', { id });
    return result.rows.length > 0 ? rawToNode(result.rows[0].n) : null;
  }

  async updateNode(id: number, properties: NodeProperties): Promise<Node> {
    const result = await this.executeCypher(
      'MATCH (n) WHERE id(n) = $id SET n += $props RETURN n',
      { id, props: properties }
    );
    if (result.rows.length === 0) throw new NexusSDKError('Node not found');
    return rawToNode(result.rows[0].n);
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
    const params: Record<string, unknown> = {};
    if (properties && Object.keys(properties).length > 0) {
      const props = properties as Record<string, unknown>;
      cypher +=
        ' WHERE ' +
        Object.keys(props)
          .map((key, i) => {
            const p = `p${i}`;
            params[p] = props[key];
            return `n.${key} = $${p}`;
          })
          .join(' AND ');
    }
    cypher += ' RETURN n';
    if (limit) cypher += ` LIMIT ${limit}`;
    const result = await this.executeCypher(cypher, params);
    return result.rows.map((row) => rawToNode(row.n));
  }

  async createRelationship(
    startNodeId: number,
    endNodeId: number,
    type: string,
    properties?: RelationshipProperties
  ): Promise<Relationship> {
    const { clause, params: propParams } = properties
      ? inlineProps(properties as Record<string, unknown>, 'p')
      : { clause: '', params: {} };
    const cypher = `MATCH (a), (b) WHERE id(a) = $startId AND id(b) = $endId CREATE (a)-[r:${type}${clause}]->(b) RETURN r`;
    const params: Record<string, unknown> = {
      startId: startNodeId,
      endId: endNodeId,
      ...propParams,
    };
    const result = await this.executeCypher(cypher, params);
    if (result.rows.length === 0) throw new NexusSDKError('Failed to create relationship');
    return rawToRelationship(result.rows[0].r, startNodeId, endNodeId);
  }

  async getRelationship(id: number): Promise<Relationship | null> {
    const result = await this.executeCypher(
      'MATCH ()-[r]->() WHERE id(r) = $id RETURN r',
      { id }
    );
    return result.rows.length > 0 ? rawToRelationship(result.rows[0].r) : null;
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
    const databases = Array.isArray(obj.databases)
      ? obj.databases.map((db) =>
          typeof db === 'object' && db !== null
            ? String((db as { name?: unknown }).name ?? '')
            : String(db),
        )
      : [];
    const defaultDatabase =
      typeof obj.default_database === 'string'
        ? obj.default_database
        : typeof obj.defaultDatabase === 'string'
          ? obj.defaultDatabase
          : 'neo4j';
    return { databases, defaultDatabase };
  }

  async createDatabase(name: string): Promise<CreateDatabaseResponse> {
    const resp = await this.transport.execute({
      command: 'DB_CREATE',
      args: [nx.Str(name)],
    });
    const json = asSuccessMessage(nexusToJson(resp.value), name);
    if (!json.success) throw new NexusSDKError(json.message || `Failed to create '${name}'`);
    return json;
  }

  async getDatabase(name: string): Promise<DatabaseInfo> {
    // The single-database REST route returns a stub with empty
    // path/counts; the list carries the fully-populated records.
    const resp = await this.transport.execute({ command: 'DB_LIST', args: [] });
    const json = nexusToJson(resp.value) as { databases?: unknown };
    const raw = Array.isArray(json.databases)
      ? (json.databases as Array<Record<string, unknown>>).find((db) => db.name === name)
      : undefined;
    if (!raw) throw new NexusSDKError(`Database '${name}' not found`);
    return {
      name: String(raw.name ?? name),
      path: String(raw.path ?? ''),
      createdAt: Number(raw.created_at ?? 0),
      nodeCount: Number(raw.node_count ?? 0),
      relationshipCount: Number(raw.relationship_count ?? 0),
      storageSize: Number(raw.storage_size ?? 0),
    };
  }

  async dropDatabase(name: string): Promise<DropDatabaseResponse> {
    if (name === this.currentDatabase) {
      throw new NexusSDKError(
        `cannot drop database '${name}': it is the current session database (switch away first)`,
      );
    }
    const resp = await this.transport.execute({
      command: 'DB_DROP',
      args: [nx.Str(name)],
    });
    const json = asSuccessMessage(nexusToJson(resp.value), name);
    if (!json.success) throw new NexusSDKError(json.message || `Failed to drop '${name}'`);
    return { success: json.success, message: json.message };
  }

  /**
   * Current session database. The server is stateless, so this returns the
   * client-side value set by {@link switchDatabase} (default `neo4j`).
   */
  async getCurrentDatabase(): Promise<string> {
    return this.currentDatabase;
  }

  /**
   * Switch the client's session to a different database. Validates it
   * exists, then updates the client-side session so subsequent
   * {@link executeCypher} calls route to it (the server is stateless).
   */
  async switchDatabase(name: string): Promise<SwitchDatabaseResponse> {
    const { databases } = await this.listDatabases();
    if (!databases.includes(name)) {
      throw new NexusSDKError(`database '${name}' does not exist`);
    }
    this.currentDatabase = name;
    return { success: true, message: `Switched to database '${name}'` };
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
  // Surface a server-side execution/parse error instead of silently
  // returning an empty result set (which callers misread as "0 rows").
  if (typeof obj.error === 'string' && obj.error.length > 0) {
    throw new NexusSDKError(obj.error);
  }
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

/**
 * Build an inline property map with per-key parameter references
 * (`{k1: $prefix0, k2: $prefix1}`) plus the matching parameter values.
 * The server does not accept a whole-map parameter (`CREATE (n $props)`)
 * but does accept individual value parameters, so write helpers inline
 * the keys and parameterise only the values.
 */
function inlineProps(
  props: Record<string, unknown>,
  prefix: string,
): { clause: string; params: Record<string, unknown> } {
  const keys = Object.keys(props);
  if (keys.length === 0) return { clause: '', params: {} };
  const params: Record<string, unknown> = {};
  const pairs = keys.map((key, i) => {
    const p = `${prefix}${i}`;
    params[p] = props[key];
    return `${key}: $${p}`;
  });
  return { clause: ` {${pairs.join(', ')}}`, params };
}

/** Reserved keys the server folds into a node/relationship value. */
const NEXUS_ID_KEY = '_nexus_id';
const NEXUS_LABELS_KEY = '_nexus_labels';
const NEXUS_REL_TYPE_KEY = '_nexus_rel_type';

/**
 * Convert a raw server node value (`{_nexus_id, _nexus_labels, ...props}`)
 * into the SDK's `Node` shape (`{id, labels, properties}`).
 */
function rawToNode(raw: unknown): Node {
  const obj = (raw ?? {}) as Record<string, unknown>;
  const properties: NodeProperties = {};
  for (const [k, v] of Object.entries(obj)) {
    if (k === NEXUS_ID_KEY || k === NEXUS_LABELS_KEY) continue;
    properties[k] = v as NodeProperties[string];
  }
  return {
    id: Number(obj[NEXUS_ID_KEY] ?? 0),
    labels: Array.isArray(obj[NEXUS_LABELS_KEY])
      ? (obj[NEXUS_LABELS_KEY] as unknown[]).map(String)
      : [],
    properties,
  };
}

/**
 * Convert a raw server relationship value (`{_nexus_id, _nexus_rel_type,
 * type, ...props}`) into the SDK's `Relationship` shape. The endpoints are
 * not carried in the value, so callers that know them pass them in.
 */
function rawToRelationship(raw: unknown, startNodeId = 0, endNodeId = 0): Relationship {
  const obj = (raw ?? {}) as Record<string, unknown>;
  const properties: RelationshipProperties = {};
  for (const [k, v] of Object.entries(obj)) {
    if (k === NEXUS_ID_KEY || k === NEXUS_REL_TYPE_KEY || k === 'type') continue;
    properties[k] = v as RelationshipProperties[string];
  }
  return {
    id: Number(obj[NEXUS_ID_KEY] ?? 0),
    type: String(obj[NEXUS_REL_TYPE_KEY] ?? obj.type ?? ''),
    startNodeId,
    endNodeId,
    properties,
  };
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
