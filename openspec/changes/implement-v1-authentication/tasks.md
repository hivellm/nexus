# Implementation Tasks - V1 Authentication & Security

**Status**: ⚪ Not Started  
**Priority**: 🔴 CRITICAL  
**Started**: TBD  
**Target Completion**: TBD  
**Version**: v0.11.0  
**Tests**: 0/0 passing (0%)

**Dependencies**: 
- MVP complete ✅
- RBAC module ✅ (already implemented)

---

## Phase 1: Root User & Configuration (Week 1)

### 1.1 Root User Configuration
- [x] 1.1.1 Add root user configuration to `nexus-server/src/config.rs` ✅
- [x] 1.1.2 Support environment variables: `NEXUS_ROOT_USERNAME`, `NEXUS_ROOT_PASSWORD`, `NEXUS_ROOT_ENABLED` ✅
- [ ] 1.1.3 Support config file: `config/auth.toml`
- [x] 1.1.4 Default root credentials: `root/root` (configurable) ✅
- [x] 1.1.5 Add `NEXUS_DISABLE_ROOT_AFTER_SETUP` flag ✅
- [ ] 1.1.6 Implement root user auto-disable after first admin user creation
- [x] 1.1.7 Add Docker secrets support (`NEXUS_ROOT_PASSWORD_FILE`) ✅
- [x] 1.1.8 Add unit tests for root user configuration ✅

### 1.2 Root User Management
- [x] 1.2.1 Implement root user creation on startup ✅
- [x] 1.2.2 Implement root user disable functionality ✅
- [x] 1.2.3 Prevent root user deletion (only disable) ✅
- [ ] 1.2.4 Add root user validation (cannot be modified by non-root)
- [x] 1.2.5 Add unit tests for root user management ✅

### 1.3 User CRUD Operations
- [x] 1.3.1 Implement `CREATE USER username [SET PASSWORD 'password']` Cypher command ✅ (with password hashing)
- [x] 1.3.2 Implement `DROP USER username` Cypher command ✅
- [x] 1.3.3 Implement `SHOW USERS` Cypher command ✅ (already existed)
- [x] 1.3.4 Implement `SHOW USER username` Cypher command ✅
- [x] 1.3.5 Add REST endpoint: `POST /auth/users` ✅
- [x] 1.3.6 Add REST endpoint: `DELETE /auth/users/{username}` ✅
- [x] 1.3.7 Add REST endpoint: `GET /auth/users` ✅
- [x] 1.3.8 Add REST endpoint: `GET /auth/users/{username}` ✅
- [ ] 1.3.9 Add unit tests for user CRUD operations
- [ ] 1.3.10 Add integration tests (S2S)

### 1.4 Permission Management
- [x] 1.4.1 Implement `GRANT permission TO username` Cypher command ✅ (already existed)
- [x] 1.4.2 Implement `REVOKE permission FROM username` Cypher command ✅ (already existed)
- [x] 1.4.3 Add fine-grained permissions: QUEUE, CHATROOM, REST ✅
- [x] 1.4.4 Add REST endpoint: `POST /auth/users/{username}/permissions` ✅
- [x] 1.4.5 Add REST endpoint: `DELETE /auth/users/{username}/permissions/{permission}` ✅
- [x] 1.4.6 Add REST endpoint: `GET /auth/users/{username}/permissions` ✅
- [ ] 1.4.7 Add unit tests for permission management
- [ ] 1.4.8 Add integration tests (S2S)

**Phase 1 Testing & Quality**:
- [ ] Run full test suite for Phase 1
- [ ] Achieve 95%+ coverage for Phase 1
- [ ] Run clippy with -D warnings (no warnings allowed)
- [ ] Update CHANGELOG.md for Phase 1

## Phase 2: API Key Management & REST Protection (Week 1-2)

### 2.1 API Key Generation & Storage
- [ ] 2.1.1 Implement API key generation (32-char random, prefixed with `nx_`)
- [ ] 2.1.2 Implement Argon2 hashing for API keys
- [ ] 2.1.3 Add LMDB persistence for API keys (currently in-memory)
- [ ] 2.1.4 Implement key metadata (name, permissions, expiry, created_at)
- [ ] 2.1.5 Add unit tests for API key generation

