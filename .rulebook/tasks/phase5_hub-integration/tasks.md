## 1. Internal SDK Integration
- [x] 1.1 Add `hivehub-internal-sdk = "1.0.0"` + a `reqwest` (rustls
            only) dependency to `crates/nexus-server/Cargo.toml`
- [x] 1.2 Create `crates/nexus-server/src/hub/{mod,client}.rs` —
            opinionated wrapper around `HiveHubCloudClient`; redacts
            the API key in `Debug`; exposes `sdk()`, `base_url()`,
            and `ping()` so future modules don't import the SDK
            directly
- [x] 1.3 `main.rs` calls `HubClient::from_env()` immediately before
            building `NexusServer` and runs a `ping()` probe; the
            result is logged with the resolved base URL
- [x] 1.4 Reads `HIVEHUB_CLOUD_SERVICE_API_KEY` /
            `HIVEHUB_CLOUD_BASE_URL` (the env names the SDK already
            documents — single source of truth instead of re-aliasing
            to a Nexus-specific name). `HIVEHUB_DISABLED=1` is the
            explicit opt-out for local / single-tenant deployments
- [x] 1.5 `HubClient::ping()` runs a 3 s timeout HEAD via reqwest;
            returns `HubHealthStatus::{Connected, Disconnected,
            Disabled}`. Reconnection is best-effort: a probe failure
            is logged + surfaces in `/health` once §2 wires the
            handle into the server state; the server stays up so a
            transient Hub outage doesn't kill in-flight Cypher
            traffic. Four `hub::client::tests` unit tests (env
            opt-out, missing key, explicit constructor, debug
            redaction) all green.

## 2. Authentication Module
- [ ] 2.1 Create nexus-server/src/auth/hub_auth.rs
- [ ] 2.2 Implement Hub access key validation middleware
- [ ] 2.3 Extract user_id from validated tokens
- [ ] 2.4 Add UserContext struct to request state
- [ ] 2.5 Update API routes to require authentication

## 3. Database-Per-User System
- [ ] 3.1 Implement automatic database creation on first user access
- [ ] 3.2 Create database naming: user_{user_id}_nexus
- [ ] 3.3 Implement database connection pool per user
- [ ] 3.4 Add database routing logic based on user_id
- [ ] 3.5 Implement database lifecycle management

## 4. Hub API Integration
- [ ] 4.1 Implement get_user_database() via SDK
- [ ] 4.2 Request database creation through Hub
- [ ] 4.3 Implement check_quota() for nodes/relationships
- [ ] 4.4 Implement consume_credits() for LLM operations
- [ ] 4.5 Implement update_usage() for usage reporting

## 5. Credit Management
- [ ] 5.1 Check credits before LLM classification
- [ ] 5.2 Consume credits via Hub after operation
- [ ] 5.3 Track credit usage per operation type
- [ ] 5.4 Return 402 Payment Required on insufficient credits
- [ ] 5.5 Add credit balance to user info endpoint

## 6. Quota Management
- [ ] 6.1 Check node count quota before CREATE
- [ ] 6.2 Check relationship count quota before CREATE
- [ ] 6.3 Check storage quota before operations
- [ ] 6.4 Return 429 Too Many Requests on quota exceeded
- [ ] 6.5 Add quota metrics to monitoring

## 7. Usage Tracking
- [ ] 7.1 Track node creation/deletion
- [ ] 7.2 Track relationship creation/deletion
- [ ] 7.3 Track storage usage per database
- [ ] 7.4 Track credit consumption
- [ ] 7.5 Implement periodic usage sync (every 5 minutes)
- [ ] 7.6 Report usage on database modifications

## 8. API Updates
- [ ] 8.1 Add authentication to Cypher endpoint
- [ ] 8.2 Route queries to user's database
- [ ] 8.3 Add authentication to Bolt protocol
- [ ] 8.4 Update GraphQL endpoint with auth
- [ ] 8.5 Update API documentation

## 9. MCP Integration
- [ ] 9.1 Create nexus-server/src/mcp/hub_gateway.rs
- [ ] 9.2 Register MCP server with Hub on startup
- [ ] 9.3 Route MCP Cypher queries to user database
- [ ] 9.4 Validate MCP access keys through Hub
- [ ] 9.5 Add MCP operation logging

## 10. Cluster Mode
- [ ] 10.1 Implement user database shard routing
- [ ] 10.2 Propagate UserContext across nodes
- [ ] 10.3 Implement distributed quota checking
- [ ] 10.4 Add user database replication
- [ ] 10.5 Test multi-node user isolation

## 11. Data Migration
- [ ] 11.1 Create migration/hub_migration.rs
- [ ] 11.2 Identify existing databases without user_id
- [ ] 11.3 Map databases to users (interactive or config)
- [ ] 11.4 Create user-scoped databases
- [ ] 11.5 Copy data to user databases
- [ ] 11.6 Create backup before migration
- [ ] 11.7 Add rollback capability

## 12. Configuration
- [ ] 12.1 Add [hub] section to config.yml
- [ ] 12.2 Add hub.api_url configuration
- [ ] 12.3 Add hub.service_api_key (from env)
- [ ] 12.4 Add hub.enabled flag
- [ ] 12.5 Add hub.usage_report_interval
- [ ] 12.6 Add hub.database_prefix

## 13. Error Handling
- [ ] 13.1 Add HubError enum to error types
- [ ] 13.2 Handle Hub connection failures
- [ ] 13.3 Add retry logic for Hub API calls
- [ ] 13.4 Return proper HTTP status codes
- [ ] 13.5 Add detailed error logging

## 14. Testing
- [ ] 14.1 Add tests/hub_integration_test.rs
- [ ] 14.2 Mock Hub API for testing
- [ ] 14.3 Test database-per-user isolation
- [ ] 14.4 Test quota enforcement
- [ ] 14.5 Test credit management
- [ ] 14.6 Test usage reporting
- [ ] 14.7 Test cluster mode with users
- [ ] 14.8 Achieve 95%+ coverage

## 15. Documentation
- [ ] 15.1 Create docs/HUB_INTEGRATION.md
- [ ] 15.2 Document authentication flow
- [ ] 15.3 Document database-per-user model
- [ ] 15.4 Document credit system
- [ ] 15.5 Create migration guide
- [ ] 15.6 Update README with Hub setup
- [ ] 15.7 Add troubleshooting section

## 16. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 16.1 Update or create documentation covering the implementation
- [ ] 16.2 Write tests covering the new behavior
- [ ] 16.3 Run tests and confirm they pass
