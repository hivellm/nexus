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
 * Connection configuration for Nexus client
 */
export interface NexusConfig {
  /** Base URL of the Nexus server */
  baseUrl: string;
  /** Authentication configuration */
  auth: AuthConfig;
  /** Request timeout in milliseconds (default: 30000) */
  timeout?: number;
  /** Number of retry attempts for failed requests (default: 3) */
  retries?: number;
  /** Enable debug logging (default: false) */
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

