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
- [ ] 1.1 Write a failing integration test: start the server with auth enabled, create a Read-only key,
  `POST /auth/keys` with that key's `X-API-Key` header and body
  `{"name":"pwn","permissions":["SUPER"]}`. Assert today it returns `200` with a Super-permission key
  (confirms the hole) — this must flip to `403` once fixed
- [ ] 1.2 Add the `grant_permissions` variant: Read-only key calls the grant-permissions endpoint
  (`api/auth.rs:287`) targeting its own key or another key with `Admin`/`Super` (`:299-303,:339`);
  assert it currently succeeds
- [ ] 1.3 Add the `create_user`/`delete_user`/`list_users`/`revoke_permission` variants
  (`api/auth.rs:59,216,145,392`): each called by a non-Admin key; assert each currently succeeds
  (`list_users`/`revoke_permission` return data or apply the change instead of `403`)

## 2. Design the authorization check
- [ ] 2.1 Confirm `AuthMiddleware::has_permission` (`nexus-core/src/auth/middleware.rs:438` area) can be
  invoked with the authenticated `auth_context`'s permissions and a required `Permission::Admin`/`Super`
  threshold; document its exact signature in the PR description if it needs extending
- [ ] 2.2 Decide the enforcement point: a shared middleware/extractor applied to the whole `/auth/*`
  route group vs. a per-handler check at the top of each function. Prefer the shared layer so no future
  `/auth/*` handler can be added without inheriting the check; record the decision
- [ ] 2.3 Define the "no lateral escalation" rule precisely: the caller's own permission set must be a
  superset of any permission set supplied in the request body (for `create_api_key` and
  `grant_permissions`), not merely "caller has Admin"

## 3. Implement the fix
- [ ] 3.1 Wire the chosen authorization check onto `create_api_key` (`api/auth.rs:868`) — reject with
  `403` when the caller lacks `Admin`/`Super`, and additionally reject when the requested `permissions`
  (`:884-885`) exceed the caller's own
- [ ] 3.2 Wire the same check onto `grant_permissions` (`api/auth.rs:287`) — reject when the caller lacks
  `Admin`/`Super`, and when the granted permission (`:299-303,:339`) exceeds the caller's own; keep the
  existing root-account guard (`:326`) as an additional, independent check
- [ ] 3.3 Wire the same check onto `create_user` (`:59`), `delete_user` (`:216`), `list_users` (`:145`),
  and `revoke_permission` (`:392`)
- [ ] 3.4 Make every §1 test pass (`403` for the non-Admin caller in each case), then add a positive
  test: an Admin/Super key performing the same operations still succeeds

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 4.1 Update `docs/security/AUTHENTICATION.md` with the authorization contract for `/auth/*`
  management routes (required permission per route, no-lateral-escalation rule); add a CHANGELOG entry
- [ ] 4.2 Tests: each `/auth/*` management handler rejects a caller without `Admin`/`Super`; each rejects
  a body-supplied permission set exceeding the caller's own; each succeeds for a properly privileged
  caller
- [ ] 4.3 Run `cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test --workspace` — all green

## Related
- `phase0_fix-server-secure-defaults-and-dos` — H1 (auth disabled by default) makes this surface
  reachable without any credential at all
