import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { NexusClient } from '../src';

describe('Multi-Database Support', () => {
  let client: NexusClient;

  beforeAll(() => {
    client = new NexusClient({
      baseUrl: 'http://localhost:15474',
    });
  });

  afterAll(async () => {
    // Clean up any test databases that might still exist
    try {
      const databases = await client.listDatabases();
      const testDbs = databases.databases.filter((db: string) =>
        db.startsWith('test_')
      );

      // Switch to default database first
      if (testDbs.length > 0) {
        await client.switchDatabase(databases.defaultDatabase);
      }

      // Drop test databases
      for (const db of testDbs) {
        try {
          await client.dropDatabase(db);
        } catch (e) {
          // Ignore errors during cleanup
        }
      }
    } catch (e) {
      // Ignore errors during cleanup
    }
  });

  it('should list databases', async () => {
    const databases = await client.listDatabases();

    expect(databases.databases).toBeDefined();
    expect(Array.isArray(databases.databases)).toBe(true);
    expect(databases.databases.length).toBeGreaterThan(0);
    expect(databases.defaultDatabase).toBeDefined();
    expect(databases.databases).toContain(databases.defaultDatabase);
  });

  it('should create and drop a database', async () => {
    const dbName = 'test_temp_db';

    // Create database
    const createResult = await client.createDatabase(dbName);
    expect(createResult.success).toBe(true);
    expect(createResult.name).toBe(dbName);

    // Verify it exists
    let databases = await client.listDatabases();
    expect(databases.databases).toContain(dbName);

    // Drop database
    const dropResult = await client.dropDatabase(dbName);
    expect(dropResult.success).toBe(true);

    // Verify it's gone
    databases = await client.listDatabases();
    expect(databases.databases).not.toContain(dbName);
  });

  it('should switch between databases', async () => {
    const dbName = 'test_switch_db';

    try {
      // Create a test database
      await client.createDatabase(dbName);

      // Get initial database
      const initialDb = await client.getCurrentDatabase();

      // Switch to test database
      const switchResult = await client.switchDatabase(dbName);
      expect(switchResult.success).toBe(true);

      // Verify we're in the new database
      let currentDb = await client.getCurrentDatabase();
      expect(currentDb).toBe(dbName);

      // Switch back
      const switchBack = await client.switchDatabase(initialDb);
      expect(switchBack.success).toBe(true);

      // Verify we're back
      currentDb = await client.getCurrentDatabase();
      expect(currentDb).toBe(initialDb);
    } finally {
      // Clean up
      const databases = await client.listDatabases();
      await client.switchDatabase(databases.defaultDatabase);
      await client.dropDatabase(dbName);
    }
  });

  it('should get database information', async () => {
    const dbName = 'test_info_db';

    try {
      // Create a test database
      await client.createDatabase(dbName);

      // Get database info
      const dbInfo = await client.getDatabase(dbName);
      expect(dbInfo.name).toBe(dbName);
      expect(dbInfo.path).toBeDefined();
      expect(typeof dbInfo.nodeCount).toBe('number');
      expect(typeof dbInfo.relationshipCount).toBe('number');
      expect(typeof dbInfo.storageSize).toBe('number');
      expect(dbInfo.nodeCount).toBeGreaterThanOrEqual(0);
      expect(dbInfo.relationshipCount).toBeGreaterThanOrEqual(0);
      expect(dbInfo.storageSize).toBeGreaterThanOrEqual(0);
    } finally {
      // Clean up
      await client.dropDatabase(dbName);
    }
  });

  it('should isolate data between databases', async () => {
    const db1Name = 'test_isolation_db1';
    const db2Name = 'test_isolation_db2';

    try {
      // Create two test databases
      await client.createDatabase(db1Name);
      await client.createDatabase(db2Name);

      // Switch to db1 and create a node
      await client.switchDatabase(db1Name);
      let result = await client.executeCypher(
        'CREATE (n:TestNode {name: $name}) RETURN n',
        { name: 'DB1 Node' }
      );
      expect(result.rows.length).toBe(1);

      // Verify node exists in db1
      result = await client.executeCypher(
        'MATCH (n:TestNode) RETURN count(n) AS count',
        {}
      );
      expect(result.rows[0].count).toBe(1);

      // Switch to db2
      await client.switchDatabase(db2Name);

      // Verify node does NOT exist in db2 (isolation)
      result = await client.executeCypher(
        'MATCH (n:TestNode) RETURN count(n) AS count',
        {}
      );
      expect(result.rows[0].count).toBe(0);

      // Create a different node in db2
      result = await client.executeCypher(
        'CREATE (n:TestNode {name: $name}) RETURN n',
        { name: 'DB2 Node' }
      );
      expect(result.rows.length).toBe(1);

      // Verify only one node in db2
      result = await client.executeCypher(
        'MATCH (n:TestNode) RETURN count(n) AS count',
        {}
      );
      expect(result.rows[0].count).toBe(1);

      // Switch back to db1
      await client.switchDatabase(db1Name);

      // Verify still only one node in db1
      result = await client.executeCypher(
        'MATCH (n:TestNode) RETURN count(n) AS count',
        {}
      );
      expect(result.rows[0].count).toBe(1);
    } finally {
      // Clean up
      const databases = await client.listDatabases();
      await client.switchDatabase(databases.defaultDatabase);
      await client.dropDatabase(db1Name);
      await client.dropDatabase(db2Name);
    }
  });

  it('should connect to a specific database using constructor', async () => {
    const dbName = 'test_param_db';

    try {
      // Create a test database
      await client.createDatabase(dbName);

      // Create a new client connected to the specific database
      const dbClient = new NexusClient({
        baseUrl: 'http://localhost:15474',
        database: dbName,
      });

      // Verify we're connected to the right database
      const currentDb = await dbClient.getCurrentDatabase();
      expect(currentDb).toBe(dbName);
    } finally {
      // Clean up
      await client.dropDatabase(dbName);
    }
  });

  it('should not allow dropping the current database', async () => {
    const dbName = 'test_no_drop_db';

    try {
      // Create a test database
      await client.createDatabase(dbName);

      // Switch to the database
      await client.switchDatabase(dbName);

      // Try to drop it while it's active - should fail
      await expect(client.dropDatabase(dbName)).rejects.toThrow();

      // Switch to a different database
      const databases = await client.listDatabases();
      await client.switchDatabase(databases.defaultDatabase);

      // Now we should be able to drop it
      const dropResult = await client.dropDatabase(dbName);
      expect(dropResult.success).toBe(true);
    } catch (error) {
      // Clean up even if test fails
      const databases = await client.listDatabases();
      await client.switchDatabase(databases.defaultDatabase);

      try {
        await client.dropDatabase(dbName);
      } catch (e) {
        // Ignore cleanup errors
      }

      throw error;
    }
  });

  it('should not allow dropping the default database', async () => {
    // Get default database
    const databases = await client.listDatabases();
    const defaultDb = databases.defaultDatabase;

    // Try to drop it - should fail
    await expect(client.dropDatabase(defaultDb)).rejects.toThrow();
  });
});
