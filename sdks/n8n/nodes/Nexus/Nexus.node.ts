import type {
  IExecuteFunctions,
  INodeExecutionData,
  INodeType,
  INodeTypeDescription,
  IDataObject,
} from 'n8n-workflow';
import { NodeOperationError } from 'n8n-workflow';
import { NexusClient, type NexusCredentials } from './NexusClient';

export class Nexus implements INodeType {
  description: INodeTypeDescription = {
    displayName: 'Nexus',
    name: 'nexus',
    icon: 'file:nexus.svg',
    group: ['transform'],
    version: 1,
    subtitle: '={{$parameter["operation"]}}',
    description: 'Execute graph operations on Nexus Graph Database',
    defaults: {
      name: 'Nexus',
    },
    inputs: ['main'],
    outputs: ['main'],
    credentials: [
      {
        name: 'nexusApi',
        required: true,
        displayOptions: {
          show: {
            authentication: ['apiKey'],
          },
        },
      },
      {
        name: 'nexusUser',
        required: true,
        displayOptions: {
          show: {
            authentication: ['userPassword'],
          },
        },
      },
    ],
    properties: [
      {
        displayName: 'Authentication',
        name: 'authentication',
        type: 'options',
        options: [
          {
            name: 'API Key',
            value: 'apiKey',
          },
          {
            name: 'User/Password',
            value: 'userPassword',
          },
        ],
        default: 'apiKey',
        description: 'Authentication method to use',
      },
      {
        displayName: 'Operation',
        name: 'operation',
        type: 'options',
        noDataExpression: true,
        options: [
          {
            name: 'Execute Cypher',
            value: 'executeCypher',
            description: 'Execute a Cypher query',
            action: 'Execute a cypher query',
          },
          {
            name: 'Create Node',
            value: 'createNode',
            description: 'Create a new node',
            action: 'Create a new node',
          },
          {
            name: 'Read Node',
            value: 'readNode',
            description: 'Read a node by ID',
            action: 'Read a node by ID',
          },
          {
            name: 'Update Node',
            value: 'updateNode',
            description: 'Update a node',
            action: 'Update a node',
          },
          {
            name: 'Delete Node',
            value: 'deleteNode',
            description: 'Delete a node',
            action: 'Delete a node',
          },
          {
            name: 'Find Nodes',
            value: 'findNodes',
            description: 'Find nodes by label and properties',
            action: 'Find nodes by label and properties',
          },
          {
            name: 'Create Relationship',
            value: 'createRelationship',
            description: 'Create a relationship between nodes',
            action: 'Create a relationship between nodes',
          },
          {
            name: 'Read Relationship',
            value: 'readRelationship',
            description: 'Read a relationship by ID',
            action: 'Read a relationship by ID',
          },
          {
            name: 'Update Relationship',
            value: 'updateRelationship',
            description: 'Update a relationship',
            action: 'Update a relationship',
          },
          {
            name: 'Delete Relationship',
            value: 'deleteRelationship',
            description: 'Delete a relationship',
            action: 'Delete a relationship',
          },
          {
            name: 'Batch Create Nodes',
            value: 'batchCreateNodes',
            description: 'Create multiple nodes in batch',
            action: 'Create multiple nodes in batch',
          },
          {
            name: 'Batch Create Relationships',
            value: 'batchCreateRelationships',
            description: 'Create multiple relationships in batch',
            action: 'Create multiple relationships in batch',
          },
          {
            name: 'List Labels',
            value: 'listLabels',
            description: 'List all node labels',
            action: 'List all node labels',
          },
          {
            name: 'List Relationship Types',
            value: 'listRelationshipTypes',
            description: 'List all relationship types',
            action: 'List all relationship types',
          },
          {
            name: 'Get Schema',
            value: 'getSchema',
            description: 'Get database schema information',
            action: 'Get database schema information',
          },
          {
            name: 'Shortest Path',
            value: 'shortestPath',
            description: 'Find shortest path between nodes',
            action: 'Find shortest path between nodes',
          },
        ],
        default: 'executeCypher',
      },

      // Execute Cypher
      {
        displayName: 'Cypher Query',
        name: 'cypher',
        type: 'string',
        typeOptions: {
          rows: 5,
        },
        default: '',
        required: true,
        displayOptions: {
          show: {
            operation: ['executeCypher'],
          },
        },
        description: 'The Cypher query to execute',
        placeholder: 'MATCH (n:Person) RETURN n LIMIT 10',
      },
      {
        displayName: 'Query Parameters',
        name: 'queryParams',
        type: 'fixedCollection',
        typeOptions: {
          multipleValues: true,
        },
        default: {},
        displayOptions: {
          show: {
            operation: ['executeCypher'],
          },
        },
        options: [
          {
            name: 'parameter',
            displayName: 'Parameter',
            values: [
              {
                displayName: 'Name',
                name: 'name',
                type: 'string',
                default: '',
                description: 'Parameter name',
              },
              {
                displayName: 'Value',
                name: 'value',
                type: 'string',
                default: '',
                description: 'Parameter value',
              },
            ],
          },
        ],
        description: 'Query parameters to pass to the Cypher query',
      },

      // Node Operations
      {
        displayName: 'Node ID',
        name: 'nodeId',
        type: 'number',
        default: 0,
        required: true,
        displayOptions: {
          show: {
            operation: ['readNode', 'updateNode', 'deleteNode'],
          },
        },
        description: 'The ID of the node',
      },
      {
        displayName: 'Labels',
        name: 'labels',
        type: 'string',
        default: '',
        required: true,
        displayOptions: {
          show: {
            operation: ['createNode', 'findNodes'],
          },
        },
        description: 'Node labels (comma-separated for multiple labels)',
        placeholder: 'Person,Employee',
      },
      {
        displayName: 'Properties',
        name: 'properties',
        type: 'fixedCollection',
        typeOptions: {
          multipleValues: true,
        },
        default: {},
        displayOptions: {
          show: {
            operation: ['createNode', 'updateNode', 'findNodes'],
          },
        },
        options: [
          {
            name: 'property',
            displayName: 'Property',
            values: [
              {
                displayName: 'Name',
                name: 'name',
                type: 'string',
                default: '',
                description: 'Property name',
              },
              {
                displayName: 'Value',
                name: 'value',
                type: 'string',
                default: '',
                description: 'Property value',
              },
            ],
          },
        ],
        description: 'Node properties',
      },
      {
        displayName: 'Delete Relationships',
        name: 'detachDelete',
        type: 'boolean',
        default: false,
        displayOptions: {
          show: {
            operation: ['deleteNode'],
          },
        },
        description: 'Whether to also delete connected relationships (DETACH DELETE)',
      },
      {
        displayName: 'Limit',
        name: 'limit',
        type: 'number',
        default: 100,
        displayOptions: {
          show: {
            operation: ['findNodes'],
          },
        },
        description: 'Maximum number of nodes to return',
      },

      // Relationship Operations
      {
        displayName: 'Relationship ID',
        name: 'relationshipId',
        type: 'number',
        default: 0,
        required: true,
        displayOptions: {
          show: {
            operation: ['readRelationship', 'updateRelationship', 'deleteRelationship'],
          },
        },
        description: 'The ID of the relationship',
      },
      {
        displayName: 'Start Node ID',
        name: 'startNodeId',
        type: 'number',
        default: 0,
        required: true,
        displayOptions: {
          show: {
            operation: ['createRelationship', 'shortestPath'],
          },
        },
        description: 'The ID of the start node',
      },
      {
        displayName: 'End Node ID',
        name: 'endNodeId',
        type: 'number',
        default: 0,
        required: true,
        displayOptions: {
          show: {
            operation: ['createRelationship', 'shortestPath'],
          },
        },
        description: 'The ID of the end node',
      },
      {
        displayName: 'Relationship Type',
        name: 'relationshipType',
        type: 'string',
        default: '',
        required: true,
        displayOptions: {
          show: {
            operation: ['createRelationship'],
          },
        },
        description: 'The type of the relationship',
        placeholder: 'KNOWS',
      },
      {
        displayName: 'Relationship Properties',
        name: 'relationshipProperties',
        type: 'fixedCollection',
        typeOptions: {
          multipleValues: true,
        },
        default: {},
        displayOptions: {
          show: {
            operation: ['createRelationship', 'updateRelationship'],
          },
        },
        options: [
          {
            name: 'property',
            displayName: 'Property',
            values: [
              {
                displayName: 'Name',
                name: 'name',
                type: 'string',
                default: '',
                description: 'Property name',
              },
              {
                displayName: 'Value',
                name: 'value',
                type: 'string',
                default: '',
                description: 'Property value',
              },
            ],
          },
        ],
        description: 'Relationship properties',
      },

      // Batch Operations
      {
        displayName: 'Nodes JSON',
        name: 'nodesJson',
        type: 'string',
        typeOptions: {
          rows: 10,
        },
        default: '[]',
        required: true,
        displayOptions: {
          show: {
            operation: ['batchCreateNodes'],
          },
        },
        description:
          'JSON array of nodes to create. Each node should have "labels" (array) and "properties" (object).',
        placeholder: '[{"labels": ["Person"], "properties": {"name": "Alice"}}]',
      },
      {
        displayName: 'Relationships JSON',
        name: 'relationshipsJson',
        type: 'string',
        typeOptions: {
          rows: 10,
        },
        default: '[]',
        required: true,
        displayOptions: {
          show: {
            operation: ['batchCreateRelationships'],
          },
        },
        description:
          'JSON array of relationships to create. Each relationship should have "startNodeId", "endNodeId", "type", and optional "properties".',
        placeholder:
          '[{"startNodeId": 1, "endNodeId": 2, "type": "KNOWS", "properties": {"since": "2024"}}]',
      },

      // Shortest Path
      {
        displayName: 'Relationship Types Filter',
        name: 'relationshipTypesFilter',
        type: 'string',
        default: '',
        displayOptions: {
          show: {
            operation: ['shortestPath'],
          },
        },
        description: 'Filter by relationship types (comma-separated). Leave empty for all types.',
        placeholder: 'KNOWS,WORKS_WITH',
      },
      {
        displayName: 'Max Depth',
        name: 'maxDepth',
        type: 'number',
        default: 10,
        displayOptions: {
          show: {
            operation: ['shortestPath'],
          },
        },
        description: 'Maximum path depth',
      },
    ],
  };

