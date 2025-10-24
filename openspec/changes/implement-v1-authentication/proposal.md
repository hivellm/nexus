# Implement V1 Authentication & Security

## Why

Production deployment requires authentication, authorization, and rate limiting. Following Vectorizer's approach: disabled by default (localhost), required for public binding (0.0.0.0).

## What Changes

- Implement API key authentication with Argon2 hashing
- Implement RBAC (READ, WRITE, ADMIN, SUPER permissions)
- Implement rate limiting (1000/min, 10000/hour per key)
- Implement JWT token support
- Add audit logging for write operations
- Force authentication when binding to 0.0.0.0

**BREAKING**: None (opt-in feature)

## Impact

### Affected Specs
- NEW capability: `authentication`
- NEW capability: `authorization`
- NEW capability: `rate-limiting`

### Affected Code
- `nexus-core/src/auth/mod.rs` - Auth module (~600 lines)
- `nexus-server/src/middleware/auth.rs` - Auth middleware (~200 lines)
- `tests/auth_tests.rs` - Security tests (~300 lines)

### Dependencies
- Requires: MVP complete

### Timeline
- **Duration**: 1 week
- **Complexity**: Medium

