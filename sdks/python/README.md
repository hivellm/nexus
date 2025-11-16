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

### Using Context Manager

```python
async with NexusClient("http://localhost:15474") as client:
    result = await client.execute_cypher("MATCH (n) RETURN n LIMIT 10", None)
    print(f"Found {len(result.rows)} rows")
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

Run examples with:

```bash
python examples/basic_usage.py
python examples/with_auth.py
python examples/transactions.py
python examples/query_builder.py
python examples/batch_operations.py
python examples/performance_monitoring.py
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