### 2.2 API Key CRUD Operations
- [ ] 2.2.1 Implement `CREATE API KEY [FOR username] [WITH PERMISSIONS ...] [EXPIRES IN 'duration']` Cypher command
- [ ] 2.2.2 Implement `REVOKE API KEY 'key_id'` Cypher command
- [ ] 2.2.3 Implement `SHOW API KEYS [FOR username]` Cypher command
- [ ] 2.2.4 Implement `DELETE API KEY 'key_id'` Cypher command
- [ ] 2.2.5 Add REST endpoint: `POST /auth/keys`
- [ ] 2.2.6 Add REST endpoint: `GET /auth/keys`
- [ ] 2.2.7 Add REST endpoint: `GET /auth/keys/{key_id}`
- [ ] 2.2.8 Add REST endpoint: `DELETE /auth/keys/{key_id}`
- [ ] 2.2.9 Add REST endpoint: `POST /auth/keys/{key_id}/revoke`
- [ ] 2.2.10 Add unit tests for API key CRUD operations
- [ ] 2.2.11 Add integration tests (S2S)

### 2.3 Temporary Keys & Expiration
- [ ] 2.3.1 Implement expiration date for API keys
- [ ] 2.3.2 Implement automatic expiration check on validation
- [ ] 2.3.3 Implement `EXPIRES IN 'duration'` parsing (e.g., "7d", "24h", "30m")
- [ ] 2.3.4 Add cleanup job for expired keys
- [ ] 2.3.5 Add unit tests for expiration logic

### 2.4 Key Revocation
- [ ] 2.4.1 Implement key revocation (mark as revoked, not deleted)
- [ ] 2.4.2 Implement immediate revocation check on validation
- [ ] 2.4.3 Add revocation reason/comment
- [ ] 2.4.4 Add unit tests for revocation logic

### 2.5 REST Endpoint Protection
- [ ] 2.5.1 Add authentication middleware to all REST routes
- [ ] 2.5.2 Implement Bearer token extraction (`Authorization: Bearer nx_...`)
- [ ] 2.5.3 Implement API key header extraction (`X-API-Key: nx_...`)
- [ ] 2.5.4 Implement Basic auth extraction (`Authorization: Basic ...`)
- [ ] 2.5.5 Add 401 Unauthorized responses for missing/invalid credentials
- [ ] 2.5.6 Add 403 Forbidden responses for insufficient permissions
- [ ] 2.5.7 Protect all `/cypher` endpoints
- [ ] 2.5.8 Protect all `/data/*` endpoints
- [ ] 2.5.9 Protect all `/schema/*` endpoints
- [ ] 2.5.10 Protect all `/knn_traverse` endpoints
- [ ] 2.5.11 Protect all `/ingest` endpoints
- [ ] 2.5.12 Protect all `/clustering/*` endpoints
- [ ] 2.5.13 Protect all `/stats` endpoints
- [ ] 2.5.14 Make `/health` optional (configurable)
- [ ] 2.5.15 Add unit tests for endpoint protection
- [ ] 2.5.16 Add integration tests (S2S)

### 2.6 Rate Limiting Integration
- [ ] 2.6.1 Integrate rate limiter with API key authentication
- [ ] 2.6.2 Add 429 Too Many Requests responses
- [ ] 2.6.3 Add `X-RateLimit-Limit` header
- [ ] 2.6.4 Add `X-RateLimit-Remaining` header
- [ ] 2.6.5 Add `X-RateLimit-Reset` header
- [ ] 2.6.6 Add unit tests for rate limiting headers

**Phase 2 Testing & Quality**:
- [ ] Run full test suite for Phase 2
- [ ] Achieve 95%+ coverage for Phase 2
- [ ] Run clippy with -D warnings (no warnings allowed)
- [ ] Update CHANGELOG.md for Phase 2

## Phase 3: Advanced Features (Week 2-3)

### 3.1 JWT Token Support
- [ ] 3.1.1 Implement JWT token generation
- [ ] 3.1.2 Implement JWT validation
- [ ] 3.1.3 Add `POST /auth/login` endpoint (username/password -> JWT)
- [ ] 3.1.4 Add configurable token expiration
- [ ] 3.1.5 Add refresh token support
- [ ] 3.1.6 Add unit tests for JWT

### 3.2 SDK Authentication
- [ ] 3.2.1 Update Rust SDK to accept API key in constructor
- [ ] 3.2.2 Update Python SDK to accept API key in constructor
- [ ] 3.2.3 Update JavaScript SDK to accept API key in constructor
- [ ] 3.2.4 Implement Bearer token authentication in all SDKs
- [ ] 3.2.5 Add error handling for 401/403/429 in SDKs
- [ ] 3.2.6 Add key rotation support in SDKs
- [ ] 3.2.7 Add SDK authentication tests

