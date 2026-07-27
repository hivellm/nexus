//! Password hashing and verification using Argon2id.
//!
//! Passwords are hashed with Argon2id and a fresh, per-password random salt
//! (`Argon2::default()` — the same KDF and RFC 9106 default parameters
//! already used for API-key hashing in [`super::AuthManager`]), so two
//! users with the same password never produce the same stored hash and a
//! leaked hash cannot be checked against a rainbow table.
//!
//! `verify_password` also accepts hashes produced by the pre-migration,
//! unsalted single-round SHA-512 scheme this module used to implement, so
//! accounts created before this change are not locked out. Callers that can
//! observe the plaintext password on a successful login (e.g. `POST
//! /auth/login`) should call [`needs_rehash`] and, if it returns `true`,
//! store a fresh [`hash_password`] result to transparently upgrade the
//! account off the legacy scheme.

use argon2::password_hash::{SaltString, rand_core::OsRng};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use sha2::{Digest, Sha512};
use std::sync::OnceLock;

/// Hash a password using Argon2id with a freshly generated random salt.
///
/// # Panics
///
/// Panics if Argon2 hashing fails with a freshly generated
/// [`SaltString`] and arbitrary UTF-8 password bytes, which is not a
/// reachable failure mode with `Argon2::default()` — the same assumption
/// [`super::AuthManager`]'s API-key hashing already relies on.
pub fn hash_password(password: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .expect("argon2 hashing with a freshly generated salt cannot fail")
        .to_string()
}

/// Verify a password against a stored hash.
///
/// Accepts both current Argon2id PHC-format hashes (constant-time by
/// construction, via [`PasswordVerifier`]) and legacy unsalted SHA-512 hex
/// digests produced before this module was migrated (compared in constant
/// time via [`constant_time_eq`], never with `==`). See [`needs_rehash`] to
/// detect the legacy case and upgrade it.
pub fn verify_password(password: &str, hash: &str) -> bool {
    match PasswordHash::new(hash) {
        Ok(parsed) => Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => verify_legacy_sha512(password, hash),
    }
}

/// Whether `hash` is in the pre-migration legacy format (anything that does
/// not parse as an Argon2 PHC string) and should be replaced with a fresh
/// [`hash_password`] result the next time its owner authenticates
/// successfully.
pub fn needs_rehash(hash: &str) -> bool {
    PasswordHash::new(hash).is_err()
}

/// Run a full Argon2id verify against a fixed, well-formed dummy hash.
///
/// Has no real user or password behind it — it exists purely to give a
/// "user not found" code path the same Argon2 cost as a real
/// username-with-wrong-password path, closing a timing side channel that
/// would otherwise let a caller enumerate valid usernames by measuring
/// response latency alone. The return value is intentionally discarded by
/// callers; only the constant-cost side effect matters.
pub fn verify_dummy_password(password: &str) -> bool {
    verify_password(password, dummy_hash())
}

/// Lazily-computed Argon2id hash of a fixed placeholder password, used by
/// [`verify_dummy_password`]. Computed once per process via a real
/// [`hash_password`] call (rather than a hand-written PHC literal) so it is
/// always valid for whatever Argon2 parameters are configured.
fn dummy_hash() -> &'static str {
    static DUMMY: OnceLock<String> = OnceLock::new();
    DUMMY.get_or_init(|| hash_password("nexus-timing-equalization-placeholder"))
}

/// Verify against the pre-migration unsalted SHA-512 hex-digest scheme.
/// Only reachable from [`verify_password`] when `hash` fails to parse as an
/// Argon2 PHC string, i.e. only for hashes stored before this module's
/// migration to Argon2id.
fn verify_legacy_sha512(password: &str, hash: &str) -> bool {
    let mut hasher = Sha512::new();
    hasher.update(password.as_bytes());
    let computed = hex::encode(hasher.finalize());
    constant_time_eq(computed.as_bytes(), hash.as_bytes())
}

/// Constant-time byte-slice equality: always compares every byte pair (no
/// early exit on the first mismatch) so execution time does not leak how
/// many leading bytes matched. A length mismatch is checked up front —
/// lengths are not secret here (hash length is a fixed public property of
/// the algorithm, not derived from the password) — but does not
/// short-circuit the subsequent byte-for-byte comparison.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_password_hashing_is_salted() {
        // H3 regression: two hashes of the SAME password must differ (a
        // fresh random salt each time), unlike the old deterministic
        // SHA-512 scheme.
        let password = "test_password_123";
        let hash1 = hash_password(password);
        let hash2 = hash_password(password);

        assert_ne!(
            hash1, hash2,
            "same password must produce different stored hashes (per-password salt)"
        );
        // Argon2 PHC strings start with the algorithm identifier.
        assert!(hash1.starts_with("$argon2id$"));
        assert!(hash2.starts_with("$argon2id$"));
    }

    #[test]
    fn test_password_verification() {
        let password = "test_password_123";
        let hash = hash_password(password);

        assert!(verify_password(password, &hash));
        assert!(!verify_password("wrong_password", &hash));
    }

    #[test]
    fn test_different_passwords_produce_different_hashes() {
        let hash1 = hash_password("password1");
        let hash2 = hash_password("password2");

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_legacy_sha512_hash_still_verifies() {
        // Migration: a hash produced by the pre-Argon2id scheme must still
        // verify correctly so existing accounts are not locked out.
        let password = "legacy_password_123";
        let mut hasher = Sha512::new();
        hasher.update(password.as_bytes());
        let legacy_hash = hex::encode(hasher.finalize());

        assert!(verify_password(password, &legacy_hash));
        assert!(!verify_password("wrong_password", &legacy_hash));
    }

    #[test]
    fn test_needs_rehash() {
        let password = "test_password_123";
        let argon2_hash = hash_password(password);
        assert!(!needs_rehash(&argon2_hash));

        let mut hasher = Sha512::new();
        hasher.update(password.as_bytes());
        let legacy_hash = hex::encode(hasher.finalize());
        assert!(needs_rehash(&legacy_hash));
    }

    #[test]
    fn test_verify_dummy_password_always_false_and_constant_cost() {
        // The dummy verify never "succeeds" (no caller should treat it as
        // a real credential match) but must still run the full Argon2
        // verify path, not short-circuit.
        assert!(!verify_dummy_password("anything"));
        assert!(!verify_dummy_password(""));
    }

    #[test]
    fn test_constant_time_eq() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
    }
}
