#!/usr/bin/env tsx
/**
 * Import classify cache results to Nexus
 * Similar to Neo4j integration but using Nexus /cypher endpoint
 */

import { readFile, readdir, stat } from 'fs/promises';
import { join } from 'path';

const NEXUS_URL = process.env.NEXUS_URL || 'http://127.0.0.1:15474';
const CLASSIFY_CACHE_DIR = process.env.CLASSIFY_CACHE_DIR || 
  join(process.cwd(), '..', '..', 'classify', '.classify-cache');

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

/**
 * Execute Cypher query on Nexus
 */
async function executeCypher(query: string): Promise<any> {
  try {
    const response = await fetch(`${NEXUS_URL}/cypher`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({ query }),
    });

    if (!response.ok) {
      const errorText = await response.text();
      throw new Error(`Nexus query failed: ${response.status} - ${errorText}`);
    }

    const result = await response.json();
    
    // Log if there's an error in the response
    if (result.error) {
      console.error(`      ⚠️  Cypher error in response: ${result.error}`);
      console.error(`      Query (first 200 chars): ${query.substring(0, 200)}...`);
    }
    
    return result;
  } catch (error) {
    console.error(`      ❌ Failed to execute Cypher: ${error}`);
    console.error(`      Query (first 200 chars): ${query.substring(0, 200)}...`);
    throw error;
  }
}

/**
 * Read all cache files from .classify-cache directory
 */
async function readCacheFiles(cacheDir: string): Promise<Array<{ path: string; entry: CacheEntry }>> {
  const cacheEntries: Array<{ path: string; entry: CacheEntry }> = [];
  
  try {
    // Read subdirectories (first 2 chars of hash)
    const subdirs = await readdir(cacheDir);
    
    for (const subdir of subdirs) {
      const subdirPath = join(cacheDir, subdir);
      try {
        const subdirStat = await stat(subdirPath);
        if (!subdirStat.isDirectory()) continue;
        
        // Read JSON files in subdirectory
        const files = await readdir(subdirPath);
        const jsonFiles = files.filter(f => f.endsWith('.json'));
        
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
      } catch (error) {
        // Skip if not a directory or access error
        continue;
      }
    }
  } catch (error) {
    console.error(`❌ Failed to read cache directory: ${error}`);
  }
  
  return cacheEntries;
}

/**
 * Import single classify result to Nexus
 */
async function importResult(
  result: ClassifyResult,
  sourceFile: string
): Promise<{ success: boolean; error?: string }> {
  if (!result.graphStructure?.cypher) {
    return { success: false, error: 'No Cypher in result' };
  }

  // Get hash from cacheInfo or generate one
  const fileHash = result.cacheInfo?.hash || sourceFile.substring(0, 32);
  let cypher = result.graphStructure.cypher;

  // Replace CREATE with MERGE for Document nodes (avoid duplicates)
  // NOTE: Nexus parser doesn't support "doc += {...}" syntax, so we include all properties in MERGE pattern
  // Pattern matches: CREATE (doc:Document { ... }) where ... can span multiple lines
  // Use non-greedy match to capture everything between braces, including newlines
  cypher = cypher.replace(
    /CREATE \(doc:Document \{([\s\S]*?)\}\)/,
    (match, props) => {
      // Clean up properties: trim whitespace, remove trailing commas
      let cleanProps = props.trim();
      // Remove trailing comma if present
      cleanProps = cleanProps.replace(/,\s*$/, '');
      // Include file_hash as first property for uniqueness in MERGE pattern
      return `MERGE (doc:Document { file_hash: "${fileHash}", ${cleanProps} })`;
    }
  );

  try {
    const response = await executeCypher(cypher);
    
    // Check if response has error
    if (response.error) {
      return { 
        success: false, 
        error: `Cypher execution error: ${response.error}` 
      };
    }
    
    // Log successful execution
    console.log(`      ✅ Cypher executed successfully (${response.execution_time_ms}ms)`);
    
    return { success: true };
  } catch (error) {
    return { 
      success: false, 
      error: error instanceof Error ? error.message : String(error) 
    };
  }
}

/**
 * Test queries similar to Neo4j usage
 */
