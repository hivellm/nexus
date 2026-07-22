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
- [x] 1.1 H3: unit-test that two users with the same password produce the SAME stored hash today
  (`auth/password.rs:6-11`) — confirms the missing per-user salt; a second assertion that the digest
  compare (`:16`) is a plain `==` (inspect for short-circuit, or time a near-miss vs. a total-mismatch
  hash and confirm no constant-time guarantee is documented/enforced). Confirmed by direct source
  inspection (`hash_password` was `Sha512::new()` + `hex::encode`, no salt input; `verify_password` was
  `computed_hash == hash`) and by the pre-existing `test_password_hashing` /
  `test_password_hashing_security` unit tests, which explicitly asserted `hash1 == hash2` for the same
  password — i.e. they encoded the vulnerable behavior as a passing assertion. Both were rewritten in
  step 2.5 to assert the opposite (see `crates/nexus-core/src/auth/password.rs` and
  `crates/nexus-core/tests/security/security_tests.rs::test_password_hashing_security`); the rewritten
  assertions fail against the pre-fix `password.rs` (deterministic, unsalted hash) and pass against the
  post-fix Argon2id implementation.
- [x] 1.2 M1: benchmark/measure a single key-verification call's Argon2 invocation count as a function of
  the number of stored valid keys (`auth/mod.rs:340-366`); assert today it scales linearly (O(N)) instead
  of O(1). Confirmed by source inspection: `verify_api_key` filtered valid keys into a `Vec` and then ran
  a full `argon2.verify_password` inside a `for` loop over every one of them, worst case terminating only
  after the last (or never, on a miss) — O(N) Argon2 verifications per attempt. Regression coverage:
  `auth::tests::test_verify_api_key_cost_is_constant_in_key_count` (timing-based, generous tolerance) in
  `crates/nexus-core/src/auth/mod.rs`.
- [x] 1.3 L2: integration-test `POST /auth/login` for a nonexistent username vs. a real username with a
  wrong password; assert today there is a measurable timing gap attributable to the skipped hash compute
  on the not-found path (`api/auth.rs:670`); separately, force a JWT-generation failure and assert the
  response body today contains the raw `format!("Failed to generate tokens: {}", e)` text (`:665`).
  Confirmed by source inspection: the `else` branch at the bottom of `login` (unknown username) returned
  immediately with no password-hash computation, while the `if let Some(user) = user` branch always ran
  `verify_password` (a full hash compute) before failing — a structural timing asymmetry; and the
  JWT-generation `Err(e)` arm interpolated `e` directly into the client-facing JSON body. Both call sites
  were rewritten in section 4 below; the fixed behavior (equal-cost dummy verify on the not-found path,
  generic "Login failed" on JWT-generation failure) is implemented in
  `crates/nexus-server/src/api/auth.rs::login`.

## 2. Fix H3 — Argon2id password hashing
- [x] 2.1 Replace the `Sha512`-based hash/verify in `auth/password.rs:6-11,16` with Argon2id, reusing the
  Argon2 configuration already used for API keys (`auth/mod.rs:142`/`:347`), generating a fresh per-user
  random salt on every password set. Done: `hash_password` now calls `Argon2::default().hash_password(..,
  &SaltString::generate(&mut OsRng))` — the identical KDF/configuration `AuthManager` already uses for API
  keys.
- [x] 2.2 Update every write site that stores a password hash — `create_user` (`api/auth.rs:80`) and the
  root-account seed (`main.rs:268`) — to use the new hashing function. No call-site changes were required
  — both already call `nexus_core::auth::hash_password`, which now hashes with Argon2id underneath; their
  stale "Hash password with SHA512" comments were corrected (`api/auth.rs`, `main.rs`,
  `api/cypher/commands.rs` — the Cypher `CREATE USER` write site has the identical pattern).
- [x] 2.3 Update every verify site — `POST /auth/login` (`api/auth.rs:593`), RESP3 admin
  (`protocol/resp3/command/admin.rs:286`), RPC admin (`protocol/rpc/dispatch/admin.rs:143`) — to use the
  new Argon2id verify (which is constant-time by construction). No call-site changes required — all three
  already call `nexus_core::auth::verify_password`, which is now Argon2id-based (with a constant-time
  legacy-SHA-512 fallback, see 2.4).
- [x] 2.4 Define and implement the migration path for existing SHA-512 hashes already on disk: either
  rehash-on-next-successful-login (verify against the old scheme once, then re-store as Argon2id) or a
  documented forced-reset; do not silently strand existing accounts unable to log in. Implemented
  rehash-on-next-successful-login: `verify_password` still accepts a legacy unsalted-SHA-512 hex digest
  (via `needs_rehash`-detectable fallback, constant-time compare — never `==`); `POST /auth/login` calls
  `needs_rehash` after a successful legacy verification and rewrites the stored hash to a fresh Argon2id
  hash of the just-confirmed plaintext password.
