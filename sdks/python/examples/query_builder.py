"""Query builder example for Nexus Python SDK."""

import asyncio
from nexus_sdk import NexusClient, QueryBuilder


async def main():
    """Run query builder example."""
    async with NexusClient("http://localhost:15474") as client:
        # Build a query using QueryBuilder
        print("=== Using QueryBuilder ===\n")

        # Example 1: Simple MATCH query
        query1 = (
            QueryBuilder()
            .match_("(n:Person)")
            .where_("n.age > $min_age")
            .return_("n.name, n.age")
            .order_by("n.age DESC")
            .limit(10)
            .param("min_age", 25)
            .build()
        )

        print(f"Query 1:\n{query1}\n")
        result1 = await client.execute_cypher(query1.query, query1.params)
        print(f"Results: {len(result1.rows)} rows\n")

        # Example 2: CREATE with parameters
        query2 = (
            QueryBuilder()
            .create("(n:Person {name: $name, age: $age})")
            .return_("n")
            .param("name", "Charlie")
            .param("age", 28)
            .build()
        )

        print(f"Query 2:\n{query2}\n")
        result2 = await client.execute_cypher(query2.query, query2.params)
        print(f"Created: {len(result2.rows)} rows\n")

        # Example 3: Complex query with WITH
        query3 = (
            QueryBuilder()
            .match_("(p:Person)-[:KNOWS]->(f:Person)")
            .with_("p, count(f) AS friend_count")
            .where_("friend_count > $min_friends")
            .return_("p.name, friend_count")
            .order_by("friend_count DESC")
            .limit(5)
            .param("min_friends", 2)
            .build()
        )

        print(f"Query 3:\n{query3}\n")
        result3 = await client.execute_cypher(query3.query, query3.params)
        print(f"Results: {len(result3.rows)} rows\n")


if __name__ == "__main__":
    asyncio.run(main())

