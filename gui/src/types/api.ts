/**
 * Typed shapes for the Nexus REST surface the GUI consumes. Mirrors
 * the Rust API responses (`crates/nexus-server/src/api/*`) byte for
 * byte where the GUI cares; transient fields the GUI does not read
 * are omitted to keep the type surface small and maintainable.
 */

/** GET /health */
export interface HealthResponse {
  status: 'Healthy' | 'Degraded' | 'Unhealthy';
  version: string;
  uptime_seconds: number;
  timestamp: string;
  components: {
    database: ComponentStatus;
    storage: ComponentStatus;
    indexes: ComponentStatus;
    wal: ComponentStatus;
    page_cache: ComponentStatus;
  };
}

export interface ComponentStatus {
  status: 'Healthy' | 'Degraded' | 'Unhealthy';
  response_time_ms?: number;
  error?: string;
}

/** GET /stats */
export interface StatsResponse {
  catalog: {
    label_count: number;
    rel_type_count: number;
    node_count: number;
    rel_count: number;
  };
  label_index?: {
    indexed_labels: number;
  };
  knn_index?: {
    indexed_labels: number;
  };
  qps?: number;
  page_cache_hit_rate?: number;
  wal_size_bytes?: number;
  p99_latency_ms?: number;
}

/** POST /cypher */
export interface CypherRequest {
  query: string;
  params?: Record<string, unknown>;
}

export interface CypherResponse {
  columns: string[];
  /**
   * Row encoding matches the server's array-of-arrays format
   * (Neo4j compatible). Each entry in the outer array is a row;
   * each row is an array of column values aligned to `columns`.
   */
  rows: unknown[][];
  execution_time_ms: number;
  stats?: {
    nodes_created?: number;
    relationships_created?: number;
    properties_set?: number;
  };
  error?: string;
}

/** GET /schema/labels */
export interface LabelInfo {
  name: string;
  id: number;
}

export interface LabelsResponse {
  labels: LabelInfo[];
  error?: string;
}

/** GET /schema/rel_types */
export interface RelTypeInfo {
  name: string;
  id: number;
}

export interface RelTypesResponse {
  types: RelTypeInfo[];
  error?: string;
}

/** GET /schema/indexes */
export interface IndexInfo {
  name: string;
  label: string;
  properties: string[];
  type: 'btree' | 'fulltext' | 'vector';
  state: 'online' | 'populating' | 'failed';
}

export interface IndexesResponse {
  indexes: IndexInfo[];
  error?: string;
}

/** GET /procedures */
export interface ProcedureInfo {
  name: string;
  signature: string;
  description?: string;
}

export interface ProceduresResponse {
  procedures: ProcedureInfo[];
}

/** POST /knn_traverse */
export interface KnnRequest {
  label: string;
  vector: number[];
  k: number;
  ef_search?: number;
  distance?: 'cosine' | 'euclidean' | 'dot';
}

export interface KnnHit {
  node_id: number;
  score: number;
  properties?: Record<string, unknown>;
}

export interface KnnResponse {
  hits: KnnHit[];
  execution_time_ms: number;
  error?: string;
}

/** GET /replication/status */
export interface ReplicaInfo {
  host: string;
  state: 'connected' | 'lagging' | 'disconnected';
  ack_ms: number;
  lag_ms: number;
  epoch: number;
}

export interface ReplicationStatusResponse {
  master: {
    host: string;
    epoch: number;
  };
  replicas: ReplicaInfo[];
  max_lag_ms: number;
}

/** GET /audit/log */
export type AuditLevel = 'info' | 'warn' | 'error';

export interface AuditEntry {
  timestamp: string;
  level: AuditLevel;
  user: string;
  action: string;
  detail?: string;
}

export interface AuditLogResponse {
  entries: AuditEntry[];
  next_cursor?: string;
}

/** Generic error payload returned on non-2xx. */
export interface ErrorResponse {
  error: string;
  code?: string;
}
