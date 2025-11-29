# Nexus .NET SDK

Official .NET client library for [Nexus](https://github.com/hivellm/nexus), a high-performance Neo4j-compatible graph database.

## Features

- **Complete Cypher Support** - Execute any Cypher query with parameters
- **CRUD Operations** - Simplified methods for nodes and relationships
- **Batch Operations** - Create multiple nodes/relationships efficiently
- **Transaction Support** - Full ACID transaction management with async/await
- **Schema Management** - Indexes, labels, and relationship types
- **Authentication** - API keys and bearer tokens
- **Async/Await** - Fully asynchronous API with CancellationToken support
- **Type-Safe** - Strongly typed models with nullable reference types
- **.NET 8+** - Built for modern .NET with latest language features

## Installation

### NuGet Package Manager

```bash
dotnet add package Nexus.SDK
```

### Package Manager Console

```powershell
Install-Package Nexus.SDK
```

### .csproj Reference

```xml
<PackageReference Include="Nexus.SDK" Version="0.1.0" />
```

## Quick Start

```csharp
using Nexus.SDK;

// Create client
var config = new NexusClientConfig
{
    BaseUrl = "http://localhost:15474",
    ApiKey = "your-api-key", // Optional
    Timeout = TimeSpan.FromSeconds(30)
};

using var client = new NexusClient(config);

// Check connection
await client.PingAsync();

// Execute Cypher query
var result = await client.ExecuteCypherAsync(@"
    MATCH (n:Person)
    WHERE n.age > $minAge
    RETURN n.name, n.age
    ORDER BY n.age DESC
", new Dictionary<string, object?>
{
    ["minAge"] = 25
});

// Process results
foreach (var row in result.Rows)
{
    Console.WriteLine($"Name: {row["n.name"]}, Age: {row["n.age"]}");
}

Console.WriteLine($"Query took {result.Stats?.ExecutionTimeMs:F2}ms");
```

## Usage Examples

### Creating Nodes

```csharp
// Create a single node
var node = await client.CreateNodeAsync(
    new List<string> { "Person" },
    new Dictionary<string, object?>
    {
        ["name"] = "John Doe",
        ["age"] = 30,
        ["email"] = "john@example.com"
    });

Console.WriteLine($"Created node with ID: {node.Id}");

// Batch create multiple nodes
var nodes = await client.BatchCreateNodesAsync(new List<NodeInput>
{
    new()
    {
        Labels = new List<string> { "Person" },
        Properties = new Dictionary<string, object?> { ["name"] = "Alice", ["age"] = 28 }
    },
    new()
    {
        Labels = new List<string> { "Person" },
        Properties = new Dictionary<string, object?> { ["name"] = "Bob", ["age"] = 32 }
    }
});

Console.WriteLine($"Created {nodes.Count} nodes");
```

### Creating Relationships

```csharp
// Create a relationship
var rel = await client.CreateRelationshipAsync(
    node1.Id,
    node2.Id,
    "KNOWS",
    new Dictionary<string, object?>
    {
        ["since"] = "2020",
        ["strength"] = 0.8
    });

Console.WriteLine($"Created relationship: {rel.Type}");

// Batch create relationships
var rels = await client.BatchCreateRelationshipsAsync(new List<RelationshipInput>
{
    new()
    {
        StartNode = "1",
        EndNode = "2",
        Type = "KNOWS",
        Properties = new Dictionary<string, object?> { ["since"] = "2020" }
    },
    new()
    {
        StartNode = "2",
        EndNode = "3",
        Type = "WORKS_WITH",
        Properties = new Dictionary<string, object?> { ["project"] = "GraphDB" }
    }
});
```

### Reading and Updating Data

```csharp
// Get node by ID
var node = await client.GetNodeAsync("1");
Console.WriteLine($"Node: {node.Properties["name"]}");

// Update node properties
var updated = await client.UpdateNodeAsync("1", new Dictionary<string, object?>
{
    ["age"] = 31,
    ["updated_at"] = DateTimeOffset.UtcNow.ToUnixTimeSeconds()
});

// Get relationship
var rel = await client.GetRelationshipAsync("r1");

// Delete node
await client.DeleteNodeAsync("1");

// Delete relationship
await client.DeleteRelationshipAsync("r1");
```

### Transactions

```csharp
// Begin transaction
using var tx = await client.BeginTransactionAsync();

try
{
    // Execute queries in transaction
    await tx.ExecuteCypherAsync(
        "CREATE (n:Person {name: $name})",
        new Dictionary<string, object?> { ["name"] = "Transaction User" });

    await tx.ExecuteCypherAsync(
        "CREATE (n:Person {name: $name})",
        new Dictionary<string, object?> { ["name"] = "Another User" });

    // Commit transaction
    await tx.CommitAsync();
    Console.WriteLine("Transaction committed successfully");
}
catch (Exception ex)
{
    // Rollback on error
    await tx.RollbackAsync();
    Console.WriteLine($"Transaction rolled back: {ex.Message}");
}
```

### Schema Management

```csharp
// List all labels
var labels = await client.ListLabelsAsync();
Console.WriteLine($"Labels: {string.Join(", ", labels)}");

// List all relationship types
var types = await client.ListRelationshipTypesAsync();
Console.WriteLine($"Relationship types: {string.Join(", ", types)}");

// Create index
await client.CreateIndexAsync("person_name_idx", "Person", new List<string> { "name" });

// List indexes
var indexes = await client.ListIndexesAsync();
foreach (var idx in indexes)
{
    Console.WriteLine($"Index: {idx.Name} on {idx.Label}({string.Join(", ", idx.Properties)})");
}

// Delete index
await client.DeleteIndexAsync("person_name_idx");
```

### Cancellation and Timeouts

```csharp
// Use CancellationToken
using var cts = new CancellationTokenSource(TimeSpan.FromSeconds(5));

try
{
    var result = await client.ExecuteCypherAsync(
        "MATCH (n) RETURN count(n)",
        null,
        cts.Token);
}
catch (OperationCanceledException)
{
    Console.WriteLine("Query was cancelled or timed out");
}

// Use timeout per request
var config = new NexusClientConfig
{
    BaseUrl = "http://localhost:15474",
    Timeout = TimeSpan.FromSeconds(10) // Global timeout
};
```

### Error Handling

```csharp
try
{
    var result = await client.ExecuteCypherAsync("INVALID QUERY");
}
catch (NexusApiException ex)
{
    Console.WriteLine($"HTTP {ex.StatusCode}: {ex.ResponseBody}");

    switch (ex.StatusCode)
    {
        case 400:
            Console.WriteLine("Bad request - check your query syntax");
            break;
        case 401:
            Console.WriteLine("Unauthorized - check your API key");
            break;
        case 404:
            Console.WriteLine("Not found");
            break;
        case 500:
            Console.WriteLine("Server error");
            break;
    }
}
catch (NexusException ex)
{
    Console.WriteLine($"Nexus SDK error: {ex.Message}");
}
catch (HttpRequestException ex)
{
    Console.WriteLine($"Network error: {ex.Message}");
}
```

## Authentication

### API Key

```csharp
var client = new NexusClient(new NexusClientConfig
{
    BaseUrl = "http://localhost:15474",
    ApiKey = "your-api-key"
});
```

### Username/Password

```csharp
var client = new NexusClient(new NexusClientConfig
{
    BaseUrl = "http://localhost:15474",
    Username = "admin",
    Password = "password"
});
```

### Bearer Token

```csharp
var client = new NexusClient(new NexusClientConfig
{
    BaseUrl = "http://localhost:15474"
});

// Set token manually after authentication
client.SetToken("your-jwt-token");
```

## High Availability with Replication

Nexus supports master-replica replication for high availability and read scaling.
Use the **master** for all write operations and **replicas** for read operations.

### NexusCluster Implementation

```csharp
using Nexus.SDK;

namespace Nexus.SDK;

/// <summary>
/// Client for Nexus cluster with master-replica topology.
/// Routes writes to master, reads to replicas (round-robin).
/// </summary>
public class NexusCluster : IAsyncDisposable
{
    private readonly NexusClient _master;
    private readonly List<NexusClient> _replicas;
    private int _replicaIndex;

    /// <summary>
    /// Creates a new cluster client.
    /// </summary>
    /// <param name="masterUrl">URL of the master node (for writes)</param>
    /// <param name="replicaUrls">List of replica URLs (for reads)</param>
    public NexusCluster(string masterUrl, IEnumerable<string> replicaUrls)
    {
        _master = new NexusClient(new NexusClientConfig { BaseUrl = masterUrl });
        _replicas = replicaUrls
            .Select(url => new NexusClient(new NexusClientConfig { BaseUrl = url }))
            .ToList();
        _replicaIndex = 0;
    }

    /// <summary>
    /// Gets the next replica using thread-safe round-robin selection.
    /// </summary>
    private NexusClient GetNextReplica()
    {
        if (_replicas.Count == 0)
            return _master;

        var index = Interlocked.Increment(ref _replicaIndex) % _replicas.Count;
        return _replicas[index];
    }

    /// <summary>
    /// Executes a write query on the master node.
    /// </summary>
    public Task<QueryResult> WriteAsync(
        string query,
        Dictionary<string, object?>? parameters = null,
        CancellationToken cancellationToken = default)
    {
        return _master.ExecuteCypherAsync(query, parameters, cancellationToken);
    }

    /// <summary>
    /// Executes a read query on a replica node (round-robin selection).
    /// </summary>
    public Task<QueryResult> ReadAsync(
        string query,
        Dictionary<string, object?>? parameters = null,
        CancellationToken cancellationToken = default)
    {
        return GetNextReplica().ExecuteCypherAsync(query, parameters, cancellationToken);
    }

    /// <summary>
    /// Gets the master client for direct access.
    /// </summary>
    public NexusClient Master => _master;

    /// <summary>
    /// Gets all replica clients.
    /// </summary>
    public IReadOnlyList<NexusClient> Replicas => _replicas.AsReadOnly();

    /// <summary>
    /// Disposes all client connections.
    /// </summary>
    public async ValueTask DisposeAsync()
    {
        _master.Dispose();
        foreach (var replica in _replicas)
        {
            replica.Dispose();
        }
        await Task.CompletedTask;
    }
}
```

### Usage Example

```csharp
using Nexus.SDK;

// Connect to cluster
// Master handles all writes, replicas handle reads
await using var cluster = new NexusCluster(
    "http://master:15474",
    new[] { "http://replica1:15474", "http://replica2:15474" }
);

// Write operations go to master
await cluster.WriteAsync(
    "CREATE (n:Person {name: $name, age: $age}) RETURN n",
    new Dictionary<string, object?>
    {
        ["name"] = "Alice",
        ["age"] = 30
    }
);

// Read operations are distributed across replicas
var result = await cluster.ReadAsync(
    "MATCH (n:Person) WHERE n.age > $minAge RETURN n",
    new Dictionary<string, object?> { ["minAge"] = 25 }
);
Console.WriteLine($"Found {result.Rows.Count} persons");

// High-volume reads are load-balanced across replicas
for (int i = 0; i < 100; i++)
{
    await cluster.ReadAsync("MATCH (n) RETURN count(n) as total");
}
```

### Replication Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      Application                             │
│   ┌─────────────────────────────────────────────────────┐   │
│   │              NexusCluster Client                     │   │
│   │   WriteAsync() ────────┐   ReadAsync() ──────────┐   │  │
│   └────────────────────────┼─────────────────────────┼───┘  │
└────────────────────────────┼─────────────────────────┼──────┘
                             │                         │
                             ▼                         ▼
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

### Starting a Nexus Cluster

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

### Monitoring Replication Status

```csharp
using System.Net.Http.Json;

public record ReplicationStatus(
    string Role,
    bool Running,
    string Mode,
    int? ReplicaCount
);

public record MasterStats(
    ulong EntriesReplicated,
    uint ConnectedReplicas
);

public record ReplicaInfo(
    string Id,
    string Addr,
    ulong Lag,
    bool Healthy
);

public record ReplicasResponse(List<ReplicaInfo> Replicas);

public static async Task CheckReplicationStatus(string masterUrl)
{
    using var httpClient = new HttpClient();

    // Check master status
    var status = await httpClient.GetFromJsonAsync<ReplicationStatus>(
        $"{masterUrl}/replication/status"
    );
    Console.WriteLine($"Role: {status?.Role}");
    Console.WriteLine($"Running: {status?.Running}");
    Console.WriteLine($"Connected replicas: {status?.ReplicaCount ?? 0}");

    // Get master stats
    var stats = await httpClient.GetFromJsonAsync<MasterStats>(
        $"{masterUrl}/replication/master/stats"
    );
    Console.WriteLine($"Entries replicated: {stats?.EntriesReplicated}");
    Console.WriteLine($"Connected replicas: {stats?.ConnectedReplicas}");

    // List replicas
    var replicas = await httpClient.GetFromJsonAsync<ReplicasResponse>(
        $"{masterUrl}/replication/replicas"
    );
    if (replicas?.Replicas != null)
    {
        foreach (var replica in replicas.Replicas)
        {
            Console.WriteLine($"  - {replica.Id}: lag={replica.Lag}, healthy={replica.Healthy}");
        }
    }
}
```

## Configuration

```csharp
public class NexusClientConfig
{
    /// <summary>
    /// Base URL of the Nexus server (required).
    /// </summary>
    public string BaseUrl { get; set; } = "http://localhost:15474";

    /// <summary>
    /// API key for authentication (optional).
    /// </summary>
    public string? ApiKey { get; set; }

    /// <summary>
    /// Username for authentication (optional).
    /// </summary>
    public string? Username { get; set; }

    /// <summary>
    /// Password for authentication (optional).
    /// </summary>
    public string? Password { get; set; }

    /// <summary>
    /// HTTP request timeout (default: 30 seconds).
    /// </summary>
    public TimeSpan Timeout { get; set; } = TimeSpan.FromSeconds(30);
}
```

## Models

### Node

```csharp
public class Node
{
    public string Id { get; set; }                          // Unique identifier
    public List<string> Labels { get; set; }                // Node labels
    public Dictionary<string, object?> Properties { get; set; } // Properties
}
```

### Relationship

```csharp
public class Relationship
{
    public string Id { get; set; }                          // Unique identifier
    public string Type { get; set; }                        // Relationship type
    public string StartNode { get; set; }                   // Start node ID
    public string EndNode { get; set; }                     // End node ID
    public Dictionary<string, object?> Properties { get; set; } // Properties
}
```

### QueryResult

```csharp
public class QueryResult
{
    public List<string> Columns { get; set; }               // Column names
    public List<Dictionary<string, object?>> Rows { get; set; } // Result rows
    public QueryStats? Stats { get; set; }                  // Execution stats
}

public class QueryStats
{
    public int NodesCreated { get; set; }
    public int NodesDeleted { get; set; }
    public int RelationshipsCreated { get; set; }
    public int RelationshipsDeleted { get; set; }
    public int PropertiesSet { get; set; }
    public double ExecutionTimeMs { get; set; }
}
```

## Testing

Run tests with:

```bash
dotnet test
```

Run with coverage:

```bash
dotnet test --collect:"XPlat Code Coverage"
```

## Examples

See the [Examples](./Examples) directory for complete working examples:

- `BasicUsage.cs` - Basic CRUD operations
- `Transactions.cs` - Transaction management
- `BatchOperations.cs` - Batch node/relationship creation
- `SchemaManagement.cs` - Working with indexes and schema

## Performance Tips

1. **Use Batch Operations** - For creating multiple nodes/relationships
2. **Dispose Clients** - Use `using` statements to properly dispose HttpClient
3. **Connection Pooling** - HttpClient reuses connections automatically
4. **CancellationTokens** - Pass tokens to prevent hanging operations
5. **Parameterized Queries** - Always use parameters instead of string concatenation
6. **Transactions** - Group related operations for consistency and performance

## Requirements

- .NET 8.0 or higher
- Nexus server 0.11.0 or higher

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
- [Nexus TypeScript SDK](../typescript)
- [Nexus Python SDK](../python)
- [Nexus Rust SDK](../rust)
