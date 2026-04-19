# Implementation Tasks - Cluster Mode with HiveHub Integration

**Status**: Pending
**Priority**: High (enables multi-tenant shared infrastructure)

---

## Phase 1: HiveHub API Integration (2-3 weeks)

### 1. HiveHub SDK Integration
- [ ] 1.1 Add `hivehub-cloud-internal-sdk` dependency to `nexus-core/Cargo.toml`
- [ ] 1.2 Create `nexus-core/src/cluster/hivehub.rs` module as SDK wrapper
- [ ] 1.3 Initialize HiveHub SDK client with service API key from configuration
- [ ] 1.4 Implement quota fetching using SDK's `nexus().get_user_database()` method
- [ ] 1.5 Implement user metadata fetching using SDK's `get_user_info()` method
- [ ] 1.6 Implement quota checking using SDK's `nexus().check_quota()` method
- [ ] 1.7 Implement usage updates using SDK's `nexus().update_usage()` method
- [ ] 1.8 Configure SDK client with base URL, timeout, and retry policy
- [ ] 1.9 Write unit tests for HiveHub SDK wrapper
- [ ] 1.10 Write integration tests with mock SDK client

### 2. Quota Management Service
- [x] 2.1 Create `nexus-core/src/cluster/quota.rs` module
- [ ] 2.2 Implement quota cache layer on top of SDK (leveraging SDK's built-in caching)
- [x] 2.3 Implement quota validation logic (synchronous `check_rate` / `check_storage` on `LocalQuotaProvider`)
- [x] 2.4 Implement storage quota tracking per user (`record_usage` + `snapshot`)
- [x] 2.5 Implement rate limit quota tracking per user (per-minute + per-hour `RateWindow`)
- [ ] 2.6 Map SDK quota errors to Nexus quota error types
- [x] 2.7 Write unit tests for quota service (5 unit tests in `cluster::quota::tests`)
- [ ] 2.8 Write integration tests for quota enforcement

### 3. Configuration
- [x] 3.1 Add cluster mode configuration flag (`ClusterConfig { enabled, default_quotas }` in `cluster::config`)
- [ ] 3.2 Add HiveHub SDK configuration (base_url, service_api_key for SDK initialization)
- [ ] 3.3 Add SDK client configuration (timeout, retries, retry_delay)
- [ ] 3.4 Add quota cache TTL configuration (if using additional caching layer)
- [x] 3.5 Update config loading to read cluster mode settings (server `Config` carries `nexus_core::cluster::ClusterConfig`; `NEXUS_CLUSTER_ENABLED` env var flips the master switch)
- [ ] 3.6 Initialize HiveHub SDK client from configuration on server startup
- [ ] 3.7 Write tests for configuration loading and SDK initialization

---

## Phase 2: Data Segmentation by User (3-4 weeks)

### 4. Namespace System
- [x] 4.1 Create `nexus-core/src/cluster/namespace.rs` module
- [x] 4.2 Implement user namespace ID generation and validation (`UserNamespace::new` rejects empty / oversized / delimiter / control chars)
- [x] 4.3 Implement namespace prefix for storage keys (`prefix()`, `prefix_key()`, `owns()`)
- [x] 4.4 Add namespace context to execution context — `cluster::UserContext` carries namespace + api-key id + optional function allow-list
- [x] 4.5 Write unit tests for namespace system (8 unit tests in `cluster::namespace::tests`, 6 in `cluster::context::tests`)

### 5. Storage Layer Namespace Support
- [ ] 5.1 Modify catalog to support namespaced labels/types/keys
- [ ] 5.2 Modify node storage to include namespace prefix
- [ ] 5.3 Modify relationship storage to include namespace prefix
- [ ] 5.4 Modify property storage to include namespace prefix
- [ ] 5.5 Update storage queries to filter by namespace
- [ ] 5.6 Write unit tests for namespaced storage operations
- [ ] 5.7 Write integration tests for data isolation

### 6. Query Execution Namespace Scoping
- [ ] 6.1 Modify query planner to inject namespace filters
- [ ] 6.2 Update MATCH operations to scope to namespace
- [ ] 6.3 Update CREATE operations to assign namespace
- [ ] 6.4 Update UPDATE/DELETE operations to filter by namespace
- [ ] 6.5 Ensure queries cannot access data outside namespace
- [ ] 6.6 Write unit tests for namespace-scoped queries
- [ ] 6.7 Write integration tests for cross-namespace isolation

### 7. Storage Quota Tracking
- [ ] 7.1 Implement storage size calculation per namespace
- [ ] 7.2 Add storage quota check before write operations
- [ ] 7.3 Implement storage usage reporting
- [ ] 7.4 Add periodic storage quota sync with HiveHub
- [ ] 7.5 Write tests for storage quota enforcement

---

## Phase 3: Enhanced Authentication & Permissions (2-3 weeks)

### 8. API Key Enhancements
- [x] 8.1 Add `user_id` field to API key structure (pre-existing — `ApiKey.user_id: Option<String>`)
- [x] 8.2 Add `allowed_functions` field to API key (`Option<Vec<String>>`, `#[serde(default)]` for legacy records)
- [x] 8.3 Update API key creation to accept function permissions (`ApiKey::with_allowed_functions` builder method)
- [x] 8.4 Update API key storage to persist function permissions (LMDB serialization switched from bincode to JSON for forward-compat; `may_call_function` accessor added)
- [x] 8.5 Write unit tests for enhanced API keys (4 new tests in `auth::api_key::tests` covering default / restricted / empty-list / legacy-record round-trip)

### 9. Function-Level Permission Filtering
- [ ] 9.1 Extend permission enum to include function-level permissions
- [ ] 9.2 Update permission checking to validate function access
- [ ] 9.3 Add function permission middleware for MCP endpoints
- [ ] 9.4 Filter available MCP functions based on API key permissions
- [ ] 9.5 Add error responses for unauthorized function access
- [ ] 9.6 Write unit tests for function permission filtering
- [ ] 9.7 Write integration tests for MCP function isolation

### 10. Mandatory Authentication for Cluster Mode
- [x] 10.1 Update auth middleware to require auth in cluster mode (`AuthMiddleware::with_cluster_mode` + short-circuit in `requires_auth`)
- [x] 10.2 Remove public endpoint exceptions in cluster mode (`/`, `/health`, `/stats`, `/openapi.json` all require auth when `cluster_enabled`)
- [x] 10.3 Update health check to require authentication in cluster mode (covered by 10.2 — same code path)
- [x] 10.4 Update all REST endpoints to check cluster mode flag (`main.rs` wires `config.cluster.enabled` into `create_auth_middleware`)
- [x] 10.5 Ensure MCP endpoints always require auth in cluster mode (`create_mcp_router` accepts `cluster_enabled` and hands it to the auth layer)
- [x] 10.6 Write tests for mandatory authentication (`cluster_mode_requires_auth_on_every_path` + 4 `user_context_from_api_key_*` tests in `auth::middleware::tests`)
- [x] 10.7 Write tests for public endpoint blocking in cluster mode (covered by `cluster_mode_requires_auth_on_every_path` — asserts `/`, `/health`, `/stats`, `/openapi.json` all return `true`)

### 11. User Context Propagation
- [x] 11.1 Extract user_id from API key in middleware (`AuthMiddleware::user_context_from_api_key`)
- [x] 11.2 Add user context to request extensions (separate extension slot, not inside `AuthContext`)
- [x] 11.3 Propagate user context through execution context (`extract_user_context(&Request)` helper exported from `auth`)
- [x] 11.4 Ensure user context is available in all operations — the extension slot is set before `next.run(request)` so every downstream layer sees it
- [x] 11.5 Write tests for user context propagation (4 tests in `auth::middleware::tests` covering unrestricted / restricted / missing / invalid user_id cases)

---

## Phase 4: Rate Limiting & Quota Enforcement (2-3 weeks)

### 12. Rate Limiting Implementation
- [ ] 12.1 Implement per-user rate limiting using quotas
- [ ] 12.2 Add rate limit tracking per user (requests per minute/hour)
- [ ] 12.3 Implement rate limit headers (X-RateLimit-*)
- [ ] 12.4 Add rate limit middleware for all endpoints
- [ ] 12.5 Handle rate limit exceeded responses (429)
- [ ] 12.6 Write unit tests for rate limiting
- [ ] 12.7 Write integration tests for quota-based rate limiting

### 13. Quota Enforcement Middleware
- [ ] 13.1 Add quota check middleware for write operations
- [ ] 13.2 Implement storage quota check before CREATE/UPDATE
- [ ] 13.3 Implement storage quota check before data import
- [ ] 13.4 Add quota exceeded error responses
- [ ] 13.5 Write tests for quota enforcement
- [ ] 13.6 Write load tests for quota enforcement

### 14. Usage Tracking & Reporting
- [ ] 14.1 Implement usage metrics tracking (requests, storage, operations)
- [ ] 14.2 Use SDK's `nexus().update_usage()` method for periodic usage reporting to HiveHub
- [ ] 14.3 Implement usage aggregation per user before calling SDK
- [ ] 14.4 Add usage statistics endpoints (authenticated) - local stats only
- [ ] 14.5 Write tests for usage tracking
- [ ] 14.6 Write tests for usage reporting with SDK mock

---

## Phase 5: Testing & Documentation (1-2 weeks)

### 15. Comprehensive Testing
- [ ] 15.1 Write integration tests for multi-user data isolation
- [ ] 15.2 Write integration tests for quota enforcement
- [ ] 15.3 Write integration tests for function-level permissions
- [ ] 15.4 Write load tests for cluster mode performance
- [ ] 15.5 Write security tests for data leakage prevention
- [ ] 15.6 Verify all existing tests pass with cluster mode disabled
- [ ] 15.7 Achieve ≥ 95% test coverage for cluster mode code

### 16. Documentation
- [ ] 16.1 Create `docs/CLUSTER_MODE.md` user guide
- [ ] 16.2 Create `docs/specs/cluster-mode.md` technical specification
- [ ] 16.3 Update `docs/AUTHENTICATION.md` with function permissions
- [ ] 16.4 Update `docs/DEPLOYMENT_GUIDE.md` with cluster mode setup
- [ ] 16.5 Create migration guide from standalone to cluster mode
- [ ] 16.6 Update CHANGELOG.md with cluster mode features
- [ ] 16.7 Create HiveHub integration guide for operators

### 17. Code Quality & Review
- [ ] 17.1 Run cargo clippy and fix all warnings
- [ ] 17.2 Run cargo fmt on all modified files
- [ ] 17.3 Review all code for security vulnerabilities
- [ ] 17.4 Perform code review for multi-tenancy isolation
- [ ] 17.5 Update inline documentation and comments

---

## Success Criteria Checklist

- [ ] All Phase 1 tasks complete (HiveHub integration)
- [ ] All Phase 2 tasks complete (Data segmentation)
- [ ] All Phase 3 tasks complete (Enhanced auth)
- [ ] All Phase 4 tasks complete (Rate limiting & quotas)
- [ ] All Phase 5 tasks complete (Testing & docs)
- [ ] Zero data leakage between users verified
- [ ] All endpoints authenticated in cluster mode
- [ ] Function-level permissions working correctly
- [ ] Quota enforcement tested and working
- [ ] Performance overhead < 15% vs standalone
- [ ] Test coverage ≥ 95% for cluster mode code
- [ ] All documentation complete
