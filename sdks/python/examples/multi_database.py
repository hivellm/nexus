"""Multi-database support example for Nexus Python SDK."""

import asyncio
from nexus_sdk import NexusClient


async def main():
    """Run multi-database example."""
    # Create a client connecting to the default database
    async with NexusClient("http://localhost:15474") as client:
        print("=== Multi-Database Support Demo ===\n")

        # 1. List all databases
        print("1. Listing all databases...")
        databases = await client.list_databases()
        print(f"   Available databases: {databases.databases}")
        print(f"   Default database: {databases.default_database}\n")

        # 2. Create a new database
        print("2. Creating new database 'testdb'...")
        create_result = await client.create_database("testdb")
        print(f"   Result: {create_result.message}\n")

        # 3. Switch to the new database
        print("3. Switching to 'testdb'...")
        switch_result = await client.switch_database("testdb")
        print(f"   Result: {switch_result.message}\n")

        # 4. Get current database
        print("4. Getting current database...")
        current_db = await client.get_current_database()
        print(f"   Current database: {current_db}\n")

        # 5. Create data in the new database
        print("5. Creating data in 'testdb'...")
        result = await client.execute_cypher(
            "CREATE (n:Product {name: $name, price: $price}) RETURN n",
            {"name": "Laptop", "price": 999.99}
        )
        print(f"   Created {len(result.rows)} node(s)\n")

        # 6. Query data from testdb
        print("6. Querying data from 'testdb'...")
        result = await client.execute_cypher(
            "MATCH (n:Product) RETURN n.name AS name, n.price AS price",
            None
        )
        for row in result.rows:
            print(f"   Product: {row['name']}, Price: ${row['price']}\n")

        # 7. Switch back to default database
        print("7. Switching back to default database...")
        switch_result = await client.switch_database("neo4j")
        print(f"   Result: {switch_result.message}\n")

        # 8. Verify data isolation - the Product node should not exist in default db
        print("8. Verifying data isolation...")
        result = await client.execute_cypher("MATCH (n:Product) RETURN count(n) AS count", None)
        product_count = result.rows[0]['count'] if result.rows else 0
        print(f"   Product nodes in default database: {product_count}")
        print(f"   Data isolation verified: {product_count == 0}\n")

        # 9. Get database info
        print("9. Getting 'testdb' info...")
        db_info = await client.get_database("testdb")
        print(f"   Name: {db_info.name}")
        print(f"   Path: {db_info.path}")
        print(f"   Nodes: {db_info.node_count}")
        print(f"   Relationships: {db_info.relationship_count}")
        print(f"   Storage: {db_info.storage_size} bytes\n")

        # 10. Clean up - drop the test database
        print("10. Dropping 'testdb'...")
        drop_result = await client.drop_database("testdb")
        print(f"    Result: {drop_result.message}\n")

        # 11. Verify database was dropped
        print("11. Verifying 'testdb' was dropped...")
        databases = await client.list_databases()
        db_exists = "testdb" in databases.databases
        print(f"    'testdb' exists: {db_exists}")
        print(f"    Cleanup successful: {not db_exists}\n")

        print("=== Multi-Database Demo Complete ===")


if __name__ == "__main__":
    asyncio.run(main())
