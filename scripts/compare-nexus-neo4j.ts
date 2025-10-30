#!/usr/bin/env tsx
/**
 * Compare Nexus and Neo4j query results for a set of sample queries.
 * Prints the first few rows returned by each backend.
 */

interface QuerySpec {
  name: string;
  cypher: string;
}

const NEXUS_URL = process.env.NEXUS_URL ?? 'http://127.0.0.1:15474';
const NEO4J_URL = process.env.NEO4J_URL ?? 'http://127.0.0.1:7474/db/neo4j/tx/commit';
const NEO4J_AUTH = process.env.NEO4J_AUTH ?? 'neo4j:password';
const SAMPLE_ROWS = Number(process.env.SAMPLE_ROWS ?? '5');

const queries: QuerySpec[] = [
  {
    name: 'Documents sample',
    cypher: 'MATCH (d:Document) RETURN d LIMIT 5',
  },
  {
    name: 'Document-Class mentions',
    cypher: 'MATCH (d:Document)-[:MENTIONS]->(e:Class) RETURN d, e LIMIT 5',
  },
  {
    name: 'Modules sample',
    cypher: 'MATCH (m:Module) RETURN m LIMIT 5',
  },
  {
    name: 'Functions names',
    cypher: 'MATCH (f:Function) RETURN f.name AS name, f.language AS lang LIMIT 5',
  },
  {
    name: 'Domain distribution',
    cypher: 'MATCH (d:Document) RETURN d.domain AS domain, count(d) AS count ORDER BY count DESC LIMIT 5',
  },
];

async function callNexus(query: string) {
  const res = await fetch(`${NEXUS_URL}/cypher`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ query }),
  });
  if (!res.ok) {
    throw new Error(`Nexus error ${res.status}: ${await res.text()}`);
  }
  return res.json();
}

async function callNeo4j(query: string) {
  const res = await fetch(NEO4J_URL, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Basic ${Buffer.from(NEO4J_AUTH).toString('base64')}`,
    },
    body: JSON.stringify({ statements: [{ statement: query }] }),
  });
  if (!res.ok) {
    throw new Error(`Neo4j error ${res.status}: ${await res.text()}`);
  }
  const payload = await res.json();
  if (payload.errors?.length) {
    throw new Error(`Neo4j response error: ${JSON.stringify(payload.errors)}`);
  }
  const result = payload.results[0];
  const rows = result.data.map((entry: any) => entry.row);
  return { columns: result.columns, rows };
}

async function main() {
  console.log('=== Nexus vs Neo4j detailed comparison ===\n');
  for (const { name, cypher } of queries) {
    console.log(`## ${name}`);
    console.log(`Query: ${cypher}\n`);
    try {
      const [nexus, neo] = await Promise.all([callNexus(cypher), callNeo4j(cypher)]);
      console.log('Nexus columns:', nexus.columns);
      console.log('Nexus sample rows:');
      nexus.rows.slice(0, SAMPLE_ROWS).forEach((row: unknown) => {
        console.log('  ' + JSON.stringify(row, null, 2));
      });
      console.log('Neo4j columns:', neo.columns);
      console.log('Neo4j sample rows:');
      neo.rows.slice(0, SAMPLE_ROWS).forEach((row: unknown) => {
        console.log('  ' + JSON.stringify(row, null, 2));
      });
      console.log(
        `Row count Nexus=${nexus.rows.length}, Neo4j=${neo.rows.length}`,
      );
    } catch (error) {
      console.error('  Error executing query:', error);
    }
    console.log('\n');
  }
}

main().catch((error) => {
  console.error('Unexpected error:', error);
  process.exit(1);
});
