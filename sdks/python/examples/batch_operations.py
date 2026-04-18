"""Batch operations example for Nexus Python SDK."""

import asyncio
from nexus_sdk import NexusClient


async def main():
    """Run batch operations example."""
    async with NexusClient("http://localhost:15474") as client:
        print("=== Batch Operations ===\n")

        # Batch create nodes
        print("1. Batch creating nodes...")
        nodes = [
            {"labels": ["Person"], "properties": {"name": f"Person{i}", "age": 20 + i}}
            for i in range(5)
        ]

        batch_response = await client.batch_create_nodes(nodes)
        print(f"   Created {len(batch_response.node_ids)} nodes")
        print(f"   Node IDs: {batch_response.node_ids}\n")

        # Batch create relationships
        if len(batch_response.node_ids) >= 2:
            print("2. Batch creating relationships...")
            relationships = [
                {
                    "source_id": batch_response.node_ids[i],
                    "target_id": batch_response.node_ids[i + 1],
                    "rel_type": "KNOWS",
                    "properties": {"since": 2020 + i},
                }
                for i in range(len(batch_response.node_ids) - 1)
            ]

            rel_batch_response = await client.batch_create_relationships(relationships)
            print(f"   Created {len(rel_batch_response.rel_ids)} relationships")
            print(f"   Relationship IDs: {rel_batch_response.rel_ids}\n")


if __name__ == "__main__":
    asyncio.run(main())

