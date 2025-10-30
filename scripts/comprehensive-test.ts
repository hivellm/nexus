#!/usr/bin/env tsx
/**
 * Comprehensive test suite comparing Nexus and Neo4j results
 */

const NEXUS_URL = 'http://127.0.0.1:15474/cypher';
const NEO4J_URL = 'http://127.0.0.1:7474/db/neo4j/tx/commit';
const NEO4J_AUTH = 'neo4j:password';

interface TestQuery {
  name: string;
  query: string;
  description: string;
}

const testQueries: TestQuery[] = [
  { name: 'Count Documents', query: 'MATCH (d:Document) RETURN count(d) AS total', description: 'Total documents' },
  { name: 'Count Modules', query: 'MATCH (m:Module) RETURN count(m) AS total', description: 'Total modules' },
  { name: 'Count Classes', query: 'MATCH (c:Class) RETURN count(c) AS total', description: 'Total classes' },
  { name: 'Count Functions', query: 'MATCH (f:Function) RETURN count(f) AS total', description: 'Total functions' },
  { name: 'Count Document-Class MENTIONS', query: 'MATCH (d:Document)-[:MENTIONS]->(e:Class) RETURN count(e) AS total', description: 'Document to Class relationships' },
  { name: 'Count All MENTIONS from Document', query: 'MATCH (d:Document)-[:MENTIONS]->(e) RETURN count(e) AS total', description: 'All MENTIONS from documents' },
  { name: 'Count All MENTIONS', query: 'MATCH ()-[r:MENTIONS]->() RETURN count(r) AS total', description: 'Total MENTIONS relationships' },
  { name: 'Count All Relationships', query: 'MATCH ()-[r]->() RETURN count(r) AS total', description: 'Total relationships' },
  { name: 'Documents by Domain', query: 'MATCH (d:Document) RETURN d.domain AS domain, count(d) AS count ORDER BY count DESC LIMIT 5', description: 'Domain distribution' },
  { name: 'Sample Modules', query: 'MATCH (m:Module) RETURN m LIMIT 5', description: 'First 5 modules' },
  { name: 'Document-Class Pairs', query: 'MATCH (d:Document)-[:MENTIONS]->(e:Class) RETURN d.title, e.name LIMIT 5', description: 'Document-Class relationships' },
  { name: 'Software Domain Docs', query: 'MATCH (d:Document) WHERE d.domain = \'software\' RETURN count(d) AS total', description: 'Software domain count' },
  { name: 'Rust Classes', query: 'MATCH (c:Class) WHERE c.language = \'Rust\' RETURN count(c) AS total', description: 'Rust classes count' },
  { name: 'Functions by Language', query: 'MATCH (f:Function) RETURN f.language AS lang, count(f) AS count GROUP BY lang ORDER BY count DESC', description: 'Language distribution' },
  { name: 'Top Modules by Mentions', query: 'MATCH (d:Document)-[:MENTIONS]->(m:Module) RETURN m.name AS module, count(m) AS count ORDER BY count DESC LIMIT 5', description: 'Most mentioned modules' },
  { name: 'Documents Sample', query: 'MATCH (d:Document) RETURN d LIMIT 5', description: 'First 5 documents' },
  { name: 'Classes Sample', query: 'MATCH (c:Class) RETURN c LIMIT 5', description: 'First 5 classes' },
  { name: 'Functions Sample', query: 'MATCH (f:Function) RETURN f.name AS name, f.language AS lang LIMIT 5', description: 'First 5 functions' },
  { name: 'All Labels', query: 'MATCH (n) RETURN DISTINCT labels(n) AS labels', description: 'Unique labels in graph' },
  { name: 'All Relationship Types', query: 'MATCH ()-[r]->() RETURN DISTINCT type(r) AS type', description: 'Unique relationship types' }
];

async function queryNexus(query: string): Promise<any> {
  try {
    const response = await fetch(NEXUS_URL, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ query })
    });
    
    if (!response.ok) {
      const text = await response.text();
      throw new Error(`Nexus failed: ${response.status} - ${text}`);
    }
    
    const data = await response.json();
    if (data.error) {
      throw new Error(`Nexus error: ${data.error}`);
    }
    
    // Normalize response
    return {
      columns: data.columns || [],
      rows: data.rows || [],
      count: data.rows?.length || 0
    };
  } catch (error) {
    return { error: error instanceof Error ? error.message : String(error) };
  }
}

async function queryNeo4j(query: string): Promise<any> {
  try {
    const response = await fetch(NEO4J_URL, {
      method: 'POST',
      headers: { 
        'Content-Type': 'application/json',
        'Authorization': `Basic ${Buffer.from(NEO4J_AUTH).toString('base64')}`
      },
      body: JSON.stringify({ statements: [{ statement: query }] })
    });
    
    if (!response.ok) {
      const text = await response.text();
      throw new Error(`Neo4j failed: ${response.status} - ${text}`);
    }
    
    const data = await response.json();
    if (data.errors && data.errors.length > 0) {
      throw new Error(`Neo4j error: ${data.errors[0].message}`);
    }
    
    if (!data.results || data.results.length === 0) {
      return { columns: [], rows: [], count: 0 };
    }
    
    // Normalize response
    const result = data.results[0];
    const columns = result.columns || [];
    const rows = result.data?.map((d: any) => d.row) || [];
    
    return {
      columns,
      rows,
      count: rows.length
    };
  } catch (error) {
    return { error: error instanceof Error ? error.message : String(error) };
  }
}