  async execute(this: IExecuteFunctions): Promise<INodeExecutionData[][]> {
    const items = this.getInputData();
    const returnData: INodeExecutionData[] = [];
    const operation = this.getNodeParameter('operation', 0) as string;
    const authentication = this.getNodeParameter('authentication', 0) as string;

    const credentialType = authentication === 'apiKey' ? 'nexusApi' : 'nexusUser';
    const credentials = (await this.getCredentials(credentialType)) as unknown as NexusCredentials;
    const client = new NexusClient(this, credentials, credentialType);

    for (let i = 0; i < items.length; i++) {
      try {
        let result: IDataObject | IDataObject[];

        switch (operation) {
          case 'executeCypher': {
            const cypher = this.getNodeParameter('cypher', i) as string;
            const queryParamsRaw = this.getNodeParameter('queryParams', i, {}) as {
              parameter?: Array<{ name: string; value: string }>;
            };
            const params: IDataObject = {};
            if (queryParamsRaw.parameter) {
              for (const param of queryParamsRaw.parameter) {
                params[param.name] = param.value;
              }
            }
            const queryResult = await client.executeCypher(cypher, params);
            result = queryResult as unknown as IDataObject;
            break;
          }

          case 'createNode': {
            const labelsStr = this.getNodeParameter('labels', i) as string;
            const labels = labelsStr.split(',').map((l) => l.trim()).filter((l) => l);
            const propertiesRaw = this.getNodeParameter('properties', i, {}) as {
              property?: Array<{ name: string; value: string }>;
            };
            const properties: IDataObject = {};
            if (propertiesRaw.property) {
              for (const prop of propertiesRaw.property) {
                properties[prop.name] = prop.value;
              }
            }
            const node = await client.createNode(labels, properties);
            result = node as unknown as IDataObject;
            break;
          }

          case 'readNode': {
            const nodeId = this.getNodeParameter('nodeId', i) as number;
            const node = await client.getNode(nodeId);
            if (!node) {
              throw new NodeOperationError(this.getNode(), `Node with ID ${nodeId} not found`, {
                itemIndex: i,
              });
            }
            result = node as unknown as IDataObject;
            break;
          }

          case 'updateNode': {
            const nodeId = this.getNodeParameter('nodeId', i) as number;
            const propertiesRaw = this.getNodeParameter('properties', i, {}) as {
              property?: Array<{ name: string; value: string }>;
            };
            const properties: IDataObject = {};
            if (propertiesRaw.property) {
              for (const prop of propertiesRaw.property) {
                properties[prop.name] = prop.value;
              }
            }
            const updatedNode = await client.updateNode(nodeId, properties);
            result = updatedNode as unknown as IDataObject;
            break;
          }

          case 'deleteNode': {
            const nodeId = this.getNodeParameter('nodeId', i) as number;
            const detach = this.getNodeParameter('detachDelete', i, false) as boolean;
            result = await client.deleteNode(nodeId, detach);
            break;
          }

          case 'findNodes': {
            const labelsStr = this.getNodeParameter('labels', i) as string;
            const label = labelsStr.split(',')[0]?.trim() || '';
            const propertiesRaw = this.getNodeParameter('properties', i, {}) as {
              property?: Array<{ name: string; value: string }>;
            };
            const properties: IDataObject = {};
            if (propertiesRaw.property) {
              for (const prop of propertiesRaw.property) {
                properties[prop.name] = prop.value;
              }
            }
            const limit = this.getNodeParameter('limit', i, 100) as number;
            const nodes = await client.findNodes(label, properties, limit);
            result = nodes as unknown as IDataObject[];
            break;
          }

          case 'createRelationship': {
            const startNodeId = this.getNodeParameter('startNodeId', i) as number;
            const endNodeId = this.getNodeParameter('endNodeId', i) as number;
            const relationshipType = this.getNodeParameter('relationshipType', i) as string;
            const propertiesRaw = this.getNodeParameter('relationshipProperties', i, {}) as {
              property?: Array<{ name: string; value: string }>;
            };
            const properties: IDataObject = {};
            if (propertiesRaw.property) {
              for (const prop of propertiesRaw.property) {
                properties[prop.name] = prop.value;
              }
            }
            const rel = await client.createRelationship(
              startNodeId,
              endNodeId,
              relationshipType,
              Object.keys(properties).length > 0 ? properties : undefined,
            );
            result = rel as unknown as IDataObject;
            break;
          }

          case 'readRelationship': {
            const relationshipId = this.getNodeParameter('relationshipId', i) as number;
            const rel = await client.getRelationship(relationshipId);
            if (!rel) {
              throw new NodeOperationError(
                this.getNode(),
                `Relationship with ID ${relationshipId} not found`,
                { itemIndex: i },
              );
            }
            result = rel as unknown as IDataObject;
            break;
          }

          case 'updateRelationship': {
            const relationshipId = this.getNodeParameter('relationshipId', i) as number;
            const propertiesRaw = this.getNodeParameter('relationshipProperties', i, {}) as {
              property?: Array<{ name: string; value: string }>;
            };
            const properties: IDataObject = {};
            if (propertiesRaw.property) {
              for (const prop of propertiesRaw.property) {
                properties[prop.name] = prop.value;
              }
            }
            const updatedRel = await client.updateRelationship(relationshipId, properties);
            result = updatedRel as unknown as IDataObject;
            break;
          }

          case 'deleteRelationship': {
            const relationshipId = this.getNodeParameter('relationshipId', i) as number;
            result = await client.deleteRelationship(relationshipId);
            break;
          }

          case 'batchCreateNodes': {
            const nodesJson = this.getNodeParameter('nodesJson', i) as string;
            const nodes = JSON.parse(nodesJson) as Array<{
              labels: string[];
              properties: IDataObject;
            }>;
            result = await client.batchCreateNodes(nodes);
            break;
          }

          case 'batchCreateRelationships': {
            const relationshipsJson = this.getNodeParameter('relationshipsJson', i) as string;
            const relationships = JSON.parse(relationshipsJson) as Array<{
              startNodeId: number;
              endNodeId: number;
              type: string;
              properties?: IDataObject;
            }>;
            result = await client.batchCreateRelationships(relationships);
            break;
          }

          case 'listLabels': {
            const labels = await client.getLabels();
            result = { labels };
            break;
          }

          case 'listRelationshipTypes': {
            const types = await client.getRelationshipTypes();
            result = { relationshipTypes: types };
            break;
          }

          case 'getSchema': {
            const schema = await client.getSchema();
            result = schema as unknown as IDataObject;
            break;
          }

          case 'shortestPath': {
            const startNodeId = this.getNodeParameter('startNodeId', i) as number;
            const endNodeId = this.getNodeParameter('endNodeId', i) as number;
            const relTypesStr = this.getNodeParameter('relationshipTypesFilter', i, '') as string;
            const relTypes = relTypesStr
              ? relTypesStr.split(',').map((t) => t.trim()).filter((t) => t)
              : undefined;
            const maxDepth = this.getNodeParameter('maxDepth', i, 10) as number;
            result = await client.shortestPath(startNodeId, endNodeId, relTypes, maxDepth);
            break;
          }

          default:
            throw new NodeOperationError(this.getNode(), `Unknown operation: ${operation}`, {
              itemIndex: i,
            });
        }

        if (Array.isArray(result)) {
          returnData.push(
            ...result.map((item) => ({
              json: item as IDataObject,
              pairedItem: { item: i },
            })),
          );
        } else {
          returnData.push({
            json: result as IDataObject,
            pairedItem: { item: i },
          });
        }
      } catch (error) {
        if (this.continueOnFail()) {
          returnData.push({
            json: {
              error: (error as Error).message,
            },
            pairedItem: { item: i },
          });
          continue;
        }
        throw error;
      }
    }

    return [returnData];
  }
}
