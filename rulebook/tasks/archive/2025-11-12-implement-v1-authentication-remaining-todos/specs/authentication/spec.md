## ADDED Requirements

### Requirement: Comprehensive Test Coverage
The system SHALL achieve 95%+ code coverage for all authentication modules with unit tests covering edge cases, boundary conditions, and error scenarios.

#### Scenario: Audit logging edge cases
- **WHEN** testing audit logging with disabled logger
- **THEN** operations complete without errors
- **AND** no log files are created

#### Scenario: Rate limiter boundary conditions
- **WHEN** rate limiter reaches maximum requests
- **THEN** next request returns 429 Too Many Requests
- **AND** remaining count is 0

#### Scenario: JWT token expiration
- **WHEN** JWT token expires
- **THEN** validation fails with appropriate error
- **AND** refresh token can be used to obtain new access token

#### Scenario: API key expiration cleanup
- **WHEN** cleanup job runs
- **THEN** expired API keys are removed from storage
- **AND** active keys remain untouched

#### Scenario: Log rotation edge cases
- **WHEN** log rotation occurs at midnight
- **THEN** new log file is created
- **AND** old log file is compressed if compression enabled

### Requirement: Integration Test Coverage
The system SHALL provide integration tests (S2S) for complete authentication flows including login, API key usage, permission enforcement, and audit logging.

#### Scenario: Complete authentication flow
- **WHEN** user logs in with valid credentials
- **THEN** JWT token is returned
- **AND** token can be used for authenticated API calls
- **AND** audit log records authentication success

#### Scenario: API key lifecycle
- **WHEN** API key is created, used, revoked, and deleted
- **THEN** each operation succeeds
- **AND** audit log records all operations
- **AND** revoked key cannot be used for authentication

#### Scenario: Permission enforcement
- **WHEN** user with READ permission attempts write operation
- **THEN** 403 Forbidden is returned
- **AND** audit log records permission denial

#### Scenario: Rate limiting across requests
- **WHEN** multiple requests exceed rate limit
- **THEN** 429 Too Many Requests is returned
- **AND** rate limit headers are included in response

### Requirement: Security Testing
The system SHALL include security tests for common attack vectors including SQL injection, XSS, CSRF, brute force, timing attacks, and privilege escalation.

#### Scenario: SQL injection prevention
- **WHEN** malicious Cypher query contains SQL injection attempt
- **THEN** query is sanitized or rejected
- **AND** no unauthorized data access occurs

#### Scenario: Brute force prevention
- **WHEN** multiple failed login attempts occur
- **THEN** rate limiting prevents further attempts
- **AND** account lockout may be triggered

#### Scenario: Timing attack prevention
- **WHEN** password or API key validation occurs
- **THEN** validation uses constant-time comparison
- **AND** timing differences do not reveal valid credentials

#### Scenario: Privilege escalation prevention
- **WHEN** user attempts to grant themselves SUPER permission
- **THEN** operation is denied
- **AND** audit log records attempted escalation

### Requirement: Performance Testing
The system SHALL include performance tests verifying authentication overhead, rate limiting under load, and concurrent request handling.

#### Scenario: Authentication middleware overhead
- **WHEN** authentication middleware processes request
- **THEN** overhead is less than 1ms per request
- **AND** does not significantly impact API response times

#### Scenario: Rate limiting under load
- **WHEN** 1000+ requests per second are made
- **THEN** rate limiting correctly enforces limits
- **AND** system remains stable

#### Scenario: Concurrent authentication
- **WHEN** multiple requests authenticate simultaneously
- **THEN** all requests are handled correctly
- **AND** no race conditions occur

### Requirement: Docker Integration Testing
The system SHALL include Docker integration tests for root user configuration via environment variables and Docker secrets.

#### Scenario: Root user via environment variables
- **WHEN** Docker container starts with NEXUS_ROOT_USERNAME and NEXUS_ROOT_PASSWORD
- **THEN** root user is created with specified credentials
- **AND** root user can authenticate successfully

