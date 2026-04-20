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

    /// Turn a tenant-visible catalog name (`"Person"`, `"LIKES"`,
    /// `"name"`) into the prefixed form actually registered in the
    /// catalog under [`TenantIsolationMode::CatalogPrefix`].
    ///
    /// This is the one place the prefix shape is built during
    /// writes — keep it symmetric with [`Self::strip_prefix`] below.
    ///
    /// [`TenantIsolationMode::CatalogPrefix`]: super::config::TenantIsolationMode::CatalogPrefix
    pub fn catalog_name(&self, logical_name: &str) -> String {
        self.prefix_key(logical_name)
    }

    /// Inverse of [`Self::catalog_name`] — given a stored catalog
    /// name, return the tenant-visible logical name if it belongs
    /// to this namespace, or `None` otherwise.
    ///
    /// Used by discovery endpoints (SHOW LABELS, etc.) that need
    /// to present names to the caller in the shape they registered
    /// them with, stripping the `ns:<id>:` bookkeeping prefix. The
    /// trailing-delimiter check prevents the `ns:abc` prefix from
    /// accidentally matching `ns:abcd`.
    pub fn strip_prefix<'a>(&self, stored: &'a str) -> Option<&'a str> {
        let p = self.prefix();
        stored.strip_prefix(&p)
    }

    /// Global catalog names never carry the `ns:` tag — exposed so
    /// storage-layer scans can cheaply distinguish "legacy or
    /// cross-tenant system entry" from "any tenant's scoped entry"
    /// without reconstructing a namespace first.
    pub fn is_namespaced_catalog_name(s: &str) -> bool {
        s.starts_with("ns:")
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

    #[test]
    fn catalog_name_matches_prefix_key_shape() {
        // Contract: catalog_name and prefix_key produce identical
        // output. They have different names only to document intent
        // at call sites; the wire format MUST stay one thing. If
        // this test ever diverges, rename one of the methods — do
        // not diverge their output.
        let ns = UserNamespace::new("abc").unwrap();
        assert_eq!(ns.catalog_name("Person"), ns.prefix_key("Person"));
        assert_eq!(ns.catalog_name("Person"), "ns:abc:Person");
    }

    #[test]
    fn strip_prefix_roundtrips_owned_names() {
        let ns = UserNamespace::new("abc").unwrap();
        let stored = ns.catalog_name("Person");
        assert_eq!(ns.strip_prefix(&stored), Some("Person"));
    }

    #[test]
    fn strip_prefix_rejects_foreign_namespace() {
        let ns_abc = UserNamespace::new("abc").unwrap();
        let ns_xyz = UserNamespace::new("xyz").unwrap();
        let stored = ns_xyz.catalog_name("Person");
        assert_eq!(ns_abc.strip_prefix(&stored), None);
    }

    #[test]
    fn strip_prefix_rejects_prefix_collision() {
        // `abcd` must not strip as if it were `abc` — same guard
        // `owns()` has, but exercised through the strip path.
        let ns_abc = UserNamespace::new("abc").unwrap();
        let ns_abcd = UserNamespace::new("abcd").unwrap();
        let stored = ns_abcd.catalog_name("Person");
        assert_eq!(ns_abc.strip_prefix(&stored), None);
    }

    #[test]
    fn is_namespaced_catalog_name_detects_the_tag() {
        assert!(UserNamespace::is_namespaced_catalog_name("ns:abc:Person"));
        assert!(!UserNamespace::is_namespaced_catalog_name("Person"));
        // Prefix not followed by delimiter is NOT a namespaced name.
        // A catalog entry called "ns-something" is still global.
        assert!(!UserNamespace::is_namespaced_catalog_name("ns-something"));
    }
}
