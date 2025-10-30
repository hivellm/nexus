#!/usr/bin/env tsx

const NEXUS_URL = 'http://127.0.0.1:15474/cypher';

async function testQuery(name: string, query: string) {
  console.log(`\n${name}:`);
  try {
    const response = await fetch(NEXUS_URL, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ query })
    });
    
    const result = await response.json();
    console.log(`  Rows: ${result.rows.length}`);
    if (result.rows.length > 0) {
      console.log(`  First few rows:`, JSON.stringify(result.rows.slice(0, 3), null, 2));
    } else {
      console.log(`  No rows returned`);
    }
    if (result.error) {
      console.log(`  Error: ${result.error}`);
    }
  } catch (error) {
    console.log(`  Error: ${error}`);
  }
}

async function main() {
  console.log('Testing type() and DISTINCT:\n');
  
  await testQuery('All Relationship Types', 'MATCH ()-[r]->() RETURN DISTINCT type(r) AS type');
  await testQuery('All Labels', 'MATCH (n) RETURN DISTINCT labels(n) AS labels');
  
  console.log('\nDone!');
}

main().catch(console.error);

