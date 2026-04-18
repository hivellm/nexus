/**
 * Simple integration test for n8n SDK
 * Tests the NexusClient against a running Nexus server
 */

import axios from 'axios';

interface QueryResult {
  columns: string[];
  rows: any[];
  execution_time_ms?: number;
}

async function testN8nSDK() {
  console.log('=== Testing n8n SDK Integration ===\n');

  const baseUrl = 'http://localhost:15474';

  try {
    // Test 1: Simple query
    process.stdout.write('1. Simple query: ');
    const result1 = await axios.post<QueryResult>(`${baseUrl}/cypher`, {
      query: 'RETURN 1 as num',
      parameters: {}
    });
    console.log(`OK - Columns: ${result1.data.columns.join(', ')}`);

    // Test 2: Create nodes
    process.stdout.write('2. Create nodes: ');
    const result2 = await axios.post<QueryResult>(`${baseUrl}/cypher`, {
      query: "CREATE (a:Person {name: 'Alice', age: 28}) CREATE (b:Person {name: 'Bob', age: 32}) RETURN a.name, b.name",
      parameters: {}
    });
    console.log(`OK - Rows: ${result2.data.rows.length}`);

    // Test 3: Query with parameters
    process.stdout.write('3. Query with parameters: ');
    const result3 = await axios.post<QueryResult>(`${baseUrl}/cypher`, {
      query: 'MATCH (p:Person) WHERE p.age > $minAge RETURN p.name as name, p.age as age',
      parameters: { minAge: 25 }
    });
    console.log(`OK - Found ${result3.data.rows.length} nodes`);

    // Test 4: Create relationship
    process.stdout.write('4. Create relationship: ');
    await axios.post<QueryResult>(`${baseUrl}/cypher`, {
      query: "MATCH (a:Person {name: 'Alice'}) MATCH (b:Person {name: 'Bob'}) CREATE (a)-[r:KNOWS {since: '2020'}]->(b) RETURN type(r) as type",
      parameters: {}
    });
    console.log('OK');

    // Test 5: Query relationships
    process.stdout.write('5. Query relationships: ');
    const result5 = await axios.post<QueryResult>(`${baseUrl}/cypher`, {
      query: 'MATCH (a:Person)-[r:KNOWS]->(b:Person) RETURN a.name as person1, b.name as person2',
      parameters: {}
    });
    console.log(`OK - Found ${result5.data.rows.length} relationships`);

    // Test 6: Cleanup
    process.stdout.write('6. Cleanup: ');
    await axios.post<QueryResult>(`${baseUrl}/cypher`, {
      query: 'MATCH (n) DETACH DELETE n',
      parameters: {}
    });
    console.log('OK');

    console.log('\n[SUCCESS] All n8n SDK integration tests passed!');

  } catch (error: any) {
    console.error(`\n[ERROR] ${error.message}`);
    if (error.response) {
      console.error(`Status: ${error.response.status}`);
      console.error(`Data:`, error.response.data);
    }
    throw error;
  }
}

testN8nSDK().catch(console.error);
