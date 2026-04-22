# Cluster Mode

> Multi-tenant deployments on a single Nexus instance.

Cluster mode lets one Nexus server host data for many tenants at the
same time, with per-tenant data isolation, mandatory authentication,
and configurable rate limits — all enforced on top of the existing
storage and query layers with zero modifications to either.

This document is the operator guide. For the architectural rationale
see [ADR-7: catalog-prefix isolation](../.rulebook/decisions/ADR-7-cluster-mode-tenant-isolation-via-catalog-prefix-not-per-tenant-storage.md);
for the Rust surface consumed by integrations see the `nexus_core::cluster`
module docs.

---

## When to enable cluster mode

Enable cluster mode when:

- One Nexus instance serves multiple independent tenants
  (e.g. a managed hosting product, an internal shared graph
  platform).
- You need hard isolation between tenants — they must not be
  able to observe or modify each other's data.
- You want per-tenant rate limits and (eventually, via HiveHub)
  per-tenant storage quotas and usage tracking.

Stay on standalone mode when:

- Nexus runs dedicated to a single application. Cluster mode
  forces authentication on every URI and adds a catalog-prefix
  rewrite to every query; standalone keeps the pre-cluster
  behaviour byte-for-byte identical.
- You already isolate tenants at a higher layer (one Nexus
  instance per tenant). Double isolation just costs CPU.

Cluster mode is **off by default.** Opting in is explicit; there
is no automatic promotion path.

---

## Enabling cluster mode

### Environment variable

```bash
NEXUS_CLUSTER_ENABLED=true ./target/release/nexus-server
```

Accepted truthy values: `1`, `true`, `TRUE`, `yes`. Anything
else (or unset) keeps standalone mode.

The env var flips `config.cluster.enabled` to `true` and installs
`nexus_core::cluster::ClusterConfig::enabled_with_defaults()` —
which uses sensible tenant defaults but **does not** enable
catalog-prefix isolation on its own (see the next section).

### Turning on data isolation

Cluster mode's master switch gates authentication and rate
limiting; the actual data-isolation mechanism (catalog-prefix)
is a separate `isolation` field on `ClusterConfig`. It must be
flipped to `CatalogPrefix` explicitly — otherwise every tenant
still shares the same label / relationship-type / property-key
catalog entries.

In code:

```rust
let cfg = nexus_core::cluster::ClusterConfig {
    enabled: true,
    isolation: nexus_core::cluster::TenantIsolationMode::CatalogPrefix,
    default_quotas: nexus_core::cluster::TenantDefaults::default(),
};
```

The `isolation` field is separate because flipping it mid-
deployment requires a one-shot rewrite of every existing catalog
entry. `enabled = true`, `isolation = None` is a valid
configuration — it just means auth + rate limits without
tenant-scoped storage, appropriate for a deployment where
isolation is already enforced one layer up (e.g. per-tenant
Nexus instances behind a shared proxy).

---

## What changes when cluster mode is on

### Every URI requires authentication

In standalone mode `/`, `/health`, `/stats`, and `/openapi.json`
are public. In cluster mode they are not. Every request must
present a valid API key via either:

```
Authorization: Bearer nx_...
X-API-Key: nx_...
```

Health probes should carry an API key with minimal permissions.
The rejection response is `401 Unauthorized` with the standard
auth-error JSON body (`code: "AUTHENTICATION_REQUIRED"`).

### API keys are per-tenant

Each API key carries a `user_id` (stored as `Option<String>` on
the key). In cluster mode a key without a `user_id`, or with one
that fails [`UserNamespace`][user-namespace] validation (reserved
delimiter `:`, control chars, longer than 128 bytes), is rejected
at the middleware with `401 INVALID_TOKEN`. There is no "global
scope" to fall back to — every cluster-mode request MUST route
to a tenant.

[user-namespace]: ../nexus-core/src/cluster/namespace.rs

### Data is isolated at the catalog layer

When `isolation = CatalogPrefix` is active, every label /
relationship-type / property-key name gets rewritten to
`ns:<tenant>:<name>` before it reaches the catalog. Two tenants
who both create a `Person` label get distinct catalog IDs, which
flow through to distinct label bitmap indexes, which flow through
to distinct record-store iteration order — so a `MATCH (n:Person)`
from tenant A can structurally never return tenant B's rows.

The rewrite is performed by `cluster::scope::scope_query` as an
AST walk after parsing and before planning. Record stores, page
cache, WAL, indexes, and every Cypher operator are untouched —
they all continue to deal in label IDs, and label IDs are
tenant-distinct automatically.

