# Implementation Tasks - V1 Authentication

## 1. API Key Management

- [ ] 1.1 Implement ApiKey struct (id, name, key_hash, permissions, expiry)
- [ ] 1.2 Implement Argon2 password hashing
- [ ] 1.3 Implement API key generation (32-char random)
- [ ] 1.4 Store API keys in catalog (LMDB)
- [ ] 1.5 Implement POST /auth/keys endpoint
- [ ] 1.6 Implement GET /auth/keys (list keys)
- [ ] 1.7 Implement DELETE /auth/keys/{id}
- [ ] 1.8 Add unit tests (95%+ coverage)

## 2. Authentication Middleware

- [ ] 2.1 Implement Bearer token extraction
- [ ] 2.2 Implement API key validation
- [ ] 2.3 Add authentication middleware to Axum router
- [ ] 2.4 Check binding address (require auth for 0.0.0.0)
- [ ] 2.5 Add 401 Unauthorized responses
- [ ] 2.6 Add unit tests

## 3. RBAC (Role-Based Access Control)

- [ ] 3.1 Define Permission enum (READ, WRITE, ADMIN, SUPER)
- [ ] 3.2 Implement permission checking per endpoint
- [ ] 3.3 Add 403 Forbidden responses
- [ ] 3.4 Add unit tests

## 4. Rate Limiting

- [ ] 4.1 Implement rate limiter (token bucket algorithm)
- [ ] 4.2 Track requests per minute/hour per API key
- [ ] 4.3 Add 429 Too Many Requests responses
- [ ] 4.4 Add X-RateLimit-* headers
- [ ] 4.5 Add unit tests

## 5. JWT Support

- [ ] 5.1 Implement JWT token generation
- [ ] 5.2 Implement JWT validation
- [ ] 5.3 Add POST /auth/login endpoint
- [ ] 5.4 Add token expiration (configurable)
- [ ] 5.5 Add unit tests

## 6. Audit Logging

- [ ] 6.1 Log all write operations (CREATE, SET, DELETE)
- [ ] 6.2 Include user_id, timestamp, operation type
- [ ] 6.3 Persist audit log to file
- [ ] 6.4 Add audit log rotation
- [ ] 6.5 Add unit tests

## 7. Documentation & Quality

- [ ] 7.1 Update docs/ROADMAP.md
- [ ] 7.2 Add auth examples to README
- [ ] 7.3 Update CHANGELOG.md with v0.5.0
- [ ] 7.4 Run all quality checks

