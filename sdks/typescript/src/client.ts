import axios, {  type AxiosInstance } from 'axios';
import axiosRetry from 'axios-retry';
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
} from './types';
import {
  NexusSDKError,
  AuthenticationError,
  ConnectionError,
  ValidationError,
} from './errors';

/**
 * Nexus Graph Database Client
 * 
 * @example
 * ```typescript
 * const client = new NexusClient({
 *   baseUrl: 'http://localhost:7687',
 *   auth: { apiKey: 'your-api-key' }
 * });
 * 
 * const result = await client.executeCypher('MATCH (n) RETURN n LIMIT 10');
 * console.log(result.rows);
 * ```
 */
export class NexusClient {
  private readonly client: AxiosInstance;
  private readonly config: NexusConfig;
  private readonly debug: boolean;

  constructor(config: NexusConfig) {
    this.config = config;
    this.debug = config.debug ?? false;

    // Validate configuration
    this.validateConfig();

    // Create axios instance
    this.client = axios.create({
      baseURL: config.baseUrl,
      timeout: config.timeout ?? 30000,
      headers: {
        'Content-Type': 'application/json',
      },
    });

    // Setup authentication
    this.setupAuth();

    // Setup retry logic
    axiosRetry(this.client, {
      retries: config.retries ?? 3,
      retryDelay: axiosRetry.exponentialDelay,
      retryCondition: (error) => {
        return axiosRetry.isNetworkOrIdempotentRequestError(error) ||
          (error.response?.status !== undefined && error.response.status >= 500);
      },
      onRetry: (retryCount, error) => {
        if (this.debug) {
          console.log(`Retry attempt ${retryCount} for ${error.config?.url}`);
        }
      },
    });

    // Setup response interceptor
    this.client.interceptors.response.use(
      (response) => response,
      (error) => {
        if (this.debug) {
          console.error('Request failed:', error);
        }
        throw NexusSDKError.fromAxiosError(error);
      }
    );
  }

  /**
   * Validate configuration
   */
  private validateConfig(): void {
    if (!this.config.baseUrl) {
      throw new ValidationError('baseUrl is required');
    }

    if (!this.config.auth) {
      throw new ValidationError('auth configuration is required');
    }

    const hasApiKey = !!this.config.auth.apiKey;
    const hasCredentials = !!this.config.auth.username && !!this.config.auth.password;

    if (!hasApiKey && !hasCredentials) {
      throw new ValidationError('Either apiKey or username/password must be provided');
    }
  }

  /**
   * Setup authentication headers
   */
  private setupAuth(): void {
    if (this.config.auth.apiKey) {
      this.client.defaults.headers.common['X-API-Key'] = this.config.auth.apiKey;
    } else if (this.config.auth.username && this.config.auth.password) {
      const credentials = Buffer.from(
        `${this.config.auth.username}:${this.config.auth.password}`
      ).toString('base64');
      this.client.defaults.headers.common['Authorization'] = `Basic ${credentials}`;
    }
  }

  /**
   * Execute a Cypher query
   * 
   * @param cypher - The Cypher query string
   * @param params - Query parameters
   * @returns Query result
   * 
   * @example
   * ```typescript
   * const result = await client.executeCypher(
   *   'MATCH (n:Person) WHERE n.age > $age RETURN n',
   *   { age: 25 }
   * );
   * ```
   */
  async executeCypher(cypher: string, params?: QueryParams): Promise<QueryResult> {
    try {
      const response = await this.client.post<QueryResult>('/query', {
        cypher,
        params: params ?? {},
      });
      return response.data;
    } catch (error) {
      throw NexusSDKError.fromAxiosError(error);
    }
  }

  /**
   * Create a node
   * 
   * @param labels - Node labels
   * @param properties - Node properties
   * @returns Created node
   * 
   * @example
   * ```typescript
   * const node = await client.createNode(
   *   ['Person'],
   *   { name: 'Alice', age: 30 }
   * );
   * ```
   */
  async createNode(labels: string[], properties: NodeProperties): Promise<Node> {
    const labelsStr = labels.map((l) => `:${l}`).join('');
    const cypher = `CREATE (n${labelsStr} $props) RETURN n`;

    const result = await this.executeCypher(cypher, { props: properties });

    if (result.rows.length === 0) {
      throw new NexusSDKError('Failed to create node');
    }

    return result.rows[0].n as Node;
  }

  /**
   * Get node by ID
   * 
   * @param id - Node ID
   * @returns Node or null if not found
   */
  async getNode(id: number): Promise<Node | null> {
    const result = await this.executeCypher(
      'MATCH (n) WHERE id(n) = $id RETURN n',
      { id }
    );

    return result.rows.length > 0 ? (result.rows[0].n as Node) : null;
  }

  /**
   * Update node properties
   * 
   * @param id - Node ID
   * @param properties - Properties to update
   * @returns Updated node
   */
  async updateNode(id: number, properties: NodeProperties): Promise<Node> {
    const cypher = 'MATCH (n) WHERE id(n) = $id SET n += $props RETURN n';
    const result = await this.executeCypher(cypher, { id, props: properties });

    if (result.rows.length === 0) {
      throw new NexusSDKError('Node not found');
    }

    return result.rows[0].n as Node;
  }

