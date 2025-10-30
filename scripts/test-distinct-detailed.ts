#!/usr/bin/env tsx

const NEXUS_URL = 'http://127.0.0.1:15474/cypher';

async function testQuery(name: string, query: string, expectedRows?: number) {
  console.log(`\n${name}:`);
  console.log(`  Query: ${query}`);
  try {
    const response = await fetch(NEXUS_URL, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ query })
    });
    
    const result = await response.json();
    console.log(`  Rows returned: ${result.rows.length}`);
    if (expectedRows !== undefined) {
      if (result.rows.length === expectedRows) {
        console.log(`  ✅ Expected ${expectedRows} rows - MATCH!`);
      } else {
        console.log(`  ❌ Expected ${expectedRows} rows but got ${result.rows.length}`);
      }
    }
    if (result.rows.length > 0 && result.rows.length <= 10) {
      console.log(`  Values:`, JSON.stringify(result.rows, null, 2));
    } else if (result.rows.length > 0) {
      console.log(`  First 5 values:`, JSON.stringify(result.rows.slice(0, 5), null, 2));
      console.log(`  Last 5 values:`, JSON.stringify(result.rows.slice(-5), null, 2));
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
  console.log('═══════════════════════════════════════════════════════════════');
  console.log('Testing DISTINCT functionality');
  console.log('═══════════════════════════════════════════════════════════════');
  
  await testQuery('All Relationship Types (DISTINCT)', 'MATCH ()-[r]->() RETURN DISTINCT type(r) AS type', 1);
  await testQuery('All Labels (DISTINCT)', 'MATCH (n) RETURN DISTINCT labels(n) AS labels', 19);
  
  // Test without DISTINCT for comparison
  await testQuery('All Relationship Types (WITHOUT DISTINCT)', 'MATCH ()-[r]->() RETURN type(r) AS type');
  
  console.log('\n═══════════════════════════════════════════════════════════════');
  console.log('Done!');
}

main().catch(console.error);

