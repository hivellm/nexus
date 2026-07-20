# Tasks: phase0_fix-password-hashing

Passwords are stored as unsalted SHA-512 with a non-constant-time compare
(`crates/nexus-core/src/auth/password.rs:6-11,16`), reachable via `POST /auth/login` (`api/auth.rs:593`)
among other admin surfaces; API-key verification loops Argon2 over every stored key per attempt
(`auth/mod.rs:340-366`); and the login endpoint leaks a raw JWT error (`api/auth.rs:665`) while its
timing distinguishes valid from invalid usernames (`api/auth.rs:670`). Example trigger for H3/L2:
`POST /auth/login {"username":"root","password":"wrong"}` for a real vs. a made-up username — the
response time differs measurably because only the real username reaches the SHA-512 compute.

Order rationale: fix H3 first — it changes the on-disk hash format, and both M1 (which touches the same
`auth/mod.rs` verification path) and L2 (whose timing fix depends on knowing the real per-user hash
cost) build on top of it. Doing M1 or L2 first would mean redoing their work once the hash algorithm
changes underneath them.

## 1. Reproduce all three defects first (failing tests)
- [ ] 1.1 H3: unit-test that two users with the same password produce the SAME stored hash today
  (`auth/password.rs:6-11`) — confirms the missing per-user salt; a second assertion that the digest
  compare (`:16`) is a plain `==` (inspect for short-circuit, or time a near-miss vs. a total-mismatch
  hash and confirm no constant-time guarantee is documented/enforced)
- [ ] 1.2 M1: benchmark/measure a single key-verification call's Argon2 invocation count as a function of
  the number of stored valid keys (`auth/mod.rs:340-366`); assert today it scales linearly (O(N)) instead
  of O(1)
- [ ] 1.3 L2: integration-test `POST /auth/login` for a nonexistent username vs. a real username with a
  wrong password; assert today there is a measurable timing gap attributable to the skipped hash compute
  on the not-found path (`api/auth.rs:670`); separately, force a JWT-generation failure and assert the
  response body today contains the raw `format!("Failed to generate tokens: {}", e)` text (`:665`)

## 2. Fix H3 — Argon2id password hashing
- [ ] 2.1 Replace the `Sha512`-based hash/verify in `auth/password.rs:6-11,16` with Argon2id, reusing the
  Argon2 configuration already used for API keys (`auth/mod.rs:142`/`:347`), generating a fresh per-user
  random salt on every password set
- [ ] 2.2 Update every write site that stores a password hash — `create_user` (`api/auth.rs:80`) and the
  root-account seed (`main.rs:268`) — to use the new hashing function
- [ ] 2.3 Update every verify site — `POST /auth/login` (`api/auth.rs:593`), RESP3 admin
  (`protocol/resp3/command/admin.rs:286`), RPC admin (`protocol/rpc/dispatch/admin.rs:143`) — to use the
  new Argon2id verify (which is constant-time by construction)
- [ ] 2.4 Define and implement the migration path for existing SHA-512 hashes already on disk: either
  rehash-on-next-successful-login (verify against the old scheme once, then re-store as Argon2id) or a
  documented forced-reset; do not silently strand existing accounts unable to log in
- [ ] 2.5 Make the §1.1 test pass: two users with the same password now produce different stored hashes;
  add a positive test that the new verify function accepts the correct password and rejects a wrong one

## 3. Fix M1 — bound Argon2 verification cost per attempt
- [ ] 3.1 Add a fast index (e.g. a key-id prefix carried in the `nx_` token, looked up in a hash map) so
  the single candidate stored key is selected before Argon2 verify runs, replacing the loop-over-all-keys
  in `auth/mod.rs:340-366`
- [ ] 3.2 Make the §1.2 test pass: a single key-verification call now performs exactly one Argon2 verify
  regardless of how many keys are stored; add a test with a large number of stored keys confirming
  verification latency stays flat, not linear

## 4. Fix L2 — generic login errors + timing equalization
- [ ] 4.1 Replace the leaked `format!("Failed to generate tokens: {}", e)` response (`api/auth.rs:665`)
  with a generic client-facing error; log the real error server-side only
- [ ] 4.2 Equalize the not-found path (`api/auth.rs:670`): perform an Argon2id verify against a fixed
  dummy hash before returning the same generic "invalid credentials" response used for a wrong password,
  so the unknown-username path costs the same as the known-username path
- [ ] 4.3 Make the §1.3 tests pass: the timing gap between unknown-username and wrong-password responses
  is no longer measurable within a reasonable tolerance; the JWT-failure response no longer contains the
  raw internal error text

## 5. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 5.1 Update `docs/security/AUTHENTICATION.md` with the Argon2id password-hashing scheme, the
  migration path for pre-existing SHA-512 hashes, and the generic login-error contract; add a CHANGELOG
  entry
- [ ] 5.2 Tests: password hashes are salted and non-identical for identical passwords; login/verify
  round-trips correctly for new and migrated accounts; key verification cost is O(1) in the number of
  stored keys; login responses no longer leak internal errors or a username-enumeration timing signal
- [ ] 5.3 Run `cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test --workspace` — all green

## Related
- `phase0_fix-server-secure-defaults-and-dos` — H2 (no rate limiting) is what makes online guessing
  against this endpoint unbounded in the absence of these fixes
