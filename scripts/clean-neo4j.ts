#!/usr/bin/env tsx
// @ts-nocheck
/**
 * Clean Neo4j database and reimport classify cache data
 */

const NEO4J_URL = process.env.NEO4J_URL || 'http://127.0.0.1:7474/db/neo4j/tx/commit';
const NEO4J_AUTH = process.env.NEO4J_AUTH || 'neo4j:password';

async function executeNeo4j(query: string): Promise<any> {
  const response = await fetch(NEO4J_URL, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'Authorization': 'Basic ' + Buffer.from(NEO4J_AUTH).toString('base64')
    },
    body: JSON.stringify({
      statements: [{ statement: query }]
    })
  });

  if (!response.ok) {
    const errorText = await response.text();
    throw new Error(`Neo4j query failed: ${response.status} - ${errorText}`);
  }

  const result = await response.json();
  
  if (result.errors && result.errors.length > 0) {
    throw new Error(`Neo4j error: ${result.errors[0].message}`);
  }
  
  return result;
}

async function cleanNeo4j() {
  console.log('🗑️  Limpando base do Neo4j...\n');
  
  try {
    // Delete all relationships first
    console.log('  Deletando relationships...');
    await executeNeo4j('MATCH ()-[r]->() DELETE r');
    console.log('  ✅ Relationships deletados');
    
    // Then delete all nodes
    console.log('  Deletando nodes...');
    await executeNeo4j('MATCH (n) DELETE n');
    console.log('  ✅ Nodes deletados');
    
    // Verify cleanup
    const verify = await executeNeo4j('MATCH (n) RETURN count(n) AS total');
    const nodeCount = verify.results[0]?.data[0]?.row[0] || 0;
    
    const relVerify = await executeNeo4j('MATCH ()-[r]->() RETURN count(r) AS total');
    const relCount = relVerify.results[0]?.data[0]?.row[0] || 0;
    
    if (nodeCount === 0 && relCount === 0) {
      console.log('\n✅ Base do Neo4j limpa com sucesso!');
      console.log(`   Nodes: ${nodeCount}`);
      console.log(`   Relationships: ${relCount}\n`);
    } else {
      console.log(`\n⚠️  Ainda há dados na base:`);
      console.log(`   Nodes: ${nodeCount}`);
      console.log(`   Relationships: ${relCount}\n`);
    }
  } catch (error) {
    console.error(`\n❌ Erro ao limpar Neo4j: ${error.message}`);
    throw error;
  }
}

async function main() {
  console.log('═══════════════════════════════════════════════════════════════');
  console.log('Limpeza e Reimportação do Neo4j');
  console.log('═══════════════════════════════════════════════════════════════\n');
  
  // Clean Neo4j
  await cleanNeo4j();
  
  // Now import data
  console.log('📥 Importando dados do classify...\n');
  console.log('   Execute: npx tsx scripts/import-classify-to-neo4j.ts\n');
  
  console.log('═══════════════════════════════════════════════════════════════');
  console.log('Limpeza concluída! Execute o script de importação agora.');
  console.log('═══════════════════════════════════════════════════════════════');
}

main().catch(error => {
  console.error('Fatal error:', error);
  process.exit(1);
});

