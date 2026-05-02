/**
 * Authentication configuration for Nexus client
 */
export interface AuthConfig {
  /** API key for authentication */
  apiKey?: string;
  /** Username for basic authentication */
  username?: string;
  /** Password for basic authentication */
  password?: string;
}

/**
 * Transport selector. Values match the URL-scheme tokens and the
 * `NEXUS_SDK_TRANSPORT` env-var strings.
 */
export type TransportMode = 'nexus' | 'resp3' | 'http' | 'https';

/**
 * Connection configuration for Nexus client.
 *
 * Transport precedence (see `docs/specs/sdk-transport.md`):
 *   URL scheme in `baseUrl` > `NEXUS_SDK_TRANSPORT` env > `transport` field > default (`nexus`).
 */
export interface NexusConfig {
  /**
   * Endpoint URL. Accepts `nexus://` (binary RPC, default),
   * `http://` / `https://`, `resp3://`, or the bare `host[:port]` form
   * (treated as RPC).
   *
   * Defaults to `nexus://127.0.0.1:15475` when omitted.
   */
  baseUrl?: string;
  /** Authentication configuration. */
  auth?: AuthConfig;
  /** Explicit transport hint. The URL scheme wins if set. */
  transport?: TransportMode;
  /** RPC port override when `transport === 'nexus'` (default 15475). */
  rpcPort?: number;
  /** RESP3 port override (default 15476, reserved for future use). */
  resp3Port?: number;
  /** Request timeout in milliseconds for the HTTP transport (default: 30000). */
  timeout?: number;
  /** Number of retry attempts for failed HTTP requests (default: 3). */
  retries?: number;
  /** Enable debug logging (default: false). */
  debug?: boolean;
}

/**
 * Cypher query parameters
 */
export type QueryParams = Record<string, unknown>;

/**
 * Query result row
 */
export interface ResultRow {
  [key: string]: unknown;
}

/**
 * Query result set
 */
export interface QueryResult {
  /** Column names */
  columns: string[];
  /** Result rows */
  rows: ResultRow[];
}

/**
 * Node properties
 */
export type NodeProperties = Record<string, unknown>;

/**
 * Relationship properties
 */
export type RelationshipProperties = Record<string, unknown>;

/**
 * Node representation
 */
export interface Node {
  /** Node ID */
  id: number;
  /** Node labels */
  labels: string[];
  /** Node properties */
  properties: NodeProperties;
}

/**
 * Relationship representation
 */
export interface Relationship {
  /** Relationship ID */
  id: number;
  /** Relationship type */
  type: string;
  /** Source node ID */
  startNodeId: number;
  /** Target node ID */
  endNodeId: number;
  /** Relationship properties */
  properties: RelationshipProperties;
}

/**
 * Query statistics
 */
export interface QueryStatistics {
  /** Number of nodes created */
  nodesCreated: number;
  /** Number of nodes deleted */
  nodesDeleted: number;
  /** Number of relationships created */
  relationshipsCreated: number;
  /** Number of relationships deleted */
  relationshipsDeleted: number;
  /** Number of properties set */
  propertiesSet: number;
  /** Number of labels added */
  labelsAdded: number;
  /** Number of labels removed */
  labelsRemoved: number;
  /** Query execution time in milliseconds */
  executionTime: number;
}

/**
 * Schema information
 */
export interface SchemaInfo {
  /** Available labels */
  labels: string[];
  /** Available relationship types */
  relationshipTypes: string[];
  /** Available indexes */
  indexes: IndexInfo[];
}

/**
 * Index information
 */
export interface IndexInfo {
  /** Index name */
  name: string;
  /** Label */
  label: string;
  /** Properties */
  properties: string[];
  /** Index type */
  type: string;
}

/**
 * Transaction options
 */
export interface TransactionOptions {
  /** Transaction timeout in milliseconds */
  timeout?: number;
}

/**
 * Batch operation
 */
export interface BatchOperation {
  /** Cypher query */
  cypher: string;
  /** Query parameters */
  params?: QueryParams;
}

/**
 * Error response from Nexus server
 */
export interface NexusError {
  /** Error message */
  message: string;
  /** Error code */
  code?: string;
  /** Additional error details */
  details?: unknown;
}

// ============================================================================
// Database Management Types
// ============================================================================

/**
 * Database information
 */
export interface DatabaseInfo {
  /** Database name */
  name: string;
  /** Database path */
  path: string;
  /** Creation timestamp */
  createdAt: number;
  /** Number of nodes */
  nodeCount: number;
  /** Number of relationships */
  relationshipCount: number;
  /** Storage size in bytes */
  storageSize: number;
}

/**
 * Response for listing databases
 */
export interface ListDatabasesResponse {
  /** List of databases */
  databases: DatabaseInfo[];
  /** Default database name */
  defaultDatabase: string;
}

/**
 * Response for creating a database
 */
export interface CreateDatabaseResponse {
  /** Success flag */
  success: boolean;
  /** Database name */
  name: string;
  /** Message */
  message: string;
}

/**
 * Response for dropping a database
 */
export interface DropDatabaseResponse {
  /** Success flag */
  success: boolean;
  /** Message */
  message: string;
}

/**
 * Response for session database operations
 */
export interface SessionDatabaseResponse {
  /** Current database name */
  database: string;
}

/**
 * Response for switching database
 */
export interface SwitchDatabaseResponse {
  /** Success flag */
  success: boolean;
  /** Message */
  message: string;
}

// ============================================================================
// External-id Node Types (Phase9 §5.5)
// ============================================================================

/**
 * Request body for creating a node with an optional external id.
 *
 * `externalId` accepts the prefixed string form:
 *   `sha256:<hex>`, `blake3:<hex>`, `sha512:<hex>`, `uuid:<canonical>`,
 *   `str:<utf8>`, `bytes:<hex>`.
 *
 * `conflictPolicy` controls what happens when a node with the same
 * external id already exists: `"error"` (default), `"match"`, `"replace"`.
 */
export interface CreateNodeWithExternalIdRequest {
  /** Node labels */
  labels: string[];
  /** Node properties */
  properties: NodeProperties;
  /** Caller-supplied external id (required for this variant). */
  external_id: string;
  /** Conflict policy — omit to accept server default (`"error"`). */
  conflict_policy?: string;
}

/**
 * Response from `POST /data/nodes` (with or without external id).
 */
export interface CreateNodeResponse {
  /** Assigned internal node id */
  node_id: number;
  /** Human-readable result message */
  message: string;
  /** Server error string — present only on failure */
  error?: string;
}

/**
 * Response from `GET /data/nodes/by-external-id`.
 * `node` is `null` / absent when the external id is not registered.
 */
export interface GetNodeByExternalIdResponse {
  /** Resolved node, or null when not found */
  node: Node | null;
  /** Human-readable result message */
  message: string;
  /** Server error string — present only on failure */
  error?: string;
}

