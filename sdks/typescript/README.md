# @hivellm/nexus-sdk

Official TypeScript/JavaScript SDK for [Nexus Graph Database](https://github.com/hivellm/nexus).

[![npm version](https://img.shields.io/npm/v/@hivellm/nexus-sdk.svg)](https://www.npmjs.com/package/@hivellm/nexus-sdk)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## Features

- 🚀 **Full TypeScript support** with complete type definitions
- 🔐 **Multiple authentication methods** (API Key, username/password)
- 🔄 **Automatic retry logic** with exponential backoff
- 📦 **Batch operations** for improved performance
- 🎯 **Cypher query execution** with parameter support
- 🔗 **Node and relationship CRUD operations**
- 📊 **Schema management** and introspection
- 🛡️ **Comprehensive error handling**
- ✅ **Well-tested** with high code coverage

## Installation

```bash
npm install @hivellm/nexus-sdk
```

or

```bash
yarn add @hivellm/nexus-sdk
```

or

```bash
pnpm add @hivellm/nexus-sdk
```

## Quick Start

```typescript
import { NexusClient } from '@hivellm/nexus-sdk';

// Create client
const client = new NexusClient({
  baseUrl: 'http://localhost:7687',
  auth: {
    apiKey: 'your-api-key',
  },
});

// Execute a query
const result = await client.executeCypher(
  'MATCH (n:Person) WHERE n.age > $age RETURN n',
  { age: 25 }
);

console.log(result.rows);
```

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

## Error Handling

The SDK provides specific error classes for different error scenarios:

```typescript
import {
  NexusSDKError,
  AuthenticationError,
  ConnectionError,
  QueryExecutionError,
  ValidationError,
} from '@hivellm/nexus-sdk';

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
} from '@hivellm/nexus-sdk';

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

## API Reference

For detailed API documentation, visit [our documentation site](https://github.com/hivellm/nexus/tree/main/sdks/typescript).

## Contributing

Contributions are welcome! Please read our [contributing guidelines](../../CONTRIBUTING.md) first.

## License

MIT © HiveLLM

## Links

- [Nexus Graph Database](https://github.com/hivellm/nexus)
- [Issue Tracker](https://github.com/hivellm/nexus/issues)
- [Changelog](./CHANGELOG.md)