async function runTestQueries(): Promise<void> {
  console.log('\n📊 Running test queries (similar to Neo4j)...\n');

  const tests = [
    {
      name: 'Find all documents',
      query: 'MATCH (d:Document) RETURN d LIMIT 10',
    },
    {
      name: 'Count documents by domain',
      query: 'MATCH (d:Document) RETURN d.domain AS domain, count(d) AS count',
    },
    {
      name: 'Find documents mentioning PostgreSQL',
      query: `MATCH (doc:Document)-[:MENTIONS]->(entity)
              WHERE entity.name = "PostgreSQL" OR entity.name = "pg"
              RETURN doc.title, entity.name
              LIMIT 10`,
    },
    {
      name: 'Find all modules',
      query: 'MATCH (m:Module) RETURN m.name AS name, m.imports AS imports LIMIT 10',
    },
    {
      name: 'Find all classes',
      query: 'MATCH (c:Class) RETURN c.name AS name, c.description AS description LIMIT 10',
    },
    {
      name: 'Find document-entity relationships',
      query: `MATCH (doc:Document)-[r:MENTIONS]->(entity)
              RETURN doc.title AS document, entity.type AS entity_type, entity.name AS entity_name
              LIMIT 10`,
    },
    {
      name: 'Find documents in software domain',
      query: `MATCH (doc:Document {domain: "software"})
              RETURN doc.title AS title, doc.doc_type AS doc_type
              LIMIT 10`,
    },
    {
      name: 'Count entities by type',
      query: `MATCH (e)
              WHERE e:Module OR e:Class OR e:Function OR e:Database
              RETURN labels(e)[0] AS type, count(e) AS count`,
    },
  ];

  for (const test of tests) {
    try {
      const start = Date.now();
      const result = await executeCypher(test.query);
      const duration = Date.now() - start;

      console.log(`✅ ${test.name}`);
      console.log(`   Time: ${duration}ms`);
      if (result.rows && result.rows.length > 0) {
        console.log(`   Rows: ${result.rows.length}`);
        // Show first row as example
        if (result.rows[0]) {
          console.log(`   Example: ${JSON.stringify(result.rows[0]).substring(0, 100)}...`);
        }
      } else {
        console.log(`   Rows: 0`);
      }
      console.log();
    } catch (error) {
      console.log(`❌ ${test.name}`);
      console.log(`   Error: ${error instanceof Error ? error.message : String(error)}\n`);
    }
  }
}

/**
 * Main function
 */
async function main() {
  console.log('╔═══════════════════════════════════════════════════╗');
  console.log('║  Import Classify Cache to Nexus                  ║');
  console.log('╚═══════════════════════════════════════════════════╝\n');

  // Check Nexus connection
  try {
    const healthCheck = await fetch(`${NEXUS_URL}/health`);
    if (!healthCheck.ok) {
      throw new Error(`Nexus health check failed: ${healthCheck.status}`);
    }
    console.log('✅ Connected to Nexus\n');
  } catch (error) {
    console.error(`❌ Failed to connect to Nexus: ${error}`);
    console.error(`   Make sure Nexus is running on ${NEXUS_URL}`);
    process.exit(1);
  }

  // Read all cache files from .classify-cache directory
  console.log(`📁 Reading cache from: ${CLASSIFY_CACHE_DIR}\n`);
  
  let cacheEntries: Array<{ path: string; entry: CacheEntry }>;
  try {
    cacheEntries = await readCacheFiles(CLASSIFY_CACHE_DIR);
  } catch (error) {
    console.error(`❌ Failed to read cache directory: ${error}`);
    process.exit(1);
  }

  console.log(`Found ${cacheEntries.length} cache entries\n`);

  // Import each cache entry
  let imported = 0;
  let failed = 0;
  const errors: Array<{ file: string; error: string }> = [];

  for (const { path: cachePath, entry } of cacheEntries) {
    try {
      const result = entry.result;
      const fileName = cachePath.split(/[/\\]/).pop() || 'unknown';
      const sourceFile = result.file || entry.hash || 'unknown';
      
      console.log(`📄 Importing: ${fileName}`);

      const importResultData = await importResult(result, sourceFile);
      
      if (importResultData.success) {
        imported++;
        console.log(`   ✅ Imported successfully\n`);
      } else {
        failed++;
        errors.push({ file: fileName, error: importResultData.error || 'Unknown error' });
        console.log(`   ❌ Failed: ${importResultData.error}\n`);
      }
    } catch (error) {
      failed++;
      const fileName = cachePath.split(/[/\\]/).pop() || 'unknown';
      errors.push({ 
        file: fileName, 
        error: error instanceof Error ? error.message : String(error) 
      });
      console.log(`   ❌ Error: ${error}\n`);
    }
  }

  // Summary
  console.log('╔═══════════════════════════════════════════════════╗');
  console.log('║  Import Summary                                  ║');
  console.log('╚═══════════════════════════════════════════════════╝\n');
  console.log(`Total cache entries: ${cacheEntries.length}`);
  console.log(`✅ Imported: ${imported}`);
  console.log(`❌ Failed: ${failed}\n`);

  if (errors.length > 0) {
    console.log('Errors:');
    errors.forEach(({ file, error }) => {
      console.log(`  - ${file}: ${error}`);
    });
    console.log();
  }

  // Run test queries
  if (imported > 0) {
    await runTestQueries();
  }

  console.log('✨ Import complete!\n');
}

main().catch(console.error);
