import { NexusClient } from '../src';

/**
 * Basic usage example for Nexus TypeScript SDK
 */
async function main() {
  // Create client with API key authentication
  const client = new NexusClient({
    baseUrl: 'http://localhost:7687',
    auth: {
      apiKey: 'your-api-key-here',
    },
    debug: true,
  });

  try {
    // Test connection
    console.log('Testing connection...');
    await client.testConnection();
    console.log('✓ Connected to Nexus successfully\n');

    // Execute a simple query
    console.log('Executing simple query...');
    const result = await client.executeCypher('RETURN "Hello, Nexus!" AS greeting');
    console.log('Result:', result.rows[0]);
    console.log();

    // Create a node
    console.log('Creating a person node...');
    const alice = await client.createNode(['Person'], {
      name: 'Alice',
      age: 30,
      email: 'alice@example.com',
    });
    console.log('Created node:', alice);
    console.log();

    // Create another node
    console.log('Creating another person node...');
    const bob = await client.createNode(['Person'], {
      name: 'Bob',
      age: 28,
      email: 'bob@example.com',
    });
    console.log('Created node:', bob);
    console.log();

    // Create a relationship
    console.log('Creating relationship...');
    const relationship = await client.createRelationship(
      alice.id,
      bob.id,
      'KNOWS',
      { since: 2020, how: 'work' }
    );
    console.log('Created relationship:', relationship);
    console.log();

    // Query with parameters
    console.log('Querying persons older than 25...');
    const queryResult = await client.executeCypher(
      'MATCH (p:Person) WHERE p.age > $age RETURN p.name AS name, p.age AS age ORDER BY p.age',
      { age: 25 }
    );
    console.log('Results:', queryResult.rows);
    console.log();

    // Update node
    console.log('Updating Alice\'s age...');
    const updatedAlice = await client.updateNode(alice.id, { age: 31 });
    console.log('Updated node:', updatedAlice);
    console.log();

    // Find nodes
    console.log('Finding all Person nodes...');
    const persons = await client.findNodes('Person');
    console.log(`Found ${persons.length} person(s):`, persons);
    console.log();

    // Get schema
    console.log('Getting schema information...');
    const schema = await client.getSchema();
    console.log('Labels:', schema.labels);
    console.log('Relationship types:', schema.relationshipTypes);
    console.log();

    // Batch operations
    console.log('Executing batch operations...');
    const batchResults = await client.executeBatch([
      { cypher: 'MATCH (p:Person) RETURN count(p) AS count' },
      { cypher: 'MATCH ()-[r:KNOWS]->() RETURN count(r) AS count' },
    ]);
    console.log('Batch results:');
    console.log('  Persons count:', batchResults[0].rows[0].count);
    console.log('  KNOWS relationships:', batchResults[1].rows[0].count);
    console.log();

    // Cleanup
    console.log('Cleaning up...');
    await client.deleteNode(alice.id, true); // detach delete
    await client.deleteNode(bob.id, true);
    console.log('✓ Cleanup complete');

  } catch (error) {
    console.error('Error:', error);
    process.exit(1);
  }
}

// Run the example
main().catch(console.error);

