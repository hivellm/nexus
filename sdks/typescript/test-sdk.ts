import { NexusClient } from './src/client';

async function main() {
  console.log('=== Testing TypeScript SDK ===\n');

  const client = new NexusClient({
    baseUrl: 'http://localhost:15474',
    auth: { apiKey: 'test-key' }
  });

  try {
    // Test 1: Execute simple query
    process.stdout.write('1. Simple query: ');
    const result1 = await client.executeCypher('RETURN 1 as num');
    console.log(`OK - Columns: ${result1.columns.join(', ')}`);

    // Test 2: Create nodes
    process.stdout.write('2. Create nodes: ');
    const result2 = await client.executeCypher(
      "CREATE (a:Person {name: 'Alice', age: 28}) " +
      "CREATE (b:Person {name: 'Bob', age: 32}) " +
      "RETURN a.name, b.name"
    );
    console.log(`OK - Rows: ${result2.rows.length}`);

    // Test 3: Query with parameters
    process.stdout.write('3. Query with parameters: ');
    const result3 = await client.executeCypher(
      'MATCH (p:Person) WHERE p.age > $minAge RETURN p.name as name, p.age as age',
      { minAge: 25 }
    );
    console.log(`OK - Found ${result3.rows.length} nodes`);

    // Test 4: Create relationship
    process.stdout.write('4. Create relationship: ');
    const result4 = await client.executeCypher(
      "MATCH (a:Person {name: 'Alice'}) " +
      "MATCH (b:Person {name: 'Bob'}) " +
      "CREATE (a)-[r:KNOWS {since: '2020'}]->(b) " +
      "RETURN type(r) as type"
    );
    console.log('OK');

    // Test 5: Query relationships
    process.stdout.write('5. Query relationships: ');
    const result5 = await client.executeCypher(
      'MATCH (a:Person)-[r:KNOWS]->(b:Person) ' +
      'RETURN a.name as person1, b.name as person2'
    );
    console.log(`OK - Found ${result5.rows.length} relationships`);

    // Test 6: Cleanup
    process.stdout.write('6. Cleanup: ');
    await client.executeCypher('MATCH (n) DETACH DELETE n');
    console.log('OK');

    console.log('\n[SUCCESS] All TypeScript SDK tests passed!');

  } catch (error) {
    console.error(`\n[ERROR] ${error}`);
    throw error;
  }
}

main().catch(console.error);
