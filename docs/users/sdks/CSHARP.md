---
title: C# SDK
module: sdks
id: csharp-sdk
order: 6
description: .NET SDK guide
tags: [csharp, dotnet, sdk, client, library]
---

# C# SDK

Complete guide for the Nexus .NET SDK.

## Installation

```bash
dotnet add package Nexus.Sdk
```

## Quick Start

```csharp
using Nexus.Sdk;

// Create client
var client = new NexusClient("http://localhost:15474");

// Execute Cypher query
var result = await client.ExecuteCypherAsync(
    "MATCH (n:Person) RETURN n LIMIT 10",
    null
);

Console.WriteLine($"Found {result.Rows.Count} rows");
```

## Authentication

### API Key

```csharp
var client = new NexusClient("http://localhost:15474")
{
    ApiKey = "nx_abc123def456..."
};
```

### JWT Token

```csharp
var client = new NexusClient("http://localhost:15474");
var token = await client.LoginAsync("username", "password");

var authenticatedClient = new NexusClient("http://localhost:15474")
{
    JwtToken = token
};
```

## Basic Operations

### Execute Cypher Query

```csharp
// Simple query
var result = await client.ExecuteCypherAsync(
    "MATCH (n:Person) RETURN n LIMIT 10",
    null
);

// Query with parameters
var parameters = new Dictionary<string, object>
{
    { "name", "Alice" }
};
var result = await client.ExecuteCypherAsync(
    "MATCH (n:Person {name: $name}) RETURN n",
    parameters
);
```

### Create Node

```csharp
// Using Cypher
var result = await client.ExecuteCypherAsync(
    "CREATE (n:Person {name: $name, age: $age}) RETURN n",
    new Dictionary<string, object>
    {
        { "name", "Alice" },
        { "age", 30 }
    }
);

// Using API
var node = await client.CreateNodeAsync(
    new[] { "Person" },
    new Dictionary<string, object>
    {
        { "name", "Alice" },
        { "age", 30 }
    }
);
```

## Vector Search

### Create Node with Vector

```csharp
var vector = new[] { 0.1, 0.2, 0.3, 0.4 };
var result = await client.ExecuteCypherAsync(
    "CREATE (n:Person {name: $name, vector: $vector}) RETURN n",
    new Dictionary<string, object>
    {
        { "name", "Alice" },
        { "vector", vector }
    }
);
```

### KNN Search

```csharp
var queryVector = new[] { 0.1, 0.2, 0.3, 0.4 };
var result = await client.ExecuteCypherAsync(
    @"MATCH (n:Person)
      WHERE n.vector IS NOT NULL
      RETURN n.name, n.vector
      ORDER BY n.vector <-> $query_vector
      LIMIT 5",
    new Dictionary<string, object>
    {
        { "query_vector", queryVector }
    }
);
```

## Database Management

```csharp
// List databases
var databases = await client.ListDatabasesAsync();

// Create database
await client.CreateDatabaseAsync("mydb");

// Switch database
await client.SwitchDatabaseAsync("mydb");

// Get current database
var current = await client.GetCurrentDatabaseAsync();
```

## Error Handling

```csharp
try
{
    var result = await client.ExecuteCypherAsync("MATCH (n) RETURN n", null);
}
catch (NexusException ex)
{
    switch (ex)
    {
        case SyntaxError syntaxError:
            Console.WriteLine($"Syntax error: {syntaxError.Message}");
            break;
        case AuthenticationError authError:
            Console.WriteLine($"Auth error: {authError.Message}");
            break;
        default:
            Console.WriteLine($"Error: {ex.Message}");
            break;
    }
}
```

## Related Topics

- [SDKs Overview](./SDKS.md) - Compare all SDKs
- [API Reference](../api/API_REFERENCE.md) - REST API documentation
- [Cypher Guide](../cypher/CYPHER.md) - Cypher query language