  /**
   * Delete a node
   * 
   * @param id - Node ID
   * @param detach - If true, also deletes relationships (default: false)
   */
  async deleteNode(id: number, detach: boolean = false): Promise<void> {
    const cypher = detach
      ? 'MATCH (n) WHERE id(n) = $id DETACH DELETE n'
      : 'MATCH (n) WHERE id(n) = $id DELETE n';

    await this.executeCypher(cypher, { id });
  }

  /**
   * Find nodes by label and properties
   * 
   * @param label - Node label
   * @param properties - Properties to match
   * @param limit - Maximum number of nodes to return
   * @returns Array of nodes
   */
  async findNodes(
    label: string,
    properties?: NodeProperties,
    limit?: number
  ): Promise<Node[]> {
    let cypher = `MATCH (n:${label})`;

    if (properties && Object.keys(properties).length > 0) {
      cypher += ' WHERE ' + Object.keys(properties)
        .map((key) => `n.${key} = $props.${key}`)
        .join(' AND ');
    }

    cypher += ' RETURN n';

    if (limit) {
      cypher += ` LIMIT ${limit}`;
    }

    const result = await this.executeCypher(cypher, properties ? { props: properties } : undefined);
    return result.rows.map((row) => row.n as Node);
  }

  /**
   * Create a relationship between two nodes
   * 
   * @param startNodeId - Start node ID
   * @param endNodeId - End node ID
   * @param type - Relationship type
   * @param properties - Relationship properties
   * @returns Created relationship
   */
  async createRelationship(
    startNodeId: number,
    endNodeId: number,
    type: string,
    properties?: RelationshipProperties
  ): Promise<Relationship> {
    const cypher = properties
      ? `MATCH (a), (b) WHERE id(a) = $startId AND id(b) = $endId CREATE (a)-[r:${type} $props]->(b) RETURN r`
      : `MATCH (a), (b) WHERE id(a) = $startId AND id(b) = $endId CREATE (a)-[r:${type}]->(b) RETURN r`;

    const params: Record<string, unknown> = {
      startId: startNodeId,
      endId: endNodeId,
    };

    if (properties) {
      params.props = properties;
    }

    const result = await this.executeCypher(cypher, params);

    if (result.rows.length === 0) {
      throw new NexusSDKError('Failed to create relationship');
    }

    return result.rows[0].r as Relationship;
  }

  /**
   * Get relationship by ID
   * 
   * @param id - Relationship ID
   * @returns Relationship or null if not found
   */
  async getRelationship(id: number): Promise<Relationship | null> {
    const result = await this.executeCypher(
      'MATCH ()-[r]->() WHERE id(r) = $id RETURN r',
      { id }
    );

    return result.rows.length > 0 ? (result.rows[0].r as Relationship) : null;
  }

  /**
   * Delete a relationship
   * 
   * @param id - Relationship ID
   */
  async deleteRelationship(id: number): Promise<void> {
    await this.executeCypher('MATCH ()-[r]->() WHERE id(r) = $id DELETE r', { id });
  }

  /**
   * Get all labels
   * 
   * @returns Array of label names
   */
  async getLabels(): Promise<string[]> {
    try {
      const response = await this.client.get<{ labels: string[] }>('/schema/labels');
      return response.data.labels;
    } catch (error) {
      throw NexusSDKError.fromAxiosError(error);
    }
  }

  /**
   * Get all relationship types
   * 
   * @returns Array of relationship type names
   */
  async getRelationshipTypes(): Promise<string[]> {
    try {
      const response = await this.client.get<{ types: string[] }>('/schema/relationship-types');
      return response.data.types;
    } catch (error) {
      throw NexusSDKError.fromAxiosError(error);
    }
  }

  /**
   * Get schema information
   * 
   * @returns Schema information
   */
  async getSchema(): Promise<SchemaInfo> {
    try {
      const [labels, relationshipTypes] = await Promise.all([
        this.getLabels(),
        this.getRelationshipTypes(),
      ]);

      return {
        labels,
        relationshipTypes,
        indexes: [], // TODO: Implement index retrieval
      };
    } catch (error) {
      throw NexusSDKError.fromAxiosError(error);
    }
  }

  /**
   * Execute multiple queries in batch
   * 
   * @param operations - Array of batch operations
   * @returns Array of query results
   */
  async executeBatch(operations: BatchOperation[]): Promise<QueryResult[]> {
    const results = await Promise.all(
      operations.map((op) => this.executeCypher(op.cypher, op.params))
    );
    return results;
  }

  /**
   * Test connection to Nexus server
   * 
   * @returns true if connection is successful
   * @throws {ConnectionError} if connection fails
   */
  async testConnection(): Promise<boolean> {
    try {
      await this.executeCypher('RETURN 1');
      return true;
    } catch (error) {
      throw new ConnectionError('Failed to connect to Nexus server');
    }
  }

  /**
   * Get query statistics
   * 
   * @returns Query statistics
   */
  async getStatistics(): Promise<QueryStatistics> {
    try {
      const response = await this.client.get<QueryStatistics>('/admin/statistics');
      return response.data;
    } catch (error) {
      throw NexusSDKError.fromAxiosError(error);
    }
  }

  /**
   * Clear plan cache
   */
  async clearPlanCache(): Promise<void> {
    try {
      await this.client.post('/admin/plan-cache/clear');
    } catch (error) {
      throw NexusSDKError.fromAxiosError(error);
    }
  }
}

