# Tasks: phase0_fix-auth-management-authorization

None of the `/auth/*` management handlers in `crates/nexus-server/src/api/auth.rs` check the calling
key's permissions before acting on the request body. With auth enabled, a Read-only key escalates
itself to Super:

```
POST /auth/keys
X-API-Key: nx_<readonly-key>
Body: {"name": "pwn", "permissions": ["SUPER"]}
```

→ `200` with a freshly minted Super key. The same missing-check pattern applies to `grant_permissions`
(`api/auth.rs:287`), `create_user` (`:59`), `delete_user` (`:216`), `list_users` (`:145`), and
`revoke_permission` (`:392`).

Order rationale: prove the escalation with a failing test before touching any handler (§1), because a
partial fix that closes `create_api_key` but leaves `grant_permissions` open is still a full
compromise — the shared authorization design (§2) must be settled before implementation (§3), so every
handler is wired to the same check rather than six slightly different ones.

## 1. Reproduce the escalation first
- [x] 1.1 Failing test written (`readonly_key_cannot_mint_super_key` in
  `tests/auth_management_authorization_test.rs`): exercises `create_api_key` directly with a Read-only
  `AuthContext` and body `{permissions:["SUPER"]}`. Confirmed RED (returned `Ok` with a minted Super
  key); now `403`.
- [x] 1.2 `grant_permissions` variant (`readonly_key_cannot_grant_permissions`): Read-only caller
  granting `ADMIN`. Confirmed RED (succeeded); now `403`.
- [x] 1.3 `create_user`/`delete_user`/`revoke_permission` variants plus the read handlers
  (`list_users`/`get_user`/`get_user_permissions`/`list_api_keys`/`get_api_key`): each with a non-Admin
  caller. All confirmed RED (succeeded / returned data); now `403`.

## 2. Design the authorization check
- [x] 2.1 Design confirmed. Rather than `AuthMiddleware::has_permission` (which needs the AuthMiddleware
  + api_key), the caller's permissions are read directly from `auth_context.api_key.permissions` and
  checked with `PermissionSet::has_permission` (nexus-core `permissions.rs`), which already encodes the
  hierarchy (Super⊇Admin⊇Write⊇Read, etc.). No signature extension needed.
- [x] 2.2 Decision: **per-handler shared helpers** (`require_admin`, `require_permission_superset` in
  `api/auth.rs`), NOT a blanket `/auth/*` route-group layer. Rationale: (a) a layer cannot see the parsed
  body, so the no-escalation superset check must be per-handler anyway; (b) a blanket layer would wrongly
  gate the `login`/`refresh_token` authentication flows (a user logging in holds no key). The helpers are
  called at the top of each management handler.
- [x] 2.3 No-lateral-escalation rule defined and implemented: `require_permission_superset` requires the
  caller's set to `has_permission` every requested permission (superset), so an Admin key cannot
  mint/grant Super — not merely "caller has Admin".

## 3. Implement the fix
- [x] 3.1 `create_api_key`: `require_admin` + `require_permission_superset(&permissions)` (over the
  resolved set, including the `[Read,Write]` default).
- [x] 3.2 `grant_permissions`: `require_admin` + `require_permission_superset`; the existing root-account
  guard is kept as an independent check.
- [x] 3.3 `require_admin` wired onto ALL management handlers — the task's `create_user`, `delete_user`,
  `list_users`, `revoke_permission`, PLUS the same-shape siblings `get_user`, `get_user_permissions`,
  `list_api_keys`, `get_api_key`, `delete_api_key`, `revoke_api_key` (12 handlers total; only
  `login`/`refresh_token` are exempt). Read handlers that lacked `auth_context` (list_users, get_user,
  get_user_permissions, list_api_keys, get_api_key) had the extractor added and their `main.rs` router
  closures updated to pass it.
- [x] 3.4 Every §1 test passes (`403` for non-Admin), plus positives (`super_key_can_mint_super_key`,
  `admin_key_can_create_user`, `admin_key_can_list_users`) and an auth-disabled bootstrap case
  (`auth_disabled_still_allows_management`) — 15/15 green.

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [x] 4.1 Update or create documentation covering the implementation — DONE: `docs/security/AUTHENTICATION.md`
  now documents the per-route required permission, the no-vertical-escalation rule, the login/refresh
  exemption, and the auth-disabled no-op; CHANGELOG entry added under `[3.0.0]`.
- [x] 4.2 Write tests covering the new behavior — DONE: each management handler rejects a non-Admin caller
  (403); `create_api_key`/`grant_permissions` reject a body permission set exceeding the caller's own; a
  properly privileged caller still succeeds (15 tests).
- [x] 4.3 Run tests and confirm they pass — DONE (green): `cargo +nightly fmt --all`,
  `cargo clippy -p nexus-server --all-targets --all-features -- -D warnings` (0 warnings), full
  `cargo +nightly test -p nexus-server` and `cargo +nightly test --workspace` — 0 failed.

## Related
- `phase0_fix-server-secure-defaults-and-dos` — H1 (auth disabled by default) makes this surface
  reachable without any credential at all
