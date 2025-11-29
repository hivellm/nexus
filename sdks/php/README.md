# Nexus PHP SDK

Official PHP client library for [Nexus](https://github.com/hivellm/nexus), a high-performance Neo4j-compatible graph database.

## Features

- **Complete Cypher Support** - Execute any Cypher query with parameters
- **CRUD Operations** - Simplified methods for nodes and relationships
- **Batch Operations** - Create multiple nodes/relationships efficiently
- **Transaction Support** - Full ACID transaction management
- **Schema Management** - Indexes, labels, and relationship types
- **Authentication** - API keys and bearer tokens
- **Modern PHP** - PHP 8.1+ with strict types and readonly properties
- **PSR Compliant** - Follows PSR-4, PSR-12, and uses PSR-18 HTTP client
- **Type Safe** - Full type hints and return types

## Requirements

- PHP 8.1 or higher
- Composer
- Nexus server 0.11.0 or higher

## Installation

```bash
composer require hivellm/nexus-php
```

## Quick Start

```php
<?php

require 'vendor/autoload.php';

use Nexus\SDK\Config;
use Nexus\SDK\NexusClient;

// Create client
$config = new Config(
    baseUrl: 'http://localhost:15474',
    apiKey: 'your-api-key', // Optional
    timeout: 30
);

$client = new NexusClient($config);

// Check connection
$client->ping();

// Execute Cypher query
$result = $client->executeCypher(
    'MATCH (n:Person) WHERE n.age > $minAge RETURN n.name, n.age ORDER BY n.age DESC',
    ['minAge' => 25]
);

// Process results
foreach ($result->rows as $row) {
    echo "Name: {$row['n.name']}, Age: {$row['n.age']}\n";
}

echo "Query took {$result->stats?->executionTimeMs}ms\n";
```

## Usage Examples

### Creating Nodes

```php
// Create a single node
$node = $client->createNode(
    labels: ['Person'],
    properties: [
        'name' => 'John Doe',
        'age' => 30,
        'email' => 'john@example.com'
    ]
);

echo "Created node with ID: {$node->id}\n";

// Batch create multiple nodes
$nodes = $client->batchCreateNodes([
    [
        'labels' => ['Person'],
        'properties' => ['name' => 'Alice', 'age' => 28]
    ],
    [
        'labels' => ['Person'],
        'properties' => ['name' => 'Bob', 'age' => 32]
    ]
]);

echo "Created " . count($nodes) . " nodes\n";
```

### Creating Relationships

```php
// Create a relationship
$rel = $client->createRelationship(
    startNode: $node1->id,
    endNode: $node2->id,
    type: 'KNOWS',
    properties: [
        'since' => '2020',
        'strength' => 0.8
    ]
);

echo "Created relationship: {$rel->type}\n";

// Batch create relationships
$rels = $client->batchCreateRelationships([
    [
        'start_node' => '1',
        'end_node' => '2',
        'type' => 'KNOWS',
        'properties' => ['since' => '2020']
    ],
    [
        'start_node' => '2',
        'end_node' => '3',
        'type' => 'WORKS_WITH',
        'properties' => ['project' => 'GraphDB']
    ]
]);
```

### Reading and Updating Data

```php
// Get node by ID
$node = $client->getNode('1');
echo "Node: {$node->properties['name']}\n";

// Update node properties
$updated = $client->updateNode('1', [
    'age' => 31,
    'updated_at' => time()
]);

// Get relationship
$rel = $client->getRelationship('r1');

// Delete node
$client->deleteNode('1');

// Delete relationship
$client->deleteRelationship('r1');
```

### Transactions

```php
// Begin transaction
$tx = $client->beginTransaction();

try {
    // Execute queries in transaction
    $tx->executeCypher("CREATE (n:Person {name: \$name})", ['name' => 'Transaction User']);
    $tx->executeCypher("CREATE (n:Person {name: \$name})", ['name' => 'Another User']);

    // Commit transaction
    $tx->commit();
    echo "Transaction committed successfully\n";
} catch (Exception $e) {
    // Rollback on error
    $tx->rollback();
    echo "Transaction rolled back: {$e->getMessage()}\n";
}
```

### Schema Management

```php
// List all labels
$labels = $client->listLabels();
echo "Labels: " . implode(', ', $labels) . "\n";

// List all relationship types
$types = $client->listRelationshipTypes();
echo "Relationship types: " . implode(', ', $types) . "\n";

// Create index
$client->createIndex('person_name_idx', 'Person', ['name']);

// List indexes
$indexes = $client->listIndexes();
foreach ($indexes as $idx) {
    echo "Index: {$idx->name} on {$idx->label}(" . implode(', ', $idx->properties) . ")\n";
}

// Delete index
$client->deleteIndex('person_name_idx');
```

### Error Handling

```php
use Nexus\SDK\NexusApiException;
use Nexus\SDK\NexusException;

try {
    $result = $client->executeCypher('INVALID QUERY');
} catch (NexusApiException $e) {
    echo "HTTP {$e->statusCode}: {$e->responseBody}\n";

    match ($e->statusCode) {
        400 => echo "Bad request - check your query syntax\n",
        401 => echo "Unauthorized - check your API key\n",
        404 => echo "Not found\n",
        500 => echo "Server error\n",
        default => echo "Unknown error\n"
    };
} catch (NexusException $e) {
    echo "Nexus SDK error: {$e->getMessage()}\n";
} catch (Exception $e) {
    echo "Unexpected error: {$e->getMessage()}\n";
}
```

## Authentication

### API Key

```php
$config = new Config(
    baseUrl: 'http://localhost:15474',
    apiKey: 'your-api-key'
);
$client = new NexusClient($config);
```

### Username/Password

```php
$config = new Config(
    baseUrl: 'http://localhost:15474',
    username: 'admin',
    password: 'password'
);
$client = new NexusClient($config);
```

### Bearer Token

```php
$config = new Config(baseUrl: 'http://localhost:15474');
$client = new NexusClient($config);

// Set token manually after authentication
$client->setToken('your-jwt-token');
```

## Configuration

```php
class Config
{
    public function __construct(
        public string $baseUrl = 'http://localhost:15474',
        public ?string $apiKey = null,
        public ?string $username = null,
        public ?string $password = null,
        public int $timeout = 30
    ) {}
}
```

## Models

### Node

```php
class Node
{
    public string $id;                // Unique identifier
    public array $labels;             // Node labels
    public array $properties;         // Node properties
}
```

### Relationship

```php
class Relationship
{
    public string $id;                // Unique identifier
    public string $type;              // Relationship type
    public string $startNode;         // Start node ID
    public string $endNode;           // End node ID
    public array $properties;         // Relationship properties
}
```

### QueryResult

```php
class QueryResult
{
    public array $columns;            // Column names
    public array $rows;               // Result rows
    public ?QueryStats $stats;        // Execution statistics
}

class QueryStats
{
    public int $nodesCreated;
    public int $nodesDeleted;
    public int $relationshipsCreated;
    public int $relationshipsDeleted;
    public int $propertiesSet;
    public float $executionTimeMs;
}
```

## Testing

Run tests with PHPUnit:

```bash
composer test
```

Run static analysis:

```bash
composer phpstan
```

Check code style:

```bash
composer cs-check
```

Fix code style:

```bash
composer cs-fix
```

## Examples

See the [examples](./examples) directory for complete working examples:

- `basic_usage.php` - Basic CRUD operations
- `transactions.php` - Transaction management
- `batch_operations.php` - Batch node/relationship creation
- `schema_management.php` - Working with indexes and schema

## Performance Tips

1. **Use Batch Operations** - For creating multiple nodes/relationships
2. **Reuse Client** - Create one client instance and reuse it
3. **Parameterized Queries** - Always use parameters instead of string concatenation
4. **Transactions** - Group related operations for consistency and performance
5. **Connection Timeout** - Set appropriate timeout for your use case

## License

MIT License - see [LICENSE](../../LICENSE) file for details

## Contributing

Contributions are welcome! Please see [CONTRIBUTING.md](../../CONTRIBUTING.md) for guidelines.

## Support

- **Documentation**: https://github.com/hivellm/nexus
- **Issues**: https://github.com/hivellm/nexus/issues
- **Discussions**: https://github.com/hivellm/nexus/discussions

## Related

- [Nexus Go SDK](../go)
- [Nexus C# SDK](../csharp)
- [Nexus TypeScript SDK](../typescript)
- [Nexus Python SDK](../python)
- [Nexus Rust SDK](../rust)
