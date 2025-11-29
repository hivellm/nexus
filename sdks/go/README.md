# Nexus Go SDK

Official Go client library for [Nexus](https://github.com/hivellm/nexus), a high-performance Neo4j-compatible graph database.

## Features

- **Complete Cypher Support** - Execute any Cypher query with parameters
- **CRUD Operations** - Simplified methods for nodes and relationships
- **Batch Operations** - Create multiple nodes/relationships efficiently
- **Transaction Support** - Full ACID transaction management
- **Schema Management** - Indexes, labels, and relationship types
- **Authentication** - API keys and bearer tokens
- **Context Support** - Full Go context integration for cancellation and timeouts
- **Type-Safe** - Strongly typed API with proper error handling

## Installation

```bash
go get github.com/hivellm/nexus-go
```

## Quick Start

```go
package main

import (
    "context"
    "fmt"
    "log"
    "time"

    nexus "github.com/hivellm/nexus-go"
)

func main() {
    // Create client
    client := nexus.NewClient(nexus.Config{
        BaseURL:  "http://localhost:15474",
        APIKey:   "your-api-key", // Optional
        Timeout:  30 * time.Second,
    })

    ctx := context.Background()

    // Check connection
    if err := client.Ping(ctx); err != nil {
        log.Fatal("Failed to connect:", err)
    }

    // Execute Cypher query
    result, err := client.ExecuteCypher(ctx, `
        MATCH (n:Person)
        WHERE n.age > $minAge
        RETURN n.name, n.age
        ORDER BY n.age DESC
    `, map[string]interface{}{
        "minAge": 25,
    })
    if err != nil {
        log.Fatal(err)
    }

    // Process results
    for _, row := range result.Rows {
        fmt.Printf("Name: %v, Age: %v\n", row["n.name"], row["n.age"])
    }

    fmt.Printf("Query took %.2fms\n", result.Stats.ExecutionTimeMs)
}
```

## Usage Examples

### Creating Nodes

```go
// Create a single node
node, err := client.CreateNode(ctx, []string{"Person"}, map[string]interface{}{
    "name": "John Doe",
    "age":  30,
    "email": "john@example.com",
})
if err != nil {
    log.Fatal(err)
}
fmt.Printf("Created node with ID: %s\n", node.ID)

// Batch create multiple nodes
nodes, err := client.BatchCreateNodes(ctx, []struct {
    Labels     []string
    Properties map[string]interface{}
}{
    {
        Labels:     []string{"Person"},
        Properties: map[string]interface{}{"name": "Alice", "age": 28},
    },
    {
        Labels:     []string{"Person"},
        Properties: map[string]interface{}{"name": "Bob", "age": 32},
    },
})
if err != nil {
    log.Fatal(err)
}
fmt.Printf("Created %d nodes\n", len(nodes))
```

### Creating Relationships

```go
// Create a relationship
rel, err := client.CreateRelationship(ctx, node1.ID, node2.ID, "KNOWS", map[string]interface{}{
    "since": "2020",
    "strength": 0.8,
})
if err != nil {
    log.Fatal(err)
}
fmt.Printf("Created relationship: %s\n", rel.Type)

// Batch create relationships
rels, err := client.BatchCreateRelationships(ctx, []struct {
    StartNode  string
    EndNode    string
    Type       string
    Properties map[string]interface{}
}{
    {
        StartNode:  "1",
        EndNode:    "2",
        Type:       "KNOWS",
        Properties: map[string]interface{}{"since": "2020"},
    },
    {
        StartNode:  "2",
        EndNode:    "3",
        Type:       "WORKS_WITH",
        Properties: map[string]interface{}{"project": "GraphDB"},
    },
})
if err != nil {
    log.Fatal(err)
}
```

### Reading and Updating Data

```go
// Get node by ID
node, err := client.GetNode(ctx, "1")
if err != nil {
    log.Fatal(err)
}
fmt.Printf("Node: %+v\n", node)

// Update node properties
updatedNode, err := client.UpdateNode(ctx, "1", map[string]interface{}{
    "age": 31,
    "updated_at": time.Now().Unix(),
})
if err != nil {
    log.Fatal(err)
}

// Get relationship
rel, err := client.GetRelationship(ctx, "r1")
if err != nil {
    log.Fatal(err)
}

// Delete node
if err := client.DeleteNode(ctx, "1"); err != nil {
    log.Fatal(err)
}

// Delete relationship
if err := client.DeleteRelationship(ctx, "r1"); err != nil {
    log.Fatal(err)
}
```

### Transactions

```go
// Begin transaction
tx, err := client.BeginTransaction(ctx)
if err != nil {
    log.Fatal(err)
}

// Execute queries in transaction
_, err = tx.ExecuteCypher(ctx, "CREATE (n:Person {name: $name})", map[string]interface{}{
    "name": "Transaction User",
})
if err != nil {
    // Rollback on error
    tx.Rollback(ctx)
    log.Fatal(err)
}

// Commit transaction
if err := tx.Commit(ctx); err != nil {
    log.Fatal(err)
}
fmt.Println("Transaction committed successfully")
```

### Schema Management

```go
// List all labels
labels, err := client.ListLabels(ctx)
if err != nil {
    log.Fatal(err)
}
fmt.Printf("Labels: %v\n", labels)

// List all relationship types
types, err := client.ListRelationshipTypes(ctx)
if err != nil {
    log.Fatal(err)
}
fmt.Printf("Relationship types: %v\n", types)

// Create index
err = client.CreateIndex(ctx, "person_name_idx", "Person", []string{"name"})
if err != nil {
    log.Fatal(err)
}

// List indexes
indexes, err := client.ListIndexes(ctx)
if err != nil {
    log.Fatal(err)
}
for _, idx := range indexes {
    fmt.Printf("Index: %s on %s(%v)\n", idx.Name, idx.Label, idx.Properties)
}

// Delete index
err = client.DeleteIndex(ctx, "person_name_idx")
if err != nil {
    log.Fatal(err)
}
```

### Context and Timeouts

```go
// Use context with timeout
ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
defer cancel()

result, err := client.ExecuteCypher(ctx, "MATCH (n) RETURN count(n)", nil)
if err != nil {
    if ctx.Err() == context.DeadlineExceeded {
        fmt.Println("Query timed out")
    }
    log.Fatal(err)
}

// Use context with cancellation
ctx, cancel := context.WithCancel(context.Background())

go func() {
    time.Sleep(2 * time.Second)
    cancel() // Cancel the query
}()

_, err = client.ExecuteCypher(ctx, "MATCH (n)-[r*]-(m) RETURN n, m", nil)
if err != nil {
    if ctx.Err() == context.Canceled {
        fmt.Println("Query was cancelled")
    }
}
```

### Error Handling

```go
result, err := client.ExecuteCypher(ctx, "INVALID QUERY", nil)
if err != nil {
    // Check for Nexus API errors
    if nexusErr, ok := err.(*nexus.Error); ok {
        fmt.Printf("HTTP %d: %s\n", nexusErr.StatusCode, nexusErr.Message)

        switch nexusErr.StatusCode {
        case 400:
            fmt.Println("Bad request - check your query syntax")
        case 401:
            fmt.Println("Unauthorized - check your API key")
        case 404:
            fmt.Println("Not found")
        case 500:
            fmt.Println("Server error")
        }
    } else {
        // Network or other error
        fmt.Printf("Error: %v\n", err)
    }
}
```

## Authentication

### API Key

```go
client := nexus.NewClient(nexus.Config{
    BaseURL: "http://localhost:15474",
    APIKey:  "your-api-key",
})
```

### Username/Password (for token generation)

```go
client := nexus.NewClient(nexus.Config{
    BaseURL:  "http://localhost:15474",
    Username: "admin",
    Password: "password",
})
```

### Bearer Token

```go
client := nexus.NewClient(nexus.Config{
    BaseURL: "http://localhost:15474",
})
// Set token manually after authentication
client.token = "your-jwt-token"
```

## High Availability with Replication

Nexus supports master-replica replication for high availability and read scaling.
Use the **master** for all write operations and **replicas** for read operations.

### NexusCluster Implementation

```go
package nexus

import (
    "context"
    "sync/atomic"
    "time"
)

// NexusCluster manages a cluster of Nexus clients with master-replica topology.
// Writes go to master, reads are distributed across replicas using round-robin.
type NexusCluster struct {
    master       *Client
    replicas     []*Client
    replicaIndex uint64
}

// NewCluster creates a new cluster client.
//
// Parameters:
//   - masterURL: URL of the master node (for writes)
//   - replicaURLs: List of replica URLs (for reads)
func NewCluster(masterURL string, replicaURLs []string) (*NexusCluster, error) {
    master := NewClient(Config{
        BaseURL: masterURL,
        Timeout: 30 * time.Second,
    })

    replicas := make([]*Client, len(replicaURLs))
    for i, url := range replicaURLs {
        replicas[i] = NewClient(Config{
            BaseURL: url,
            Timeout: 30 * time.Second,
        })
    }

    return &NexusCluster{
        master:   master,
        replicas: replicas,
    }, nil
}

// getNextReplica returns the next replica using atomic round-robin selection.
func (c *NexusCluster) getNextReplica() *Client {
    if len(c.replicas) == 0 {
        return c.master
    }
    index := atomic.AddUint64(&c.replicaIndex, 1) % uint64(len(c.replicas))
    return c.replicas[index]
}

// Write executes a write query on the master node.
func (c *NexusCluster) Write(ctx context.Context, query string, params map[string]interface{}) (*QueryResult, error) {
    return c.master.ExecuteCypher(ctx, query, params)
}

// Read executes a read query on a replica node (round-robin selection).
func (c *NexusCluster) Read(ctx context.Context, query string, params map[string]interface{}) (*QueryResult, error) {
    return c.getNextReplica().ExecuteCypher(ctx, query, params)
}

// Master returns the master client for direct access.
func (c *NexusCluster) Master() *Client {
    return c.master
}

// Replicas returns all replica clients.
func (c *NexusCluster) Replicas() []*Client {
    return c.replicas
}
```

### Usage Example

```go
package main

import (
    "context"
    "fmt"
    "log"

    nexus "github.com/hivellm/nexus-go"
)

func main() {
    ctx := context.Background()

    // Connect to cluster
    // Master handles all writes, replicas handle reads
    cluster, err := nexus.NewCluster(
        "http://master:15474",
        []string{
            "http://replica1:15474",
            "http://replica2:15474",
        },
    )
    if err != nil {
        log.Fatal(err)
    }

    // Write operations go to master
    _, err = cluster.Write(ctx,
        "CREATE (n:Person {name: $name, age: $age}) RETURN n",
        map[string]interface{}{"name": "Alice", "age": 30},
    )
    if err != nil {
        log.Fatal(err)
    }

    // Read operations are distributed across replicas
    result, err := cluster.Read(ctx,
        "MATCH (n:Person) WHERE n.age > $minAge RETURN n",
        map[string]interface{}{"minAge": 25},
    )
    if err != nil {
        log.Fatal(err)
    }
    fmt.Printf("Found %d persons\n", len(result.Rows))

    // High-volume reads are load-balanced across replicas
    for i := 0; i < 100; i++ {
        _, err := cluster.Read(ctx, "MATCH (n) RETURN count(n) as total", nil)
        if err != nil {
            log.Printf("Read error: %v", err)
        }
    }
}
```

### Replication Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      Application                             │
│   ┌─────────────────────────────────────────────────────┐   │
│   │              NexusCluster Client                     │   │
│   │   Write() ─────────────┐     Read() ─────────────┐   │  │
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

```go
package main

import (
    "encoding/json"
    "fmt"
    "net/http"
)

type ReplicationStatus struct {
    Role         string `json:"role"`
    Running      bool   `json:"running"`
    Mode         string `json:"mode"`
    ReplicaCount *int   `json:"replica_count,omitempty"`
}

type MasterStats struct {
    EntriesReplicated  uint64 `json:"entries_replicated"`
    ConnectedReplicas  uint32 `json:"connected_replicas"`
}

type ReplicaInfo struct {
    ID      string `json:"id"`
    Addr    string `json:"addr"`
    Lag     uint64 `json:"lag"`
    Healthy bool   `json:"healthy"`
}

type ReplicasResponse struct {
    Replicas []ReplicaInfo `json:"replicas"`
}

func checkReplicationStatus(masterURL string) error {
    // Check master status
    resp, err := http.Get(masterURL + "/replication/status")
    if err != nil {
        return err
    }
    defer resp.Body.Close()

    var status ReplicationStatus
    if err := json.NewDecoder(resp.Body).Decode(&status); err != nil {
        return err
    }
    fmt.Printf("Role: %s\n", status.Role)
    fmt.Printf("Running: %v\n", status.Running)
    if status.ReplicaCount != nil {
        fmt.Printf("Connected replicas: %d\n", *status.ReplicaCount)
    }

    // Get master stats
    resp, err = http.Get(masterURL + "/replication/master/stats")
    if err != nil {
        return err
    }
    defer resp.Body.Close()

    var stats MasterStats
    if err := json.NewDecoder(resp.Body).Decode(&stats); err != nil {
        return err
    }
    fmt.Printf("Entries replicated: %d\n", stats.EntriesReplicated)
    fmt.Printf("Connected replicas: %d\n", stats.ConnectedReplicas)

    // List replicas
    resp, err = http.Get(masterURL + "/replication/replicas")
    if err != nil {
        return err
    }
    defer resp.Body.Close()

    var replicas ReplicasResponse
    if err := json.NewDecoder(resp.Body).Decode(&replicas); err != nil {
        return err
    }
    for _, r := range replicas.Replicas {
        fmt.Printf("  - %s: lag=%d, healthy=%v\n", r.ID, r.Lag, r.Healthy)
    }

    return nil
}
```

## Configuration Options

```go
type Config struct {
    BaseURL  string        // Nexus server URL (required)
    APIKey   string        // API key for authentication (optional)
    Username string        // Username for authentication (optional)
    Password string        // Password for authentication (optional)
    Timeout  time.Duration // HTTP client timeout (default: 30s)
}
```

## Types

### Node

```go
type Node struct {
    ID         string                 // Unique node identifier
    Labels     []string               // Node labels
    Properties map[string]interface{} // Node properties
}
```

### Relationship

```go
type Relationship struct {
    ID         string                 // Unique relationship identifier
    Type       string                 // Relationship type
    StartNode  string                 // Start node ID
    EndNode    string                 // End node ID
    Properties map[string]interface{} // Relationship properties
}
```

### QueryResult

```go
type QueryResult struct {
    Columns []string                 // Column names
    Rows    []map[string]interface{} // Result rows
    Stats   *QueryStats              // Execution statistics
}

type QueryStats struct {
    NodesCreated         int     // Number of nodes created
    NodesDeleted         int     // Number of nodes deleted
    RelationshipsCreated int     // Number of relationships created
    RelationshipsDeleted int     // Number of relationships deleted
    PropertiesSet        int     // Number of properties set
    ExecutionTimeMs      float64 // Query execution time in milliseconds
}
```

## Testing

Run tests with:

```bash
go test -v
```

Run tests with coverage:

```bash
go test -v -cover -coverprofile=coverage.out
go tool cover -html=coverage.out
```

## Examples

See the [examples](./examples) directory for complete working examples:

- `basic_usage.go` - Basic CRUD operations
- `transactions.go` - Transaction management
- `batch_operations.go` - Batch node/relationship creation
- `schema_management.go` - Working with indexes and schema

## Performance Tips

1. **Use Batch Operations** - For creating multiple nodes/relationships, use batch methods for better performance
2. **Connection Pooling** - The HTTP client reuses connections automatically
3. **Context Timeouts** - Set appropriate timeouts to prevent hanging operations
4. **Parameterized Queries** - Always use parameters instead of string concatenation
5. **Transactions** - Group related operations in transactions for consistency and performance

## Requirements

- Go 1.21 or higher
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

- [Nexus TypeScript SDK](../typescript)
- [Nexus Python SDK](../python)
- [Nexus Rust SDK](../rust)
