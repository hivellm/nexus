---
title: Authentication
module: api
id: authentication
order: 2
description: API keys, JWT, RBAC, and rate limiting
tags: [authentication, security, api-keys, jwt, rbac]
---

# Authentication

Complete guide to authentication and authorization in Nexus.

## Overview

Nexus provides comprehensive authentication and authorization:

- **Root User**: Initial superuser account
- **User Management**: Create, delete, and manage users
- **API Keys**: Long-lived credentials for programmatic access
- **JWT Tokens**: Short-lived tokens for user sessions
- **Permissions**: Fine-grained access control (READ, WRITE, ADMIN, SUPER)
- **Rate Limiting**: Protection against abuse

## Root User Setup

The root user is the initial superuser account created on system startup.

### Configuration

**Environment Variables:**
```bash
export NEXUS_ROOT_USERNAME="admin"
export NEXUS_ROOT_PASSWORD="secure_password_123"
export NEXUS_ROOT_ENABLED="true"
export NEXUS_DISABLE_ROOT_AFTER_SETUP="true"
```

**Default Values:**
- Username: `root`
- Password: `root` (⚠️ **CHANGE IN PRODUCTION**)
- Enabled: `true`

### Auto-Disable Root User

When `NEXUS_DISABLE_ROOT_AFTER_SETUP=true`, the root user is automatically disabled after the first admin user is created.

## API Keys

API keys are long-lived credentials for programmatic access.

### Creating API Keys

**REST API:**
```bash
POST /auth/api-keys
Content-Type: application/json
Authorization: Bearer <token>

{
  "name": "production-api-key",
  "permissions": ["READ", "WRITE"],
  "expires_at": "2025-12-31T23:59:59Z"
}
```

**Response:**
```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "name": "production-api-key",
  "key": "nx_abc123def456...",
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

## JWT Tokens

JWT tokens are short-lived credentials for user sessions.

### Login

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
  "token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
  "expires_at": "2025-01-01T01:00:00Z",
  "refresh_token": "refresh_token_here"
}
```

### Using JWT Tokens

```bash
curl -H "Authorization: Bearer <token>" \
     https://api.example.com/cypher
```

## Permissions

Nexus supports fine-grained permissions:

- **READ**: Execute read-only queries (MATCH, RETURN)
- **WRITE**: Execute write operations (CREATE, MERGE, SET, DELETE)
- **ADMIN**: Manage users, API keys, and databases
- **SUPER**: Full system access (root user only)

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

**Cypher:**
```cypher
GRANT READ, WRITE, ADMIN TO alice
```

## Rate Limiting

Rate limiting protects against abuse:

- **Default**: 1000 requests/minute per API key
- **Default**: 10000 requests/hour per API key

### Rate Limit Headers

```
X-RateLimit-Limit: 1000
X-RateLimit-Remaining: 999
X-RateLimit-Reset: 1704067200
```

## Security Best Practices

1. **Change Default Root Password** - Always change the default `root` password
2. **Disable Root After Setup** - Enable `NEXUS_DISABLE_ROOT_AFTER_SETUP`
3. **Use Strong Passwords** - Minimum 12 characters, mixed case, numbers, symbols
4. **Rotate API Keys** - Regularly rotate API keys
5. **Use HTTPS** - Always use HTTPS in production
6. **Monitor Audit Logs** - Regularly review authentication logs
7. **Limit Permissions** - Grant minimum required permissions

## Related Topics

- [API Reference](./API_REFERENCE.md) - Complete API documentation
- [Deployment Guide](../../DEPLOYMENT_GUIDE.md) - Production deployment
- [Security Audit](../../SECURITY_AUDIT.md) - Security best practices

