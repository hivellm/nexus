# Nexus Python SDK

[![PyPI](https://img.shields.io/pypi/v/nexus-sdk?style=flat-square)](https://pypi.org/project/nexus-sdk/)
[![License](https://img.shields.io/pypi/l/nexus-sdk?style=flat-square)](LICENSE)
[![Python](https://img.shields.io/pypi/pyversions/nexus-sdk?style=flat-square)](https://www.python.org/)
[![CI](https://img.shields.io/github/actions/workflow/status/hivellm/nexus/ci.yml?style=flat-square)](https://github.com/hivellm/nexus/actions)

Official Python SDK for Nexus graph database.

## Installation

```bash
pip install nexus-sdk
```

## Usage

### Basic Example

```python
import asyncio
from nexus_sdk import NexusClient

async def main():
    # Create a client
    client = NexusClient("http://localhost:15474")

    # Execute a Cypher query
    result = await client.execute_cypher("MATCH (n) RETURN n LIMIT 10", None)
    print(f"Found {len(result.rows)} rows")

    # Create a node
    create_response = await client.create_node(
        labels=["Person"],
        properties={"name": "Alice"}
    )
    print(f"Created node with ID: {create_response.node_id}")

    # Get a node
    node = await client.get_node(create_response.node_id)
    if node:
        print(f"Node: {node}")

    await client.close()

asyncio.run(main())
```

### With Authentication

```python
# Using API key
client = NexusClient(
    "http://localhost:15474",
    api_key="your-api-key"
)

# Or using username/password
client = NexusClient(
    "http://localhost:15474",
    username="user",
    password="pass"
)
```

### Schema Management

```python
# Create a label
response = await client.create_label("Person")

# List all labels
labels = await client.list_labels()
print(f"Labels: {labels.labels}")

# Create a relationship type
response = await client.create_rel_type("KNOWS")

# List all relationship types
types = await client.list_rel_types()
print(f"Types: {types.types}")
```

### Query Builder

```python
from nexus_sdk import QueryBuilder

# Build queries type-safely
query = (
    QueryBuilder()
    .match_("(n:Person)")
    .where_("n.age > $min_age")
    .return_("n.name, n.age")
    .order_by("n.age DESC")
    .limit(10)
    .param("min_age", 25)
    .build()
)

result = await client.execute_cypher(query.query, query.params)
```

### Batch Operations

```python
# Batch create nodes
nodes = [
    {"labels": ["Person"], "properties": {"name": f"Person{i}", "age": 20 + i}}
    for i in range(10)
]
batch_response = await client.batch_create_nodes(nodes)
print(f"Created {len(batch_response.node_ids)} nodes")

# Batch create relationships
relationships = [
    {
        "source_id": node_ids[i],
        "target_id": node_ids[i + 1],
        "rel_type": "KNOWS",
        "properties": {"since": 2020},
    }
    for i in range(len(node_ids) - 1)
]
rel_batch = await client.batch_create_relationships(relationships)
```

### Performance Monitoring

```python
# Get query statistics
stats = await client.get_query_statistics()
print(f"Total queries: {stats.statistics.total_queries}")
print(f"Average time: {stats.statistics.average_execution_time_ms}ms")

# Get slow queries
slow_queries = await client.get_slow_queries()
for query in slow_queries.queries:
    print(f"Slow query: {query.query} ({query.execution_time_ms}ms)")

# Get plan cache statistics
cache_stats = await client.get_plan_cache_statistics()
print(f"Hit rate: {cache_stats.hit_rate:.2%}")

# Clear plan cache
await client.clear_plan_cache()
```

### Advanced Transactions

```python
from nexus_sdk import Transaction

# Begin transaction with Transaction class
tx = await client.begin_transaction()
print(f"Transaction active: {tx.is_active()}")
print(f"Status: {tx.status()}")

# Execute queries within transaction
result = await tx.execute("CREATE (n:Person {name: $name}) RETURN n", {"name": "Alice"})

# Commit or rollback
await tx.commit()  # or tx.rollback()
```

### Multi-Database Support

```python
# List all databases
databases = await client.list_databases()
print(f"Available databases: {databases.databases}")
print(f"Default database: {databases.default_database}")

# Create a new database
create_result = await client.create_database("mydb")
print(f"Created: {create_result.name}")

# Switch to the new database
switch_result = await client.switch_database("mydb")
print(f"Switched to: mydb")

# Get current database
current_db = await client.get_current_database()
print(f"Current database: {current_db}")

# Create data in the current database
result = await client.execute_cypher(
    "CREATE (n:Product {name: $name}) RETURN n",
    {"name": "Laptop"}
)

# Get database information
db_info = await client.get_database("mydb")
print(f"Nodes: {db_info.node_count}, Relationships: {db_info.relationship_count}")

# Drop database (must not be current database)
await client.switch_database("neo4j")  # Switch away first
drop_result = await client.drop_database("mydb")

# Or connect directly to a specific database
async with NexusClient("http://localhost:15474", database="mydb") as client:
    # All operations will use 'mydb'
    result = await client.execute_cypher("MATCH (n) RETURN n LIMIT 10", None)
```

### Using Context Manager

```python
async with NexusClient("http://localhost:15474") as client:
    result = await client.execute_cypher("MATCH (n) RETURN n LIMIT 10", None)
    print(f"Found {len(result.rows)} rows")
```

### High Availability with Replication

Nexus supports master-replica replication for high availability and read scaling.
Use the **master** for all write operations and **replicas** for read operations.

```python
import asyncio
from nexus_sdk import NexusClient

class NexusCluster:
    """Client for Nexus cluster with master-replica topology."""

    def __init__(self, master_url: str, replica_urls: list[str]):
        """
        Initialize cluster client.

        Args:
            master_url: URL of the master node (for writes)
            replica_urls: List of replica URLs (for reads)
        """
        self.master = NexusClient(master_url)
        self.replicas = [NexusClient(url) for url in replica_urls]
        self._replica_index = 0

    def _get_replica(self) -> NexusClient:
        """Round-robin replica selection."""
        if not self.replicas:
            return self.master
        replica = self.replicas[self._replica_index]
        self._replica_index = (self._replica_index + 1) % len(self.replicas)
        return replica

    async def write(self, query: str, params: dict = None):
        """Execute write query on master."""
        return await self.master.execute_cypher(query, params)

    async def read(self, query: str, params: dict = None):
        """Execute read query on replica (round-robin)."""
        return await self._get_replica().execute_cypher(query, params)

    async def close(self):
        """Close all connections."""
        await self.master.close()
        for replica in self.replicas:
            await replica.close()

async def main():
    # Connect to cluster
    # Master handles all writes, replicas handle reads
    cluster = NexusCluster(
        master_url="http://master:15474",
        replica_urls=[
            "http://replica1:15474",
            "http://replica2:15474",
        ]
    )

    # Write operations go to master
    await cluster.write(
        "CREATE (n:Person {name: $name, age: $age}) RETURN n",
        {"name": "Alice", "age": 30}
    )

    # Read operations are distributed across replicas
    result = await cluster.read(
        "MATCH (n:Person) WHERE n.age > $min_age RETURN n",
        {"min_age": 25}
    )
    print(f"Found {len(result.rows)} persons")

    # High-volume reads are load-balanced
    for i in range(100):
        result = await cluster.read("MATCH (n) RETURN count(n) as total", None)

    await cluster.close()

asyncio.run(main())
```

#### Replication Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      Application                             │
│   ┌─────────────────────────────────────────────────────┐   │
│   │              NexusCluster Client                     │   │
│   │   write() ──────────┐     read() ───────────────┐   │   │
│   └─────────────────────┼───────────────────────────┼───┘   │
└─────────────────────────┼───────────────────────────┼───────┘
                          │                           │
                          ▼                           ▼
              ┌───────────────────┐     ┌─────────────────────┐
              │      MASTER       │     │      REPLICAS       │
              │   (writes only)   │────▶│   (reads only)      │
              │                   │ WAL │  ┌───────────────┐  │
              │ • CREATE          │────▶│  │   Replica 1   │  │
              │ • UPDATE          │     │  └───────────────┘  │
              │ • DELETE          │     │  ┌───────────────┐  │
              │ • MERGE           │────▶│  │   Replica 2   │  │
              └───────────────────┘     │  └───────────────┘  │
                                        └─────────────────────┘
```

#### Starting a Nexus Cluster

```bash
# Start master node
NEXUS_REPLICATION_ROLE=master \
NEXUS_REPLICATION_BIND_ADDR=0.0.0.0:15475 \
./nexus-server

# Start replica 1
NEXUS_REPLICATION_ROLE=replica \
NEXUS_REPLICATION_MASTER_ADDR=master:15475 \
./nexus-server

# Start replica 2
NEXUS_REPLICATION_ROLE=replica \
NEXUS_REPLICATION_MASTER_ADDR=master:15475 \
./nexus-server
```

#### Monitoring Replication Status

```python
import httpx

async def check_replication_status(master_url: str):
    async with httpx.AsyncClient() as client:
        # Check master status
        response = await client.get(f"{master_url}/replication/status")
        status = response.json()
        print(f"Role: {status['role']}")
        print(f"Running: {status['running']}")
        print(f"Connected replicas: {status.get('replica_count', 0)}")

        # Get master stats
        response = await client.get(f"{master_url}/replication/master/stats")
        stats = response.json()
        print(f"Entries replicated: {stats['entries_replicated']}")
        print(f"Connected replicas: {stats['connected_replicas']}")

        # List replicas
        response = await client.get(f"{master_url}/replication/replicas")
        replicas = response.json()
        for replica in replicas['replicas']:
            print(f"  - {replica['id']}: lag={replica['lag']}, healthy={replica['healthy']}")
```

## Features

- ✅ Cypher query execution
- ✅ Database statistics
- ✅ Health check
- ✅ Node CRUD operations (Create, Read, Update, Delete)
- ✅ Relationship CRUD operations (Create, Update, Delete)
- ✅ Schema management (Labels, Relationship Types)
- ✅ Transaction support (BEGIN, COMMIT, ROLLBACK)
- ✅ **Batch operations** (batch create nodes/relationships)
- ✅ **Performance monitoring** (query statistics, slow queries, plan cache)
- ✅ **Query Builder** (type-safe Cypher query construction)
- ✅ **Advanced Transaction** (Transaction class with state management)
- ✅ **Multi-database support** (create, list, switch, drop databases)
- ✅ Proper error handling
- ✅ Type-safe models with Pydantic
- ✅ Async/await support

## Dependencies

Install from PyPI:

```bash
pip install nexus-sdk
```

Or install from source:

```bash
pip install -r requirements.txt
```

### Core Dependencies

- `httpx>=0.24.0` - Modern HTTP client
- `pydantic>=2.0.0` - Data validation

### Development Dependencies

```bash
pip install -r requirements-dev.txt
```

## Examples

See the `examples/` directory for complete examples:

- `basic_usage.py` - Basic operations with nodes, relationships, and schema
- `with_auth.py` - Authentication examples
- `transactions.py` - Advanced transaction management with Transaction class
- `query_builder.py` - Query builder usage examples
- `batch_operations.py` - Batch create operations
- `performance_monitoring.py` - Performance monitoring examples
- `multi_database.py` - Multi-database support examples

Run examples with:

```bash
python examples/basic_usage.py
python examples/with_auth.py
python examples/transactions.py
python examples/query_builder.py
python examples/batch_operations.py
python examples/performance_monitoring.py
python examples/multi_database.py
```

## Testing

Run tests with:

```bash
# Install development dependencies
pip install -e ".[dev]"

# Run unit tests
pytest

# Run with coverage
pytest --cov=nexus_sdk --cov-report=html
```

## License

Licensed under the Apache License, Version 2.0.

See [LICENSE](LICENSE) for details.