### Rate limits apply per tenant

Every request passes through `cluster::middleware::quota_middleware_handler`,
which calls `LocalQuotaProvider::check_rate` with the request's
namespace. The default limits are 6 000 req/min and 300 000 req/hour
per tenant — generous enough to never trip on synthetic load tests,
but a hard ceiling on long-run abuse.

Responses tag each allowed call with `X-RateLimit-Remaining`.
Denied calls return `429 Too Many Requests` with:

```json
{
  "code": "RATE_LIMIT_EXCEEDED",
  "message": "per-minute rate limit exceeded",
  "retry_after_seconds": 42
}
```

plus a `Retry-After` header. SDK clients should back off on
`retry_after_seconds`.

### Function-level permissions on API keys

API keys optionally carry an `allowed_functions: Option<Vec<String>>`
allow-list. `None` = unrestricted (the pre-cluster default); an
explicit list restricts the key to exactly those MCP / RPC tool
names. An empty `Some(vec![])` is a third state meaning "may call
nothing" — useful for health-probe-only keys.

Handlers enforce the allow-list with:

```rust
let ctx = nexus_core::auth::extract_user_context(&request)
    .ok_or(StatusCode::UNAUTHORIZED)?;
ctx.require_may_call("cypher.execute")?;  // 403 FUNCTION_NOT_ALLOWED on deny
```

Discovery endpoints can trim advertised tools with:

```rust
let visible = ctx.filter_callable(&all_tool_names);
```

Rejection is a structured `FunctionAccessError` with a stable
`code = "FUNCTION_NOT_ALLOWED"` (pinned as `FunctionAccessError::CODE`
so SDK decoders never need to match on human-readable message text).

---

## Migration path from standalone to cluster mode

1. **Stop serving new writes.** Either take the server down or
   proxy requests to read-only mode. Cluster mode changes the
   catalog's key space; in-flight writes at the exact moment of
   transition could land in the wrong tenant.
2. **Back up `data/`.** The catalog migration is additive
   (old unscoped entries stay in place; new tenant-scoped entries
   get written alongside), but taking a snapshot before any
   production flip is basic hygiene.
3. **Regenerate API keys.** Existing keys were stored in the
   pre-cluster binary format and will not deserialise under the
   new binary — the `auth/storage.rs` layer now uses JSON, not
   bincode. Standalone deployments can re-seed on first boot;
   cluster-mode deployments need to issue new keys to each tenant
   via `AuthManager::generate_api_key_for_user`.
4. **Provision tenant namespaces.** Each tenant needs a stable
   `user_id` that passes `UserNamespace::new` validation (no
   `:`, no control chars, ≤128 bytes). UUIDs are ideal.
5. **Start the server with `NEXUS_CLUSTER_ENABLED=true`.** Every
   inbound request now requires auth; every query from an
   authenticated tenant routes to the catalog-prefixed namespace.
   The first write from each tenant warms up their catalog
   entries; subsequent writes are zero-overhead relative to
   standalone mode (a single HashMap lookup on the rewritten
   name).

**Existing graph data created under standalone mode is NOT
automatically reassigned to any tenant.** It remains under the
unscoped catalog names and is invisible to cluster-mode tenants
(which only query scoped names). The recommended posture:

- For a fresh cluster deployment: start with an empty `data/`.
- For an existing deployment converting to cluster mode: export
  the standalone data, re-import it as a specific tenant's
  data, and flip cluster mode on.

---

## Configuration reference

The core config type is `nexus_core::cluster::ClusterConfig`:

```rust
pub struct ClusterConfig {
    pub enabled: bool,
    pub isolation: TenantIsolationMode,
    pub default_quotas: TenantDefaults,
}

pub enum TenantIsolationMode { None, CatalogPrefix }

pub struct TenantDefaults {
    pub storage_mb: u64,               // per-tenant storage budget
    pub requests_per_minute: u32,      // soft rate limit, per tenant
    pub requests_per_hour: u32,        // soft rate limit, per tenant
}
```

Defaults: `storage_mb = 1024`, `requests_per_minute = 6000`,
`requests_per_hour = 300000`.

The server-side wrapper (`nexus_server::config::Config.cluster`)
reuses the same struct; no server-specific shape.

---

## Observability

### Log lines

