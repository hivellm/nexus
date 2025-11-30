"""Transaction examples for Nexus Python SDK."""

import asyncio
from nexus_sdk import NexusClient, Transaction


async def main():
    """Run transaction examples."""
    async with NexusClient("http://localhost:15474") as client:
        # Begin a transaction using Transaction class
        print("=== Beginning Transaction ===")
        tx = await client.begin_transaction()
        print(f"Transaction ID: {tx.transaction_id}")
        print(f"Transaction active: {tx.is_active()}")
        print(f"Transaction status: {tx.status()}\n")

        # Execute queries within transaction
        print("=== Creating Nodes in Transaction ===")
        result1 = await tx.execute(
            "CREATE (n:Person {name: $name}) RETURN n",
            {"name": "Alice"},
        )
        print(f"Created node 1: {result1.rows}\n")

        result2 = await tx.execute(
            "CREATE (n:Person {name: $name}) RETURN n",
            {"name": "Bob"},
        )
        print(f"Created node 2: {result2.rows}\n")

        # Commit transaction
        print("=== Committing Transaction ===")
        await tx.commit()
        print(f"Transaction status after commit: {tx.status()}\n")

        # Example: Rollback on error
        print("=== Example: Rollback on Error ===")
        tx2 = await client.begin_transaction()
        try:
            # Try to execute a query
            await tx2.execute("CREATE (n:Person {name: 'Charlie'}) RETURN n", None)
            # If we get here, commit
            await tx2.commit()
            print("Transaction committed")
        except Exception as e:
            print(f"Error occurred: {e}")
            # Rollback on error
            await tx2.rollback()
            print(f"Transaction rolled back. Status: {tx2.status()}")


if __name__ == "__main__":
    asyncio.run(main())

