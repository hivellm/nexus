import axios, { AxiosInstance, AxiosError } from 'axios';
import type {
  ServerConfig,
  ApiResponse,
  QueryResult,
  DatabaseStats,
  ServerHealth,
  LabelInfo,
  RelationshipTypeInfo,
  IndexInfo,
  GraphData,
  DatabaseInfo,
  ListDatabasesResponse,
  CreateDatabaseResponse,
  DropDatabaseResponse,
  SwitchDatabaseResponse,
} from '@/types';

export class NexusApiClient {
  private client: AxiosInstance;
  private config: ServerConfig;

  constructor(config: ServerConfig) {
    this.config = config;
    let baseURL: string;
    if (config.host) {
      const protocol = config.ssl ? 'https' : 'http';
      baseURL = `${protocol}://${config.host}:${config.port || 15474}`;
    } else {
      baseURL = config.port ? `${config.url}:${config.port}` : config.url || 'http://localhost:15474';
    }

    this.client = axios.create({
      baseURL,
      timeout: config.timeout || 30000,
      headers: {
        'Content-Type': 'application/json',
        ...(config.apiKey && { 'X-API-Key': config.apiKey }),
      },
    });
  }

  updateConfig(updates: Partial<ServerConfig>): void {
    this.config = { ...this.config, ...updates };
    let baseURL: string;
    if (this.config.host) {
      const protocol = this.config.ssl ? 'https' : 'http';
      baseURL = `${protocol}://${this.config.host}:${this.config.port || 15474}`;
    } else {
      baseURL = this.config.port ? `${this.config.url}:${this.config.port}` : this.config.url || 'http://localhost:15474';
    }
    this.client.defaults.baseURL = baseURL;
    if (updates.apiKey !== undefined) {
      this.client.defaults.headers['X-API-Key'] = updates.apiKey || '';
    }
    if (updates.timeout !== undefined) {
      this.client.defaults.timeout = updates.timeout;
    }
  }

  private handleError(error: AxiosError): ApiResponse<never> {
    if (error.response) {
      const data = error.response.data as any;
      return {
        success: false,
        error: data?.error || data?.message || `Server error: ${error.response.status}`,
      };
    } else if (error.request) {
      return {
        success: false,
        error: 'No response from server. Please check your connection.',
      };
    } else {
      return {
        success: false,
        error: error.message || 'Unknown error occurred',
      };
    }
  }

  // Health check
  async healthCheck(): Promise<ApiResponse<ServerHealth>> {
    try {
      const response = await this.client.get('/health');
      return { success: true, data: response.data };
    } catch (error) {
      return this.handleError(error as AxiosError);
    }
  }

  // Execute Cypher query
  async executeCypher(query: string, params?: Record<string, any>): Promise<ApiResponse<QueryResult>> {
    try {
      const startTime = Date.now();
      const response = await this.client.post('/cypher', {
        query: query,
        params: params || {},
      });
      const executionTime = Date.now() - startTime;

      // Handle different response formats
      const data = response.data;
      let columns: string[] = [];
      let rows: any[] = [];

      if (data.columns && data.rows) {
        // Nexus format: { columns: ["n"], rows: [[{...}], [{...}]] }
        // Each row is an array of column values
        columns = data.columns;

        // Transform rows from [[val1], [val2]] to [{col1: val1}, {col2: val2}]
        rows = data.rows.map((row: any[]) => {
          const rowObj: Record<string, any> = {};
          columns.forEach((col, idx) => {
            rowObj[col] = row[idx];
          });
          return rowObj;
        });
      } else if (Array.isArray(data)) {
        // Array format: [{ ... }, { ... }]
        rows = data;
        if (rows.length > 0) {
          columns = Object.keys(rows[0]);
        }
      } else if (data.result) {
        // Wrapped format: { result: [...] }
        rows = Array.isArray(data.result) ? data.result : [data.result];
        if (rows.length > 0) {
          columns = Object.keys(rows[0]);
        }
      }

      const result: QueryResult = {
        columns,
        rows,
        executionTime,
        rowCount: rows.length,
      };

      return { success: true, data: result };
    } catch (error) {
      return this.handleError(error as AxiosError);
    }
  }

