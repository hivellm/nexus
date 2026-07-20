# Proposal: phase0_fix-server-secure-defaults-and-dos

**Priority: HIGH — the server ships with authentication off by default, no active rate limiting, no
request timeout, a default root/root credential once auth is turned on, a public `/stats` leak, and
permissive CORS — six independent hardening gaps that compound into an unauthenticated, DoS-able,
information-leaking default deployment.** Found in a server/auth security audit; not previously
reported.

## Why

Six related defects in `nexus-server`'s defaults and request-handling pipeline, each independently
exploitable:

**H1 — auth off by default, no public-binding guard.** `config.rs:255` sets `enabled: false` by
default; `main.rs:493` only enables the auth middleware `if config.auth.enabled || cluster_enabled`.
`required_for_public` (`config.rs:222`/`:256`) is parsed from config but never read anywhere at
runtime — a dead flag. Starting the server bound to `0.0.0.0` (e.g. `NEXUS_ADDR=0.0.0.0:15474`) with
default config serves the full API, including all data and the `/auth/*` management surface from
`phase0_fix-auth-management-authorization`, completely unauthenticated, with no boot-time warning.

**H2 — no rate limiting active.** `main.rs:483` constructs a limiter but discards it:
`let _rate_limiter = RateLimiter::new();` — bound to `_`, never used. `rate_limit_middleware`
(`middleware/rate_limit.rs:175`) is never `.layer()`-ed onto the router. The auth middleware's own
integrated limiter path is dead code — its `None` branch is the one always taken
(`auth/middleware.rs:552`). Nothing throttles `POST /auth/login` or API-key attempts, so credential
guessing and key-guessing floods are unbounded.

**H4 — no query/request timeout.** `api/cypher/execute/handler.rs` reads `start_time.elapsed()` only
to populate the response's `execution_time_ms` field (`:60,101,192`); it is never used to abort a
running query. The router (`main.rs:1069-1080`) layers `DefaultBodyLimit`, compression, CORS, and
tracing, but no `TimeoutLayer`. A pathological Cypher statement or a slowloris-style connection holds a
worker thread and its admission permit indefinitely (the admission concurrency queue at `main.rs:976`
and the 16 MiB body cap at `:1074` mitigate other vectors but not this one).

**M2 — default root/root.** `config.rs:244-246` defaults to `username: "root"`, `password: "root"`,
`enabled: true`. An operator who enables auth without also setting `NEXUS_ROOT_PASSWORD` gets a live
root/root account.

**M3 — `/stats` public even when auth is enabled.** `auth/middleware.rs:413-415` unconditionally
returns `requires_auth("/stats") == false`; the route is registered at `main.rs:755`. Node count,
relationship count, and storage statistics are readable by anyone regardless of the auth setting.
(`/health` and `/` are public too, unless `require_health_auth` is set — default false — a narrower,
accepted case.)

**M4 — permissive CORS.** `main.rs:1078` applies `CorsLayer::permissive()`, allowing any origin to read
API responses from a browser. Combined with H1 (auth off by default), any website can issue
cross-origin requests against a default deployment and read the results.

## What Changes

- **H1**: at boot, if the bind address is unspecified (`0.0.0.0`) or otherwise public and
  `auth.enabled == false`, hard-fail startup unless the operator explicitly opts out; enforce
  `required_for_public` at runtime (or remove it if superseded by the hard-fail).
- **H2**: `.layer()` `rate_limit_middleware` onto the router, keyed on the client socket IP
  (`rate_limit.rs:181`, already the safe keying choice); remove the discarded `_rate_limiter` in favor
  of the wired limiter.
- **H4**: add a `tower_http::timeout::TimeoutLayer` to the router and/or a statement-level deadline
  enforced inside the Cypher executor, so a pathological query or slow client cannot hold a worker
  indefinitely.
- **M2**: require an explicit `NEXUS_ROOT_PASSWORD` (or equivalent) whenever auth is enabled; refuse to
  boot with the literal default password, or at minimum emit a loud boot-time warning.
- **M3**: gate `/stats` behind the same auth check as the rest of the API when auth is enabled, or make
  it configurable the way `/health` already is via `require_health_auth`.
- **M4**: replace `CorsLayer::permissive()` with a configured origin allow-list (empty/same-origin by
  default; operator-supplied list for cross-origin deployments).

## Impact

- Affected specs: `docs/security/AUTHENTICATION.md` (default-auth and root-credential policy),
  `docs/users/configuration/PERFORMANCE_TUNING.md` or a new server-hardening doc (timeout/rate-limit
  defaults)
- Affected code: `nexus-server/src/config.rs`, `nexus-server/src/main.rs`,
  `nexus-server/src/middleware/rate_limit.rs`, `nexus-core/src/auth/middleware.rs`,
  `nexus-server/src/api/cypher/execute/handler.rs`
- Breaking change: YES — a server bound to a public address with auth disabled will refuse to start
  (H1); the default root password will no longer be accepted silently (M2); CORS will no longer allow
  arbitrary origins by default (M4). All three are intentional — they close defaults that are unsafe in
  production. NO breaking change for H2/H4/M3 beyond added latency/throttling for abusive traffic.
- User benefit: a default deployment can no longer be stood up fully open to the internet by accident;
  login/key-guessing is throttled; a runaway query can no longer exhaust the server; `/stats` no longer
  leaks through an enabled auth boundary; browser-based cross-origin reads are restricted to configured
  origins.
- Related: `phase0_fix-auth-management-authorization` (H1 is what makes that privilege-escalation
  surface reachable with zero credentials), `phase0_fix-password-hashing` (H2's throttle is the other
  half of bounding online password/key guessing)
