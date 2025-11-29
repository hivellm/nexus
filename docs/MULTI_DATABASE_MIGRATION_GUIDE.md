# Multi-Database Migration Guide

This guide helps you migrate from a single-database setup to using multiple databases in Nexus.

## Table of Contents

1. [Overview](#overview)
2. [Migration from Single-Database](#migration-from-single-database)
3. [Backup and Restore](#backup-and-restore)
4. [Best Practices](#best-practices)
5. [Common Migration Scenarios](#common-migration-scenarios)
6. [Troubleshooting](#troubleshooting)

## Overview

Nexus multi-database support enables:
- **Data isolation** between different applications or tenants
- **Independent schema management** per database
- **Simplified multi-tenancy** without complex filtering
- **Better resource management** and performance monitoring

### Key Concepts

- **Default Database**: The primary database (typically `neo4j`) used when no specific database is selected
- **Session Database**: The database currently active in your session or connection
- **Data Isolation**: Each database has completely separate data - nodes, relationships, and schema

## Migration from Single-Database

### Step 1: Assess Your Current Setup

Before migrating, understand your current data structure:

```cypher
// Count your existing data
MATCH (n) RETURN count(n) AS nodeCount

MATCH ()-[r]->() RETURN count(r) AS relationshipCount

// List all labels
CALL db.labels() YIELD label RETURN label

// List all relationship types
CALL db.relationshipTypes() YIELD relationshipType RETURN relationshipType
```

### Step 2: Plan Your Database Structure

Decide how to organize your data across multiple databases:

**Option A: By Application**
- `users_db` - User and authentication data
- `products_db` - Product catalog
- `analytics_db` - Analytics and reporting data

**Option B: By Tenant**
- `tenant_a` - Tenant A's complete dataset
- `tenant_b` - Tenant B's complete dataset
- `shared` - Shared reference data

**Option C: By Environment**
- `production` - Production data
- `staging` - Staging data
- `development` - Development data

### Step 3: Create New Databases

Using Cypher:
```cypher
CREATE DATABASE users_db
CREATE DATABASE products_db
CREATE DATABASE analytics_db
```

Using REST API:
```bash
curl -X POST http://localhost:15474/databases \
  -H "Content-Type: application/json" \
  -d '{"name": "users_db"}'

curl -X POST http://localhost:15474/databases \
  -H "Content-Type: application/json" \
  -d '{"name": "products_db"}'
```

Using CLI:
```bash
nexus db create users_db
nexus db create products_db
nexus db create analytics_db
```

### Step 4: Export Data from Default Database

**Option 1: Using APOC (if available)**
```cypher
// Export all data to JSON
CALL apoc.export.json.all("backup.json", {useTypes:true})
```

**Option 2: Using Cypher queries**
```cypher
// Export users
:USE neo4j
MATCH (u:User)
RETURN u
// Save output and import into users_db

// Export products
MATCH (p:Product)
RETURN p
// Save output and import into products_db
```

**Option 3: Using the Nexus REST API**
```bash
# Export data from default database
curl http://localhost:15474/data/export > default_backup.json
```

### Step 5: Import Data into New Databases

**Switch to target database and import:**

```cypher
// Switch to users database
:USE users_db

// Import users
UNWIND $users AS userData
CREATE (u:User)
SET u = userData.properties
```

**Using REST API:**
```bash
# Switch database
curl -X PUT http://localhost:15474/session/database \
  -H "Content-Type: application/json" \
  -d '{"name": "users_db"}'

# Import data
curl -X POST http://localhost:15474/cypher \
  -H "Content-Type: application/json" \
  -d '{
    "query": "CREATE (u:User {name: $name, email: $email})",
    "params": {"name": "Alice", "email": "alice@example.com"}
  }'
```

### Step 6: Update Application Code

**Before (Single Database):**
```python
from nexus_sdk import NexusClient

client = NexusClient("http://localhost:15474")
result = await client.execute_cypher("MATCH (u:User) RETURN u", None)
```

**After (Multi-Database):**
```python
from nexus_sdk import NexusClient

# Connect to specific database
client = NexusClient("http://localhost:15474", database="users_db")
result = await client.execute_cypher("MATCH (u:User) RETURN u", None)

# Or switch databases dynamically
client = NexusClient("http://localhost:15474")
await client.switch_database("users_db")
result = await client.execute_cypher("MATCH (u:User) RETURN u", None)
```

### Step 7: Verify Migration

```cypher
// Check each database
:USE users_db
MATCH (n) RETURN count(n) AS nodeCount

:USE products_db
MATCH (n) RETURN count(n) AS nodeCount

// Verify data isolation
:USE users_db
MATCH (p:Product) RETURN count(p) AS productCount
// Should return 0 if properly isolated
```

### Step 8: Update Access Patterns

Update your application to use the correct database for each operation:

```typescript
// User operations
const usersClient = new NexusClient({
  baseUrl: 'http://localhost:15474',
  database: 'users_db'
});

// Product operations
const productsClient = new NexusClient({
  baseUrl: 'http://localhost:15474',
  database: 'products_db'
});
```

## Backup and Restore

### Backing Up Databases

**Option 1: Per-Database Export**
```bash
# Export each database separately
for db in users_db products_db analytics_db; do
  curl -X PUT http://localhost:15474/session/database \
    -H "Content-Type: application/json" \
    -d "{\"name\": \"$db\"}"

  curl http://localhost:15474/data/export > "${db}_backup_$(date +%Y%m%d).json"
done
```

**Option 2: File System Backup**
```bash
# Stop the server
nexus stop

# Backup the entire data directory
cp -r data/ backups/data_$(date +%Y%m%d)/

# Restart the server
nexus start
```

### Restoring Databases

**Option 1: Restore from Export**
```bash
# Create database
curl -X POST http://localhost:15474/databases \
  -H "Content-Type: application/json" \
  -d '{"name": "users_db"}'

# Switch to database
curl -X PUT http://localhost:15474/session/database \
  -H "Content-Type: application/json" \
  -d '{"name": "users_db"}'

# Import data
curl -X POST http://localhost:15474/data/import \
  -H "Content-Type: application/json" \
  --data @users_db_backup_20250128.json
```

**Option 2: Restore from File System**
```bash
# Stop the server
nexus stop

# Restore data directory
rm -rf data/
cp -r backups/data_20250128/ data/

# Restart the server
nexus start
```

### Automated Backup Scripts

**Bash script for daily backups:**
```bash
#!/bin/bash
# backup_databases.sh

NEXUS_URL="http://localhost:15474"
BACKUP_DIR="/backups/nexus"
DATE=$(date +%Y%m%d_%H%M%S)

# Get list of databases
DATABASES=$(curl -s $NEXUS_URL/databases | jq -r '.databases[]')

# Backup each database
for db in $DATABASES; do
  echo "Backing up $db..."

  # Switch to database
  curl -s -X PUT $NEXUS_URL/session/database \
    -H "Content-Type: application/json" \
    -d "{\"name\": \"$db\"}" > /dev/null

  # Export data
  curl -s $NEXUS_URL/data/export > "$BACKUP_DIR/${db}_${DATE}.json"

  echo "✓ Backed up $db"
done

# Cleanup old backups (keep last 7 days)
find $BACKUP_DIR -name "*.json" -mtime +7 -delete
```

**Python script for scheduled backups:**
```python
import os
import asyncio
from datetime import datetime
from nexus_sdk import NexusClient

async def backup_all_databases():
    client = NexusClient("http://localhost:15474")
    backup_dir = "/backups/nexus"
    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")

    # Get list of databases
    databases = await client.list_databases()

    for db_name in databases.databases:
        print(f"Backing up {db_name}...")

        # Switch to database
        await client.switch_database(db_name)

        # Export data (implement based on your export mechanism)
        # This is a placeholder - implement your actual export logic
        backup_path = f"{backup_dir}/{db_name}_{timestamp}.json"
        # await export_database(client, backup_path)

        print(f"✓ Backed up {db_name}")

    # Cleanup old backups
    cleanup_old_backups(backup_dir, days=7)

def cleanup_old_backups(backup_dir, days=7):
    import time
    cutoff = time.time() - (days * 86400)

    for filename in os.listdir(backup_dir):
        filepath = os.path.join(backup_dir, filename)
        if os.path.getmtime(filepath) < cutoff:
            os.remove(filepath)
            print(f"Deleted old backup: {filename}")

if __name__ == "__main__":
    asyncio.run(backup_all_databases())
```

## Best Practices

### 1. Database Naming Conventions

- Use lowercase names
- Use underscores for multi-word names
- Be descriptive but concise
- Examples: `user_data`, `product_catalog`, `analytics`

### 2. Connection Management

**Create separate clients per database:**
```python
# Good
users_client = NexusClient("http://localhost:15474", database="users_db")
products_client = NexusClient("http://localhost:15474", database="products_db")

# Avoid switching in the same client frequently
```

### 3. Data Organization

- Keep related data in the same database
- Avoid cross-database queries (not supported yet)
- Use consistent schema across similar databases
- Document which data lives in which database

### 4. Security

- Use different credentials per database when available
- Implement access control at the application level
- Audit database access patterns
- Monitor for unauthorized database switching

### 5. Monitoring

```python
# Monitor database sizes
async def monitor_databases():
    client = NexusClient("http://localhost:15474")
    databases = await client.list_databases()

    for db_name in databases.databases:
        db_info = await client.get_database(db_name)
        print(f"{db_name}:")
        print(f"  Nodes: {db_info.node_count}")
        print(f"  Relationships: {db_info.relationship_count}")
        print(f"  Storage: {db_info.storage_size / 1024 / 1024:.2f} MB")
```

## Common Migration Scenarios

### Scenario 1: Multi-Tenant SaaS Application

**Before:** Single database with tenant_id on every node
```cypher
CREATE (u:User {tenant_id: 1, name: "Alice"})
CREATE (u:User {tenant_id: 2, name: "Bob"})
```

**After:** Separate database per tenant
```cypher
// In tenant_1 database
CREATE (u:User {name: "Alice"})

// In tenant_2 database
CREATE (u:User {name: "Bob"})
```

**Benefits:**
- Complete data isolation
- Simpler queries (no tenant_id filtering)
- Easier backup/restore per tenant
- Better performance (smaller graphs)

### Scenario 2: Separating Test and Production Data

**Create databases:**
```cypher
CREATE DATABASE production
CREATE DATABASE staging
CREATE DATABASE development
```

**Update deployment scripts:**
```bash
# Production
export NEXUS_DATABASE=production
npm run migrate

# Staging
export NEXUS_DATABASE=staging
npm run migrate
```

### Scenario 3: Microservices Architecture

Each microservice gets its own database:

```python
# User service
user_client = NexusClient("http://localhost:15474", database="user_service")

# Order service
order_client = NexusClient("http://localhost:15474", database="order_service")

# Inventory service
inventory_client = NexusClient("http://localhost:15474", database="inventory_service")
```

## Troubleshooting

### Cannot Drop Database

**Error:** "Cannot drop the currently active database"

**Solution:**
```cypher
// Switch to a different database first
:USE neo4j
DROP DATABASE users_db
```

### Database Not Found

**Error:** "Database 'mydb' does not exist"

**Solution:**
```cypher
// List available databases
SHOW DATABASES

// Create the database if needed
CREATE DATABASE mydb
```

### Data Not Visible After Switch

**Issue:** Created data in one database but can't see it after switching

**Solution:** This is expected - verify you're in the correct database
```cypher
// Check current database
RETURN database() AS current_db

// Switch to correct database
:USE mydb

// Verify data
MATCH (n) RETURN count(n)
```

### Session Not Persisting Database

**Issue:** Database resets after reconnecting

**Solution:** Specify database in connection
```python
# Instead of switching after connection
client = NexusClient("http://localhost:15474", database="mydb")
```

### Performance Degradation After Migration

**Issue:** Queries slower after splitting into multiple databases

**Causes:**
- Too many small databases (overhead)
- Poor data distribution
- Unnecessary database switching

**Solutions:**
- Consolidate related data
- Use connection pooling per database
- Minimize database switches in hot paths

## Additional Resources

- [Multi-Database User Guide](USER_GUIDE.md#multi-database-support)
- [Database API Reference](api/openapi.yml)
- [SDK Examples](../sdks/)
  - [Python SDK Multi-Database Example](../sdks/python/examples/multi_database.py)
  - [TypeScript SDK Multi-Database Example](../sdks/typescript/examples/multi-database.ts)
  - [Rust SDK Multi-Database Example](../sdks/rust/examples/multi_database.rs)