  // Get database statistics
  async getStats(): Promise<ApiResponse<DatabaseStats>> {
    try {
      const response = await this.client.get('/stats');
      const data = response.data;

      // Map API response to DatabaseStats interface
      // API returns: { catalog: { label_count, rel_type_count, node_count, rel_count }, label_index: {...}, knn_index: {...} }
      const stats: DatabaseStats = {
        nodeCount: data.catalog?.node_count ?? 0,
        relationshipCount: data.catalog?.rel_count ?? 0,
        labelCount: data.catalog?.label_count ?? 0,
        relationshipTypeCount: data.catalog?.rel_type_count ?? 0,
        propertyKeyCount: 0,
        indexCount: data.label_index?.indexed_labels ?? 0,
        storageSize: 0,
        uptime: 0,
      };

      return { success: true, data: stats };
    } catch (error) {
      return this.handleError(error as AxiosError);
    }
  }

  // Schema operations
  async getLabels(): Promise<ApiResponse<LabelInfo[]>> {
    try {
      const response = await this.client.get('/schema/labels');
      return { success: true, data: response.data };
    } catch (error) {
      return this.handleError(error as AxiosError);
    }
  }

  async getRelationshipTypes(): Promise<ApiResponse<RelationshipTypeInfo[]>> {
    try {
      const response = await this.client.get('/schema/rel_types');
      return { success: true, data: response.data };
    } catch (error) {
      return this.handleError(error as AxiosError);
    }
  }

  async getPropertyKeys(): Promise<ApiResponse<string[]>> {
    try {
      const response = await this.client.get('/property_keys');
      return { success: true, data: response.data };
    } catch (error) {
      return this.handleError(error as AxiosError);
    }
  }

  // Index operations
  async getIndexes(): Promise<ApiResponse<IndexInfo[]>> {
    try {
      const response = await this.client.get('/schema/indexes');
      return { success: true, data: response.data };
    } catch (error) {
      return this.handleError(error as AxiosError);
    }
  }

  async createIndex(label: string, properties: string[]): Promise<ApiResponse<void>> {
    try {
      await this.client.post('/schema/indexes', { label, properties });
      return { success: true };
    } catch (error) {
      return this.handleError(error as AxiosError);
    }
  }

  async dropIndex(name: string): Promise<ApiResponse<void>> {
    try {
      await this.client.delete(`/schema/indexes/${name}`);
      return { success: true };
    } catch (error) {
      return this.handleError(error as AxiosError);
    }
  }

  // Graph visualization data
  async getGraphData(query: string, limit: number = 100): Promise<ApiResponse<GraphData>> {
    try {
      const result = await this.executeCypher(query);
      if (!result.success || !result.data) {
        return { success: false, error: result.error };
      }

      // Transform query result to graph data
      // Nexus returns: rows like [{n: {...}, r: {...}, m: {...}}]
      // where n/m are nodes and r is relationship
      const nodes: Map<string | number, any> = new Map();
      const relationships: any[] = [];
      const columns = result.data.columns;

      for (const row of result.data.rows) {
        // Process each column value
        for (let i = 0; i < columns.length; i++) {
          const colName = columns[i];
          const value = row[colName];

          if (!value || typeof value !== 'object') continue;

          // Determine if it's a relationship:
          // - Column name is 'r' or contains 'rel'
          // - AND there are adjacent columns (pattern: n, r, m)
          // - AND has 'type' property (relationships always have type)
          const hasAdjacentColumns = columns.length >= 3 && i > 0 && i < columns.length - 1;
          const isRelColumn = colName.toLowerCase() === 'r' || colName.toLowerCase().includes('rel');
          const hasTypeNoCommonNodeProps = value.type && !value.name && !value.title && !value.age && !value.city;

          const isRel = hasAdjacentColumns && (isRelColumn || hasTypeNoCommonNodeProps);

          if (isRel) {
            // It's a relationship - we need to find connected nodes
            // Look for nodes in adjacent columns
            const prevCol = columns[i - 1];
            const nextCol = columns[i + 1];
            const startNode = prevCol ? row[prevCol]?._nexus_id : null;
            const endNode = nextCol ? row[nextCol]?._nexus_id : null;

            if (startNode && endNode) {
              relationships.push({
                id: value._nexus_id || `rel-${relationships.length}`,
                type: value.type || 'RELATED',
                startNode: startNode,
                endNode: endNode,
                properties: this.extractProperties(value),
              });
            }
          } else {
            // It's a node
            const nodeId = value._nexus_id || value.id;
            if (nodeId && !nodes.has(nodeId)) {
              nodes.set(nodeId, {
                id: nodeId,
                labels: value._nexus_labels || value.labels || [colName.toUpperCase()],
                properties: this.extractProperties(value),
              });
            }
          }
        }
      }

      return {
        success: true,
        data: {
          nodes: Array.from(nodes.values()).slice(0, limit),
          relationships: relationships.slice(0, limit),
        },
      };
    } catch (error) {
      return this.handleError(error as AxiosError);
    }
  }

