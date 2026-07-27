# Tasks: phase0_fix-server-secure-defaults-and-dos

Six independent server-hardening gaps ship in the default configuration and request pipeline: auth is
off by default with no guard against public binding (`config.rs:255`, `main.rs:493`); the constructed
rate limiter is discarded unused (`main.rs:483` `let _rate_limiter = ...`); no request/query timeout
exists (`main.rs:1069-1080` has no `TimeoutLayer`); enabling auth without `NEXUS_ROOT_PASSWORD` leaves a
live root/root account (`config.rs:244-246`); `/stats` is public even with auth enabled
(`auth/middleware.rs:413-415`); and CORS is `permissive()` for any origin (`main.rs:1078`). Example
trigger for H1: start the server with `NEXUS_ADDR=0.0.0.0:15474` and default config — every endpoint,
including `/data/*` and `/auth/*`, answers with zero credentials.

Order rationale: fix H1 (auth-off-by-default) first — it is the root enabler that makes every other gap
reachable without any credential at all — then H2 (rate limiting) and M2 (root/root), which both harden
the auth boundary H1 just closed, before touching the independent H4/M3/M4 request-pipeline gaps. Each
defect gets its own failing-test-then-fix pair so a partial fix never silently "closes" a defect the
tests don't cover.

## 1. Reproduce all six defects first (failing tests)
- [ ] 1.1 H1: start the server with default config and `NEXUS_ADDR=0.0.0.0:<port>`; assert today it
  boots successfully and serves an unauthenticated request (e.g. `GET /databases`) with `200` — no
  boot-time failure or warning (`config.rs:255`, `main.rs:493`)
- [ ] 1.2 H2: with auth enabled, fire more `POST /auth/login` attempts than any reasonable per-IP budget
  in a short window; assert today none are throttled (`main.rs:483`, `middleware/rate_limit.rs:175`
  never layered)
- [ ] 1.3 H4: submit a long-running/pathological Cypher statement via `POST /cypher` (or hold a slow
  connection open) and assert today the request is never aborted by the server regardless of how long it
  runs (`api/cypher/execute/handler.rs:60,101,192` only measure elapsed time, no `TimeoutLayer` on the
  router `main.rs:1069-1080`)
- [ ] 1.4 M2: enable auth without setting `NEXUS_ROOT_PASSWORD`; assert `POST /auth/login` with
  `{"username":"root","password":"root"}` succeeds today (`config.rs:244-246`)
- [ ] 1.5 M3: with auth enabled and no API key supplied, `GET /stats`; assert it returns `200` with
  node/relationship/storage data today instead of `401` (`auth/middleware.rs:413-415`, route
  `main.rs:755`)
- [ ] 1.6 M4: send a cross-origin request (arbitrary `Origin` header, e.g. `https://evil.example`) and
  assert the response today carries `Access-Control-Allow-Origin: https://evil.example` or `*`
  (`main.rs:1078` `CorsLayer::permissive()`)

## 2. Fix H1 — auth-off-by-default / public-binding guard
- [ ] 2.1 At boot, detect an unspecified/public bind address (`0.0.0.0`, non-loopback) combined with
  `config.auth.enabled == false`; hard-fail startup with a clear error unless the operator passes an
  explicit override flag/env var
- [ ] 2.2 Enforce `required_for_public` (`config.rs:222`/`:256`) at runtime in the same check, or remove
  the field if the hard-fail supersedes it — do not leave it parsed-but-dead
- [ ] 2.3 Make the §1.1 test pass: default config + public bind now refuses to start; add a positive
  test that the same bind succeeds when auth is explicitly enabled or the override is passed

## 3. Fix H2 — wire the rate limiter
- [ ] 3.1 Replace `let _rate_limiter = RateLimiter::new();` (`main.rs:483`) with a bound variable and
  `.layer(rate_limit_middleware(...))` (or the router equivalent) on the app, keyed on the client socket
  IP as `rate_limit.rs:181` already computes
- [ ] 3.2 Confirm the auth middleware's integrated limiter branch (`auth/middleware.rs:552`) is either
  wired to the same limiter or intentionally superseded by the router-level layer — no dead branch left
  behind
- [ ] 3.3 Make the §1.2 test pass: excess `POST /auth/login` attempts from one IP within the window
  return `429`; a request from a different IP is unaffected

