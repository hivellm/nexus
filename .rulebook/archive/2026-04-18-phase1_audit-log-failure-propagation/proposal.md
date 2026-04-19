# Proposal: phase1_audit-log-failure-propagation

## Why

`auth/middleware.rs:339` does `let _ = audit_logger.log(...)`, swallowing the
result of an audit-log write on an authenticated request path. Silently
discarding the failure of a security audit log is the exact pattern
compliance frameworks reject — auditable events must either be written or
the request must be rejected, never "we tried and gave up quietly."

## What Changes

- Change the `audit_logger.log(...)` call site to propagate errors.
- Depending on policy, either:
  - return `Err(Error::Internal("audit log failed"))` so the middleware
    rejects the request (fail-closed), or
  - write to a stderr fallback + emit a `tracing::error!` + increment a
    metric `audit_log_failures_total` so ops can alarm on it.
- Audit other `let _ = ` patterns in `nexus-core/src/auth/` and apply the
  same reasoning.

## Impact

- Affected specs: `docs/AUTHENTICATION.md`, `docs/SECURITY_AUDIT.md`
- Affected code:
  - `nexus-core/src/auth/middleware.rs:339`
  - any companion audit-log sites (search: `audit_logger.log`, `let _ = .*audit`)
- Breaking change: potentially YES (requests may now fail when audit fails)
  — but that is the correct compliance posture
- User benefit: audit log reliability matches what the docs claim
