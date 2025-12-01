---
title: TypeScript SDK
module: sdks
id: typescript-sdk
order: 3
description: TypeScript/JavaScript SDK guide
tags: [typescript, javascript, sdk, client, library]
---

# TypeScript SDK

Complete guide for the Nexus TypeScript/JavaScript SDK.

## Installation

```bash
npm install @hivellm/nexus-sdk
```

## Quick Start

```typescript
import { NexusClient } from '@hivellm/nexus-sdk';

// Create client
const client = new NexusClient({
  baseUrl: 'http://localhost:15474'
});

// Execute Cypher query
const result = await client.executeCypher(
  'MATCH (n:Person) RETURN n.name, n.age LIMIT 10'
);

console.log(result.rows);
```

## Authentication

### API Key

```typescript
import { NexusClient } from '@hivellm/nexus-sdk';

const client = new NexusClient({
  baseUrl: 'http://localhost:15474',
  auth: {
    apiKey: 'nx_abc123def456...'
  }
});
```

### JWT Token

```typescript
import { NexusClient } from '@hivellm/nexus-sdk';

// Login first
const client = new NexusClient({ baseUrl: 'http://localhost:15474' });
const { token } = await client.login('username', 'password');

// Use token
const authenticatedClient = new NexusClient({
  baseUrl: 'http://localhost:15474',
  auth: {
    jwtToken: token
  }
});
```

## Basic Operations

### Execute Cypher Query

```typescript
// Simple query
const result = await client.executeCypher(
  'MATCH (n:Person) RETURN n LIMIT 10'
);

// Query with parameters
const result = await client.executeCypher(
  'MATCH (n:Person {name: $name}) RETURN n',
  { name: 'Alice' }
);
```

### Create Node

```typescript
// Using Cypher
const result = await client.executeCypher(
  'CREATE (n:Person {name: $name, age: $age}) RETURN n',
  { name: 'Alice', age: 30 }
);

// Using API
const node = await client.createNode({
  labels: ['Person'],
  properties: { name: 'Alice', age: 30 }
});
```

### Query Nodes

```typescript
// Query by label
const nodes = await client.queryNodes({
  label: 'Person',
  limit: 10
});

// Query with filter
const nodes = await client.queryNodes({
  label: 'Person',
  where: 'n.age > 25',
  limit: 10
});
```

## Vector Search

### Create Node with Vector

```typescript
const result = await client.executeCypher(
  `CREATE (n:Person {
    name: $name,
    vector: $vector
  }) RETURN n`,
  {
    name: 'Alice',
    vector: [0.1, 0.2, 0.3, 0.4]
  }
);
```

### KNN Search

```typescript
// Using Cypher
const result = await client.executeCypher(
  `MATCH (n:Person)
   WHERE n.vector IS NOT NULL
   RETURN n.name, n.vector
   ORDER BY n.vector <-> $query_vector
   LIMIT 5`,
  { query_vector: [0.1, 0.2, 0.3, 0.4] }
);

// Using KNN endpoint
const results = await client.knnTraverse({
  label: 'Person',
  vector: [0.1, 0.2, 0.3, 0.4],
  k: 10,
  where: 'n.age > 25'
});
```

## Database Management

### List Databases

```typescript
const { databases, defaultDatabase } = await client.listDatabases();
databases.forEach(db => {
  console.log(`Database: ${db.name}, Nodes: ${db.nodeCount}`);
});
```

### Create Database

```typescript
await client.createDatabase('mydb');
```

### Switch Database

```typescript
await client.switchDatabase('mydb');
```

### Get Current Database

```typescript
const current = await client.getCurrentDatabase();
console.log(`Current database: ${current}`);
```

## TypeScript Types

```typescript
import { 
  NexusClient, 
  QueryResult, 
  Node, 
  Relationship 
} from '@hivellm/nexus-sdk';

// Query result type
const result: QueryResult = await client.executeCypher('...');

// Node type
const node: Node = {
  id: 1,
  labels: ['Person'],
  properties: { name: 'Alice', age: 30 }
};

// Relationship type
const rel: Relationship = {
  id: 1,
  type: 'KNOWS',
  startNodeId: 1,
  endNodeId: 2,
  properties: { since: '2020' }
};
```

## Error Handling

```typescript
import { NexusClient, NexusError } from '@hivellm/nexus-sdk';

const client = new NexusClient({ baseUrl: 'http://localhost:15474' });

try {
  const result = await client.executeCypher('MATCH (n) RETURN n');
} catch (error) {
  if (error instanceof NexusError) {
    console.error(`Error: ${error.message}`);
    console.error(`Type: ${error.errorType}`);
    console.error(`Status: ${error.statusCode}`);
  }
}
```

## Advanced Features

### Connection Pooling

```typescript
const client = new NexusClient({
  baseUrl: 'http://localhost:15474',
  maxConnections: 10,
  connectionTimeout: 30000
});
```

### Retry Logic

```typescript
import { NexusClient, RetryConfig } from '@hivellm/nexus-sdk';

const client = new NexusClient({
  baseUrl: 'http://localhost:15474',
  retry: {
    maxRetries: 3,
    retryDelay: 1000
  } as RetryConfig
});
```

## Examples

### Social Network

```typescript
// Create users
await client.executeCypher(
  `CREATE 
    (alice:Person {name: "Alice", age: 30}),
    (bob:Person {name: "Bob", age: 28})`
);

// Create relationship
await client.executeCypher(
  `MATCH (a:Person {name: "Alice"}), (b:Person {name: "Bob"})
   CREATE (a)-[:KNOWS {since: "2020"}]->(b)`
);

// Query relationships
const result = await client.executeCypher(
  `MATCH (a:Person)-[r:KNOWS]->(b:Person)
   RETURN a.name, b.name, r.since`
);
```

### Recommendation System

```typescript
// Find similar users
const result = await client.executeCypher(
  `MATCH (user:Person {id: 1})-[:LIKES]->(item:Item),
        (similar:Person)-[:LIKES]->(item)
   WHERE similar.vector IS NOT NULL
     AND similar.vector <-> user.vector < 0.3
   RETURN DISTINCT similar.name, 
          similar.vector <-> user.vector as similarity
   ORDER BY similarity
   LIMIT 10`
);
```

## Browser Usage

```typescript
// Works in browser with fetch API
import { NexusClient } from '@hivellm/nexus-sdk';

const client = new NexusClient({
  baseUrl: 'https://api.example.com',
  fetch: window.fetch // Use browser fetch
});
```

## Related Topics

- [SDKs Overview](./SDKS.md) - Compare all SDKs
- [API Reference](../api/API_REFERENCE.md) - REST API documentation
- [Cypher Guide](../cypher/CYPHER.md) - Cypher query language

