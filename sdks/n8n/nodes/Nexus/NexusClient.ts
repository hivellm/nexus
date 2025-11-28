import type { IExecuteFunctions, IHttpRequestMethods, IDataObject, JsonObject } from 'n8n-workflow';
import { NodeApiError, NodeOperationError } from 'n8n-workflow';

export interface NexusCredentials {
  host: string;
  port: number;
  useTls: boolean;
  apiKey?: string;
  username?: string;
  password?: string;
}

export interface QueryResult {
  columns: string[];
  rows: IDataObject[];
  execution_time_ms?: number;
}

export interface NexusNode {
  id: number;
  labels: string[];
  properties: IDataObject;
}

export interface NexusRelationship {
  id: number;
  type: string;
  start_node_id: number;
  end_node_id: number;
  properties: IDataObject;
}

export interface SchemaInfo {
  labels: string[];
  relationship_types: string[];
}

export class NexusClient {
  private readonly executeFunctions: IExecuteFunctions;
  private readonly baseUrl: string;
  private readonly credentialType: string;

  constructor(
    executeFunctions: IExecuteFunctions,
    credentials: NexusCredentials,
    credentialType: string,
  ) {
    this.executeFunctions = executeFunctions;
    this.credentialType = credentialType;
    const protocol = credentials.useTls ? 'https' : 'http';
    this.baseUrl = `${protocol}://${credentials.host}:${credentials.port}`;
  }

  private async request<T>(
    method: IHttpRequestMethods,
    endpoint: string,
    body?: IDataObject,
  ): Promise<T> {
    try {
      const options = {
        method,
        url: `${this.baseUrl}${endpoint}`,
        body,
        json: true,
      };

      const response = await this.executeFunctions.helpers.requestWithAuthentication.call(
        this.executeFunctions,
        this.credentialType,
        options,
      );

      return response as T;
    } catch (error) {
      if (error instanceof NodeApiError || error instanceof NodeOperationError) {
        throw error;
      }
      throw new NodeApiError(this.executeFunctions.getNode(), { message: (error as Error).message } as JsonObject, {
        message: `Nexus API request failed: ${(error as Error).message}`,
      });
    }
  }

  async executeCypher(cypher: string, params: IDataObject = {}): Promise<QueryResult> {
    return this.request<QueryResult>('POST', '/query', { cypher, params });
  }

  async createNode(labels: string[], properties: IDataObject): Promise<NexusNode> {
    const labelsStr = labels.map((l) => `:${l}`).join('');
    const cypher = `CREATE (n${labelsStr} $props) RETURN n`;
    const result = await this.executeCypher(cypher, { props: properties });

    if (result.rows.length === 0) {
      throw new NodeOperationError(this.executeFunctions.getNode(), 'Failed to create node');
    }

    return result.rows[0].n as unknown as NexusNode;
  }

  async getNode(id: number): Promise<NexusNode | null> {
    const result = await this.executeCypher('MATCH (n) WHERE id(n) = $id RETURN n', { id });
    return result.rows.length > 0 ? (result.rows[0].n as unknown as NexusNode) : null;
  }

  async updateNode(id: number, properties: IDataObject): Promise<NexusNode> {
    const cypher = 'MATCH (n) WHERE id(n) = $id SET n += $props RETURN n';
    const result = await this.executeCypher(cypher, { id, props: properties });

    if (result.rows.length === 0) {
      throw new NodeOperationError(this.executeFunctions.getNode(), 'Node not found');
    }

    return result.rows[0].n as unknown as NexusNode;
  }

  async deleteNode(id: number, detach: boolean = false): Promise<{ deleted: boolean; nodeId: number }> {
    const cypher = detach
      ? 'MATCH (n) WHERE id(n) = $id DETACH DELETE n RETURN count(*) AS deleted'
      : 'MATCH (n) WHERE id(n) = $id DELETE n RETURN count(*) AS deleted';

    const result = await this.executeCypher(cypher, { id });
    const deleted = result.rows.length > 0 && (result.rows[0].deleted as number) > 0;
    return { deleted, nodeId: id };
  }

  async findNodes(
    label: string,
    properties?: IDataObject,
    limit?: number,
  ): Promise<NexusNode[]> {
    let cypher = `MATCH (n:${label})`;

    if (properties && Object.keys(properties).length > 0) {
      const conditions = Object.keys(properties)
        .map((key) => `n.${key} = $props.${key}`)
        .join(' AND ');
      cypher += ` WHERE ${conditions}`;
    }

    cypher += ' RETURN n';

    if (limit) {
      cypher += ` LIMIT ${limit}`;
    }

    const result = await this.executeCypher(cypher, properties ? { props: properties } : {});
    return result.rows.map((row) => row.n as unknown as NexusNode);
  }

