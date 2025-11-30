#!/usr/bin/env python3
"""Simple test for Python SDK."""

import asyncio
from nexus_sdk import NexusClient

async def main():
    print("=== Testing Python SDK ===\n")

    client = NexusClient(base_url="http://localhost:15474")

    try:
        # Test 1: Execute simple query
        print("1. Simple query: ", end="")
        result = await client.execute_cypher("RETURN 1 as num")
        print(f"OK - Columns: {result.columns}")

        # Test 2: Create nodes
        print("2. Create nodes: ", end="")
        result = await client.execute_cypher(
            "CREATE (a:Person {name: 'Alice', age: 28}) "
            "CREATE (b:Person {name: 'Bob', age: 32}) "
            "RETURN a.name, b.name"
        )
        print(f"OK - Rows: {len(result.rows)}")

        # Test 3: Query with parameters
        print("3. Query with parameters: ", end="")
        result = await client.execute_cypher(
            "MATCH (p:Person) WHERE p.age > $minAge RETURN p.name as name, p.age as age",
            parameters={"minAge": 25}
        )
        print(f"OK - Found {len(result.rows)} nodes")

        # Test 4: Create relationship
        print("4. Create relationship: ", end="")
        result = await client.execute_cypher(
            "MATCH (a:Person {name: 'Alice'}) "
            "MATCH (b:Person {name: 'Bob'}) "
            "CREATE (a)-[r:KNOWS {since: '2020'}]->(b) "
            "RETURN type(r) as type"
        )
        print("OK")

        # Test 5: Query relationships
        print("5. Query relationships: ", end="")
        result = await client.execute_cypher(
            "MATCH (a:Person)-[r:KNOWS]->(b:Person) "
            "RETURN a.name as person1, b.name as person2"
        )
        print(f"OK - Found {len(result.rows)} relationships")

        # Test 6: Cleanup
        print("6. Cleanup: ", end="")
        result = await client.execute_cypher("MATCH (n) DETACH DELETE n")
        print("OK")

        print("\n[SUCCESS] All Python SDK tests passed!")

    except Exception as e:
        print(f"\n[ERROR] {e}")
        import traceback
        traceback.print_exc()
        raise
    finally:
        await client.close()

if __name__ == "__main__":
    asyncio.run(main())
