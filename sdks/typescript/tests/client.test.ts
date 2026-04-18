import { describe, it, expect, beforeAll } from 'vitest';
import { NexusClient } from '../src/client';
import { ValidationError } from '../src/errors';

describe('NexusClient', () => {
  describe('Configuration Validation', () => {
    it('should throw error if baseUrl is missing', () => {
      expect(() => {
        new NexusClient({
          baseUrl: '',
          auth: { apiKey: 'test-key' },
        });
      }).toThrow(ValidationError);
    });

    it('should throw error if auth is missing', () => {
      expect(() => {
        new NexusClient({
          baseUrl: 'http://localhost:7687',
          auth: {} as any,
        });
      }).toThrow(ValidationError);
    });

    it('should accept valid configuration with API key', () => {
      expect(() => {
        new NexusClient({
          baseUrl: 'http://localhost:7687',
          auth: { apiKey: 'test-key' },
        });
      }).not.toThrow();
    });

    it('should accept valid configuration with username/password', () => {
      expect(() => {
        new NexusClient({
          baseUrl: 'http://localhost:7687',
          auth: { username: 'user', password: 'pass' },
        });
      }).not.toThrow();
    });
  });

  describe('Cypher Query Execution', () => {
    let client: NexusClient;

    beforeAll(() => {
      client = new NexusClient({
        baseUrl: process.env.NEXUS_URL || 'http://localhost:7687',
        auth: {
          apiKey: process.env.NEXUS_API_KEY || 'test-key',
        },
      });
    });

    it('should execute simple query', async () => {
      const result = await client.executeCypher('RETURN 1 AS num');
      expect(result.columns).toContain('num');
      expect(result.rows).toHaveLength(1);
      expect(result.rows[0].num).toBe(1);
    });

    it('should execute query with parameters', async () => {
      const result = await client.executeCypher(
        'RETURN $value AS result',
        { value: 'test' }
      );
      expect(result.rows[0].result).toBe('test');
    });
  });

  describe('Node Operations', () => {
    let client: NexusClient;

    beforeAll(() => {
      client = new NexusClient({
        baseUrl: process.env.NEXUS_URL || 'http://localhost:7687',
        auth: {
          apiKey: process.env.NEXUS_API_KEY || 'test-key',
        },
      });
    });

    it('should create a node', async () => {
      const node = await client.createNode(['TestPerson'], {
        name: 'Alice',
        age: 30,
      });

      expect(node).toBeDefined();
      expect(node.labels).toContain('TestPerson');
      expect(node.properties.name).toBe('Alice');
      expect(node.properties.age).toBe(30);
    });

    it('should get node by ID', async () => {
      const createdNode = await client.createNode(['TestPerson'], {
        name: 'Bob',
      });

      const retrievedNode = await client.getNode(createdNode.id);
      expect(retrievedNode).toBeDefined();
      expect(retrievedNode?.properties.name).toBe('Bob');
    });

    it('should update node', async () => {
      const node = await client.createNode(['TestPerson'], {
        name: 'Charlie',
        age: 25,
      });

      const updatedNode = await client.updateNode(node.id, { age: 26 });
      expect(updatedNode.properties.age).toBe(26);
    });

    it('should find nodes', async () => {
      await client.createNode(['TestPerson'], { name: 'Dave', city: 'NYC' });
      await client.createNode(['TestPerson'], { name: 'Eve', city: 'NYC' });

      const nodes = await client.findNodes('TestPerson', { city: 'NYC' });
      expect(nodes.length).toBeGreaterThanOrEqual(2);
    });

    it('should delete node', async () => {
      const node = await client.createNode(['TestPerson'], {
        name: 'Frank',
      });

      await client.deleteNode(node.id);
      const retrievedNode = await client.getNode(node.id);
      expect(retrievedNode).toBeNull();
    });
  });

  describe('Relationship Operations', () => {
    let client: NexusClient;

    beforeAll(() => {
      client = new NexusClient({
        baseUrl: process.env.NEXUS_URL || 'http://localhost:7687',
        auth: {
          apiKey: process.env.NEXUS_API_KEY || 'test-key',
        },
      });
    });

    it('should create relationship', async () => {
      const node1 = await client.createNode(['TestPerson'], { name: 'Alice' });
      const node2 = await client.createNode(['TestPerson'], { name: 'Bob' });

      const rel = await client.createRelationship(
        node1.id,
        node2.id,
        'KNOWS',
        { since: 2020 }
      );

      expect(rel).toBeDefined();
      expect(rel.type).toBe('KNOWS');
      expect(rel.properties.since).toBe(2020);
    });

    it('should get relationship by ID', async () => {
      const node1 = await client.createNode(['TestPerson'], { name: 'Charlie' });
      const node2 = await client.createNode(['TestPerson'], { name: 'Dave' });

      const createdRel = await client.createRelationship(
        node1.id,
        node2.id,
        'KNOWS'
      );

      const retrievedRel = await client.getRelationship(createdRel.id);
      expect(retrievedRel).toBeDefined();
      expect(retrievedRel?.type).toBe('KNOWS');
    });
  });

  describe('Schema Operations', () => {
    let client: NexusClient;

    beforeAll(() => {
      client = new NexusClient({
        baseUrl: process.env.NEXUS_URL || 'http://localhost:7687',
        auth: {
          apiKey: process.env.NEXUS_API_KEY || 'test-key',
        },
      });
    });

    it('should get labels', async () => {
      const labels = await client.getLabels();
      expect(Array.isArray(labels)).toBe(true);
    });

    it('should get relationship types', async () => {
      const types = await client.getRelationshipTypes();
      expect(Array.isArray(types)).toBe(true);
    });

    it('should get schema', async () => {
      const schema = await client.getSchema();
      expect(schema).toBeDefined();
      expect(schema.labels).toBeDefined();
      expect(schema.relationshipTypes).toBeDefined();
    });
  });

  describe('Batch Operations', () => {
    let client: NexusClient;

    beforeAll(() => {
      client = new NexusClient({
        baseUrl: process.env.NEXUS_URL || 'http://localhost:7687',
        auth: {
          apiKey: process.env.NEXUS_API_KEY || 'test-key',
        },
      });
    });

    it('should execute batch operations', async () => {
      const results = await client.executeBatch([
        { cypher: 'RETURN 1 AS num' },
        { cypher: 'RETURN 2 AS num' },
        { cypher: 'RETURN $value AS num', params: { value: 3 } },
      ]);

      expect(results).toHaveLength(3);
      expect(results[0].rows[0].num).toBe(1);
      expect(results[1].rows[0].num).toBe(2);
      expect(results[2].rows[0].num).toBe(3);
    });
  });
});

