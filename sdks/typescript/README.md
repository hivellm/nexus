# @hivehub/nexus-sdk

Official TypeScript/JavaScript SDK for [Nexus Graph Database](https://github.com/hivellm/nexus).

[![npm version](https://img.shields.io/npm/v/@hivehub/nexus-sdk.svg)](https://www.npmjs.com/package/@hivehub/nexus-sdk)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

> **Compatibility:** SDK 2.0.0 ↔ `nexus-server` 2.0.0. SDK and
> server move in lockstep on the same X.Y.Z train. See
> [`docs/COMPATIBILITY_MATRIX.md`](../../docs/COMPATIBILITY_MATRIX.md).

## Features

- **Binary RPC by default** — connects over `nexus://127.0.0.1:15475` using length-prefixed MessagePack; 3–10× lower latency and 40–60% smaller payloads than the legacy HTTP path.
- **HTTP fallback in one line** — pass `http://…` or set `transport: 'http'` to use the REST transport (browser builds, firewalls, diagnostic).
- **Full TypeScript support** with complete type definitions.
- **Multiple authentication methods** — API Key or username/password (both work on RPC + HTTP).
- **Cypher, CRUD, schema, multi-database** — every manager method routes through the same transport.
- **Comprehensive error handling** with structured `NexusSDKError`.

## Installation

```bash
npm install @hivehub/nexus-sdk
```

or

```bash
yarn add @hivehub/nexus-sdk
```

or

```bash
pnpm add @hivehub/nexus-sdk
```

## Quick Start (RPC — default)

```typescript
import { NexusClient } from '@hivehub/nexus-sdk';

// Defaults to nexus://127.0.0.1:15475 (binary RPC).
const client = new NexusClient();

const result = await client.executeCypher(
  'MATCH (n:Person) WHERE n.age > $age RETURN n',
  { age: 25 }
);
console.log(result.rows);

await client.close(); // release the persistent TCP socket
```

## Transports

| URL form                   | Transport                | Default port | Use case                                    |
|----------------------------|--------------------------|--------------|---------------------------------------------|
| `nexus://host[:port]`      | Binary RPC (MessagePack) | `15475`      | **Default.** Lowest latency.                |
| `http://host[:port]`       | HTTP/JSON (axios)        | `15474`      | Browser, firewalls, diagnostic.             |
| `https://host[:port]`      | HTTPS/JSON               | `443`        | Public-internet HTTP with TLS.              |
| `resp3://host[:port]`      | RESP3 (reserved)         | `15476`      | Not yet shipped — throws on construction.  |

Precedence: **URL scheme > `NEXUS_SDK_TRANSPORT` env var > `transport` field > default (`nexus`)**.

```typescript
// HTTP fallback
const client = new NexusClient({
  baseUrl: 'http://localhost:15474',
  auth: { apiKey: process.env.NEXUS_API_KEY },
});

// Keep the URL but force HTTP via env var (`NEXUS_SDK_TRANSPORT=http`)

// Or set the transport field explicitly
const client2 = new NexusClient({
  baseUrl: 'host:15474',        // bare form — URL scheme not set
  transport: 'http',
});
```

Full spec: [`docs/specs/sdk-transport.md`](../../docs/specs/sdk-transport.md).

## Authentication

### API Key Authentication

```typescript
const client = new NexusClient({
  baseUrl: 'http://localhost:7687',
  auth: {
    apiKey: 'your-api-key',
  },
});
```

### Username/Password Authentication

```typescript
const client = new NexusClient({
  baseUrl: 'http://localhost:7687',
  auth: {
    username: 'admin',
    password: 'password',
  },
});
```

## Usage Examples

### Node Operations

#### Create a Node

```typescript
const node = await client.createNode(['Person'], {
  name: 'Alice',
  age: 30,
  email: 'alice@example.com',
});
```

#### Get Node by ID

```typescript
const node = await client.getNode(123);
```

#### Update Node

```typescript
const updatedNode = await client.updateNode(123, {
  age: 31,
  city: 'New York',
});
```

#### Find Nodes

```typescript
const persons = await client.findNodes('Person', { city: 'New York' }, 10);
```

#### Delete Node

```typescript
await client.deleteNode(123);

// Delete with relationships
await client.deleteNode(123, true); // detach delete
```

### Relationship Operations

#### Create Relationship

```typescript
const relationship = await client.createRelationship(
  startNodeId,
  endNodeId,
  'KNOWS',
  { since: 2020 }
);
```

#### Get Relationship by ID

```typescript
const rel = await client.getRelationship(456);
```

#### Delete Relationship

```typescript
await client.deleteRelationship(456);
```

### Cypher Queries

#### Simple Query

```typescript
const result = await client.executeCypher('MATCH (n:Person) RETURN n LIMIT 10');
```

#### Query with Parameters

```typescript
const result = await client.executeCypher(
  'MATCH (n:Person) WHERE n.age > $age AND n.city = $city RETURN n',
  {
    age: 25,
    city: 'New York',
  }
);
```

#### Complex Query

```typescript
const result = await client.executeCypher(`
  MATCH (person:Person)-[:KNOWS]->(friend)
  WHERE person.city = $city
  RETURN person.name AS person, 
         collect(friend.name) AS friends
  ORDER BY person.name
`, { city: 'New York' });
```

### Batch Operations

```typescript
const results = await client.executeBatch([
  { cypher: 'MATCH (p:Person) RETURN count(p) AS count' },
  { cypher: 'MATCH ()-[r:KNOWS]->() RETURN count(r) AS count' },
  {
    cypher: 'MATCH (p:Person {name: $name}) RETURN p',
    params: { name: 'Alice' },
  },
]);
```

### Schema Operations

#### Get All Labels

```typescript
const labels = await client.getLabels();
console.log('Available labels:', labels);
```

#### Get Relationship Types

```typescript
const types = await client.getRelationshipTypes();
console.log('Available relationship types:', types);
```

#### Get Complete Schema

```typescript
const schema = await client.getSchema();
console.log('Labels:', schema.labels);
console.log('Relationship types:', schema.relationshipTypes);
```

### Statistics and Monitoring

```typescript
const stats = await client.getStatistics();
console.log('Query statistics:', stats);
```

### Plan Cache Management

```typescript
await client.clearPlanCache();
```

### Multi-Database Support

```typescript
// List all databases
const databases = await client.listDatabases();
console.log('Available databases:', databases.databases);
console.log('Default database:', databases.defaultDatabase);

// Create a new database
const createResult = await client.createDatabase('mydb');
console.log('Created:', createResult.name);

// Switch to the new database
const switchResult = await client.switchDatabase('mydb');
console.log('Switched to: mydb');

// Get current database
const currentDb = await client.getCurrentDatabase();
console.log('Current database:', currentDb);

// Create data in the current database
const result = await client.executeCypher(
  'CREATE (n:Product {name: $name}) RETURN n',
  { name: 'Laptop' }
);

// Get database information
const dbInfo = await client.getDatabase('mydb');
console.log(`Nodes: ${dbInfo.nodeCount}, Relationships: ${dbInfo.relationshipCount}`);

// Drop database (must not be current database)
await client.switchDatabase('neo4j');  // Switch away first
const dropResult = await client.dropDatabase('mydb');

// Or connect directly to a specific database
const dbClient = new NexusClient({
  baseUrl: 'http://localhost:15474',
  database: 'mydb',  // Connect to specific database
});
// All operations will use 'mydb'
const result = await dbClient.executeCypher('MATCH (n) RETURN n LIMIT 10', {});
```

## Configuration Options

```typescript
interface NexusConfig {
  baseUrl: string;           // Nexus server URL
  auth: AuthConfig;          // Authentication configuration
  timeout?: number;          // Request timeout (default: 30000ms)
  retries?: number;          // Number of retries (default: 3)
  debug?: boolean;           // Enable debug logging (default: false)
}
```

## High Availability with Replication

Nexus supports master-replica replication for high availability and read scaling.
Use the **master** for all write operations and **replicas** for read operations.

### NexusCluster Class

```typescript
import { NexusClient } from '@hivehub/nexus-sdk';

/**
 * Client for Nexus cluster with master-replica topology.
 * Routes writes to master, reads to replicas (round-robin).
 */
class NexusCluster {
  private master: NexusClient;
  private replicas: NexusClient[];
  private replicaIndex = 0;

  constructor(masterUrl: string, replicaUrls: string[]) {
    this.master = new NexusClient({ baseUrl: masterUrl });
    this.replicas = replicaUrls.map(url => new NexusClient({ baseUrl: url }));
  }

  private getNextReplica(): NexusClient {
    if (this.replicas.length === 0) return this.master;
    const replica = this.replicas[this.replicaIndex];
    this.replicaIndex = (this.replicaIndex + 1) % this.replicas.length;
    return replica;
  }

  /** Execute write query on master */
  async write(query: string, params: Record<string, unknown> = {}) {
    return this.master.executeCypher(query, params);
  }

  /** Execute read query on replica (round-robin) */
  async read(query: string, params: Record<string, unknown> = {}) {
    return this.getNextReplica().executeCypher(query, params);
  }

  /** Get master client for direct access */
  getMaster(): NexusClient {
    return this.master;
  }

  /** Get all replica clients */
  getReplicas(): NexusClient[] {
    return this.replicas;
  }
}
```

### Usage Example

```typescript
import { NexusClient } from '@hivehub/nexus-sdk';

async function main() {
  // Connect to cluster
  // Master handles all writes, replicas handle reads
  const cluster = new NexusCluster(
    'http://master:15474',
    [
      'http://replica1:15474',
      'http://replica2:15474',
    ]
  );

  // Write operations go to master
  await cluster.write(
    'CREATE (n:Person {name: $name, age: $age}) RETURN n',
    { name: 'Alice', age: 30 }
  );

  // Read operations are distributed across replicas
  const result = await cluster.read(
    'MATCH (n:Person) WHERE n.age > $minAge RETURN n',
    { minAge: 25 }
  );
  console.log(`Found ${result.rows.length} persons`);

  // High-volume reads are load-balanced
  for (let i = 0; i < 100; i++) {
    await cluster.read('MATCH (n) RETURN count(n) as total', {});
  }
}

main();
```

### Replication Architecture

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

```typescript
async function checkReplicationStatus(masterUrl: string) {
  // Check master status
  const statusRes = await fetch(`${masterUrl}/replication/status`);
  const status = await statusRes.json();
  console.log(`Role: ${status.role}`);
  console.log(`Running: ${status.running}`);
  console.log(`Connected replicas: ${status.replica_count ?? 0}`);

  // Get master stats
  const statsRes = await fetch(`${masterUrl}/replication/master/stats`);
  const stats = await statsRes.json();
  console.log(`Entries replicated: ${stats.entries_replicated}`);
  console.log(`Connected replicas: ${stats.connected_replicas}`);

  // List replicas
  const replicasRes = await fetch(`${masterUrl}/replication/replicas`);
  const replicas = await replicasRes.json();
  for (const replica of replicas.replicas) {
    console.log(`  - ${replica.id}: lag=${replica.lag}, healthy=${replica.healthy}`);
  }
}
```

## Error Handling

The SDK provides specific error classes for different error scenarios:

```typescript
import {
  NexusSDKError,
  AuthenticationError,
  ConnectionError,
  QueryExecutionError,
  ValidationError,
} from '@hivehub/nexus-sdk';

try {
  await client.executeCypher('MATCH (n) RETURN n');
} catch (error) {
  if (error instanceof AuthenticationError) {
    console.error('Authentication failed');
  } else if (error instanceof QueryExecutionError) {
    console.error('Query execution failed');
  } else if (error instanceof NexusSDKError) {
    console.error('SDK error:', error.message);
    console.error('Status code:', error.statusCode);
  }
}
```

## TypeScript Support

The SDK is written in TypeScript and provides complete type definitions:

```typescript
import type {
  Node,
  Relationship,
  QueryResult,
  SchemaInfo,
} from '@hivehub/nexus-sdk';

const result: QueryResult = await client.executeCypher('MATCH (n) RETURN n');
const nodes: Node[] = await client.findNodes('Person');
```

## Testing

```bash
# Run tests
npm test

# Run tests in watch mode
npm run test:watch

# Generate coverage report
npm run test:coverage
```

## Examples

Check the [examples](./examples) directory for more usage examples:

- [basic-usage.ts](./examples/basic-usage.ts) - Basic CRUD operations
- [advanced-queries.ts](./examples/advanced-queries.ts) - Complex Cypher queries
- [multi-database.ts](./examples/multi-database.ts) - Multi-database support

## API Reference

For detailed API documentation, visit [our documentation site](https://github.com/hivellm/nexus/tree/main/sdks/typescript).

## Contributing

Contributions are welcome! Please read our [contributing guidelines](../../CONTRIBUTING.md) first.

## License

Apache-2.0 © HiveLLM — see [LICENSE](./LICENSE) for details.

## Links

- [Nexus Graph Database](https://github.com/hivellm/nexus)
- [Issue Tracker](https://github.com/hivellm/nexus/issues)
- [Changelog](./CHANGELOG.md)