  async createRelationship(
    startNodeId: number,
    endNodeId: number,
    type: string,
    properties?: IDataObject,
  ): Promise<NexusRelationship> {
    const cypher = properties
      ? `MATCH (a), (b) WHERE id(a) = $startId AND id(b) = $endId CREATE (a)-[r:${type} $props]->(b) RETURN r, id(a) AS startId, id(b) AS endId`
      : `MATCH (a), (b) WHERE id(a) = $startId AND id(b) = $endId CREATE (a)-[r:${type}]->(b) RETURN r, id(a) AS startId, id(b) AS endId`;

    const params: IDataObject = {
      startId: startNodeId,
      endId: endNodeId,
    };

    if (properties) {
      params.props = properties;
    }

    const result = await this.executeCypher(cypher, params);

    if (result.rows.length === 0) {
      throw new NodeOperationError(
        this.executeFunctions.getNode(),
        'Failed to create relationship - nodes not found',
      );
    }

    const row = result.rows[0];
    const rel = row.r as unknown as IDataObject;
    return {
      id: rel.id as number,
      type,
      start_node_id: startNodeId,
      end_node_id: endNodeId,
      properties: (rel.properties as IDataObject) || {},
    };
  }

  async getRelationship(id: number): Promise<NexusRelationship | null> {
    const result = await this.executeCypher(
      'MATCH (a)-[r]->(b) WHERE id(r) = $id RETURN r, type(r) AS relType, id(a) AS startId, id(b) AS endId',
      { id },
    );

    if (result.rows.length === 0) {
      return null;
    }

    const row = result.rows[0];
    const rel = row.r as unknown as IDataObject;
    return {
      id: rel.id as number,
      type: row.relType as string,
      start_node_id: row.startId as number,
      end_node_id: row.endId as number,
      properties: (rel.properties as IDataObject) || {},
    };
  }

  async updateRelationship(id: number, properties: IDataObject): Promise<NexusRelationship> {
    const result = await this.executeCypher(
      'MATCH (a)-[r]->(b) WHERE id(r) = $id SET r += $props RETURN r, type(r) AS relType, id(a) AS startId, id(b) AS endId',
      { id, props: properties },
    );

    if (result.rows.length === 0) {
      throw new NodeOperationError(this.executeFunctions.getNode(), 'Relationship not found');
    }

    const row = result.rows[0];
    const rel = row.r as unknown as IDataObject;
    return {
      id: rel.id as number,
      type: row.relType as string,
      start_node_id: row.startId as number,
      end_node_id: row.endId as number,
      properties: (rel.properties as IDataObject) || {},
    };
  }

  async deleteRelationship(id: number): Promise<{ deleted: boolean; relationshipId: number }> {
    const result = await this.executeCypher(
      'MATCH ()-[r]->() WHERE id(r) = $id DELETE r RETURN count(*) AS deleted',
      { id },
    );
    const deleted = result.rows.length > 0 && (result.rows[0].deleted as number) > 0;
    return { deleted, relationshipId: id };
  }

  async batchCreateNodes(
    nodes: Array<{ labels: string[]; properties: IDataObject }>,
  ): Promise<{ created: number; nodes: NexusNode[] }> {
    const createdNodes: NexusNode[] = [];

    for (const node of nodes) {
      const created = await this.createNode(node.labels, node.properties);
      createdNodes.push(created);
    }

    return {
      created: createdNodes.length,
      nodes: createdNodes,
    };
  }

  async batchCreateRelationships(
    relationships: Array<{
      startNodeId: number;
      endNodeId: number;
      type: string;
      properties?: IDataObject;
    }>,
  ): Promise<{ created: number; relationships: NexusRelationship[] }> {
    const createdRels: NexusRelationship[] = [];

    for (const rel of relationships) {
      const created = await this.createRelationship(
        rel.startNodeId,
        rel.endNodeId,
        rel.type,
        rel.properties,
      );
      createdRels.push(created);
    }

    return {
      created: createdRels.length,
      relationships: createdRels,
    };
  }

  async getLabels(): Promise<string[]> {
    const response = await this.request<{ labels: string[] }>('GET', '/schema/labels');
    return response.labels;
  }

  async getRelationshipTypes(): Promise<string[]> {
    const response = await this.request<{ types: string[] }>('GET', '/schema/relationship-types');
    return response.types;
  }

  async getSchema(): Promise<SchemaInfo> {
    const [labels, relationshipTypes] = await Promise.all([
      this.getLabels(),
      this.getRelationshipTypes(),
    ]);

    return {
      labels,
      relationship_types: relationshipTypes,
    };
  }

  async shortestPath(
    startNodeId: number,
    endNodeId: number,
    relationshipTypes?: string[],
    maxDepth?: number,
  ): Promise<{ path: number[]; length: number; relationships: number[] }> {
    let relFilter = '';
    if (relationshipTypes && relationshipTypes.length > 0) {
      relFilter = `:${relationshipTypes.join('|')}`;
    }

    const depthLimit = maxDepth ? `*..${maxDepth}` : '*';

    const cypher = `
      MATCH (start), (end)
      WHERE id(start) = $startId AND id(end) = $endId
      MATCH path = shortestPath((start)-[${relFilter}${depthLimit}]-(end))
      RETURN [n IN nodes(path) | id(n)] AS nodeIds,
             [r IN relationships(path) | id(r)] AS relIds,
             length(path) AS pathLength
    `;

    const result = await this.executeCypher(cypher, {
      startId: startNodeId,
      endId: endNodeId,
    });

    if (result.rows.length === 0) {
      return { path: [], length: -1, relationships: [] };
    }

    const row = result.rows[0];
    return {
      path: row.nodeIds as number[],
      length: row.pathLength as number,
      relationships: row.relIds as number[],
    };
  }

  async testConnection(): Promise<boolean> {
    try {
      await this.executeCypher('RETURN 1 AS test');
      return true;
    } catch {
      return false;
    }
  }
}
