# Proposal: HiveHub.Cloud Integration

## Why

Nexus needs to integrate with HiveHub.Cloud to operate as a managed multi-tenant knowledge graph service with database-per-user isolation, credit-based billing, and centralized authentication. Currently, Nexus operates standalone without user context or billing integration. This integration will enable HiveHub.Cloud to provide Nexus as a secure managed service with complete database isolation and credit management for LLM operations.

## What Changes

Implement HiveHub.Cloud integration in Nexus including:

- **Internal SDK Client**: Integrate `hivehub-internal-sdk` for Hub API communication
- **Authentication Layer**: Validate users via Hub-issued access keys
- **Database-Per-User**: Automatic creation of exclusive databases per user
- **Credit System**: Check and consume credits for LLM operations via Hub API
- **Quota Enforcement**: Check node/relationship limits before operations
- **Usage Reporting**: Track and report nodes, relationships, storage, credits to Hub
- **MCP Integration**: Register with Hub's MCP gateway for Cypher query access
- **Cluster Support**: User database routing and distributed operations
- **Data Migration**: Tools to migrate existing databases to user-scoped model

## Impact

- Affected code:
  - New `nexus-server/src/hub/` - Hub integration module
  - New `nexus-server/src/auth/hub_auth.rs` - Hub authentication
  - Modified `nexus-server/src/database/` - Multi-database management
  - Modified `nexus-server/src/api/` - Add user context to API
  - Modified `nexus-server/src/cluster/` - User routing in cluster
- Breaking change: YES - Requires database migration, API changes
- User benefit: Secure multi-tenant knowledge graph with credit-based billing

