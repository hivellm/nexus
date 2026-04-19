# Cluster Mode — Technical Specification

> Wire-level and engine-level spec for Nexus's multi-tenant
> deployment shape. Operator-facing material lives in
> [`docs/CLUSTER_MODE.md`](../CLUSTER_MODE.md); this file is the
> contract every component in the pipeline must honour.

---

## 1. Goals and non-goals

### Goals

- **Hard tenant data isolation** on a single Nexus instance. A
  tenant's nodes, relationships, and property keys must be
  structurally invisible to every other tenant — not a soft
  filter enforced by discipline.
- **Per-tenant request rate limits** with transparent headers so
  SDK clients back off gracefully.
- **Per-tenant storage quotas** with a write-path gate that
  rejects over-budget tenants before their write touches storage.
- **Fine-grained MCP / RPC function permissions** per API key,
  orthogonal to the existing coarse `Permission` enum.
- **Zero overhead for standalone deployments.** Cluster-mode
  code paths must short-circuit on a single boolean before any
  extra work.

### Non-goals

- Network-level tenant isolation (VPC per tenant, dedicated
  listeners). Out of scope — Nexus is expected to run behind
  the operator's network controls.
- Per-tenant storage-engine files. The catalog-prefix approach
  shares the record stores across tenants and achieves isolation
  via catalog ID separation; see ADR-7.
- Cross-tenant admin queries over Cypher. Use the quota provider
  / usage snapshot APIs, which are tenant-scoped by design.

---

## 2. Configuration model

```rust
pub struct ClusterConfig {
    pub enabled: bool,
    pub isolation: TenantIsolationMode,
    pub default_quotas: TenantDefaults,
}

pub enum TenantIsolationMode { None, CatalogPrefix }

pub struct TenantDefaults {
    pub storage_mb: u64,
    pub requests_per_minute: u32,
    pub requests_per_hour: u32,
}
```

- `enabled = false` ⇒ standalone semantics in every layer.
  Every cluster-mode code path must either short-circuit on this
  flag or be gated on a resource (`UserContext`, `QuotaProvider`)
  that is absent when `enabled = false`.
- `enabled = true` and `isolation = None` is a valid config
  shape: auth + rate limiting, no catalog-level tenant scoping.
  Use when isolation is already enforced by an outer layer
  (e.g. one Nexus per tenant behind a shared proxy).
- `isolation = CatalogPrefix` flips on the AST-rewrite pass in
  `Engine::execute_cypher_with_context` (see §5).
- `default_quotas` are consumed by the local `QuotaProvider`
  implementation only. Future external providers (HiveHub)
  ignore it and consult the control plane.

Env var: `NEXUS_CLUSTER_ENABLED=1|true|TRUE|yes` flips
`enabled` at startup. YAML config is a follow-up.

---

## 3. Identity model

### 3.1 `UserNamespace`

Validated tenant identifier. Invariants enforced at construction
time by `UserNamespace::new`:

