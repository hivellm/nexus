//! User namespace primitive for cluster mode.
//!
//! A `UserNamespace` is a validated identifier that every read/write
//! is scoped to when cluster mode is active. The type's job is to:
//!
//! * Turn an opaque `user_id` (from an API key, a JWT, …) into a
//!   canonical representation that is cheap to compare / clone / hash.
//! * Reject identifiers that would collide with the storage-layer
//!   delimiter (`:`) or produce ambiguous prefixes.
//! * Provide a deterministic prefix factory so the storage and index
//!   layers can build namespaced keys without re-implementing the
//!   escaping rules in each call site.
//!
//! Standalone-mode data is NEVER exposed as a namespace — callers
//! pass `Option<UserNamespace>` and `None` means "global scope".
//! This keeps the type total: constructing a `UserNamespace` always
//! produces a non-empty prefix.

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::Error;

/// Character forbidden inside a raw user id because the storage
/// layer uses it as the namespace / key separator. Keeping this
/// explicit (rather than inlining `':'` in each check) makes the
/// invariant obvious and easy to audit.
const NAMESPACE_DELIMITER: char = ':';

/// Prefix stamped on every namespaced key. Distinguishes user-owned
/// keys from catalog metadata / system keys that must remain global
/// even in cluster mode.
const NAMESPACE_TAG: &str = "ns";

/// Validated, opaque per-user namespace identifier.
///
/// Construct via [`UserNamespace::new`]; the wrapped string is
/// guaranteed to be non-empty, free of the namespace delimiter, and
/// within [`UserNamespace::MAX_LEN`] bytes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UserNamespace(String);

impl UserNamespace {
    /// Hard upper bound on the raw user-id length. Chosen so that
    /// the resulting prefix (`ns:<id>:`) still fits comfortably in
    /// an LMDB key along with a meaningful trailing user key.
    pub const MAX_LEN: usize = 128;

    /// Build a namespace from a user identifier.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidInput`] if the input is empty,
    /// exceeds [`Self::MAX_LEN`] bytes, contains the reserved
    /// delimiter `':'`, or contains any control character. These
    /// rules are strict on purpose — a bad id silently accepted
    /// here becomes a cross-tenant data-leak vector once it reaches
    /// the storage layer.
    pub fn new(user_id: impl Into<String>) -> Result<Self, Error> {
        let raw = user_id.into();
        if raw.is_empty() {
            return Err(Error::invalid_input("user namespace id must not be empty"));
        }
        if raw.len() > Self::MAX_LEN {
            return Err(Error::invalid_input(format!(
                "user namespace id exceeds {} bytes (got {})",
                Self::MAX_LEN,
                raw.len()
            )));
        }
        if let Some(bad) = raw
            .chars()
            .find(|c| *c == NAMESPACE_DELIMITER || c.is_control())
        {
            return Err(Error::invalid_input(format!(
                "user namespace id contains forbidden character {bad:?}"
            )));
        }
        Ok(Self(raw))
    }

    /// Raw id without any prefix. Useful for logging / telemetry;
    /// do NOT use this as a storage key on its own.
    pub fn as_id(&self) -> &str {
        &self.0
    }

    /// Stable prefix stamped onto every storage key owned by this
    /// namespace, e.g. `"ns:abc123:"`.
    ///
    /// Allocates a fresh `String` — fine at key-construction time,
    /// but hot paths should cache it via [`Self::prefix_bytes`] or
    /// pre-build keys once per query.
    pub fn prefix(&self) -> String {
        format!(
            "{tag}{sep}{id}{sep}",
            tag = NAMESPACE_TAG,
            sep = NAMESPACE_DELIMITER,
            id = self.0,
        )
    }

    /// Build a namespaced storage key by prepending this namespace's
    /// prefix to `key`. Callers that build many keys back-to-back
    /// should prefer caching [`Self::prefix`] and pushing directly.
    pub fn prefix_key(&self, key: &str) -> String {
        let mut out = self.prefix();
        out.push_str(key);
        out
    }

    /// Whether `storage_key` belongs to this namespace. Used by
    /// scans / reverse-lookups to enforce isolation at read time
    /// when a foreign key is encountered unexpectedly (defence in
    /// depth — the writer path should already have prefixed it).
    pub fn owns(&self, storage_key: &str) -> bool {
        storage_key.starts_with(&self.prefix())
    }
}

impl fmt::Display for UserNamespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_id() {
        let err = UserNamespace::new("").unwrap_err();
        assert!(err.to_string().contains("must not be empty"), "got: {err}");
    }

    #[test]
    fn rejects_delimiter() {
        let err = UserNamespace::new("has:colon").unwrap_err();
        assert!(
            err.to_string().contains("forbidden character"),
            "got: {err}"
        );
    }

    #[test]
    fn rejects_control_char() {
        let err = UserNamespace::new("has\tnull").unwrap_err();
        assert!(
            err.to_string().contains("forbidden character"),
            "got: {err}"
        );
    }

    #[test]
    fn rejects_oversized() {
        let big = "a".repeat(UserNamespace::MAX_LEN + 1);
        let err = UserNamespace::new(big).unwrap_err();
        assert!(err.to_string().contains("exceeds"), "got: {err}");
    }

    #[test]
    fn accepts_uuid_shaped_ids() {
        let ns = UserNamespace::new("550e8400-e29b-41d4-a716-446655440000").unwrap();
        assert_eq!(ns.as_id(), "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn prefix_shape_is_stable() {
        let ns = UserNamespace::new("abc").unwrap();
        assert_eq!(ns.prefix(), "ns:abc:");
        assert_eq!(ns.prefix_key("node/42"), "ns:abc:node/42");
    }

    #[test]
    fn owns_recognises_own_keys() {
        let ns = UserNamespace::new("abc").unwrap();
        assert!(ns.owns("ns:abc:anything"));
        assert!(!ns.owns("ns:xyz:anything"));
        assert!(!ns.owns("node/42"));
    }

    #[test]
    fn owns_rejects_prefix_collision() {
        // `abcd` must not be a prefix of the `abc` namespace — the
        // trailing delimiter in `prefix()` is what prevents the
        // classic `/foo` vs `/foobar` authorisation bug.
        let ns = UserNamespace::new("abc").unwrap();
        assert!(!ns.owns("ns:abcd:anything"));
    }

    #[test]
    fn roundtrips_through_serde() {
        let ns = UserNamespace::new("abc").unwrap();
        let json = serde_json::to_string(&ns).unwrap();
        let parsed: UserNamespace = serde_json::from_str(&json).unwrap();
        assert_eq!(ns, parsed);
    }
}
