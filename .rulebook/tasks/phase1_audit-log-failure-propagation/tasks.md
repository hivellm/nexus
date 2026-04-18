## 1. Implementation
- [ ] 1.1 Decide the policy with project owner: fail-closed (reject request) vs fail-open with metric (record failure, still serve request) — document the choice
- [ ] 1.2 Replace `let _ = audit_logger.log(...)` at `auth/middleware.rs:339` with explicit handling per policy
- [ ] 1.3 Grep `let _ = .*audit` across `nexus-core/src/auth/` and apply the same treatment to every match
- [ ] 1.4 Add a Prometheus counter `audit_log_failures_total` exported at `/prometheus`
- [ ] 1.5 Run `cargo clippy -p nexus-core -- -D warnings` to catch newly-surfaced unused-Result warnings

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 2.1 Update `docs/SECURITY_AUDIT.md` and `docs/AUTHENTICATION.md` with the chosen policy
- [ ] 2.2 Add a test that injects a failing `AuditLogger` and asserts the chosen behaviour (reject or metric increment)
- [ ] 2.3 Run tests and confirm they pass