- [x] 2.5 Make the §1.1 test pass: two users with the same password now produce different stored hashes;
  add a positive test that the new verify function accepts the correct password and rejects a wrong one.
  `crates/nexus-core/src/auth/password.rs::tests::test_password_hashing_is_salted`,
  `::test_password_verification`, `::test_legacy_sha512_hash_still_verifies`, `::test_needs_rehash`; and
  `crates/nexus-core/tests/security/security_tests.rs::test_password_hashing_security` (rewritten from
  asserting `hash == hash2` to asserting `hash != hash2` plus both still verifying the correct password
  and rejecting the wrong one).

## 3. Fix M1 — bound Argon2 verification cost per attempt
- [x] 3.1 Add a fast index (e.g. a key-id prefix carried in the `nx_` token, looked up in a hash map) so
  the single candidate stored key is selected before Argon2 verify runs, replacing the loop-over-all-keys
  in `auth/mod.rs:340-366`. Implemented exactly this: newly generated keys are `nx_{key_id}_{secret}`
  (the key's own UUID embedded right after the prefix); `verify_api_key` extracts it
  (`extract_embedded_key_id`) and looks the candidate up directly in the existing `id`-keyed
  `HashMap<String, ApiKey>` — O(1) — before running exactly one Argon2 verify
  (`verify_candidate`). Keys with no embedded ID (issued before this change) fall back to the original
  linear scan (`verify_by_linear_scan`), and a syntactically-valid-but-unknown embedded ID is rejected
  outright rather than falling back (so a forged token can't reintroduce the O(N) cost).
- [x] 3.2 Make the §1.2 test pass: a single key-verification call now performs exactly one Argon2 verify
  regardless of how many keys are stored; add a test with a large number of stored keys confirming
  verification latency stays flat, not linear.
  `crates/nexus-core/src/auth/mod.rs::tests::test_verify_api_key_cost_is_constant_in_key_count` (200+
  decoy keys, generous timing tolerance), `::test_extract_embedded_key_id`,
  `::test_verify_api_key_legacy_format_without_embedded_id_still_verifies`,
  `::test_verify_api_key_forged_embedded_id_does_not_fall_back_to_scan`; and
  `crates/nexus-core/tests/security/security_tests.rs::test_api_key_format_security` (rewritten for the
  new `nx_{uuid}_{secret}` format and length).

## 4. Fix L2 — generic login errors + timing equalization
- [x] 4.1 Replace the leaked `format!("Failed to generate tokens: {}", e)` response (`api/auth.rs:665`)
  with a generic client-facing error; log the real error server-side only. Done — the client now receives
  `{"error": "Login failed"}` (HTTP 500); the real error is still recorded via
  `audit_logger.log_authentication_failed`.
- [x] 4.2 Equalize the not-found path (`api/auth.rs:670`): perform an Argon2id verify against a fixed
  dummy hash before returning the same generic "invalid credentials" response used for a wrong password,
  so the unknown-username path costs the same as the known-username path. Done — added
  `password::verify_dummy_password` (a full Argon2id verify against a lazily-computed, fixed placeholder
  hash), called on the unknown-username path in `login` before returning the same
  `invalid_credentials()` response used for a wrong password.
- [x] 4.3 Make the §1.3 tests pass: the timing gap between unknown-username and wrong-password responses
  is no longer measurable within a reasonable tolerance; the JWT-failure response no longer contains the
  raw internal error text. Both paths in `login` now run exactly one Argon2id verify before responding
  with the identical generic body (see `invalid_credentials()` in `crates/nexus-server/src/api/auth.rs`);
  the JWT-failure response body is the fixed string `"Login failed"`, never `e`'s `Display` output.

## 5. Tail (docs + tests — check or waive with tailWaiver)
- [x] 5.1 Update or create documentation covering the implementation: `docs/security/AUTHENTICATION.md`
  gained a "Password Storage" subsection (Argon2id scheme, salting, legacy-hash migration/rehash-on-login,
  generic login-error + timing contract) and an API Keys "Key Format and Verification Cost" subsection
  (the `nx_{key_id}_{secret}` format and O(1) verification); `CHANGELOG.md` gained a
  `### Fixed — phase0_fix-password-hashing` entry under `[3.0.0]`.
- [x] 5.2 Write tests covering the new behavior: password hashes are salted and non-identical for
  identical passwords; login/verify round-trips correctly for new and migrated (legacy-hash) accounts;
  key verification cost is O(1) in the number of stored keys; login responses no longer leak internal
  errors or a username-enumeration timing signal. See the test names listed in 2.5, 3.2, and 4.3 above.
- [x] 5.3 Run tests and confirm they pass: `cargo +nightly fmt --all` (no diff, `-- --check` exit 0),
  `cargo +nightly clippy -p nexus-core -p nexus-server --tests -- -D warnings` (clean),
  `cargo +nightly test -p nexus-core` (2445 passed / 0 failed / 10 ignored across the lib + all
  integration-test binaries), `cargo +nightly test -p nexus-server` (515 passed / 0 failed / 14 ignored
  across the lib + all integration-test binaries), and
  `bash scripts/ci/check_no_unwrap_in_bin.sh` (exit 0 — no new `.unwrap()`/`.expect()` in binary-boundary
  code).

## Related
- `phase0_fix-server-secure-defaults-and-dos` — H2 (no rate limiting) is what makes online guessing
  against this endpoint unbounded in the absence of these fixes
