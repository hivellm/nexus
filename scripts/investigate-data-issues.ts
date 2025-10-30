#!/usr/bin/env tsx

const NEXUS_URL = 'http://127.0.0.1:15474/cypher';

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

async function analyze() {
  console.log('═══════════════════════════════════════════════════════════════');
  console.log('Investigando Problemas nos Dados');
  console.log('═══════════════════════════════════════════════════════════════\n');
  
  // 1. Verificar documentos com/sem domain
  console.log('1. Verificando propriedade domain nos Documents:');
  const domainCheck = await queryNexus('MATCH (d:Document) RETURN count(d) AS total, count(d.domain) AS with_domain');
  console.log('  Resultado:', JSON.stringify(domainCheck.rows[0], null, 2));
  
  // 2. Verificar documentos sem domain
  console.log('\n2. Contando Documents sem domain:');
  const nullDomain = await queryNexus('MATCH (d:Document) WHERE d.domain IS NULL RETURN count(d) AS null_count');
  console.log('  Resultado:', JSON.stringify(nullDomain.rows[0], null, 2));
  
  // 3. Distribuição de labels
  console.log('\n3. Distribuição de labels (top 10):');
  const labelsDist = await queryNexus('MATCH (n) RETURN DISTINCT labels(n) AS labels, count(*) AS count ORDER BY count DESC LIMIT 10');
  labelsDist.rows.forEach(row => {
    console.log(`  ${JSON.stringify(row[0])}: ${row[1]}`);
  });
  
  // 4. Distribuição de domains
  console.log('\n4. Distribuição de domains:');
  const domainsDist = await queryNexus('MATCH (d:Document) RETURN d.domain AS domain, count(d) AS count ORDER BY count DESC');
  domainsDist.rows.forEach(row => {
    console.log(`  domain=${row[0] === null ? 'NULL' : row[0]}: count=${row[1]}`);
  });
  
  // 5. Verificar se há documentos duplicados ou com labels incorretos
  console.log('\n5. Verificando estrutura dos Documents:');
  const docSample = await queryNexus('MATCH (d:Document) RETURN d LIMIT 3');
  docSample.rows.forEach((row, idx) => {
    console.log(`  Doc ${idx + 1}:`, JSON.stringify(row[0], null, 2));
  });
  
  // 6. Verificar relações Document -> Class
  console.log('\n6. Verificando relações Document -> Class:');
  const docClassRel = await queryNexus('MATCH (d:Document)-[:MENTIONS]->(c:Class) RETURN count(*) AS total');
  console.log('  Total relações Document->Class:', docClassRel.rows[0]?.[0]);
  
  // 7. Verificar relações Document -> Module
  console.log('\n7. Verificando relações Document -> Module:');
  const docModuleRel = await queryNexus('MATCH (d:Document)-[:MENTIONS]->(m:Module) RETURN count(*) AS total');
  console.log('  Total relações Document->Module:', docModuleRel.rows[0]?.[0]);
  
  // 8. Verificar quais labels existem SEM Document
  console.log('\n8. Labels de nodes que NÃO são Documents:');
  const nonDocLabels = await queryNexus('MATCH (n) WHERE NOT (n:Document) RETURN DISTINCT labels(n) AS labels, count(*) AS count ORDER BY count DESC LIMIT 10');
  nonDocLabels.rows.forEach(row => {
    console.log(`  ${JSON.stringify(row[0])}: ${row[1]}`);
  });
  
  // 9. Verificar nodes que têm label Document mas não têm propriedade domain
  console.log('\n9. Nodes com label Document mas sem domain (primeiros 5):');
  const docsWithoutDomain = await queryNexus('MATCH (d:Document) WHERE d.domain IS NULL RETURN d LIMIT 5');
  docsWithoutDomain.rows.forEach((row, idx) => {
    const node = row[0];
    const labels = node.labels || [];
    const props = Object.keys(node).filter(k => k !== 'labels' && k !== '_nexus_id');
    console.log(`  Doc ${idx + 1}: labels=${JSON.stringify(labels)}, props=${props.join(', ')}`);
    console.log(`    Sample: ${JSON.stringify(node).substring(0, 100)}...`);
  });
  
  // 10. Verificar quantos nodes têm múltiplos labels
  console.log('\n10. Verificando múltiplos labels nos nodes:');
  const multiLabel = await queryNexus('MATCH (n) RETURN labels(n) AS labels, count(*) AS count ORDER BY count DESC LIMIT 10');
  multiLabel.rows.forEach(row => {
    const labels = row[0];
    console.log(`  ${JSON.stringify(labels)}: ${row[1]} nodes`);
  });
  
  console.log('\n═══════════════════════════════════════════════════════════════');
  console.log('Análise Completa!');
  console.log('═══════════════════════════════════════════════════════════════');
}

analyze().catch(console.error);

