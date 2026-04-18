---
title: Go SDK
module: sdks
id: go-sdk
order: 5
description: Go SDK guide
tags: [go, sdk, client, library]
---

# Go SDK

Complete guide for the Nexus Go SDK.

## Installation

```bash
go get github.com/hivellm/nexus-sdk-go
```

## Quick Start

```go
package main

import (
    "fmt"
    "log"
    "github.com/hivellm/nexus-sdk-go"
)

func main() {
    // Create client
    client := nexus.NewClient("http://localhost:15474")
    
    // Execute Cypher query
    result, err := client.ExecuteCypher("MATCH (n:Person) RETURN n LIMIT 10", nil)
    if err != nil {
        log.Fatal(err)
    }
    
    fmt.Printf("Found %d rows\n", len(result.Rows))
}
```

## Authentication

### API Key

```go
client := nexus.NewClient("http://localhost:15474")
client.SetAPIKey("nx_abc123def456...")
```

### JWT Token

```go
client := nexus.NewClient("http://localhost:15474")
token, err := client.Login("username", "password")
if err != nil {
    log.Fatal(err)
}

client.SetJWTToken(token)
```

## Basic Operations

### Execute Cypher Query

```go
// Simple query
result, err := client.ExecuteCypher("MATCH (n:Person) RETURN n LIMIT 10", nil)

// Query with parameters
params := map[string]interface{}{
    "name": "Alice",
}
result, err := client.ExecuteCypher(
    "MATCH (n:Person {name: $name}) RETURN n",
    params,
)
```

### Create Node

```go
// Using Cypher
result, err := client.ExecuteCypher(
    "CREATE (n:Person {name: $name, age: $age}) RETURN n",
    map[string]interface{}{
        "name": "Alice",
        "age": 30,
    },
)

// Using API
node, err := client.CreateNode(
    []string{"Person"},
    map[string]interface{}{
        "name": "Alice",
        "age": 30,
    },
)
```

## Vector Search

### Create Node with Vector

```go
vector := []float64{0.1, 0.2, 0.3, 0.4}
result, err := client.ExecuteCypher(
    "CREATE (n:Person {name: $name, vector: $vector}) RETURN n",
    map[string]interface{}{
        "name": "Alice",
        "vector": vector,
    },
)
```

### KNN Search

```go
queryVector := []float64{0.1, 0.2, 0.3, 0.4}
result, err := client.ExecuteCypher(
    `MATCH (n:Person)
     WHERE n.vector IS NOT NULL
     RETURN n.name, n.vector
     ORDER BY n.vector <-> $query_vector
     LIMIT 5`,
    map[string]interface{}{
        "query_vector": queryVector,
    },
)
```

## Database Management

```go
// List databases
databases, err := client.ListDatabases()

// Create database
err := client.CreateDatabase("mydb")

// Switch database
err := client.SwitchDatabase("mydb")

// Get current database
current, err := client.GetCurrentDatabase()
```

## Error Handling

```go
result, err := client.ExecuteCypher("MATCH (n) RETURN n", nil)
if err != nil {
    switch e := err.(type) {
    case *nexus.SyntaxError:
        log.Printf("Syntax error: %s", e.Message)
    case *nexus.AuthenticationError:
        log.Printf("Auth error: %s", e.Message)
    default:
        log.Printf("Error: %v", err)
    }
}
```

## Related Topics

- [SDKs Overview](./SDKS.md) - Compare all SDKs
- [API Reference](../api/API_REFERENCE.md) - REST API documentation
- [Cypher Guide](../cypher/CYPHER.md) - Cypher query language

