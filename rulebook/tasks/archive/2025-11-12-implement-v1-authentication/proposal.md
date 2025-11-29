# Implement V1 Authentication & Security

## Why

Production deployment requires comprehensive authentication, authorization, and access control. The system must support:
- Root user configuration for initial setup (Docker-friendly)
- User management (create, remove, grant/revoke permissions)
- Temporary access keys with expiration
- Fine-grained permissions per functionality (queue, chatroom, REST routes)
- Protection of all REST endpoints
- SDK and MCP integration with API keys

## What Changes

### Core Authentication Features
- Root user configuration via environment variables or config file
- Default root credentials: `root/root` (configurable, can be disabled after setup)
- User management: create, remove, grant permissions, revoke permissions
- API key authentication with Argon2 hashing
- Temporary access keys with expiration and revocation
- RBAC with fine-grained permissions (READ, WRITE, ADMIN, SUPER, QUEUE, CHATROOM, REST)
- Rate limiting (1000/min, 10000/hour per key)
- JWT token support for session-based auth
- Audit logging for all operations

### Security Features
- Force authentication when binding to 0.0.0.0 (public)
- Optional authentication for localhost (development)
- Root user disable option after initial configuration
- Key revocation and expiration
- Permission-based access control per endpoint

### Integration Points
- REST API endpoints protection (all routes)
- SDK authentication via API keys
- MCP server authentication via API keys
- Queue system permissions
- Chatroom permissions

**BREAKING**: None (opt-in feature, disabled by default for localhost)

## Impact

### Affected Specs
- NEW capability: `authentication`
- NEW capability: `authorization`
- NEW capability: `rate-limiting`
- NEW capability: `user-management`
- NEW capability: `key-management`

### Affected Code
- `nexus-core/src/auth/mod.rs` - Auth module (extend existing ~600 lines)
- `nexus-core/src/auth/user.rs` - User management (~300 lines)
- `nexus-core/src/auth/key.rs` - API key management (~400 lines)
- `nexus-server/src/middleware/auth.rs` - Auth middleware (~300 lines)
- `nexus-server/src/api/auth.rs` - Auth REST endpoints (~500 lines)
- `nexus-server/src/config.rs` - Root user configuration (~100 lines)
- `tests/auth_tests.rs` - Security tests (~500 lines)
- `tests/integration/auth_s2s_test.rs` - Integration tests (~300 lines)

### Dependencies
- Requires: MVP complete
- Requires: RBAC module (already implemented)

### Timeline
- **Duration**: 2-3 weeks
- **Complexity**: High
- **Priority**: 🔴 CRITICAL (for production deployment)

## Detailed Requirements

### 1. Root User Configuration

**Environment Variables:**
- `NEXUS_ROOT_USERNAME` - Root username (default: "root")
- `NEXUS_ROOT_PASSWORD` - Root password (default: "root")
- `NEXUS_ROOT_ENABLED` - Enable root user (default: "true")
- `NEXUS_DISABLE_ROOT_AFTER_SETUP` - Auto-disable root after first admin user created (default: "false")

**Config File Support:**
- `config/auth.toml` - Authentication configuration
- Support for Docker secrets and environment variable injection

**Root User Capabilities:**
- Full system access (SUPER permission)
- Create/remove users
- Grant/revoke all permissions
- Manage API keys
- Cannot be deleted (only disabled)

### 2. User Management

**Operations:**
- `CREATE USER username [WITH PASSWORD 'password']` - Create new user
- `DROP USER username` - Remove user
- `GRANT permission TO username` - Grant permission
- `REVOKE permission FROM username` - Revoke permission
- `SHOW USERS` - List all users
- `SHOW USER username` - Show user details and permissions

**Permissions:**
- `READ` - Read-only access to data
- `WRITE` - Write access to data
- `ADMIN` - Administrative operations
- `SUPER` - Superuser (full access)
- `QUEUE` - Queue operations (publish, consume)
- `CHATROOM` - Chatroom operations
- `REST` - REST API access

### 3. API Key Management

