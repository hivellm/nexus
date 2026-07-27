# hyper only keeps a keep-alive connection when the request body is fully consumed — a 429 (or any early rejection) must drain it first

**Category:** rust / server-middleware
**Tags:** nexus-server, axum, hyper, rate-limit, connection-reset, binary-boundary-error-handling

## Description

`crates/nexus-server/src/middleware/rate_limit.rs`'s `rate_limit_middleware`
returned `429 Too Many Requests` by constructing a fresh `Response` and
dropping the incoming `axum::http::Request<Body>` — the request's body
stream was never read. For small bodies this is harmless (hyper can
discard a short unread body on `Drop`), but for a large unread body
(e.g. an `/ingest` bulk-load payload mid-flight) hyper refuses to drain
it and instead aborts the keep-alive TCP connection with an RST. On
Windows the client observes `WSAECONNABORTED` (os error 10053) *during*
`send`, not a readable HTTP 429 — the rate limit fired correctly but the
client never got to see the response.

The fix: before constructing the 429 response, split the request into
parts + body and explicitly drain it:

```rust
let (_parts, req_body) = request.into_parts();
let _ = axum::body::to_bytes(req_body, RATE_LIMIT_BODY_DRAIN_CAP_BYTES).await;
```

Ignore the `Result` — draining is best-effort; a body that exceeds the
cap still gets *mostly* read (up to the cap) before `to_bytes` bails,
which is enough in practice, and the response is returned either way.
Choose the cap to match (or exceed) the server's configured
`DefaultBodyLimit` (`Config::max_body_size_bytes`, 16 MiB by default) so
legitimate bulk payloads are always fully drained.

**General rule**: any Axum/tower middleware that short-circuits a
request AFTER the body has started arriving (auth rejection, rate
limit, admission control, etc.) must drain the body before responding,
or risk the same abortive-RST failure mode for large bodies. Middleware
that rejects *before* any body bytes are read (e.g. a header-only check)
does not have this problem — hyper can cleanly close an unstarted body.

## Companion fix: exempt loopback + make the limiter configurable

Same task also added `enabled: bool` and `exempt_loopback: bool` to the
existing `RateLimitConfig` struct (kept in `middleware/rate_limit.rs`,
NOT duplicated into `config.rs`), wired via `NEXUS_RATE_LIMIT_ENABLED`,
`NEXUS_RATE_LIMIT_MAX_REQUESTS`, `NEXUS_RATE_LIMIT_WINDOW_SECS`,
`NEXUS_RATE_LIMIT_BURST`, `NEXUS_RATE_LIMIT_EXEMPT_LOOPBACK` (defaults:
enabled=true, 100/60s+20 burst, exempt_loopback=true — matches the
project's existing "auth disabled for localhost" posture). The bypass
check (`!config.enabled || (config.exempt_loopback && addr.ip().is_loopback())`)
lives at the very top of the middleware fn, before any token is
consumed — a full pass-through to `next.run(request).await`, no
`X-RateLimit-*` headers attached.

`Config` (in `config.rs`) gained a `pub rate_limit: RateLimitConfig`
field (type imported from `crate::middleware::RateLimitConfig` — no
new/duplicate type). `Config::from_env()` parses the 5 env vars using
the file's existing `std::env::var(..).ok().and_then(|v| v.parse().ok()).unwrap_or(<field>_defaults.<field>)`
style (see `rpc_defaults`/`resp3_enabled` block a few lines above for
the established pattern this mirrors).

## When to use

Any future middleware that owns request rejection logic and runs after
Axum has already started streaming the body into the handler chain.

## When not to use

Middleware that only inspects headers/extensions and rejects before the
body extractor runs — draining is a no-op cost there, but not a
correctness requirement.
