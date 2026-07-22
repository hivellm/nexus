# Proposal: phase0_fix-password-hashing

**Priority: HIGH — user passwords are stored as unsalted, timing-unsafe SHA-512 (rainbow-table
crackable), API-key verification is O(N) Argon2 per login attempt (CPU-exhaustion DoS), and the login
endpoint leaks internal errors plus a timing side-channel that enumerates valid usernames.** Found in a
server/auth security audit; not previously reported.

## Why

**H3 — unsalted SHA-512 + non-constant-time compare.** `crates/nexus-core/src/auth/password.rs:6-11`
hashes passwords with plain `Sha512` — no per-user salt, no key-derivation-function work factor.
`:16` compares the computed digest with `computed_hash == hash`, an `==` on byte data that short-circuits
on the first mismatched byte — not constant-time. This path is reachable via `POST /auth/login`
(`api/auth.rs:593`), the RESP3 admin command (`protocol/resp3/command/admin.rs:286`), and the RPC admin
command (`protocol/rpc/dispatch/admin.rs:143`); the same hashing is used to store new passwords in
`create_user` (`api/auth.rs:80`) and to seed the root account (`main.rs:268`). A fast, unsalted, single-
round hash is crackable at GPU/rainbow-table speed if the credential store leaks, and two users with the
same password produce identical hashes (a further leak on database compromise). This is inconsistent
with the codebase's own API-key hashing, which already does this correctly with Argon2id + per-key salt
(`auth/mod.rs:142`/`:347`) — the fix is to bring passwords in line with the pattern already used for
keys.

**M1 — O(N_keys) Argon2 verification per login attempt.** `auth/mod.rs:340-366`: verifying an
`nx_`-prefixed API key loops over every stored valid key and runs a full Argon2 verify against each one
until a match is found (or the loop is exhausted). Argon2 is deliberately expensive (that is the point
of a KDF); doing it once per stored key, per attempt, means CPU cost per request scales linearly with
the number of active keys — an attacker (or just a busy deployment) can exhaust server CPU with
authentication traffic alone.

**L2 — login error leak + timing-based username enumeration.** `api/auth.rs:665` returns
`format!("Failed to generate tokens: {}", e)` directly to the client on JWT-generation failure — an
internal error message leaked over the wire. Separately, at `api/auth.rs:670`, the unknown-username path
returns immediately without ever computing a password hash, while a known username always triggers a
SHA-512 compute before the mismatch is reported (H3's cost, reused here) — the response-time difference
lets an attacker distinguish valid from invalid usernames by timing alone, without needing the correct
password.

## What Changes

- **H3**: switch password hashing to Argon2id with a per-user random salt, reusing the Argon2 instance
  already configured for API keys (`auth/mod.rs`); replace the `==` digest compare with a constant-time
  comparison (Argon2's verify already does this internally once adopted).
- **M1**: add a fast lookup (e.g. an index keyed by a key-id prefix embedded in the `nx_` token) to
  select the single candidate stored key before running Argon2 verify, so verification is O(1) Argon2
  calls per attempt instead of O(N_keys).
- **L2**: return a generic, non-leaking error to the client on any login failure (JWT generation
  included), logging the real error server-side only; equalize timing between the unknown-user and
  known-user-wrong-password paths by always performing an equivalent-cost hash/verify operation (a dummy
  verify against a fixed hash) on the not-found path.

## Impact

- Affected specs: `docs/security/AUTHENTICATION.md` (password storage algorithm, login error contract)
- Affected code: `nexus-core/src/auth/password.rs`, `nexus-core/src/auth/mod.rs`,
  `nexus-server/src/api/auth.rs`
- Breaking change: YES — existing SHA-512 password hashes cannot be verified by an Argon2id-only path; a
  migration (rehash-on-next-successful-login, or a one-time forced reset) is required for any stored
  passwords. NO breaking change to the public login request/response shape beyond the error message text
  (L2, intentional).
- User benefit: leaked password hashes are no longer trivially crackable; identical passwords no longer
  produce identical hashes; a flood of login/key attempts can no longer exhaust server CPU by itself; the
  login endpoint no longer leaks internal errors or lets an attacker enumerate valid usernames by timing.
- Related: `phase0_fix-server-secure-defaults-and-dos` (H2 — the missing rate limit is what makes online
  guessing against this endpoint unbounded in the first place), `phase0_fix-auth-management-authorization`
  (adjacent auth-surface hardening)