`cluster::scope::scope_query` emits a `tracing::trace!` with the
namespace and clause count on every rewrite. Enable with
`RUST_LOG=nexus_core::cluster=trace`. In production set this to
`info` and only the quota middleware's 429 path logs regularly.

### Response headers

| Header | When | Meaning |
|---|---|---|
| `X-RateLimit-Remaining` | Allow | Requests left in the narrower window (minute or hour, whichever is tighter). |
| `Retry-After` | 429 | Seconds to wait before retrying. Derived from the remaining time in the quota window that denied the request. |

### Tenant identity in handlers

`nexus_core::auth::extract_user_context(&Request) -> Option<&UserContext>`
returns the per-request identity when cluster mode is on.
`UserContext` exposes:

- `namespace()` — the `UserNamespace` the request is scoped to.
- `api_key_id()` — the stable identifier of the key that
  authenticated the request. Use for audit logging; never for
  authorisation decisions (those go through `may_call` /
  `require_may_call`, not key identity).
- `may_call(name)` / `require_may_call(name)` — function-level
  permission check against the key's allow-list.
- `filter_callable(names)` — trim a list of tool names to those
  the caller may invoke.
- `allowed_functions()` — raw view of the allow-list (for
  reporting / admin endpoints).

---

## Testing against cluster mode

The multi-tenant isolation contract is covered end-to-end by
`nexus-core/tests/cluster_isolation_tests.rs`:

- `two_tenants_do_not_see_each_others_nodes` — reads.
- `relationship_types_are_also_isolated` — relationship types.
- `alice_cannot_delete_bobs_data_via_label_match` — DELETE.
- `standalone_mode_is_unaffected_by_context_api` — regression
  guard that the new context-aware entry point behaves exactly
  like the legacy `execute_cypher` when mode is `None`.

Cluster-mode integration tests must use `setup_isolated_test_engine`
(not the default `setup_test_engine`) because the default test
harness shares an LMDB catalog across every test in the binary to
sidestep a Windows TlsFull limit — that sharing is fatal for
isolation-correctness tests but irrelevant for feature-correctness
tests.

---

## Known limitations

- **Label-less `MATCH (n)` is not isolated by catalog-prefix.**
  Any query that omits a label has no catalog entry to rewrite
  and will scan every node, including other tenants'. Cluster-
  mode deployments are expected to always use labelled queries
  (they already do for indexing). If you need truly label-less
  queries, front Nexus with a per-tenant database instead.
- **Cross-tenant admin queries are not supported from Cypher.**
  Use the quota provider's `snapshot(ns)` method or the usage
  telemetry pipeline (planned) for operational views.
- **Storage byte accounting uses a flat-rate heuristic.** Every
  successful write charges 256 bytes against the tenant's
  `storage_bytes_used`. The gate (`provider.check_storage(ns, 0)`
  before dispatch) rejects if the tenant is already past their
  budget, and `record_usage` bumps the counter after every
  successful write. Precise per-record accounting (counting the
  actual node + property bytes written) is a Phase 4 follow-up —
  the current heuristic is conservative enough to avoid false
  rejections (tenants stay under-reported) but accurate enough
  that an unbounded-write attack deterministically hits the cap.

## Measured overhead

Run the benchmarks yourself with:

```text
cargo +nightly bench -p nexus-core --bench cluster_mode_benchmark
```

Reference numbers from the reference development box (Windows,
AMD, unloaded). Useful shape for operators weighing the
standalone → cluster flip, NOT a guarantee:

| Path | Standalone | Cluster (`CatalogPrefix`) | Delta |
|---|---:|---:|---:|
| `execute_cypher(CREATE (n:Person))` end-to-end | ~620 µs | ~623 µs | <1% |
| `scope_query` AST walk on a complex query | 1.0 µs | 2.0 µs | +1.0 µs |
| `LocalQuotaProvider::check_rate` | n/a | ~82 ns | — |
| `check_rate` + `record_usage` paired | n/a | ~110 ns | — |

The cluster-mode overhead is dominated by the AST walk (~1 µs)
and the override install/clear (~2 µs). The quota provider
round-trip adds another ~110 ns per write. All of this is a tiny
fraction of the ~620 µs baseline single-CREATE execution time,
so flipping on cluster mode costs well under the 15% budget the
task proposal set.
- **HiveHub SDK integration is not yet wired.** `QuotaProvider`
  is a trait with a local implementation; a HiveHub-backed
  implementation that consults the control-plane quota API lands
  in a follow-up (Phase 1 §1-§3).
