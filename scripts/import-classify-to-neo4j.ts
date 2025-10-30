#!/usr/bin/env tsx
// @ts-nocheck
/**
 * Import classify cache results to Neo4j
 * Mirrors the Nexus import script but targets Neo4j's HTTP API
 */

import { readFile, readdir, stat } from 'fs/promises';
import { join } from 'path';

const NEO4J_URL = process.env.NEO4J_URL || 'http://127.0.0.1:7474/db/neo4j/tx/commit';
const NEO4J_AUTH = process.env.NEO4J_AUTH || 'neo4j:password';
const CLASSIFY_CACHE_DIR =
  process.env.CLASSIFY_CACHE_DIR || join(process.cwd(), '..', 'classify', '.classify-cache');

interface ClassifyResult {
  file?: string;
  classification?: {
    template?: string;
    confidence?: number;
    domain?: string;
    docType?: string;
  };
  graphStructure?: {
    cypher?: string;
    entities?: Array<{
      type: string;
      properties: Record<string, any>;
    }>;
  };
  cacheInfo?: {
    hash?: string;
    cached?: boolean;
  };
  fulltextMetadata?: {
    keywords?: string[];
    summary?: string;
  };
}

interface CacheEntry {
  hash: string;
  result: ClassifyResult;
  cachedAt: number;
  accessedAt: number;
  accessCount: number;
}

async function executeNeo4j(query: string): Promise<void> {
  const response = await fetch(NEO4J_URL, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Basic ${Buffer.from(NEO4J_AUTH).toString('base64')}`,
    },
    body: JSON.stringify({ statements: [{ statement: query }] }),
  });

  if (!response.ok) {
    const errorText = await response.text();
    throw new Error(`Neo4j query failed: ${response.status} - ${errorText}`);
  }

  const payload = await response.json();
  if (payload.errors && payload.errors.length > 0) {
    throw new Error(`Neo4j response error: ${JSON.stringify(payload.errors)}`);
  }
}

async function readCacheFiles(cacheDir: string): Promise<Array<{ path: string; entry: CacheEntry }>> {
  const cacheEntries: Array<{ path: string; entry: CacheEntry }> = [];

  try {
    const subdirs = await readdir(cacheDir);

    for (const subdir of subdirs) {
      const subdirPath = join(cacheDir, subdir);
      try {
        const subdirStat = await stat(subdirPath);
        if (!subdirStat.isDirectory()) continue;

        const files = await readdir(subdirPath);
        const jsonFiles = files.filter((f) => f.endsWith('.json'));

        for (const file of jsonFiles) {
          const filePath = join(subdirPath, file);
          try {
            const content = await readFile(filePath, 'utf-8');
            const entry: CacheEntry = JSON.parse(content);
            cacheEntries.push({ path: filePath, entry });
          } catch (error) {
            console.warn(`⚠️  Failed to read cache file ${file}: ${error}`);
          }
        }
      } catch {
        continue;
      }
    }
  } catch (error) {
    console.error(`❌ Failed to read cache directory: ${error}`);
  }

  return cacheEntries;
}

async function importResult(result: ClassifyResult, sourceFile: string): Promise<boolean> {
  if (!result.graphStructure?.cypher) {
    console.warn(`   ⚠️  ${sourceFile} does not contain a Cypher payload`);
    return false;
  }

  const fileHash = result.cacheInfo?.hash || sourceFile.substring(0, 32);
  let cypher = result.graphStructure.cypher;

  cypher = cypher.replace(
    /CREATE \(doc:Document \{([\s\S]*?)\}\)/,
    (_match, props) => {
      let cleanProps = props.trim();
      cleanProps = cleanProps.replace(/,\s*$/, '');
      return `MERGE (doc:Document { file_hash: "${fileHash}", ${cleanProps} })`;
    }
  );

  try {
    await executeNeo4j(cypher);
    return true;
  } catch (error) {
    console.error(`      ❌ Neo4j Cypher error: ${error}`);
    console.error(`      Query (first 200 chars): ${cypher.substring(0, 200)}...`);
    return false;
  }
}

async function runTestQueries(): Promise<void> {
  console.log('\n📊 Running test queries (Neo4j) ...\n');

  const queries = [
    {
      name: 'Count documents',
      query: 'MATCH (d:Document) RETURN count(d) AS count',
    },
    {
      name: 'Sample documents',
      query: 'MATCH (d:Document) RETURN d LIMIT 5',
    },
    {
      name: 'Document-Class mentions',
      query:
        'MATCH (d:Document)-[:MENTIONS]->(e:Class) RETURN d.title AS doc, e.name AS class LIMIT 5',
    },
  ];

  for (const test of queries) {
    try {
      const response = await fetch(NEO4J_URL, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          Authorization: `Basic ${Buffer.from(NEO4J_AUTH).toString('base64')}`,
        },
        body: JSON.stringify({ statements: [{ statement: test.query }] }),
      });

      if (!response.ok) {
        throw new Error(`HTTP ${response.status}`);
      }

      const payload = await response.json();
      if (payload.errors && payload.errors.length > 0) {
        throw new Error(JSON.stringify(payload.errors));
      }

      const result = payload.results[0];
      console.log(`✅ ${test.name}`);
      console.log(`   Columns: ${JSON.stringify(result.columns)}`);
      console.log(`   Rows: ${result.data.length}`);
      if (result.data.length > 0) {
        console.log(
          '   Example:',
          JSON.stringify(result.data[0].row, null, 2).slice(0, 200)
        );
      }
    } catch (error) {
      console.error(`❌ ${test.name} failed: ${error}`);
    }
    console.log('');
  }
}

async function main(): Promise<void> {
  console.log('╔═══════════════════════════════════════════════════╗');
  console.log('║  Import Classify Cache to Neo4j                  ║');
  console.log('╚═══════════════════════════════════════════════════╝');
  console.log('');

  console.log(`📁 Reading cache from: ${CLASSIFY_CACHE_DIR}`);
  const entries = await readCacheFiles(CLASSIFY_CACHE_DIR);
  console.log(`Found ${entries.length} cache entries\n`);

  let imported = 0;
  let failed = 0;

  for (const { entry, path } of entries) {
    const sourceFile = entry.result.file || path;
    console.log(`📄 Importing: ${sourceFile}`);
    const success = await importResult(entry.result, sourceFile);
    if (success) {
      console.log('   ✅ Imported successfully\n');
      imported += 1;
    } else {
      console.log('   ❌ Failed\n');
      failed += 1;
    }
  }

  console.log('╔═══════════════════════════════════════════════════╗');
  console.log('║  Import Summary                                   ║');
  console.log('╚═══════════════════════════════════════════════════╝');
  console.log(`Total cache entries: ${entries.length}`);
  console.log(`✅ Imported: ${imported}`);
  console.log(`❌ Failed: ${failed}`);

  await runTestQueries();
}

main().catch((error) => {
  console.error('Unexpected error:', error);
  process.exit(1);
});