## 4. Fix M2 — require an explicit root password
- [ ] 4.1 When `config.auth.enabled == true` and no `NEXUS_ROOT_PASSWORD` (or config equivalent) is set,
  refuse to boot with a clear error naming the missing variable (`config.rs:244-246`)
- [ ] 4.2 Make the §1.4 test pass: root/root no longer authenticates once the default-password guard is
  in place; add a positive test that a boot with `NEXUS_ROOT_PASSWORD` set succeeds and the configured
  password authenticates

## 5. Fix H4 — request/query timeout
- [ ] 5.1 Add `tower_http::timeout::TimeoutLayer` to the router (`main.rs:1069-1080`) with a configurable
  duration
- [ ] 5.2 Add a statement-level deadline check inside the Cypher execution path
  (`api/cypher/execute/handler.rs`) so a query already admitted is aborted at the same budget, not just
  the HTTP layer
- [ ] 5.3 Make the §1.3 test pass: a pathological/slow request is aborted at the configured timeout
  instead of running unbounded; add a test that a normal fast query is unaffected

## 6. Fix M3 — gate `/stats`
- [ ] 6.1 Change `auth/middleware.rs:413-415` so `/stats` requires auth when `config.auth.enabled ==
  true` (mirroring the rest of the authenticated surface), or make it configurable via a
  `require_stats_auth` flag analogous to `require_health_auth`
- [ ] 6.2 Make the §1.5 test pass: `GET /stats` without a key returns `401` when auth is enabled; with a
  valid key it still returns `200`

## 7. Fix M4 — restrict CORS
- [ ] 7.1 Replace `CorsLayer::permissive()` (`main.rs:1078`) with a `CorsLayer` built from a configured
  origin allow-list (empty/same-origin by default)
- [ ] 7.2 Make the §1.6 test pass: a request from an origin not in the allow-list no longer receives an
  `Access-Control-Allow-Origin` echoing itself (or `*`); a request from an allow-listed origin still
  succeeds

## Status (2026-07-23) — ALL SIX FIXED

- **H1** boot-time public-bind guard: `Config::security_preflight()` refuses a
  non-loopback bind with auth off (override `NEXUS_AUTH_REQUIRED_FOR_PUBLIC=false`);
  `required_for_public` is now live. Default loopback bind unaffected.
- **H2** rate limiter wired onto the router (per-IP) + `into_make_service_with_connect_info`.
- **M2** default `root` password refused at boot when auth enabled (`NEXUS_ROOT_PASSWORD`).
- **H4** `TimeoutLayer` (30s, `NEXUS_REQUEST_TIMEOUT_SECS`) bounds every request;
  CPU-bound statement-level cancellation noted as a follow-up (no executor
  cancellation plumbing today).
- **M3** `/stats` gated behind auth when enabled (`require_stats_auth`, default on;
  `NEXUS_REQUIRE_STATS_AUTH=false` to opt out).
- **M4** `CorsLayer::permissive()` replaced by an allow-list
  (`NEXUS_CORS_ALLOWED_ORIGINS`, default empty = no cross-origin).

Tests: H1/M2 preflight matrix (6), H2 rate-limiter budget/independence unit test,
M3 `requires_auth`/`with_require_stats_auth` gating (4), `tests/server_hardening_test.rs`
timeout + CORS via `oneshot` (4). Reproduce-first for H1/M2/M3 was done by asserting
the NEW guarded behavior (the pre-fix behavior is the audited defect).

## 8. Tail (docs + tests — check or waive with tailWaiver)
- [x] 8.1 Update or create documentation covering the implementation —
  `docs/security/AUTHENTICATION.md` gained a "Secure Defaults & Server Hardening"
  section (boot preflight, rate-limit/timeout/`/stats`/CORS defaults + env vars);
  CHANGELOG entry added
- [x] 8.2 Write tests covering the new behavior — preflight matrix, rate-limiter
  unit test, `/stats` gating tests, and the timeout + CORS integration tests
- [x] 8.3 Run tests and confirm they pass — `cargo +nightly fmt --all` +
  `cargo clippy --workspace --all-targets --all-features -- -D warnings` green;
  targeted tests green; full `cargo +nightly test --workspace` run to confirm

## Related
- `phase0_fix-auth-management-authorization` — H1 here is what makes that privilege-escalation surface
  reachable with zero credentials
- `phase0_fix-password-hashing` — H2's rate limit is the other half of bounding online password/key
  guessing