  private extractProperties(value: any): Record<string, any> {
    const props: Record<string, any> = {};
    for (const [key, val] of Object.entries(value)) {
      if (!key.startsWith('_nexus_') && key !== 'labels' && key !== 'type' && key !== 'id') {
        props[key] = val;
      }
    }
    return props;
  }

  // KNN search
  async knnSearch(
    embedding: number[],
    k: number,
    label?: string
  ): Promise<ApiResponse<QueryResult>> {
    try {
      const response = await this.client.post('/knn', {
        embedding,
        k,
        label,
      });
      return { success: true, data: response.data };
    } catch (error) {
      return this.handleError(error as AxiosError);
    }
  }

  // Data import/export
  async importData(data: { nodes: any[]; relationships: any[] }): Promise<ApiResponse<{ imported: number }>> {
    try {
      const response = await this.client.post('/data/import', data);
      return { success: true, data: response.data };
    } catch (error) {
      return this.handleError(error as AxiosError);
    }
  }

  async exportData(format: 'json' | 'cypher' = 'json'): Promise<ApiResponse<string>> {
    try {
      const response = await this.client.get(`/data/export?format=${format}`);
      return { success: true, data: response.data };
    } catch (error) {
      return this.handleError(error as AxiosError);
    }
  }

  // Get server logs
  async getLogs(): Promise<ApiResponse<any[]>> {
    try {
      const response = await this.client.get('/logs');
      return { success: true, data: response.data };
    } catch (error) {
      return this.handleError(error as AxiosError);
    }
  }

  // Get query history
  async getQueryHistory(): Promise<ApiResponse<any[]>> {
    try {
      const response = await this.client.get('/query-history');
      return { success: true, data: response.data };
    } catch (error) {
      return this.handleError(error as AxiosError);
    }
  }

  // Get server config
  async getConfig(): Promise<ApiResponse<any>> {
    try {
      const response = await this.client.get('/config');
      return { success: true, data: response.data };
    } catch (error) {
      return this.handleError(error as AxiosError);
    }
  }

  // =========================================================================
  // Database Management Methods
  // =========================================================================

  // List all databases
  async listDatabases(): Promise<ApiResponse<ListDatabasesResponse>> {
    try {
      const response = await this.client.get('/databases');
      return { success: true, data: response.data };
    } catch (error) {
      return this.handleError(error as AxiosError);
    }
  }

  // Create a new database
  async createDatabase(name: string): Promise<ApiResponse<CreateDatabaseResponse>> {
    try {
      const response = await this.client.post('/databases', { name });
      return { success: true, data: response.data };
    } catch (error) {
      return this.handleError(error as AxiosError);
    }
  }

  // Get database information
  async getDatabase(name: string): Promise<ApiResponse<DatabaseInfo>> {
    try {
      const response = await this.client.get(`/databases/${name}`);
      return { success: true, data: response.data };
    } catch (error) {
      return this.handleError(error as AxiosError);
    }
  }

  // Drop a database
  async dropDatabase(name: string): Promise<ApiResponse<DropDatabaseResponse>> {
    try {
      const response = await this.client.delete(`/databases/${name}`);
      return { success: true, data: response.data };
    } catch (error) {
      return this.handleError(error as AxiosError);
    }
  }

  // Get current session database
  async getCurrentDatabase(): Promise<ApiResponse<string>> {
    try {
      const response = await this.client.get('/session/database');
      return { success: true, data: response.data.database };
    } catch (error) {
      return this.handleError(error as AxiosError);
    }
  }

  // Switch to a different database
  async switchDatabase(name: string): Promise<ApiResponse<SwitchDatabaseResponse>> {
    try {
      const response = await this.client.put('/session/database', { name });
      return { success: true, data: response.data };
    } catch (error) {
      return this.handleError(error as AxiosError);
    }
  }
}

export type { ServerConfig };
