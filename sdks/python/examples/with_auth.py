"""Authentication examples for Nexus Python SDK."""

import asyncio
from nexus_sdk import NexusClient


async def main():
    """Run authentication examples."""
    # Using API key
    print("=== Using API Key ===")
    async with NexusClient(
        "http://localhost:15474", api_key="your-api-key"
    ) as client:
        healthy = await client.health_check()
        print(f"Server is healthy: {healthy}")

    # Using username/password
    print("\n=== Using Username/Password ===")
    async with NexusClient(
        "http://localhost:15474", username="user", password="pass"
    ) as client:
        healthy = await client.health_check()
        print(f"Server is healthy: {healthy}")

    # Without authentication (if server allows)
    print("\n=== Without Authentication ===")
    async with NexusClient("http://localhost:15474") as client:
        healthy = await client.health_check()
        print(f"Server is healthy: {healthy}")


if __name__ == "__main__":
    asyncio.run(main())

