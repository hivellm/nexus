import { NexusClient } from './src/client';

interface TestCounter {
  passed: number;
  failed: number;
  total: number;
}

const counter: TestCounter = { passed: 0, failed: 0, total: 0 };

function test(name: string, condition: boolean, expected?: any, actual?: any): void {
  counter.total++;
  if (condition) {
    counter.passed++;
    console.log(`  [OK] ${name}`);
  } else {
    counter.failed++;
    let msg = `  [FAIL] ${name}`;
    if (expected !== undefined && actual !== undefined) {
      msg += ` (expected: ${expected}, got: ${actual})`;
    }
    console.log(msg);
  }
}

function summary(): boolean {
  console.log('\n' + '='.repeat(60));
  console.log(`Results: ${counter.passed}/${counter.total} tests passed`);
  if (counter.failed > 0) {
    console.log(`Failed: ${counter.failed} tests`);
  }
  console.log('='.repeat(60));
  return counter.failed === 0;
}

async function main() {
  console.log('='.repeat(60));
  console.log('COMPREHENSIVE TYPESCRIPT SDK TEST SUITE');
  console.log('='.repeat(60));

  const client = new NexusClient({
    baseUrl: 'http://localhost:15474',
    auth: { apiKey: 'test-key' }
  });

  try {
    // Setup - Clean database
    console.log('\n[SETUP] Cleaning database...');
    await client.executeCypher('MATCH (n) DETACH DELETE n');

    // Test 1: Basic Node Creation
    console.log('\n[1] BASIC NODE OPERATIONS');
    let result = await client.executeCypher(
      "CREATE (p:Person {name: 'Alice', age: 30, city: 'New York'}) RETURN p"
    );
    test('Create node with properties', result.rows.length === 1);

    result = await client.executeCypher('MATCH (p:Person) RETURN count(p) as count');
    test('Count nodes', result.rows[0][0] === 1, 1, result.rows[0][0]);

    // Test 2: Multiple Nodes
    console.log('\n[2] MULTIPLE NODES');
    await client.executeCypher(`
      CREATE (p1:Person {name: 'Bob', age: 25, salary: 50000})
      CREATE (p2:Person {name: 'Carol', age: 35, salary: 80000})
      CREATE (c:Company {name: 'TechCorp', founded: 2010})
    `);

    result = await client.executeCypher('MATCH (n) RETURN count(n) as count');
    test('Multiple nodes created', result.rows[0][0] === 4, 4, result.rows[0][0]);

    // Test 3: Parameterized Queries
    console.log('\n[3] PARAMETERIZED QUERIES');
    result = await client.executeCypher(
      'MATCH (p:Person) WHERE p.age > $minAge RETURN p.name as name',
      { minAge: 28 }
    );
    test('WHERE with parameters', result.rows.length >= 2);

    // Test 4: Aggregations
    console.log('\n[4] AGGREGATION FUNCTIONS');
    result = await client.executeCypher('MATCH (p:Person) RETURN avg(p.age) as avgAge');
    test('AVG function', result.rows[0][0] === 30);

    result = await client.executeCypher(
      'MATCH (p:Person) WHERE p.salary IS NOT NULL RETURN sum(p.salary) as total'
    );
    test('SUM function', result.rows[0][0] === 130000, 130000, result.rows[0][0]);

    result = await client.executeCypher(
      'MATCH (p:Person) RETURN max(p.age) as maxAge, min(p.age) as minAge'
    );
    test('MAX/MIN functions', result.rows[0][0] === 35 && result.rows[0][1] === 25);

    // Test 5: ORDER BY and LIMIT
    console.log('\n[5] ORDERING AND LIMITING');
    result = await client.executeCypher(
      'MATCH (p:Person) RETURN p.name as name, p.age as age ORDER BY p.age DESC LIMIT 2'
    );
    test('ORDER BY DESC', result.rows.length === 2);

    result = await client.executeCypher(
      'MATCH (p:Person) RETURN p.name as name ORDER BY p.name SKIP 1 LIMIT 2'
    );
    test('SKIP and LIMIT', result.rows.length === 2);

    // Test 6: WHERE Clauses
    console.log('\n[6] COMPLEX WHERE CLAUSES');
    result = await client.executeCypher(
      'MATCH (p:Person) WHERE p.age >= 25 AND p.age <= 35 RETURN count(p) as count'
    );
    test('WHERE with AND', result.rows[0][0] === 3, 3, result.rows[0][0]);

    result = await client.executeCypher(
      "MATCH (p:Person) WHERE p.name IN ['Alice', 'Bob'] RETURN count(p) as count"
    );
    test('WHERE IN', result.rows[0][0] === 2, 2, result.rows[0][0]);

    result = await client.executeCypher(
      'MATCH (p:Person) WHERE p.salary IS NULL RETURN count(p) as count'
    );
    test('IS NULL', result.rows[0][0] === 1, 1, result.rows[0][0]);

    // Test 7: DISTINCT
    console.log('\n[7] DISTINCT VALUES');
    await client.executeCypher("CREATE (:Tag {name: 'A'}), (:Tag {name: 'A'}), (:Tag {name: 'B'})");
    result = await client.executeCypher('MATCH (t:Tag) RETURN DISTINCT t.name as name');
    test('DISTINCT', result.rows.length === 2, 2, result.rows.length);

    // Test 8: COLLECT
    console.log('\n[8] COLLECT AGGREGATION');
    result = await client.executeCypher('MATCH (p:Person) RETURN collect(p.name) as names');
    test('COLLECT function', Array.isArray(result.rows[0][0]) && result.rows[0][0].length === 3);

    // Test 9: String Functions
    console.log('\n[9] STRING OPERATIONS');
    result = await client.executeCypher(
      "RETURN toUpper('hello') as upper, toLower('WORLD') as lower"
    );
    test('toUpper/toLower', result.rows[0][0] === 'HELLO' && result.rows[0][1] === 'world');

    result = await client.executeCypher(
      "MATCH (p:Person {name: 'Alice'}) RETURN substring(p.city, 0, 3) as prefix"
    );
    test('substring', result.rows.length > 0 && result.rows[0][0] === 'New');

    // Test 10: Mathematical Operations
    console.log('\n[10] MATHEMATICAL OPERATIONS');
    result = await client.executeCypher(
      'RETURN 10 + 5 as sum, 10 - 5 as diff, 10 * 5 as prod, 10 / 5 as quot'
    );
    test('Math operations',
      result.rows[0][0] === 15 &&
      result.rows[0][1] === 5 &&
      result.rows[0][2] === 50 &&
      result.rows[0][3] === 2
    );

    // Test 11: CASE Expressions
    console.log('\n[11] CASE EXPRESSIONS');
    result = await client.executeCypher(`
      MATCH (p:Person)
      RETURN p.name as name,
             CASE
               WHEN p.age < 30 THEN 'Young'
               WHEN p.age >= 30 THEN 'Mature'
             END as category
    `);
    test('CASE expression', result.rows.length === 3);

    // Test 12: UNWIND
    console.log('\n[12] UNWIND');
    result = await client.executeCypher('UNWIND [1, 2, 3] as num RETURN num');
    test('UNWIND list', result.rows.length === 3);

    // Test 13: NULL Handling
    console.log('\n[13] NULL HANDLING');
    result = await client.executeCypher('RETURN NULL as value');
    test('Return NULL', result.rows[0][0] === null);

    result = await client.executeCypher('RETURN coalesce(NULL, 42) as value');
    test('COALESCE function', result.rows[0][0] === 42);

    // Test 14: UNION
    console.log('\n[14] UNION QUERIES');
    result = await client.executeCypher(`
      MATCH (p:Person) RETURN p.name as name
      UNION
      MATCH (c:Company) RETURN c.name as name
    `);
    test('UNION', result.rows.length === 4, 4, result.rows.length);

    // Test 15: SET and UPDATE
    console.log('\n[15] UPDATE OPERATIONS');
    await client.executeCypher("MATCH (p:Person {name: 'Alice'}) SET p.age = 31");
    result = await client.executeCypher("MATCH (p:Person {name: 'Alice'}) RETURN p.age as age");
    test('SET property', result.rows[0][0] === 31, 31, result.rows[0][0]);

    // Test 16: REMOVE
    console.log('\n[16] REMOVE PROPERTY');
    await client.executeCypher("MATCH (p:Person {name: 'Bob'}) REMOVE p.salary");
    result = await client.executeCypher("MATCH (p:Person {name: 'Bob'}) RETURN p.salary as salary");
    test('REMOVE property', result.rows[0][0] === null);

    // Test 17: DELETE
    console.log('\n[17] DELETE OPERATIONS');
    await client.executeCypher("MATCH (t:Tag) DELETE t");
    result = await client.executeCypher('MATCH (t:Tag) RETURN count(t) as count');
    test('DELETE nodes', result.rows[0][0] === 0, 0, result.rows[0][0]);

    // Test 18: MERGE
    console.log('\n[18] MERGE OPERATIONS');
    result = await client.executeCypher(`
      MERGE (p:Person {name: 'Dave'})
      ON CREATE SET p.created = true
      RETURN p.created as created
    `);
    test('MERGE with ON CREATE', result.rows[0][0] === true);

    result = await client.executeCypher(`
      MERGE (p:Person {name: 'Dave'})
      ON MATCH SET p.matched = true
      RETURN p.matched as matched
    `);
    test('MERGE with ON MATCH', result.rows[0][0] === true);

    // Test 19: COUNT with WHERE
    console.log('\n[19] COUNT WITH FILTER');
    result = await client.executeCypher(`
      MATCH (p:Person)
      WHERE p.age > 25
      RETURN count(p) as count
    `);
    test('COUNT with WHERE', result.rows[0][0] >= 2);

    // Test 20: Multiple WHERE conditions
    console.log('\n[20] MULTIPLE WHERE CONDITIONS');
    result = await client.executeCypher(`
      MATCH (p:Person)
      WHERE p.age > 25 OR p.name = 'Bob'
      RETURN count(p) as count
    `);
    test('WHERE with OR', result.rows[0][0] >= 2);

    // Cleanup
    console.log('\n[CLEANUP] Cleaning database...');
    await client.executeCypher('MATCH (n) DETACH DELETE n');
    result = await client.executeCypher('MATCH (n) RETURN count(n) as count');
    test('Database cleaned', result.rows[0][0] === 0, 0, result.rows[0][0]);

  } catch (error: any) {
    console.error(`\n[ERROR] ${error.message}`);
    if (error.stack) {
      console.error(error.stack);
    }
  }

  // Print summary
  const success = summary();
  process.exit(success ? 0 : 1);
}

main().catch(console.error);