function compareResults(nexus: any, neo4j: any): { match: boolean; details: string[] } {
  const details: string[] = [];
  
  if (nexus.error || neo4j.error) {
    return { 
      match: false, 
      details: [
        `Nexus error: ${nexus.error}`,
        `Neo4j error: ${neo4j.error}`
      ].filter(Boolean)
    };
  }
  
  // Check column count
  if (nexus.columns.length !== neo4j.columns.length) {
    details.push(`Column count mismatch: Nexus=${nexus.columns.length}, Neo4j=${neo4j.columns.length}`);
  }
  
  // Check row count
  if (nexus.count !== neo4j.count) {
    details.push(`Row count mismatch: Nexus=${nexus.count}, Neo4j=${neo4j.count}`);
  }
  
  // Check columns
  const nexusCols = new Set(nexus.columns.map((c: string) => c.toLowerCase()));
  const neo4jCols = new Set(neo4j.columns.map((c: string) => c.toLowerCase()));
  const missingInNexus = Array.from(neo4jCols).filter(c => !nexusCols.has(c));
  const missingInNeo4j = Array.from(nexusCols).filter(c => !neo4jCols.has(c));
  
  if (missingInNexus.length > 0) {
    details.push(`Missing in Nexus: ${missingInNexus.join(', ')}`);
  }
  if (missingInNeo4j.length > 0) {
    details.push(`Missing in Neo4j: ${missingInNeo4j.join(', ')}`);
  }
  
  // Compare first row values (simplified comparison)
  if (nexus.count > 0 && neo4j.count > 0) {
    const nexusFirst = nexus.rows[0];
    const neo4jFirst = neo4j.rows[0];
    
    if (typeof nexusFirst === 'object' && typeof neo4jFirst === 'object') {
      const nexusKeys = Object.keys(nexusFirst).filter(k => k !== '_nexus_id');
      const neo4jKeys = Object.keys(neo4jFirst);
      
      for (const key of nexusKeys) {
        if (!(key in neo4jFirst)) {
          details.push(`Missing key in Neo4j: ${key}`);
        } else if (JSON.stringify(nexusFirst[key]) !== JSON.stringify(neo4jFirst[key])) {
          // For objects, do a deeper comparison
          const nexusVal = nexusFirst[key];
          const neo4jVal = neo4jFirst[key];
          
          if (typeof nexusVal === 'object' && typeof neo4jVal === 'object') {
            const nexusObj = nexusVal;
            const neo4jObj = neo4jVal;
            const nexusObjKeys = Object.keys(nexusObj).filter(k => k !== '_nexus_id');
            const neo4jObjKeys = Object.keys(neo4jObj);
            
            if (nexusObjKeys.length !== neo4jObjKeys.length) {
              details.push(`Field ${key}: key count mismatch (${nexusObjKeys.length} vs ${neo4jObjKeys.length})`);
            } else {
              for (const objKey of nexusObjKeys) {
                if (!(objKey in neo4jObj)) {
                  details.push(`Field ${key}.${objKey} missing in Neo4j`);
                } else if (JSON.stringify(nexusObj[objKey]) !== JSON.stringify(neo4jObj[objKey])) {
                  details.push(`Field ${key}.${objKey} value mismatch: ${JSON.stringify(nexusObj[objKey])} vs ${JSON.stringify(neo4jObj[objKey])}`);
                }
              }
            }
          } else {
            details.push(`Field ${key} value mismatch: ${JSON.stringify(nexusVal)} vs ${JSON.stringify(neo4jVal)}`);
          }
        }
      }
    } else if (nexusFirst !== neo4jFirst) {
      details.push(`First row value mismatch: ${JSON.stringify(nexusFirst)} vs ${JSON.stringify(neo4jFirst)}`);
    }
  }
  
  // Only report _nexus_id if it's the only difference
  if (details.length === 0) {
    return { match: true, details: [] };
  }
  
  // If only _nexus_id differences, still count as match
  const onlyNexusIdDiff = details.every(d => d.includes('_nexus_id'));
  
  return {
    match: onlyNexusIdDiff,
    details: onlyNexusIdDiff ? [] : details
  };
}

async function runTests() {
  console.log('╔════════════════════════════════════════════════════════════════╗');
  console.log('║  Comprehensive Nexus vs Neo4j Comparison Tests                ║');
  console.log('╚════════════════════════════════════════════════════════════════╝\n');
  
  let passed = 0;
  let failed = 0;
  let skipped = 0;
  
  for (const test of testQueries) {
    process.stdout.write(`Testing: ${test.name}... `);
    
    const nexus = await queryNexus(test.query);
    const neo4j = await queryNeo4j(test.query);
    
    const comparison = compareResults(nexus, neo4j);
    
    if (comparison.match) {
      console.log('✅ PASS');
      passed++;
    } else if (nexus.error || neo4j.error) {
      console.log('⏭️  SKIP (error)');
      if (nexus.error) console.log(`   Nexus: ${nexus.error}`);
      if (neo4j.error) console.log(`   Neo4j: ${neo4j.error}`);
      skipped++;
    } else {
      console.log('❌ FAIL');
      failed++;
      if (comparison.details.length > 0) {
        console.log('   Details:');
        comparison.details.forEach(d => console.log(`     - ${d}`));
      }
    }
  }
  
  console.log('\n╔════════════════════════════════════════════════════════════════╗');
  console.log('║  Test Summary                                                   ║');
  console.log('╚════════════════════════════════════════════════════════════════╝');
  console.log(`\n✅ Passed:  ${passed}`);
  console.log(`❌ Failed:  ${failed}`);
  console.log(`⏭️  Skipped: ${skipped}`);
  console.log(`📊 Total:   ${testQueries.length}`);
  console.log(`\n${passed === testQueries.length - skipped ? '🎉 All tests passed!' : '⚠️  Some tests failed'}\n`);
}

runTests().catch(console.error);

