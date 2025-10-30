#!/usr/bin/env tsx
/**
 * Debug script to import a single classify cache file and see detailed errors
 */

import { readFile, readdir, stat } from 'fs/promises';
import { join } from 'path';

const NEXUS_URL = process.env.NEXUS_URL || 'http://127.0.0.1:15474';
const CLASSIFY_CACHE_DIR = process.env.CLASSIFY_CACHE_DIR || 
  join(process.cwd(), '..', '..', 'classify', '.classify-cache');

interface ClassifyResult {
  file?: string;
  graphStructure?: {
    cypher?: string;
  };
  cacheInfo?: {
    hash?: string;
  };
}

interface CacheEntry {
  hash: string;
  result: ClassifyResult;
}

async function executeCypher(query: string): Promise<any> {
  const response = await fetch(`${NEXUS_URL}/cypher`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({ query }),
  });

  if (!response.ok) {
    const errorText = await response.text();
    throw new Error(`HTTP ${response.status}: ${errorText}`);
  }

  return await response.json();
}

async function main() {
  console.log('🔍 Debug Single Import\n');
  
  // Check Nexus connection
  try {
    await fetch(`${NEXUS_URL}/health`);
    console.log('✅ Nexus connected\n');
  } catch (error) {
    console.error(`❌ Failed to connect to Nexus: ${error}`);
    process.exit(1);
  }

  // Read first cache file
  console.log(`📁 Reading cache from: ${CLASSIFY_CACHE_DIR}\n`);
  
  let foundFile: { path: string; entry: CacheEntry } | null = null;
  
  try {
    const subdirs = await readdir(CLASSIFY_CACHE_DIR);
    
    for (const subdir of subdirs) {
      const subdirPath = join(CLASSIFY_CACHE_DIR, subdir);
      try {
        const subdirStat = await stat(subdirPath);
        if (!subdirStat.isDirectory()) continue;
        
        const files = await readdir(subdirPath);
        const jsonFiles = files.filter(f => f.endsWith('.json'));
        
        if (jsonFiles.length > 0) {
          const filePath = join(subdirPath, jsonFiles[0]);
          const content = await readFile(filePath, 'utf-8');
          const entry: CacheEntry = JSON.parse(content);
          foundFile = { path: filePath, entry };
          break;
        }
      } catch (error) {
        continue;
      }
    }
  } catch (error) {
    console.error(`❌ Failed to read cache: ${error}`);
    process.exit(1);
  }

  if (!foundFile) {
    console.error('❌ No cache files found');
    process.exit(1);
  }

  console.log(`📄 Using file: ${foundFile.path.split(/[/\\]/).pop()}\n`);
  
  const result = foundFile.entry.result;
  const fileHash = result.cacheInfo?.hash || foundFile.entry.hash;
  
  if (!result.graphStructure?.cypher) {
    console.error('❌ No Cypher in result');
    process.exit(1);
  }

  let cypher = result.graphStructure.cypher;
  console.log(`📝 Original Cypher (first 500 chars):\n${cypher.substring(0, 500)}...\n`);

  // Apply same transformations as import script (fixed version)
  // Nexus parser doesn't support += syntax, so include file_hash in MERGE pattern
  cypher = cypher.replace(
    /CREATE \(doc:Document \{([^}]+)\}\)/s,
    (match, props) => {
      const cleanProps = props.trim().replace(/,\s*$/, '');
      return `MERGE (doc:Document { file_hash: "${fileHash}", ${cleanProps} })`;
    }
  );

  console.log(`📝 Transformed Cypher (first 500 chars):\n${cypher.substring(0, 500)}...\n`);

  // Split into individual statements if multiple CREATE statements
  const statements = cypher.split(/(?=CREATE|MERGE)/).filter(s => s.trim());
  console.log(`📊 Found ${statements.length} Cypher statements\n`);

  // Execute first statement only
  if (statements.length > 0) {
    const firstStatement = statements[0].trim();
    console.log(`🚀 Executing first statement:\n${firstStatement.substring(0, 300)}...\n`);
    
    try {
      const response = await executeCypher(firstStatement);
      
      console.log(`✅ Response received:`);
      console.log(`   Execution time: ${response.execution_time_ms}ms`);
      console.log(`   Rows: ${response.rows?.length || 0}`);
      console.log(`   Columns: ${response.columns?.join(', ') || 'none'}`);
      
      if (response.error) {
        console.log(`\n❌ ERROR in response: ${response.error}`);
      } else {
        console.log(`\n✅ Query executed successfully!`);
        
        // Check if nodes were created
        console.log(`\n🔍 Verifying nodes were created...`);
        const checkQuery = 'MATCH (n) RETURN count(n) AS total';
        const checkResponse = await executeCypher(checkQuery);
        console.log(`   Total nodes in Nexus: ${JSON.stringify(checkResponse.rows)}`);
      }
    } catch (error) {
      console.error(`\n❌ Execution failed:`);
      console.error(`   ${error}`);
    }
  }
}

main().catch(console.error);