### 3.3 MCP Authentication
- [ ] 3.3.1 Add `NEXUS_MCP_API_KEY` environment variable support
- [ ] 3.3.2 Implement MCP server API key validation on startup
- [ ] 3.3.3 Implement MCP operation permission checking
- [ ] 3.3.4 Add MCP authentication tests

### 3.4 Queue Permissions
- [ ] 3.4.1 Add `QUEUE:READ` permission check for consume operations
- [ ] 3.4.2 Add `QUEUE:WRITE` permission check for publish operations
- [ ] 3.4.3 Add `QUEUE:ADMIN` permission check for queue management
- [ ] 3.4.4 Add unit tests for queue permissions

### 3.5 Chatroom Permissions
- [ ] 3.5.1 Add `CHATROOM:READ` permission check for read operations
- [ ] 3.5.2 Add `CHATROOM:WRITE` permission check for send operations
- [ ] 3.5.3 Add `CHATROOM:ADMIN` permission check for chatroom management
- [ ] 3.5.4 Add unit tests for chatroom permissions

### 3.6 Audit Logging
- [ ] 3.6.1 Implement audit log structure (JSON format)
- [ ] 3.6.2 Log user creation/deletion
- [ ] 3.6.3 Log permission grants/revocations
- [ ] 3.6.4 Log API key creation/revocation
- [ ] 3.6.5 Log authentication failures
- [ ] 3.6.6 Log all write operations (CREATE, SET, DELETE)
- [ ] 3.6.7 Implement log rotation (daily)
- [ ] 3.6.8 Implement log compression
- [ ] 3.6.9 Add configurable retention period
- [ ] 3.6.10 Add unit tests for audit logging

**Phase 3 Testing & Quality**:
- [ ] Run full test suite for Phase 3
- [ ] Achieve 95%+ coverage for Phase 3
- [ ] Run clippy with -D warnings (no warnings allowed)
- [ ] Update CHANGELOG.md for Phase 3

## Phase 4: Testing & Documentation (Week 3)

### 4.1 Comprehensive Testing
- [ ] 4.1.1 Add unit tests for all authentication modules (95%+ coverage)
- [ ] 4.1.2 Add integration tests (S2S) for all endpoints
- [ ] 4.1.3 Add security tests (penetration testing)
- [ ] 4.1.4 Add performance tests (rate limiting under load)
- [ ] 4.1.5 Add Docker integration tests

### 4.2 Documentation
- [ ] 4.2.1 Update `docs/AUTHENTICATION.md` with full guide
- [ ] 4.2.2 Update `docs/API.md` with authentication requirements
- [ ] 4.2.3 Update `README.md` with authentication examples
- [ ] 4.2.4 Update `docs/ROADMAP.md` with authentication status
- [ ] 4.2.5 Add Docker deployment guide with root user setup
- [ ] 4.2.6 Add SDK authentication examples

### 4.3 Security Audit
- [ ] 4.3.1 Review all authentication code for security vulnerabilities
- [ ] 4.3.2 Test for common attacks (SQL injection, XSS, CSRF)
- [ ] 4.3.3 Verify Argon2 configuration (secure parameters)
- [ ] 4.3.4 Verify key generation randomness
- [ ] 4.3.5 Verify rate limiting effectiveness

### 4.4 Final Quality Checks
- [ ] 4.4.1 Run complete test suite (100% pass rate required)
- [ ] 4.4.2 Verify 95%+ code coverage for entire auth module
- [ ] 4.4.3 Run cargo clippy with -D warnings (no warnings allowed)
- [ ] 4.4.4 Run cargo fmt --all (formatting check)
- [ ] 4.4.5 Run type-check / compilation check
- [ ] 4.4.6 Update CHANGELOG.md with complete feature list
- [ ] 4.4.7 Tag release version (v0.11.0)
- [ ] 4.4.8 Update version in Cargo.toml
- [ ] 4.4.9 Create release notes

## Estimated Timeline

- **Phase 1**: 1 week (Root user & basic auth)
- **Phase 2**: 1-2 weeks (API keys & REST protection)
- **Phase 3**: 1 week (Advanced features)
- **Phase 4**: 3-5 days (Testing & documentation)
- **Total**: 2-3 weeks

## Notes

- Root user should be disabled by default in production after initial setup
- All authentication should be opt-in for localhost (development)
- All authentication should be required for 0.0.0.0 (public)
- API keys should be stored hashed (never plaintext)
- Rate limiting should be configurable per user/key
- Audit logs should be tamper-proof (append-only)
- Code quality standards must be maintained throughout (95%+ coverage, no clippy warnings)
- All changes must follow AGENTS.md Rust guidelines (Edition 2024, nightly toolchain)
