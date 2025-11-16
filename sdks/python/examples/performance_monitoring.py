"""Performance monitoring example for Nexus Python SDK."""

import asyncio
from nexus_sdk import NexusClient


async def main():
    """Run performance monitoring example."""
    async with NexusClient("http://localhost:15474") as client:
        print("=== Performance Monitoring ===\n")

        # Get query statistics
        print("1. Getting query statistics...")
        try:
            stats = await client.get_query_statistics()
            print(f"   Total queries: {stats.statistics.total_queries}")
            print(f"   Successful: {stats.statistics.successful_queries}")
            print(f"   Failed: {stats.statistics.failed_queries}")
            print(f"   Average time: {stats.statistics.average_execution_time_ms}ms\n")
        except Exception as e:
            print(f"   Error: {e}\n")

        # Get slow queries
        print("2. Getting slow queries...")
        try:
            slow_queries = await client.get_slow_queries()
            print(f"   Found {slow_queries.count} slow queries")
            for query in slow_queries.queries[:5]:  # Show first 5
                print(f"   - {query.query[:50]}... ({query.execution_time_ms}ms)\n")
        except Exception as e:
            print(f"   Error: {e}\n")

        # Get plan cache statistics
        print("3. Getting plan cache statistics...")
        try:
            cache_stats = await client.get_plan_cache_statistics()
            print(f"   Cached plans: {cache_stats.cached_plans}")
            print(f"   Hit rate: {cache_stats.hit_rate:.2%}")
            print(f"   Memory usage: {cache_stats.current_memory_bytes} bytes\n")
        except Exception as e:
            print(f"   Error: {e}\n")

        # Clear plan cache
        print("4. Clearing plan cache...")
        try:
            result = await client.clear_plan_cache()
            print(f"   Cache cleared: {result}\n")
        except Exception as e:
            print(f"   Error: {e}\n")


if __name__ == "__main__":
    asyncio.run(main())

