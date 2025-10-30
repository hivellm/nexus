#!/usr/bin/env tsx
/**
 * Populate Nexus with test data for comparison testing
 */

const NEXUS_URL = 'http://127.0.0.1:15474/cypher';

async function query(cypher: string) {
  const res = await fetch(NEXUS_URL, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ query: cypher })
  });
  return await res.json();
}

async function main() {
  console.log('📊 Populating test data...\n');
  
  // Create Documents
  for (let i = 0; i < 10; i++) {
    await query(`CREATE (d:Document {title: "Doc${i}", domain: "software"})`);
  }
  console.log('✅ Created 10 Documents');
  
  // Create Classes
  for (let i = 0; i < 10; i++) {
    await query(`CREATE (c:Class {name: "Class${i}", language: "Rust"})`);
  }
  console.log('✅ Created 10 Classes');
  
  // Create Modules
  for (let i = 0; i < 10; i++) {
    await query(`CREATE (m:Module {name: "Module${i}"})`);
  }
  console.log('✅ Created 10 Modules');
  
  // Create Functions
  for (let i = 0; i < 5; i++) {
    await query(`CREATE (f:Function {name: "func${i}", language: "Rust"})`);
  }
  for (let i = 5; i < 10; i++) {
    await query(`CREATE (f:Function {name: "func${i}", language: "Python"})`);
  }
  console.log('✅ Created 10 Functions');
  
  // Create relationships
  console.log('\n📝 Creating relationships...');
  for (let i = 0; i < 10; i++) {
    // Each document mentions a class
    await query(`MATCH (d:Document {title: "Doc${i}"}), (c:Class {name: "Class${i}"}) CREATE (d)-[:MENTIONS]->(c)`);
    // Each document mentions a module
    await query(`MATCH (d:Document {title: "Doc${i}"}), (m:Module {name: "Module${i}"}) CREATE (d)-[:MENTIONS]->(m)`);
  }
  
  // Some documents mention functions
  for (let i = 0; i < 10; i++) {
    await query(`MATCH (d:Document {title: "Doc${i}"}), (f:Function {name: "func${i}"}) CREATE (d)-[:MENTIONS]->(f)`);
  }
  console.log('✅ Created 30 MENTIONS relationships');
  
  // Verify counts
  console.log('\n📊 Verification:');
  const docCount = await query('MATCH (d:Document) RETURN count(d) AS total');
  console.log(`  Documents: ${docCount.rows[0][0]}`);
  
  const classCount = await query('MATCH (c:Class) RETURN count(c) AS total');
  console.log(`  Classes: ${classCount.rows[0][0]}`);
  
  const relCount = await query('MATCH ()-[r:MENTIONS]->() RETURN count(r) AS total');
  console.log(`  MENTIONS: ${relCount.rows[0][0]}`);
  
  console.log('\n✨ Done!');
}

main().catch(console.error);