#### Scenario: Root user via Docker secrets
- **WHEN** Docker container uses NEXUS_ROOT_PASSWORD_FILE
- **THEN** password is read from secrets file
- **AND** root user is created with password from file

#### Scenario: Root user auto-disable
- **WHEN** first admin user is created
- **THEN** root user is automatically disabled if NEXUS_DISABLE_ROOT_AFTER_SETUP is enabled
- **AND** root user cannot authenticate after disable

### Requirement: AuthContext Extraction
The system SHALL extract actor information (user_id, username) from AuthContext in request extensions for audit logging.

#### Scenario: Extract user from AuthContext
- **WHEN** authenticated request includes AuthContext in extensions
- **THEN** audit log includes user_id and username
- **AND** actor information is accurate

#### Scenario: Extract API key from AuthContext
- **WHEN** API key authenticated request includes AuthContext
- **THEN** audit log includes API key ID
- **AND** user_id is None if key is not user-bound

### Requirement: Complete Documentation
The system SHALL provide comprehensive documentation including authentication guide, API documentation updates, deployment guide, and SDK examples.

#### Scenario: Authentication guide completeness
- **WHEN** user reads docs/AUTHENTICATION.md
- **THEN** guide includes root user setup, user management, API keys, JWT, permissions, rate limiting, and audit logging
- **AND** examples are provided for each feature

#### Scenario: API documentation updates
- **WHEN** user reads API documentation
- **THEN** all protected endpoints include authentication requirements
- **AND** error responses (401, 403, 429) are documented
- **AND** rate limit headers are documented

#### Scenario: Docker deployment guide
- **WHEN** user deploys via Docker
- **THEN** guide explains root user configuration
- **AND** security best practices are provided
- **AND** environment variables are documented

### Requirement: Security Audit
The system SHALL undergo security audit including code review, attack vector testing, cryptographic verification, and rate limiting verification.

#### Scenario: Code review for vulnerabilities
- **WHEN** security audit is performed
- **THEN** all authentication code is reviewed
- **AND** vulnerabilities are identified and fixed
- **AND** security best practices are followed

#### Scenario: Cryptographic verification
- **WHEN** Argon2 configuration is verified
- **THEN** memory cost is >= 65536 KB
- **AND** time cost is >= 3
- **AND** parallelism is >= 4

#### Scenario: API key generation verification
- **WHEN** API keys are generated
- **THEN** keys use cryptographically secure random number generator
- **AND** keys are sufficiently random (pass statistical tests)

#### Scenario: Rate limiting effectiveness
- **WHEN** rate limiting is tested
- **THEN** limits are enforced correctly
- **AND** DoS attacks are prevented
- **AND** legitimate users are not blocked

### Requirement: Quality Checks
The system SHALL pass all quality checks including complete test suite, code coverage verification, clippy checks, and release preparation.

#### Scenario: Complete test suite
- **WHEN** test suite is executed
- **THEN** all tests pass (100% pass rate)
- **AND** no flaky tests exist

#### Scenario: Code coverage verification
- **WHEN** coverage report is generated
- **THEN** authentication module achieves 95%+ coverage
- **AND** uncovered code paths are documented or tested

#### Scenario: Clippy checks
- **WHEN** clippy is run with -D warnings
- **THEN** no warnings are generated
- **AND** code follows Rust best practices

#### Scenario: Release preparation
- **WHEN** release is prepared
- **THEN** CHANGELOG.md is updated with complete feature list
- **AND** version is bumped to 0.11.0
- **AND** release notes are created

## MODIFIED Requirements

### Requirement: Audit Logging
The system SHALL log all security-sensitive operations including user management, permission changes, API key operations, authentication events, and write operations. Audit logs SHALL include actor information (user_id, username, API key ID) extracted from AuthContext when available.

#### Scenario: Log with actor information
- **WHEN** authenticated user performs operation
- **THEN** audit log includes user_id and username from AuthContext
- **AND** actor information is accurate

#### Scenario: Log with API key information
- **WHEN** API key authenticated request performs operation
- **THEN** audit log includes API key ID
- **AND** user_id is included if key is user-bound

