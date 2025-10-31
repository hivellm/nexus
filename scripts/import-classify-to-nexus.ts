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
const LOG_FILE = process.env.LOG_FILE || join(process.cwd(), 'import-nexus.log');
const VERBOSE = process.env.VERBOSE === 'true';

// Import statistics tracking
interface ImportStats {
  totalFiles: number;
  imported: number;
  failed: number;
  skipped: number;
  nodesByType: Record<string, number>;
  relationshipsByType: Record<string, number>;
  startTime: number;
  endTime?: number;
  errors: Array<{ file: string; error: string; timestamp: number }>;
}

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
 * Log message with timestamp
 */
function log(message: string, level: 'info' | 'warn' | 'error' | 'debug' = 'info'): void {
  const timestamp = new Date().toISOString();
  const prefix = {
    info: '   ',
    warn: '⚠️ ',
    error: '❌',
    debug: '🔍',
  }[level];
  
  console.log(`[${timestamp}] ${prefix} ${message}`);
}

/**
 * Log verbose message (only if VERBOSE=true)
 */
function logVerbose(message: string): void {
  if (VERBOSE) {
    log(message, 'debug');
  }
}

/**
 * Extract entity statistics from Cypher query
 */
function extractEntityStats(cypher: string): { nodes: Record<string, number>; relationships: Record<string, number> } {
  const stats = { nodes: {} as Record<string, number>, relationships: {} as Record<string, number> };
  
  // Extract node types from CREATE/MERGE patterns
  const nodePattern = /(?:CREATE|MERGE)\s+\([^:]+:(\w+)/g;
  let match;
  while ((match = nodePattern.exec(cypher)) !== null) {
    const nodeType = match[1];
    stats.nodes[nodeType] = (stats.nodes[nodeType] || 0) + 1;
  }
  
  // Extract relationship types
  const relPattern = /-\[:(\w+)\]->/g;
  while ((match = relPattern.exec(cypher)) !== null) {
    const relType = match[1];
    stats.relationships[relType] = (stats.relationships[relType] || 0) + 1;
  }
  
  return stats;
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
  sourceFile: string,
  stats: ImportStats
): Promise<{ success: boolean; error?: string; entityStats?: { nodes: Record<string, number>; relationships: Record<string, number> } }> {
  if (!result.graphStructure?.cypher) {
    return { success: false, error: 'No Cypher in result' };
  }

  // Get hash from cacheInfo or generate one
  const fileHash = result.cacheInfo?.hash || sourceFile.substring(0, 32);
  let cypher = result.graphStructure.cypher;

  // Extract statistics before transformation
  const entityStats = extractEntityStats(cypher);
  logVerbose(`Entities in ${sourceFile}: ${JSON.stringify(entityStats)}`);

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
    const start = Date.now();
    const response = await executeCypher(cypher);
    const duration = Date.now() - start;
    
    // Check if response has error
    if (response.error) {
      log(`Cypher execution error: ${response.error}`, 'error');
      return { 
        success: false, 
        error: `Cypher execution error: ${response.error}` 
      };
    }
    
    // Update statistics
    for (const [nodeType, count] of Object.entries(entityStats.nodes)) {
      stats.nodesByType[nodeType] = (stats.nodesByType[nodeType] || 0) + count;
    }
    for (const [relType, count] of Object.entries(entityStats.relationships)) {
      stats.relationshipsByType[relType] = (stats.relationshipsByType[relType] || 0) + count;
    }
    
    // Log successful execution
    logVerbose(`Cypher executed successfully (${duration}ms, response: ${response.execution_time_ms}ms)`);
    if (VERBOSE && entityStats.nodes) {
      logVerbose(`Created nodes: ${JSON.stringify(entityStats.nodes)}`);
    }
    if (VERBOSE && entityStats.relationships) {
      logVerbose(`Created relationships: ${JSON.stringify(entityStats.relationships)}`);
    }
    
    return { success: true, entityStats };
  } catch (error) {
    const errorMsg = error instanceof Error ? error.message : String(error);
    log(`Import failed: ${errorMsg}`, 'error');
    return { 
      success: false, 
      error: errorMsg
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
  console.log(`Verbose logging: ${VERBOSE ? 'enabled' : 'disabled'} (set VERBOSE=true for detailed output)\n`);

  // Initialize statistics
  const stats: ImportStats = {
    totalFiles: 0,
    imported: 0,
    failed: 0,
    skipped: 0,
    nodesByType: {},
    relationshipsByType: {},
    startTime: Date.now(),
    errors: [],
  };

  // Check Nexus connection
  try {
    log('Checking Nexus connection...', 'info');
    const healthCheck = await fetch(`${NEXUS_URL}/health`);
    if (!healthCheck.ok) {
      throw new Error(`Nexus health check failed: ${healthCheck.status}`);
    }
    log(`Connected to Nexus at ${NEXUS_URL}`, 'info');
    console.log();
  } catch (error) {
    log(`Failed to connect to Nexus: ${error}`, 'error');
    log(`Make sure Nexus is running on ${NEXUS_URL}`, 'error');
    process.exit(1);
  }

  // Read all cache files from .classify-cache directory
  log(`Reading cache from: ${CLASSIFY_CACHE_DIR}`, 'info');
  console.log();
  
  let cacheEntries: Array<{ path: string; entry: CacheEntry }>;
  try {
    cacheEntries = await readCacheFiles(CLASSIFY_CACHE_DIR);
  } catch (error) {
    log(`Failed to read cache directory: ${error}`, 'error');
    process.exit(1);
  }

  stats.totalFiles = cacheEntries.length;
  log(`Found ${cacheEntries.length} cache entries`, 'info');
  console.log();

  // Import each cache entry with progress tracking
  for (let i = 0; i < cacheEntries.length; i++) {
    const { path: cachePath, entry } = cacheEntries[i];
    const progress = ((i + 1) / cacheEntries.length * 100).toFixed(1);
    
    try {
      const result = entry.result;
      const fileName = cachePath.split(/[/\\]/).pop() || 'unknown';
      const sourceFile = result.file || entry.hash || 'unknown';
      
      console.log(`[${i + 1}/${cacheEntries.length}] (${progress}%) 📄 ${fileName}`);
      logVerbose(`Source file: ${sourceFile}`);

      const importResultData = await importResult(result, sourceFile, stats);
      
      if (importResultData.success) {
        stats.imported++;
        log('Imported successfully', 'info');
        if (VERBOSE && importResultData.entityStats) {
          logVerbose(`Nodes: ${JSON.stringify(importResultData.entityStats.nodes)}`);
          logVerbose(`Relationships: ${JSON.stringify(importResultData.entityStats.relationships)}`);
        }
        console.log();
      } else {
        stats.failed++;
        stats.errors.push({ 
          file: fileName, 
          error: importResultData.error || 'Unknown error',
          timestamp: Date.now()
        });
        log(`Failed: ${importResultData.error}`, 'error');
        console.log();
      }
    } catch (error) {
      stats.failed++;
      const fileName = cachePath.split(/[/\\]/).pop() || 'unknown';
      const errorMsg = error instanceof Error ? error.message : String(error);
      stats.errors.push({ 
        file: fileName, 
        error: errorMsg,
        timestamp: Date.now()
      });
      log(`Error: ${errorMsg}`, 'error');
      console.log();
    }
  }

  stats.endTime = Date.now();

  // Calculate duration
  const durationSec = ((stats.endTime - stats.startTime) / 1000).toFixed(2);

  // Summary
  console.log('╔═══════════════════════════════════════════════════╗');
  console.log('║  Import Summary                                   ║');
  console.log('╚═══════════════════════════════════════════════════╝\n');
  
  log(`Total files processed: ${stats.totalFiles}`, 'info');
  log(`✅ Imported: ${stats.imported}`, 'info');
  log(`❌ Failed: ${stats.failed}`, stats.failed > 0 ? 'warn' : 'info');
  log(`Duration: ${durationSec}s`, 'info');
  log(`Average: ${(stats.imported / parseFloat(durationSec)).toFixed(2)} files/sec`, 'info');
  console.log();

  // Node statistics
  if (Object.keys(stats.nodesByType).length > 0) {
    log('Nodes created by type:', 'info');
    for (const [nodeType, count] of Object.entries(stats.nodesByType).sort((a, b) => b[1] - a[1])) {
      log(`  ${nodeType}: ${count}`, 'info');
    }
    console.log();
  }

  // Relationship statistics
  if (Object.keys(stats.relationshipsByType).length > 0) {
    log('Relationships created by type:', 'info');
    for (const [relType, count] of Object.entries(stats.relationshipsByType).sort((a, b) => b[1] - a[1])) {
      log(`  ${relType}: ${count}`, 'info');
    }
    console.log();
  }

  // Errors
  if (stats.errors.length > 0) {
    log('Errors encountered:', 'error');
    stats.errors.forEach(({ file, error }) => {
      log(`  ${file}: ${error}`, 'error');
    });
    console.log();
  }

  // Write detailed log to file
  try {
    const { writeFile } = await import('fs/promises');
    const logContent = JSON.stringify({
      summary: {
        totalFiles: stats.totalFiles,
        imported: stats.imported,
        failed: stats.failed,
        skipped: stats.skipped,
        durationSeconds: parseFloat(durationSec),
        throughput: parseFloat((stats.imported / parseFloat(durationSec)).toFixed(2)),
      },
      nodesByType: stats.nodesByType,
      relationshipsByType: stats.relationshipsByType,
      errors: stats.errors,
      timestamp: new Date(stats.startTime).toISOString(),
    }, null, 2);
    
    await writeFile(LOG_FILE, logContent);
    log(`Detailed log written to: ${LOG_FILE}`, 'info');
    console.log();
  } catch (error) {
    log(`Failed to write log file: ${error}`, 'warn');
  }

  // Run test queries
  if (stats.imported > 0) {
    await runTestQueries();
  }

  log('✨ Import complete!', 'info');
  console.log();
  
  // Exit with appropriate code
  process.exit(stats.failed > 0 ? 1 : 0);
}

main().catch(console.error);
