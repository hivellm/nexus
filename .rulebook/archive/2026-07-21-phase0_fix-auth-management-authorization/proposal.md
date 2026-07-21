# Proposal: phase0_fix-auth-management-authorization

**Priority: CRITICAL — any authenticated API key, including a Read-only key, can mint a Super-permission
key or otherwise manage users/permissions with no authorization check.** Found in a server/auth security
audit; not previously reported.

## Why

None of the `/auth/*` management handlers in `crates/nexus-server/src/api/auth.rs` check the calling
key's own permissions before acting — they only authenticate the caller and then honor whatever the
request body asks for:

- `create_api_key` (`api/auth.rs:868`) builds the new key's permission set straight from the request
  body, including `ADMIN`/`SUPER` (`:884-885`). The `auth_context` extracted at `:870` is used only to
  attribute the audit log entry — it is never compared against the requested permissions.
- `grant_permissions` (`api/auth.rs:287`) grants any permission named in the body, including
  `Admin`/`Super` (`:299-303,:339`); the only guard is a check that blocks touching the root account
  (`:326`) — there is no check that the CALLER holds (or exceeds) the permission being granted.
- `create_user` (`:59`), `delete_user` (`:216`), `list_users` (`:145`), `revoke_permission` (`:392`)
  follow the identical pattern: `auth_context` is threaded through for audit logging only.

The auth middleware itself (`crates/nexus-core/src/auth/middleware.rs:438`) only authenticates — it
verifies the key exists and is valid — and no route on the auth management surface calls
`AuthMiddleware::has_permission` or inspects the authenticated key's `permissions` before performing the
requested management action. Authorization is simply absent on this surface.

With auth ENABLED, a Read-only key escalates itself to Super in one request:

```
POST /auth/keys
X-API-Key: nx_<readonly-key>
{"name": "pwn", "permissions": ["SUPER"]}
```

→ `200 OK` with a freshly minted Super-permission API key. From there the attacker holds full
administrative control of the server (user management, all data, all future key issuance). With auth
DISABLED (the default — see `phase0_fix-server-secure-defaults-and-dos` H1), the same requests are
simply unauthenticated and reachable by anyone who can open a TCP connection.

## What Changes

- Add an authorization check to every `/auth/*` management handler (`create_api_key`,
  `grant_permissions`, `create_user`, `delete_user`, `list_users`, `revoke_permission`, and any sibling
  handler in `api/auth.rs` with the same shape) requiring the CALLING key to hold `Permission::Admin` or
  `Permission::Super`, using `AuthMiddleware::has_permission` (or an equivalent authorization layer
  applied uniformly to the route group).
- Reject body-supplied permission sets that exceed the caller's own permissions — a key must never be
  able to grant or mint a permission it does not itself hold (no vertical escalation via a lateral
  request).
- Prefer a shared middleware/extractor over one-off checks in each handler, so the invariant holds for
  any future `/auth/*` route without repeating the check by hand.

## Impact

- Affected specs: `docs/security/AUTHENTICATION.md` (authorization contract for `/auth/*` management
  routes)
- Affected code: `nexus-server/src/api/auth.rs` (`create_api_key`, `grant_permissions`, `create_user`,
  `delete_user`, `list_users`, `revoke_permission`), `nexus-core/src/auth/middleware.rs`
  (`AuthMiddleware::has_permission` wiring)
- Breaking change: YES for any deployment currently relying on non-Admin keys to self-manage
  permissions (not a supported/intended use); NO for legitimate Admin/Super-key workflows
- User benefit: a Read-only or low-privilege API key can no longer escalate itself or any other key to
  Admin/Super, and user/permission management is restricted to callers who are already authorized to
  perform it
- Related: `phase0_fix-server-secure-defaults-and-dos` (H1 — auth disabled by default makes this surface
  reachable unauthenticated), `phase0_fix-password-hashing` (adjacent auth-surface hardening)
