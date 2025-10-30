#!/usr/bin/env tsx

const NEXUS_URL = process.env.NEXUS_URL || 'http://127.0.0.1:15474';

async function testQuery(name: string, query: string) {
  const response = await fetch(`${NEXUS_URL}/cypher`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({ query }),
  });

  const result = await response.json();
  console.log(`\n✅ ${name}:`);
  console.log(`   Rows: ${result.rows.length}`);
  if (result.rows.length > 0) {
    console.log(`   First row:`, JSON.stringify(result.rows[0], null, 2));
  }
  return result;
}

async function main() {
  console.log('🔍 Testing Property Persistence after Server Restart\n');
  
  // Test 1: Simple document query
  await testQuery(
    'Simple Document Query',
    'MATCH (d:Document) RETURN d LIMIT 1'
  );
  
  // Test 2: Properties projection
  await testQuery(
    'Properties Projection',
    'MATCH (d:Document) RETURN d.domain AS domain, d.doc_type AS doc_type, d.title AS title LIMIT 3'
  );
  
  // Test 3: Count with properties
  await testQuery(
    'Count by Domain',
    'MATCH (d:Document) RETURN d.domain AS domain, count(d) AS count'
  );
  
  // Test 4: Filter by property
  await testQuery(
    'Filter by Domain',
    'MATCH (d:Document {domain: "software"}) RETURN d.title AS title LIMIT 3'
  );
  
  // Test 5: Check all properties exist
  const result = await testQuery(
    'Check All Properties',
    'MATCH (d:Document) RETURN keys(d) AS keys LIMIT 1'
  );
  
  console.log('\n✨ All tests completed!');
}

main().catch(console.error);

