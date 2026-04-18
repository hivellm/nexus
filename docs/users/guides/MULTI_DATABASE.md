---
title: Multi-Database
module: guides
id: multi-database
order: 2
description: Working with multiple databases
tags: [multi-database, isolation, multi-tenancy]
---

# Multi-Database

Complete guide for working with multiple databases in Nexus.

## Overview

Nexus supports multiple databases within a single server instance, enabling:
- **Data Isolation**: Each database is completely isolated
- **Multi-Tenancy**: Separate databases for different tenants
- **Logical Separation**: Organize data by project or environment

## Key Concepts

- **Database**: An isolated data store with its own nodes, relationships, and indexes
- **Default Database**: The `neo4j` database is created automatically
- **Session Database**: Each session maintains a current database context
- **Data Isolation**: Each database is completely isolated from others

## Managing Databases

### List Databases

```cypher
SHOW DATABASES
```

**Response:**
```
| name  | status |
|-------|--------|
| neo4j | online |
| mydb  | online |
```

### Create Database

```cypher
CREATE DATABASE mydb
```

Database names must be alphanumeric with underscores and hyphens.

### Drop Database

```cypher
DROP DATABASE mydb
```

⚠️ **Warning**: This permanently deletes all data in the database.

### Switch Database

```cypher
:USE mydb
```

After switching, all subsequent queries execute against the selected database.

### Get Current Database

```cypher
RETURN database() AS current_db
```

Or use the alias:
```cypher
RETURN db() AS current_db
```

## REST API

### List Databases

```bash
GET /databases
```

**Response:**
```json
{
  "databases": [
    {
      "name": "neo4j",
      "path": "data/neo4j",
      "created_at": 1700000000,
      "node_count": 1000,
      "relationship_count": 5000
    }
  ],
  "default_database": "neo4j"
}
```

### Create Database

```bash
POST /databases
Content-Type: application/json

{
  "name": "mydb"
}
```

### Get Database Info

```bash
GET /databases/mydb
```

### Drop Database

```bash
DELETE /databases/mydb
```

### Switch Database

```bash
PUT /session/database
Content-Type: application/json

{
  "name": "mydb"
}
```

## SDK Examples

### Python

```python
from nexus_sdk import NexusClient

client = NexusClient("http://localhost:15474")

# List databases
response = client.list_databases()
for db in response.databases:
    print(f"Database: {db.name}, Nodes: {db.node_count}")

# Create database
client.create_database("mydb")

# Switch database
client.switch_database("mydb")

# Get current database
current = client.get_current_database()
print(f"Current database: {current}")
```

### TypeScript

```typescript
import { NexusClient } from '@hivellm/nexus-sdk';

const client = new NexusClient({
  baseUrl: 'http://localhost:15474'
});

// List databases
const { databases, defaultDatabase } = await client.listDatabases();
databases.forEach(db => console.log(`Database: ${db.name}`));

// Create database
await client.createDatabase('mydb');

// Switch database
await client.switchDatabase('mydb');

// Get current database
const current = await client.getCurrentDatabase();
console.log(`Current database: ${current}`);
```

## Use Cases

### Multi-Tenancy

```cypher
-- Tenant 1
:USE tenant1_db
CREATE (n:User {id: 1, name: "Alice"})

-- Tenant 2
:USE tenant2_db
CREATE (n:User {id: 1, name: "Bob"})
```

### Environment Separation

```cypher
-- Development
:USE dev_db
CREATE (n:TestData {value: "test"})

-- Production
:USE prod_db
CREATE (n:ProductionData {value: "real"})
```

### Project Organization

```cypher
-- Project A
:USE project_a_db
CREATE (n:Project {name: "Project A"})

-- Project B
:USE project_b_db
CREATE (n:Project {name: "Project B"})
```

## Best Practices

1. **Use Meaningful Names**: Choose descriptive database names
2. **Separate Environments**: Use different databases for dev, test, prod
3. **Multi-Tenancy**: Use separate databases for each tenant
4. **Backup Strategy**: Implement backup procedures for each database
5. **Avoid Dropping Active Databases**: Switch to a different database before dropping

## Related Topics

- [Configuration Guide](../configuration/CONFIGURATION.md) - Server configuration
- [Operations Guide](../operations/) - Service management
- [API Reference](../api/API_REFERENCE.md) - REST API