- Non-empty.
- ≤128 bytes (conservative ceiling that keeps the full
  `ns:<id>:` prefix inside LMDB's key-size envelope).
- No reserved delimiter `:`. The delimiter is the catalog-
  prefix separator; accepting it in the id would let a
  malicious id collide with another tenant's prefix.
- No control characters.

### 3.2 `UserContext`

Per-request identity derived from an authenticated API key.
Carries the namespace, the api-key id (for audit, never for
authorisation), and an optional function allow-list
(`Option<Arc<BTreeSet<String>>>`). `Arc` so clone is O(1).

Construction is the auth middleware's responsibility:

```rust
let ctx = match &api_key.user_id {
    Some(uid) => UserContext::new_from_api_key(...)?,
    None      => return 401 INVALID_TOKEN, // cluster-mode only
};
req.extensions_mut().insert(ctx);
```

Downstream handlers read the context with
`nexus_core::auth::extract_user_context(&Request) -> Option<&UserContext>`.

### 3.3 API keys in cluster mode

`ApiKey` carries three cluster-relevant fields:

- `user_id: Option<String>` — the raw namespace id. Cluster
  mode requires `Some(...)`; standalone tolerates `None`.
- `allowed_functions: Option<Vec<String>>` — function allow-list.
  `None` means unrestricted; `Some(vec![])` means "may call
  nothing" (distinct, intentional state).
- `permissions: Vec<Permission>` — coarse R/W/Admin capability
  classes, unchanged from pre-cluster-mode.

Storage: API keys serialise via `serde_json` (switched from
bincode to gain forward-compat for appended fields). Existing
bincode records don't carry over — deployments upgrading to
cluster mode must re-seed. The test-suite catalog key was
bumped to `nexus_test_auth_shared_v2` for the same reason.

---

## 4. Authentication contract

Cluster mode makes authentication mandatory on every URI:

| URI | Standalone | Cluster |
|---|---|---|
| `/` | public | 401 |
| `/health` | public | 401 |
| `/stats` | public | 401 |
| `/openapi.json` | public | 401 |
| `/cypher` | depends on `auth.enabled` | 401 |
| `/mcp` | depends on `auth.enabled` | 401 |
| `/cluster/stats/self` | 404 CLUSTER_MODE_DISABLED | 401 / 200 |

`AuthMiddleware::requires_auth(uri)` short-circuits to `true`
when `is_cluster_mode() == true`. The uri allow-list is never
consulted.

On successful auth the middleware installs both an
`AuthContext` (classic, carries `ApiKey`) AND a `UserContext`
(cluster-mode, carries `UserNamespace`). The two live in
separate request-extension slots; the `UserContext` slot is
absent in standalone mode. Handlers that only need the namespace
can depend on `UserContext` without pulling in `auth` types.

Function-level permission check (per-handler):

```rust
ctx.require_may_call("cypher.execute")
    .map_err(|e| (StatusCode::FORBIDDEN, Json(e)))?;
```

Error body shape (stable):

```json
{
  "function": "nexus.admin.drop_database",
  "code": "FUNCTION_NOT_ALLOWED",
  "message": "function '...' is not in this API key's allow-list"
}
```

---

## 5. Data-isolation mechanism

Under `TenantIsolationMode::CatalogPrefix`:

### 5.1 AST rewrite

`cluster::scope::scope_query(&mut ast, ns, mode)` walks a parsed
`CypherQuery` and rewrites every catalog-facing name to its
namespaced form (`Person` → `ns:<tenant>:Person`). Positions
rewritten:

- `NodePattern.labels` in MATCH / CREATE / MERGE.
- `RelationshipPattern.types` in same.
- `SetItem::Label` in SET.
- `RemoveItem::Label` in REMOVE.
- Property keys: `NodePattern.properties`,
  `RelationshipPattern.properties`, `SetItem::Property.property`,
  `RemoveItem::Property.property`, `PropertyAccess.property`,
  `Expression::Map` keys, `Exists.pattern` sub-walks, CASE /
  BinaryOp / UnaryOp / ArrayIndex / ArraySlice / FunctionCall
  argument recursions.

Idempotent: running the walker twice on the same AST is a
no-op because already-namespaced names (starting with `ns:`)
pass through unchanged. Nested `execute_cypher` dispatches
cannot double-prefix.

### 5.2 Executor handoff

The executor's `execute(Query)` re-parses `Query.cypher` by
default. To prevent the AST rewrite from being silently
discarded, `Engine::execute_cypher_with_context` installs the
scoped AST via a one-shot `preparsed_ast_override` slot on
`ExecutorShared` (`Arc<Mutex<Option<CypherQuery>>>`). The
executor consumes it via `.take()` before the parse step; the
caller wraps the install in an RAII guard that clears the slot
on every return path, so a leftover override cannot leak into
the next unrelated call. See the "One-shot preparsed_ast
override" knowledge pattern for the general shape.

### 5.3 Downstream transparency

Record stores, page cache, WAL, label bitmap, KNN index, and
every Cypher operator are tenant-oblivious. They all continue
to deal in `LabelId` / `TypeId` / `KeyId` — and those IDs are
tenant-distinct automatically because the catalog lookups used
the scoped NAMES. Isolation propagates for free.

### 5.4 DELETE — special path

`MATCH ... DELETE` historically reconstructed a Cypher string
by hand and re-parsed it. Under cluster mode that round-trip
would split `ns:alice:Person` on its own `:` delimiter into
three separate labels. The fix:
`execute_match_delete_query` now hands the scoped AST directly
to the executor via `preparsed_ast_override` instead of the
reconstruct-and-reparse dance.

### 5.5 Known limitations

- Label-less `MATCH (n)` cannot be scoped by catalog prefix.
  Cluster-mode deployments must use labelled queries.
- Property-chain bytes are not included in
  `storage_bytes_for_namespace`; it's a lower bound.
- Relationship counts are not incremented in the catalog by
  `executor::operators::create` today, so the rel half of the
  byte calculation is zero on the write path. Fixing
  `create.rs` to call `increment_rel_count` is a separate
  follow-up; the helper itself reads both columns.

---

## 6. Quota model

### 6.1 `QuotaProvider` trait

```rust
pub trait QuotaProvider: Send + Sync {
    fn check_rate(&self, ns: &UserNamespace) -> QuotaDecision;
    fn check_storage(&self, ns: &UserNamespace, bytes: u64) -> QuotaDecision;
    fn record_usage(&self, ns: &UserNamespace, delta: UsageDelta);
    fn snapshot(&self, ns: &UserNamespace) -> Option<QuotaSnapshot>;
}
```

Synchronous by design — the hot path consults the provider on
every request and must not block on network I/O. External
providers (future HiveHub-backed impl) hide async/batched
control-plane traffic behind this sync façade via a background
sync task plus an in-memory cache.

### 6.2 `LocalQuotaProvider`

In-process implementation built on `parking_lot::RwLock<HashMap>`:

- Per-tenant `RateWindow` pair (per-minute, per-hour). Fixed-
  duration window reset on expiry; accurate enough for long-run
  abuse caps without the complexity of a ring buffer.
- Per-tenant `storage_bytes_used` accumulator. `record_usage`
  bumps it; `check_storage` rejects if
  `used + incoming > limit`.

### 6.3 HTTP translation

The quota middleware (`cluster::middleware::quota_middleware_handler`)
reads the `UserContext` out of request extensions and calls
`check_rate`. Outcomes:

| Decision | HTTP | Body |
|---|---|---|
| `Allow { remaining }` | 200 + next.run(request) | handler's own body |
| `Deny { reason, retry_after }` | 429 | `{"code":"RATE_LIMIT_EXCEEDED","message":"<reason>","retry_after_seconds":N}` |

Allow responses get `X-RateLimit-Remaining: <n>`. Deny
responses get `Retry-After: <seconds>`.

### 6.4 Engine translation

`Engine::execute_cypher_with_context` gates writes with
`provider.check_storage(ns, 0)` before dispatch. A denial
converts to `Error::QuotaExceeded(reason)`. After a successful
write the same function charges
`provider.record_usage(ns, UsageDelta { storage_bytes: 256,
requests: 1 })` — a flat-rate heuristic for the first cut.

Reads are never quota-gated. `is_write_query(&ast)` short-
circuits the gate on non-write clauses.

---

## 7. Wire contracts

### 7.1 `GET /cluster/stats/self`

Body on success (200):

```json
{
  "tenant_id": "alice",
  "storage_bytes_used": 1280,
  "storage_bytes_limit": 1073741824,
  "requests_this_minute": 12,
  "requests_per_minute_limit": 6000,
  "requests_this_hour": 43,
  "requests_per_hour_limit": 300000
}
```

Error codes (both on 404):

- `CLUSTER_MODE_DISABLED` — server is in standalone mode, no
  provider installed. Client should stop polling.
- `TENANT_UNKNOWN` — cluster mode is on, request carries a
  valid UserContext, but the tenant has never touched the
  server yet, so the provider has no snapshot. Expected on
  first boot per tenant.
- `NO_TENANT_CONTEXT` (401) — request lacks a UserContext.
  Should not happen in cluster mode because the auth middleware
  would have rejected earlier; included as a defence-in-depth.

### 7.2 Error body contract

All cluster-mode error responses use a flat `{code, message,
...}` shape with a stable uppercase-snake `code` that SDK
decoders match on. Message text is informational and may
change between releases; `code` MUST NOT without a semver
bump of the wire.

Stable codes:

- `AUTHENTICATION_REQUIRED` — 401
- `INVALID_TOKEN` — 401
- `INSUFFICIENT_PERMISSIONS` — 403 (coarse Permission)
- `FUNCTION_NOT_ALLOWED` — 403 (fine `allowed_functions`)
- `RATE_LIMIT_EXCEEDED` — 429
- `CLUSTER_MODE_DISABLED` — 404 (on `/cluster/*`)
- `TENANT_UNKNOWN` — 404 (on `/cluster/stats/self`)
- `NO_TENANT_CONTEXT` — 401 (defence-in-depth on `/cluster/*`)

---

## 8. Test matrix

Contract tests live in two places:

- **`nexus-core/src/cluster/`** — per-module unit tests.
  Namespace validation (8), config serde (4), context function
  allow-list (11), quota rate windows (5), scope walker (14),
  quota middleware (3).
- **`nexus-core/tests/cluster_isolation_tests.rs`** — end-to-end
  integration. Two-tenant node isolation, relationship-type
  isolation, standalone-mode regression guard, alice-deletes-bob
  attack, storage quota allow/deny/read-safe/standalone-ignores,
  `storage_bytes_for_namespace` arithmetic + cross-tenant
  isolation.

Every contract documented in this spec must have a pinned test
in one of those two places. If this spec gains a new contract
without a corresponding test, consider it speculative.

---

## 9. Changelog

- **1.0.0** — cluster mode shipped. Auth + rate limits + catalog-
  prefix isolation + per-tenant stats + write-path storage
  quota gate. HiveHub SDK integration and load tests are
  follow-ups.
