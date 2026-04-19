# Authentication & Authorization Guide

This guide covers authentication and authorization features in Nexus Graph Database, including root user setup, user management, API keys, JWT tokens, permissions, rate limiting, and audit logging.

## Table of Contents

1. [Overview](#overview)
2. [Root User Setup](#root-user-setup)
3. [User Management](#user-management)
4. [API Keys](#api-keys)
5. [JWT Tokens](#jwt-tokens)
6. [Permissions](#permissions)
7. [Rate Limiting](#rate-limiting)
8. [Audit Logging](#audit-logging)
9. [Security Best Practices](#security-best-practices)
10. [API Examples](#api-examples)

## Overview

Nexus provides a comprehensive authentication and authorization system with:

- **Root User**: Initial superuser account for system setup
- **User Management**: Create, delete, and manage users
- **API Keys**: Long-lived credentials for programmatic access
- **JWT Tokens**: Short-lived tokens for user sessions
- **Permissions**: Fine-grained access control (READ, WRITE, ADMIN, SUPER, etc.)
- **Rate Limiting**: Protection against brute force and abuse
- **Audit Logging**: Complete audit trail of authentication events

## Root User Setup

The root user is the initial superuser account created on system startup. It has full system access (SUPER permission) and cannot be deleted (only disabled).

### Configuration

Root user can be configured via:

1. **Environment Variables** (highest priority):
   ```bash
   export NEXUS_ROOT_USERNAME="admin"
   export NEXUS_ROOT_PASSWORD="secure_password_123"
   export NEXUS_ROOT_ENABLED="true"
   export NEXUS_DISABLE_ROOT_AFTER_SETUP="true"
   ```

2. **Config File** (`config/auth.toml`):
   ```toml
   [root_user]
   username = "admin"
   password = "secure_password_123"
   enabled = true
   disable_after_setup = true
   ```

3. **Docker Secrets** (for production):
   ```bash
   # Mount secret file
   docker run -v /run/secrets/root_password:/run/secrets/root_password ...
   # Or use environment variable
   export NEXUS_ROOT_PASSWORD_FILE="/run/secrets/root_password"
   ```

### Default Values

- **Username**: `root`
- **Password**: `root` (⚠️ **CHANGE IN PRODUCTION**)
- **Enabled**: `true`
- **Disable After Setup**: `false`

### Auto-Disable Root User

When `NEXUS_DISABLE_ROOT_AFTER_SETUP=true`, the root user is automatically disabled after the first admin user is created. This is recommended for production deployments.

### Root User Capabilities

- Full system access (SUPER permission)
- Create/remove users
- Grant/revoke all permissions
- Manage API keys
- Cannot be deleted (only disabled)

## User Management

### Creating Users

**REST API:**
```bash
POST /auth/users
Content-Type: application/json

{
  "username": "alice",
  "password": "secure_password",
  "permissions": ["READ", "WRITE"]
}
```

**Cypher Command:**
```cypher
CREATE USER alice SET PASSWORD 'secure_password'
GRANT READ, WRITE TO alice
```

### Listing Users

**REST API:**
```bash
GET /auth/users
Authorization: Bearer <token>
```

**Cypher Command:**
```cypher
SHOW USERS
```

### Viewing User Details

**REST API:**
```bash
GET /auth/users/alice
Authorization: Bearer <token>
```

**Cypher Command:**
```cypher
SHOW USER alice
```

### Deleting Users

**REST API:**
```bash
DELETE /auth/users/alice
Authorization: Bearer <token>
```

**Cypher Command:**
```cypher
DROP USER alice
```

### Granting Permissions

**REST API:**
```bash
POST /auth/users/alice/permissions
Content-Type: application/json
Authorization: Bearer <token>

{
  "permissions": ["READ", "WRITE", "ADMIN"]
}
```

**Cypher Command:**
```cypher
GRANT READ, WRITE, ADMIN TO alice
```

### Revoking Permissions

**REST API:**
```bash
DELETE /auth/users/alice/permissions
Content-Type: application/json
Authorization: Bearer <token>

{
  "permissions": ["ADMIN"]
}
```

**Cypher Command:**
```cypher
REVOKE ADMIN FROM alice
```

## API Keys

API keys are long-lived credentials for programmatic access. They are hashed using Argon2 before storage.

### Creating API Keys

**REST API:**
```bash
POST /auth/api-keys
Content-Type: application/json
Authorization: Bearer <token>

{
  "name": "production-api-key",
  "permissions": ["READ", "WRITE"],
  "expires_at": "2025-12-31T23:59:59Z"  # Optional
}
```

**Response:**
```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "name": "production-api-key",
  "key": "nx_abc123def456...",  # Full key (only shown once!)
  "permissions": ["READ", "WRITE"],
  "created_at": "2025-01-01T00:00:00Z",
  "expires_at": "2025-12-31T23:59:59Z"
}
```

⚠️ **Important**: Store the full key (`nx_...`) securely. It cannot be retrieved later.

### Using API Keys

**Bearer Token:**
```bash
curl -H "Authorization: Bearer nx_abc123def456..." \
     https://api.example.com/cypher
```

**X-API-Key Header:**
```bash
curl -H "X-API-Key: nx_abc123def456..." \
     https://api.example.com/cypher
```

### Listing API Keys

**REST API:**
```bash
GET /auth/api-keys
Authorization: Bearer <token>
```

**Response:**
```json
{
  "keys": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "name": "production-api-key",
      "permissions": ["READ", "WRITE"],
      "created_at": "2025-01-01T00:00:00Z",
      "expires_at": "2025-12-31T23:59:59Z",
      "last_used": "2025-01-15T10:30:00Z",
      "is_active": true,
      "is_revoked": false
    }
  ]
}
```

### Revoking API Keys

**REST API:**
```bash
POST /auth/api-keys/{key_id}/revoke
Content-Type: application/json
Authorization: Bearer <token>

{
  "reason": "Key compromised"
}
```

### Deleting API Keys

**REST API:**
```bash
DELETE /auth/api-keys/{key_id}
Authorization: Bearer <token>
```

### API Key Expiration

API keys can have an expiration date. Expired keys are automatically rejected:

```json
{
  "name": "temporary-key",
  "expires_at": "2025-12-31T23:59:59Z"
}
```

## JWT Tokens

JWT tokens are short-lived credentials for user sessions. They are generated after successful login and can be refreshed.

### Login

**REST API:**
```bash
POST /auth/login
Content-Type: application/json

{
  "username": "alice",
  "password": "secure_password"
}
```

**Response:**
```json
{
  "access_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
  "refresh_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
  "expires_in": 3600,
  "token_type": "Bearer"
}
```

### Using JWT Tokens

**Bearer Token:**
```bash
curl -H "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..." \
     https://api.example.com/cypher
```

### Refreshing Tokens

**REST API:**
```bash
POST /auth/refresh
Content-Type: application/json

{
  "refresh_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..."
}
```

### Token Expiration

- **Access Token**: 1 hour (configurable via `JWT_EXPIRATION_SECONDS`)
- **Refresh Token**: 7 days (configurable via `JWT_REFRESH_EXPIRATION_SECONDS`)

## Permissions

Nexus uses a fine-grained permission system:

### Permission Types

- **READ**: Read-only access to data
- **WRITE**: Write access to data (includes READ)
- **ADMIN**: Administrative operations (includes READ, WRITE)
- **SUPER**: Superuser (full access, includes all permissions)
- **QUEUE**: Queue operations (publish, consume)
- **CHATROOM**: Chatroom operations
- **REST**: REST API access

### Permission Hierarchy

```
SUPER
  ├── ADMIN
  │     ├── WRITE
  │     │     └── READ
  │     └── QUEUE
  │     └── CHATROOM
  │     └── REST
```

### Checking Permissions

Permissions are automatically checked by the authentication middleware. If a user lacks required permissions, the API returns:

```json
{
  "error": {
    "code": "FORBIDDEN",
    "message": "Insufficient permissions: ADMIN required"
  }
}
```

### Required Permissions by Endpoint

| Endpoint | Required Permission |
|----------|-------------------|
| `GET /cypher` (read queries) | READ |
| `POST /cypher` (write queries) | WRITE |
| `POST /auth/users` | ADMIN |
| `DELETE /auth/users/{username}` | ADMIN |
| `POST /auth/api-keys` | ADMIN |
| `DELETE /auth/api-keys/{id}` | ADMIN |
| `POST /auth/users/{username}/permissions` | SUPER |

### Function-level permissions (cluster mode)

The `READ` / `WRITE` / `ADMIN` / ... enum above gates broad
capability classes — it answers "can this key run a CREATE at
all?". **Cluster mode** adds a second, orthogonal axis on top:
an explicit allow-list of MCP / RPC function names the key may
invoke. This is the difference between "has WRITE permission"
(coarse) and "may call `cypher.execute` but NOT
`nexus.admin.drop_database`" (fine).

Model:

```rust
pub struct ApiKey {
    // ... permissions: Vec<Permission>, ...
    pub allowed_functions: Option<Vec<String>>,
}
```

- `None` — unrestricted. Matches the pre-cluster-mode default;
  the key can call every function its `Permission` set allows.
- `Some(vec![…])` — the key can ONLY invoke functions whose
  canonical name appears in the list. Comparison is
  case-sensitive and exact; function-name canonicalisation
  (casing, prefixing) is the caller's responsibility.
- `Some(vec![])` — a deliberate third state. "May call NOTHING."
  Useful for health-probe-only keys that exist to verify
  uptime without any query capability.

Enforcement happens in handlers via:

```rust
let ctx = nexus_core::auth::extract_user_context(&request)
    .ok_or(StatusCode::UNAUTHORIZED)?;
ctx.require_may_call("cypher.execute")?;
```

Rejections come back as `403 Forbidden` with a stable body:

```json
{
  "function": "nexus.admin.drop_database",
  "code": "FUNCTION_NOT_ALLOWED",
  "message": "function 'nexus.admin.drop_database' is not in this API key's allow-list"
}
```

SDK decoders should match on `code`, never on `message` —
`FunctionAccessError::CODE` is the stable wire contract.

Discovery endpoints (MCP tool listing, etc.) should pre-filter
the advertised tools with `ctx.filter_callable(&tool_names)` so
clients only ever see operations they can actually invoke. Saves
a round-trip per "you can't call that" error.

### Tenant binding (cluster mode)

In cluster mode every API key **must** carry a `user_id`. The
auth middleware rejects any key without one (or with one that
fails `UserNamespace::new` validation — reserved `:` delimiter,
control characters, overlong) with `401 INVALID_TOKEN`. There is
no "global scope" fallback in cluster mode; a request that can't
be routed to a tenant cannot execute.

Standalone mode is unchanged: `user_id` stays optional, and
legacy keys that never had one continue to work.

## Rate Limiting

Rate limiting protects against brute force attacks and API abuse.

### Configuration

Rate limits are configured per-IP using a token bucket algorithm:

- **Default**: 1000 requests/minute, 10000 requests/hour
- **Configurable**: Via `AuthConfig.rate_limits`

### Rate Limit Headers

Responses include rate limit information:

```
X-RateLimit-Limit: 1000
X-RateLimit-Remaining: 999
X-RateLimit-Reset: 1640995200
```

### Rate Limit Exceeded

When rate limit is exceeded:

**Status Code**: `429 Too Many Requests`

**Response:**
```json
{
  "error": {
    "code": "RATE_LIMIT_EXCEEDED",
    "message": "Rate limit exceeded. Try again later."
  }
}
```

## Audit Logging

Audit logging records all authentication and authorization events for security and compliance.

### Logged Events

- User login attempts (success/failure)
- User logout
- API key usage
- Permission changes
- User creation/deletion
- API key creation/revocation/deletion

### Log Format

Audit logs are stored as JSON lines:

```json
{
  "timestamp": "2025-01-15T10:30:00Z",
  "event": "authentication_success",
  "actor": {
    "user_id": "alice",
    "username": "alice",
    "api_key_id": null
  },
  "ip_address": "192.168.1.100",
  "result": "success"
}
```

### Log Location

Audit logs are stored in: `{data_dir}/audit/audit-{YYYY-MM-DD}.log`

### Log Rotation

- **Daily rotation**: New log file each day
- **Compression**: Old logs are compressed (`.log.gz`)
- **Retention**: 90 days (configurable)

### Disabling Audit Logging

Set `AUDIT_ENABLED=false` or configure in `AuditConfig`:

```rust
let audit_config = AuditConfig {
    enabled: false,
    // ...
};
```

### Audit-log failure handling (fail-open)

If an audit-log *write* itself fails (disk full, fsync failure, etc.),
Nexus preserves the user-visible response status and exposes the
condition via the `nexus_audit_log_failures_total` Prometheus counter
plus a `tracing::error!(target = "audit_log")` event. The HTTP response
is **not** converted to `500` because doing so would hand an attacker
who can cause I/O pressure a lever to mass-reject legitimate traffic.

Alarm on the counter:

```promql
increase(nexus_audit_log_failures_total[5m]) > 0
```

Full rationale, call-site inventory, and regression-test pointers live in
`docs/security/SECURITY_AUDIT.md §5 "Audit-log failure policy (fail-open)"`.

## Security Best Practices

### 1. Change Default Root Password

⚠️ **CRITICAL**: Always change the default root password (`root/root`) in production:

```bash
export NEXUS_ROOT_PASSWORD="strong_random_password"
```

### 2. Use Strong Passwords

- Minimum 12 characters
- Mix of uppercase, lowercase, numbers, symbols
- Avoid dictionary words

### 3. Enable Authentication for Public Bindings

When binding to `0.0.0.0` (public), always enable authentication:

```bash
export NEXUS_AUTH_ENABLED="true"
export NEXUS_AUTH_REQUIRED_FOR_PUBLIC="true"
```

### 4. Use API Keys for Production

API keys are more secure than passwords for programmatic access:

- Long-lived credentials
- Can be revoked independently
- Can have expiration dates
- Tracked in audit logs

### 5. Rotate API Keys Regularly

- Rotate API keys every 90 days
- Revoke compromised keys immediately
- Use different keys for different environments

### 6. Use JWT Tokens for User Sessions

JWT tokens are ideal for user sessions:

- Short-lived (1 hour)
- Refreshable
- Stateless (no server-side storage)

### 7. Enable Audit Logging

Always enable audit logging in production:

```bash
export AUDIT_ENABLED="true"
export AUDIT_RETENTION_DAYS="90"
```

### 8. Monitor Rate Limits

Monitor rate limit headers to detect abuse:

```bash
# Check rate limit headers
curl -I https://api.example.com/cypher
```

### 9. Use HTTPS in Production

Always use HTTPS in production to protect credentials in transit.

### 10. Disable Root After Setup

Enable auto-disable root after setup:

```bash
export NEXUS_DISABLE_ROOT_AFTER_SETUP="true"
```

## API Examples

### Complete Authentication Flow

```bash
# 1. Login
TOKEN=$(curl -X POST https://api.example.com/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"alice","password":"secure_password"}' \
  | jq -r '.access_token')

# 2. Use token for API calls
curl -H "Authorization: Bearer $TOKEN" \
     https://api.example.com/cypher \
     -d '{"query": "MATCH (n) RETURN n LIMIT 10"}'

# 3. Create API key
API_KEY=$(curl -X POST https://api.example.com/auth/api-keys \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name":"production-key","permissions":["READ","WRITE"]}' \
  | jq -r '.key')

# 4. Use API key
curl -H "X-API-Key: $API_KEY" \
     https://api.example.com/cypher \
     -d '{"query": "MATCH (n) RETURN n LIMIT 10"}'
```

### Error Handling

```bash
# Invalid credentials
curl -X POST https://api.example.com/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"alice","password":"wrong"}'

# Response: 401 Unauthorized
{
  "error": {
    "code": "INVALID_CREDENTIALS",
    "message": "Invalid username or password"
  }
}

# Insufficient permissions
curl -X POST https://api.example.com/auth/users \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"username":"bob","password":"pass"}'

# Response: 403 Forbidden
{
  "error": {
    "code": "FORBIDDEN",
    "message": "Insufficient permissions: ADMIN required"
  }
}
```

## Configuration Reference

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `NEXUS_ROOT_USERNAME` | Root username | `root` |
| `NEXUS_ROOT_PASSWORD` | Root password | `root` |
| `NEXUS_ROOT_ENABLED` | Enable root user | `true` |
| `NEXUS_DISABLE_ROOT_AFTER_SETUP` | Auto-disable root after setup | `false` |
| `NEXUS_AUTH_ENABLED` | Enable authentication | `false` |
| `NEXUS_AUTH_REQUIRED_FOR_PUBLIC` | Require auth for public binding | `true` |
| `JWT_SECRET` | JWT signing secret (auto-generated if not set) | - |
| `JWT_EXPIRATION_SECONDS` | Access token expiration | `3600` |
| `JWT_REFRESH_EXPIRATION_SECONDS` | Refresh token expiration | `604800` |
| `AUDIT_ENABLED` | Enable audit logging | `true` |
| `AUDIT_RETENTION_DAYS` | Audit log retention | `90` |

### Config File (`config/auth.toml`)

```toml
[root_user]
username = "root"
password = "root"
enabled = true
disable_after_setup = false

[auth]
enabled = false
required_for_public = true
require_health_auth = false
default_permissions = ["READ", "WRITE"]

[rate_limits]
per_minute = 1000
per_hour = 10000

[audit]
enabled = true
retention_days = 90
compress_logs = true
```

## Troubleshooting

### Authentication Not Working

1. Check if authentication is enabled:
   ```bash
   echo $NEXUS_AUTH_ENABLED
   ```

2. Verify API key format:
   - Must start with `nx_`
   - Must be 35 characters total

3. Check audit logs:
   ```bash
   tail -f data/audit/audit-$(date +%Y-%m-%d).log
   ```

### Rate Limit Issues

1. Check rate limit headers:
   ```bash
   curl -I https://api.example.com/cypher
   ```

2. Adjust rate limits in config if needed

### Permission Denied

1. Verify user permissions:
   ```bash
   curl https://api.example.com/auth/users/alice \
     -H "Authorization: Bearer $TOKEN"
   ```

2. Check required permissions for endpoint

## Additional Resources

- [API Reference](api/openapi.yml)
- [Deployment Guide](DEPLOYMENT_GUIDE.md)
- [Security Best Practices](#security-best-practices)

