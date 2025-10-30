#!/usr/bin/env tsx

const NEXUS_URL = process.env.NEXUS_URL || 'http://127.0.0.1:15474';

async function test() {
  const query = {
    query: 'MATCH (d:Document) RETURN d LIMIT 1'
  };

  const response = await fetch(`${NEXUS_URL}/cypher`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(query),
  });

  const result = await response.json();
  console.log('Result:', JSON.stringify(result, null, 2));
  
  if (result.rows && result.rows.length > 0) {
    const firstRow = result.rows[0];
    console.log('\nFirst row:', JSON.stringify(firstRow, null, 2));
    if (firstRow[0]) {
      const node = firstRow[0];
      console.log('\nNode keys:', Object.keys(node));
      console.log('Node properties:', JSON.stringify(node, null, 2));
    }
  }
}

test().catch(console.error);

