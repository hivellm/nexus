"""Basic usage example for Nexus Python SDK."""

import asyncio
from nexus_sdk import NexusClient


async def main():
    """Run basic usage example."""
    # Create a client
    async with NexusClient("http://localhost:15474") as client:
        # Execute a Cypher query
        result = await client.execute_cypher("MATCH (n) RETURN n LIMIT 10", None)
        print(f"Found {len(result.rows)} rows")

        # Create a node
        create_response = await client.create_node(
            labels=["Person"], properties={"name": "Alice", "age": 30}
        )
        print(f"Created node with ID: {create_response.node_id}")

        # Get a node
        node = await client.get_node(create_response.node_id)
        if node:
            print(f"Node: {node}")

        # Update a node
        update_response = await client.update_node(
            create_response.node_id, properties={"age": 31}
        )
        print(f"Updated node: {update_response.node}")

        # Create another node
        create_response2 = await client.create_node(
            labels=["Person"], properties={"name": "Bob", "age": 25}
        )

        # Create a relationship
        rel_response = await client.create_relationship(
            source_id=create_response.node_id,
            target_id=create_response2.node_id,
            rel_type="KNOWS",
            properties={"since": 2020},
        )
        print(f"Created relationship with ID: {rel_response.relationship_id}")

        # Get database statistics
        stats = await client.get_stats()
        print(f"Database stats: {stats}")

        # Health check
        healthy = await client.health_check()
        print(f"Server is healthy: {healthy}")


if __name__ == "__main__":
    asyncio.run(main())

