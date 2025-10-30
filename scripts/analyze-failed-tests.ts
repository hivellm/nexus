#!/usr/bin/env tsx

const NEXUS_URL = 'http://127.0.0.1:15474/cypher';
const NEO4J_URL = 'http://127.0.0.1:7474/db/neo4j/tx/commit';
const NEO4J_AUTH = 'neo4j:password';

async function queryNexus(query: string) {
  try {
    const response = await fetch(NEXUS_URL, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ query })
    });
    const result = await response.json();
    return { rows: result.rows || [], error: result.error };
  } catch (error) {
    return { rows: [], error: error.message };
  }
}

async function queryNeo4j(query: string) {
  try {
    const response = await fetch(NEO4J_URL, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Authorization': 'Basic ' + Buffer.from(NEO4J_AUTH).toString('base64')
      },
      body: JSON.stringify({ statements: [{ statement: query }] })
    });
    const result = await response.json();
    if (result.errors && result.errors.length > 0) {
      return { rows: [], error: result.errors[0].message };
    }
    return { rows: result.results[0]?.data?.map((d: any) => d.row) || [], error: null };
  } catch (error) {
    return { rows: [], error: error.message };
  }
}

async function analyzeTest(name: string, query: string) {
  console.log(`\n${'='.repeat(70)}`);
  console.log(`${name}`);
  console.log(`Query: ${query}`);
  console.log(`${'='.repeat(70)}`);
  
  const nexus = await queryNexus(query);
  const neo4j = await queryNeo4j(query);
  
  console.log(`\nNexus:`);
  console.log(`  Rows: ${nexus.rows.length}`);
  if (nexus.rows.length > 0) {
    console.log(`  First row:`, JSON.stringify(nexus.rows[0], null, 2));
    if (nexus.rows.length > 1) {
      console.log(`  Second row:`, JSON.stringify(nexus.rows[1], null, 2));
    }
  }
  if (nexus.error) console.log(`  Error: ${nexus.error}`);
  
  console.log(`\nNeo4j:`);
  console.log(`  Rows: ${neo4j.rows.length}`);
  if (neo4j.rows.length > 0) {
    console.log(`  First row:`, JSON.stringify(neo4j.rows[0], null, 2));
    if (neo4j.rows.length > 1) {
      console.log(`  Second row:`, JSON.stringify(neo4j.rows[1], null, 2));
    }
  }
  if (neo4j.error) console.log(`  Error: ${neo4j.error}`);
  
  if (nexus.rows.length !== neo4j.rows.length) {
    console.log(`\n⚠️  Row count mismatch: Nexus=${nexus.rows.length}, Neo4j=${neo4j.rows.length}`);
  } else if (nexus.rows.length > 0 && JSON.stringify(nexus.rows[0]) !== JSON.stringify(neo4j.rows[0])) {
    console.log(`\n⚠️  Value mismatch in first row`);
  } else if (nexus.rows.length === neo4j.rows.length && nexus.rows.length > 0) {
    console.log(`\n✅ Results match!`);
  }
}

async function main() {
  console.log('═══════════════════════════════════════════════════════════════');
  console.log('Análise Detalhada dos Testes que Falharam');
  console.log('═══════════════════════════════════════════════════════════════');
  
  // Test 1: Count differences
  await analyzeTest('1. Count Documents', 'MATCH (d:Document) RETURN count(d) AS total');
  await analyzeTest('2. Count Modules', 'MATCH (m:Module) RETURN count(m) AS total');
  await analyzeTest('3. Count Classes', 'MATCH (c:Class) RETURN count(c) AS total');
  
  // Test 2: Relationship counts
  await analyzeTest('4. Count All MENTIONS', 'MATCH ()-[r:MENTIONS]->() RETURN count(r) AS total');
  
  // Test 3: Domain distribution
  await analyzeTest('5. Documents by Domain', 'MATCH (d:Document) RETURN d.domain AS domain, count(d) AS count ORDER BY count DESC LIMIT 5');
  
  // Test 4: Document-Class pairs
  await analyzeTest('6. Document-Class Pairs', 'MATCH (d:Document)-[:MENTIONS]->(e:Class) RETURN d.title, e.name LIMIT 5');
  
  // Test 5: Top modules
  await analyzeTest('7. Top Modules by Mentions', 'MATCH (d:Document)-[:MENTIONS]->(m:Module) RETURN m.name AS module, count(m) AS count ORDER BY count DESC LIMIT 5');
  
  console.log('\n═══════════════════════════════════════════════════════════════');
  console.log('Análise Completa!');
  console.log('═══════════════════════════════════════════════════════════════');
}

main().catch(console.error);

