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
        query,
        params: params || {},
      });
      const executionTime = Date.now() - startTime;

      const result: QueryResult = {
        columns: response.data.columns || [],
        rows: response.data.rows || [],
        executionTime,
        rowCount: response.data.rows?.length || 0,
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
      return { success: true, data: response.data };
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
      const nodes: Map<string | number, any> = new Map();
      const relationships: any[] = [];

      for (const row of result.data.rows) {
        for (const value of Object.values(row)) {
          if (this.isNode(value)) {
            nodes.set(value._nexus_id || value.id, {
              id: value._nexus_id || value.id,
              labels: value._nexus_labels || value.labels || [],
              properties: this.extractProperties(value),
            });
          } else if (this.isRelationship(value)) {
            relationships.push({
              id: value._nexus_id || value.id,
              type: value.type || value._nexus_type,
              startNode: value._nexus_start || value.startNode,
              endNode: value._nexus_end || value.endNode,
              properties: this.extractProperties(value),
            });
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

  private isNode(value: any): boolean {
    return value && typeof value === 'object' &&
      (value._nexus_type === 'node' || value._nexus_labels || value.labels);
  }

  private isRelationship(value: any): boolean {
    return value && typeof value === 'object' &&
      (value._nexus_type === 'relationship' || (value.type && (value._nexus_start || value.startNode)));
  }

  private extractProperties(value: any): Record<string, any> {
    const props: Record<string, any> = {};
    for (const [key, val] of Object.entries(value)) {
      if (!key.startsWith('_nexus_') && key !== 'labels' && key !== 'type') {
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
}

export type { ServerConfig };