**Key Types:**
- Permanent keys (no expiration)
- Temporary keys (with expiration date)
- Revocable keys (can be revoked before expiration)

**Operations:**
- `CREATE API KEY [FOR username] [WITH PERMISSIONS ...] [EXPIRES IN 'duration']` - Create API key
- `REVOKE API KEY 'key_id'` - Revoke API key
- `SHOW API KEYS [FOR username]` - List API keys
- `DELETE API KEY 'key_id'` - Delete API key

**Key Format:**
- 32-character random string
- Prefixed with `nx_` for identification
- Example: `nx_a1b2c3d4e5f6g7h8i9j0k1l2m3n4o5p6`

### 4. REST API Protection

**Protected Endpoints:**
- All `/cypher` endpoints
- All `/data/*` endpoints
- All `/schema/*` endpoints
- All `/knn_traverse` endpoints
- All `/ingest` endpoints
- All `/clustering/*` endpoints
- All `/stats` endpoints
- All `/health` endpoints (optional, configurable)

**Authentication Methods:**
- Bearer token: `Authorization: Bearer nx_...`
- API key header: `X-API-Key: nx_...`
- Basic auth: `Authorization: Basic base64(username:password)`

**Response Codes:**
- `401 Unauthorized` - Missing or invalid credentials
- `403 Forbidden` - Insufficient permissions
- `429 Too Many Requests` - Rate limit exceeded

### 5. SDK Integration

**SDK Requirements:**
- All SDKs must accept API key in constructor
- SDKs should use Bearer token authentication
- SDKs should handle 401/403/429 errors gracefully
- SDKs should support key rotation

**Example SDK Usage:**
```rust
let client = NexusClient::new("http://localhost:15474")
    .with_api_key("nx_...")
    .build();
```

### 6. MCP Integration

**MCP Server Requirements:**
- MCP server must authenticate with API key
- MCP server should use same authentication as REST API
- MCP server should respect permissions (read-only vs write)

**Configuration:**
- `NEXUS_MCP_API_KEY` - API key for MCP server
- MCP server validates key on startup
- MCP operations respect user permissions

### 7. Queue & Chatroom Permissions

**Queue Permissions:**
- `QUEUE:READ` - Consume from queues
- `QUEUE:WRITE` - Publish to queues
- `QUEUE:ADMIN` - Manage queues (create, delete)

**Chatroom Permissions:**
- `CHATROOM:READ` - Read messages
- `CHATROOM:WRITE` - Send messages
- `CHATROOM:ADMIN` - Manage chatrooms

### 8. Audit Logging

**Logged Operations:**
- User creation/deletion
- Permission grants/revocations
- API key creation/revocation
- Authentication failures
- All write operations (CREATE, SET, DELETE)

**Log Format:**
- JSON format with timestamp, user_id, operation, result
- Rotated daily with compression
- Configurable retention period

## Implementation Phases

### Phase 1: Root User & Basic Auth (Week 1)
- Root user configuration
- Basic authentication middleware
- User CRUD operations
- Permission management

### Phase 2: API Keys & REST Protection (Week 1-2)
- API key generation and management
- REST endpoint protection
- Rate limiting integration
- SDK authentication support

### Phase 3: Advanced Features (Week 2-3)
- Temporary keys with expiration
- Key revocation
- MCP authentication
- Queue/Chatroom permissions
- Audit logging

### Phase 4: Testing & Documentation (Week 3)
- Comprehensive test coverage (95%+)
- Integration tests
- Security audit
- Documentation updates

## Success Criteria

- ✅ Root user can be configured via environment variables
- ✅ Root user can be disabled after setup
- ✅ All REST endpoints are protected
- ✅ SDKs can authenticate with API keys
- ✅ MCP server can authenticate with API keys
- ✅ Fine-grained permissions work for queue/chatroom
- ✅ Temporary keys expire correctly
- ✅ Key revocation works immediately
- ✅ Rate limiting prevents abuse
- ✅ Audit logging captures all operations
- ✅ 95%+ test coverage
- ✅ All security best practices followed
