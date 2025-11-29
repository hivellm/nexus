#!/usr/bin/env python3
"""Comprehensive test suite for Python SDK."""

import asyncio
from nexus_sdk import NexusClient

class TestCounter:
    def __init__(self):
        self.passed = 0
        self.failed = 0
        self.total = 0

    def test(self, name, condition, expected=None, actual=None):
        self.total += 1
        if condition:
            self.passed += 1
            print(f"  [OK] {name}")
        else:
            self.failed += 1
            msg = f"  [FAIL] {name}"
            if expected is not None and actual is not None:
                msg += f" (expected: {expected}, got: {actual})"
            print(msg)

    def summary(self):
        print(f"\n{'='*60}")
        print(f"Results: {self.passed}/{self.total} tests passed")
        if self.failed > 0:
            print(f"Failed: {self.failed} tests")
        print(f"{'='*60}")
        return self.failed == 0

async def main():
    print("=" * 60)
    print("COMPREHENSIVE PYTHON SDK TEST SUITE")
    print("=" * 60)

    counter = TestCounter()
    client = NexusClient(base_url="http://localhost:15474")

    try:
        # Setup - Clean database
        print("\n[SETUP] Cleaning database...")
        await client.execute_cypher("MATCH (n) DETACH DELETE n")

        # Test 1: Basic Node Creation
        print("\n[1] BASIC NODE OPERATIONS")
        result = await client.execute_cypher(
            "CREATE (p:Person {name: 'Alice', age: 30, city: 'New York'}) RETURN p"
        )
        counter.test("Create node with properties", len(result.rows) == 1)

        result = await client.execute_cypher("MATCH (p:Person) RETURN count(p) as count")
        counter.test("Count nodes", result.rows[0][0] == 1, 1, result.rows[0][0])

        # Test 2: Multiple Nodes with Different Labels
        print("\n[2] MULTIPLE LABELS AND NODES")
        await client.execute_cypher("""
            CREATE (p1:Person:Employee {name: 'Bob', age: 25, salary: 50000})
            CREATE (p2:Person:Manager {name: 'Carol', age: 35, salary: 80000})
            CREATE (c:Company {name: 'TechCorp', founded: 2010})
        """)

        result = await client.execute_cypher("MATCH (n) RETURN count(n) as count")
        counter.test("Multiple nodes created", result.rows[0][0] == 4, 4, result.rows[0][0])

        result = await client.execute_cypher("MATCH (p:Employee) RETURN count(p) as count")
        counter.test("Count by label", result.rows[0][0] == 1, 1, result.rows[0][0])

        # Test 3: Parameterized Queries
        print("\n[3] PARAMETERIZED QUERIES")
        result = await client.execute_cypher(
            "MATCH (p:Person) WHERE p.age > $minAge RETURN p.name as name",
            parameters={"minAge": 28}
        )
        counter.test("WHERE with parameters", len(result.rows) >= 2)

        result = await client.execute_cypher(
            "MATCH (p:Person {name: $name}) RETURN p.age as age",
            parameters={"name": "Alice"}
        )
        counter.test("Property match with parameters",
                    len(result.rows) > 0 and result.rows[0][0] == 30,
                    30, result.rows[0][0] if len(result.rows) > 0 else None)

        # Test 4: Relationships
        print("\n[4] RELATIONSHIPS")
        await client.execute_cypher("""
            MATCH (alice:Person {name: 'Alice'})
            MATCH (bob:Person {name: 'Bob'})
            MATCH (carol:Person {name: 'Carol'})
            MATCH (company:Company {name: 'TechCorp'})
            CREATE (alice)-[:KNOWS {since: 2020}]->(bob)
            CREATE (alice)-[:MANAGES]->(carol)
            CREATE (bob)-[:WORKS_FOR {since: 2021}]->(company)
            CREATE (carol)-[:WORKS_FOR {since: 2019}]->(company)
        """)

        result = await client.execute_cypher("MATCH ()-[r]->() RETURN count(r) as count")
        counter.test("Relationships created", result.rows[0][0] == 4, 4, result.rows[0][0])

        result = await client.execute_cypher(
            "MATCH (p:Person)-[:WORKS_FOR]->(c:Company) RETURN count(p) as count"
        )
        counter.test("Pattern matching", result.rows[0][0] == 2, 2, result.rows[0][0])

        # Test 5: Aggregations
        print("\n[5] AGGREGATION FUNCTIONS")
        result = await client.execute_cypher(
            "MATCH (p:Person) RETURN avg(p.age) as avgAge"
        )
        counter.test("AVG function", result.rows[0][0] == 30.0, 30.0, result.rows[0][0])

        result = await client.execute_cypher(
            "MATCH (p:Person) WHERE p.salary IS NOT NULL RETURN sum(p.salary) as total"
        )
        counter.test("SUM function", result.rows[0][0] == 130000, 130000, result.rows[0][0])

        result = await client.execute_cypher(
            "MATCH (p:Person) RETURN max(p.age) as maxAge, min(p.age) as minAge"
        )
        counter.test("MAX/MIN functions", result.rows[0][0] == 35 and result.rows[0][1] == 25)

        # Test 6: ORDER BY and LIMIT
        print("\n[6] ORDERING AND LIMITING")
        result = await client.execute_cypher(
            "MATCH (p:Person) RETURN p.name as name ORDER BY p.age DESC LIMIT 2"
        )
        counter.test("ORDER BY DESC", len(result.rows) == 2 and result.rows[0][0] == "Carol")

        result = await client.execute_cypher(
            "MATCH (p:Person) RETURN p.name as name ORDER BY p.name SKIP 1 LIMIT 2"
        )
        counter.test("SKIP and LIMIT", len(result.rows) == 2)

        # Test 7: WHERE Clauses
        print("\n[7] COMPLEX WHERE CLAUSES")
        result = await client.execute_cypher(
            "MATCH (p:Person) WHERE p.age >= 25 AND p.age <= 35 RETURN count(p) as count"
        )
        counter.test("WHERE with AND", result.rows[0][0] == 3, 3, result.rows[0][0])

        result = await client.execute_cypher(
            "MATCH (p:Person) WHERE p.name IN ['Alice', 'Bob'] RETURN count(p) as count"
        )
        counter.test("WHERE IN", result.rows[0][0] == 2, 2, result.rows[0][0])

        result = await client.execute_cypher(
            "MATCH (p:Person) WHERE p.salary IS NULL RETURN count(p) as count"
        )
        counter.test("IS NULL", result.rows[0][0] == 1, 1, result.rows[0][0])

        # Test 8: DISTINCT
        print("\n[8] DISTINCT VALUES")
        result = await client.execute_cypher(
            "MATCH (p:Person)-[:WORKS_FOR]->(c) RETURN DISTINCT c.name as company"
        )
        counter.test("DISTINCT", len(result.rows) == 1)

        # Test 9: COLLECT
        print("\n[9] COLLECT AGGREGATION")
        result = await client.execute_cypher(
            "MATCH (p:Person) RETURN collect(p.name) as names"
        )
        counter.test("COLLECT function", len(result.rows[0][0]) == 3, 3, len(result.rows[0][0]))

        # Test 10: String Functions
        print("\n[10] STRING OPERATIONS")
        result = await client.execute_cypher(
            "RETURN toUpper('hello') as upper, toLower('WORLD') as lower"
        )
        counter.test("toUpper/toLower", result.rows[0][0] == "HELLO" and result.rows[0][1] == "world")

        result = await client.execute_cypher(
            "MATCH (p:Person {name: 'Alice'}) RETURN substring(p.city, 0, 3) as prefix"
        )
        counter.test("substring", result.rows[0][0] == "New", "New", result.rows[0][0])

        # Test 11: Mathematical Operations
        print("\n[11] MATHEMATICAL OPERATIONS")
        result = await client.execute_cypher(
            "RETURN 10 + 5 as sum, 10 - 5 as diff, 10 * 5 as prod, 10 / 5 as quot"
        )
        counter.test("Math operations",
                    result.rows[0][0] == 15 and
                    result.rows[0][1] == 5 and
                    result.rows[0][2] == 50)

        # Test 12: CASE Expressions
        print("\n[12] CASE EXPRESSIONS")
        result = await client.execute_cypher("""
            MATCH (p:Person)
            RETURN p.name as name,
                   CASE
                     WHEN p.age < 30 THEN 'Young'
                     WHEN p.age >= 30 THEN 'Mature'
                   END as category
        """)
        counter.test("CASE expression", len(result.rows) == 3)

        # Test 13: WITH Clause
        print("\n[13] WITH CLAUSE")
        result = await client.execute_cypher("""
            MATCH (p:Person)
            WITH p, p.age as age
            WHERE age > 25
            RETURN count(p) as count
        """)
        counter.test("WITH clause", result.rows[0][0] == 2, 2, result.rows[0][0])

        # Test 14: UNWIND
        print("\n[14] UNWIND")
        result = await client.execute_cypher(
            "UNWIND [1, 2, 3] as num RETURN num"
        )
        counter.test("UNWIND list", len(result.rows) == 3)

        # Test 15: Relationship Properties
        print("\n[15] RELATIONSHIP PROPERTIES")
        result = await client.execute_cypher("""
            MATCH (p:Person)-[r:KNOWS]->(other)
            RETURN r.since as since
        """)
        counter.test("Relationship properties",
                    len(result.rows) == 1 and result.rows[0][0] == 2020,
                    2020, result.rows[0][0] if result.rows else None)

        # Test 16: Multiple Relationship Types
        print("\n[16] MULTIPLE RELATIONSHIP TYPES")
        result = await client.execute_cypher("""
            MATCH (alice:Person {name: 'Alice'})-[r]->(target)
            RETURN type(r) as relType, target.name as name
        """)
        counter.test("Multiple relationship types", len(result.rows) == 2)

        # Test 17: Path Queries
        print("\n[17] PATH QUERIES")
        result = await client.execute_cypher("""
            MATCH path = (alice:Person {name: 'Alice'})-[*1..2]->(target)
            RETURN count(path) as pathCount
        """)
        counter.test("Variable length paths", result.rows[0][0] >= 2)

        # Test 18: EXISTS
        print("\n[18] EXISTS PREDICATE")
        result = await client.execute_cypher("""
            MATCH (p:Person)
            WHERE EXISTS((p)-[:MANAGES]->())
            RETURN count(p) as count
        """)
        counter.test("EXISTS predicate", result.rows[0][0] == 1, 1, result.rows[0][0])

        # Test 19: DELETE Operations
        print("\n[19] DELETE OPERATIONS")
        await client.execute_cypher("""
            MATCH (p:Person {name: 'Alice'})-[r:KNOWS]->()
            DELETE r
        """)
        result = await client.execute_cypher(
            "MATCH ()-[r:KNOWS]->() RETURN count(r) as count"
        )
        counter.test("DELETE relationship", result.rows[0][0] == 0, 0, result.rows[0][0])

        # Test 20: MERGE Operations
        print("\n[20] MERGE OPERATIONS")
        result = await client.execute_cypher("""
            MERGE (p:Person {name: 'Dave'})
            ON CREATE SET p.created = true
            RETURN p.created as created
        """)
        counter.test("MERGE with ON CREATE", result.rows[0][0] == True)

        result = await client.execute_cypher("""
            MERGE (p:Person {name: 'Dave'})
            ON MATCH SET p.matched = true
            RETURN p.matched as matched
        """)
        counter.test("MERGE with ON MATCH", result.rows[0][0] == True)

        # Cleanup
        print("\n[CLEANUP] Cleaning database...")
        await client.execute_cypher("MATCH (n) DETACH DELETE n")
        result = await client.execute_cypher("MATCH (n) RETURN count(n) as count")
        counter.test("Database cleaned", result.rows[0][0] == 0, 0, result.rows[0][0])

    except Exception as e:
        print(f"\n[ERROR] {e}")
        import traceback
        traceback.print_exc()
    finally:
        await client.close()

    # Print summary
    success = counter.summary()
    return 0 if success else 1

if __name__ == "__main__":
    import sys
    sys.exit(asyncio.run(main()))
