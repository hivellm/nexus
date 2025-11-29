# Proposal - Multi-Database Support

## Summary

Implement multi-database support for Nexus graph database, allowing users to create, manage, and switch between multiple isolated databases within a single Nexus instance.

## Motivation

Currently, Nexus operates with a single default database. Multi-database support enables:

1. **Multi-tenancy**: Different applications or tenants can have isolated databases
2. **Environment separation**: Development, staging, and production data can coexist
3. **Testing**: Isolated test databases that don't affect production data
4. **Neo4j compatibility**: Match Neo4j Enterprise Edition's multi-database capabilities

## Proposed Solution

### Architecture

1. **Database Manager**: New component to manage multiple database instances
2. **Database Catalog**: Metadata storage for database configurations
3. **Session Context**: Track current database per connection/session
4. **Storage Isolation**: Separate storage directories per database

### Cypher Commands

```cypher
-- List databases
SHOW DATABASES

-- Create database
CREATE DATABASE mydb

-- Drop database
DROP DATABASE mydb

-- Switch database
:USE mydb

-- Show current database
:USE
```

### REST API Extensions

```
GET    /api/v1/databases          - List all databases
POST   /api/v1/databases          - Create database
DELETE /api/v1/databases/{name}   - Drop database
PUT    /api/v1/session/database   - Switch database
```

## Dependencies

- Storage layer modifications
- Session management updates
- Cypher executor extensions
- REST API extensions

## Components to Update

After server implementation, the following components must be updated:

1. **SDKs** - Python, TypeScript, Rust SDKs need database parameter and management methods
2. **REST API** - New endpoints for database operations
3. **CLI** - Already implemented (db list, create, switch, drop)
4. **GUI** - Database selector, management view
5. **Documentation** - User guide, API docs
6. **CHANGELOG.md** - Feature announcement
7. **README.md** - Multi-database section

## Risks

- Increased complexity in storage management
- Memory overhead for multiple database instances
- Migration path for existing single-database installations

## Timeline

Estimated effort: 2-3 weeks

## Success Criteria

- [ ] Multiple databases can be created and managed
- [ ] Data is fully isolated between databases
- [ ] CLI commands work with multi-database
- [ ] Neo4j compatibility for SHOW DATABASES, CREATE DATABASE, DROP DATABASE
- [ ] Performance: No significant overhead for single-database usage
