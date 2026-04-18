import { NexusClient } from '../src';

/**
 * Multi-database support example for Nexus TypeScript SDK
 */
async function main() {
  // Create client connecting to the default database
  const client = new NexusClient({
    baseUrl: 'http://localhost:15474',
    debug: true,
  });

  try {
    console.log('=== Multi-Database Support Demo ===\n');

    // 1. List all databases
    console.log('1. Listing all databases...');
    const databases = await client.listDatabases();
    console.log(`   Available databases: ${databases.databases.join(', ')}`);
    console.log(`   Default database: ${databases.defaultDatabase}\n`);

    // 2. Create a new database
    console.log('2. Creating new database "testdb"...');
    const createResult = await client.createDatabase('testdb');
    console.log(`   Result: ${createResult.message}\n`);

    // 3. Switch to the new database
    console.log('3. Switching to "testdb"...');
    const switchResult = await client.switchDatabase('testdb');
    console.log(`   Result: ${switchResult.message}\n`);

    // 4. Get current database
    console.log('4. Getting current database...');
    const currentDb = await client.getCurrentDatabase();
    console.log(`   Current database: ${currentDb}\n`);

    // 5. Create data in the new database
    console.log('5. Creating data in "testdb"...');
    const result = await client.executeCypher(
      'CREATE (n:Product {name: $name, price: $price}) RETURN n',
      { name: 'Laptop', price: 999.99 }
    );
    console.log(`   Created ${result.rows.length} node(s)\n`);

    // 6. Query data from testdb
    console.log('6. Querying data from "testdb"...');
    const queryResult = await client.executeCypher(
      'MATCH (n:Product) RETURN n.name AS name, n.price AS price',
      {}
    );
    queryResult.rows.forEach((row: any) => {
      console.log(`   Product: ${row.name}, Price: $${row.price}`);
    });
    console.log();

    // 7. Switch back to default database
    console.log('7. Switching back to default database...');
    const switchBack = await client.switchDatabase('neo4j');
    console.log(`   Result: ${switchBack.message}\n`);

    // 8. Verify data isolation - the Product node should not exist in default db
    console.log('8. Verifying data isolation...');
    const isolationCheck = await client.executeCypher(
      'MATCH (n:Product) RETURN count(n) AS count',
      {}
    );
    const productCount = isolationCheck.rows[0]?.count || 0;
    console.log(`   Product nodes in default database: ${productCount}`);
    console.log(`   Data isolation verified: ${productCount === 0}\n`);

    // 9. Get database info
    console.log('9. Getting "testdb" info...');
    const dbInfo = await client.getDatabase('testdb');
    console.log(`   Name: ${dbInfo.name}`);
    console.log(`   Path: ${dbInfo.path}`);
    console.log(`   Nodes: ${dbInfo.nodeCount}`);
    console.log(`   Relationships: ${dbInfo.relationshipCount}`);
    console.log(`   Storage: ${dbInfo.storageSize} bytes\n`);

    // 10. Clean up - drop the test database
    console.log('10. Dropping "testdb"...');
    const dropResult = await client.dropDatabase('testdb');
    console.log(`    Result: ${dropResult.message}\n`);

    // 11. Verify database was dropped
    console.log('11. Verifying "testdb" was dropped...');
    const finalDatabases = await client.listDatabases();
    const dbExists = finalDatabases.databases.includes('testdb');
    console.log(`    "testdb" exists: ${dbExists}`);
    console.log(`    Cleanup successful: ${!dbExists}\n`);

    console.log('=== Multi-Database Demo Complete ===');

  } catch (error) {
    console.error('Error:', error);
    process.exit(1);
  }
}

// Run the example
main().catch(console.error);
