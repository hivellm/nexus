//! Per-request identity for cluster mode.
//!
//! [`UserContext`] is the one piece of authenticated state lower
//! layers (storage, query executor, quota middleware) get to see.
//! Anything richer than a namespace — plan names, billing flags,
//! audit metadata — stays with the auth / HTTP layer that built
//! the context. Keeping this type small is deliberate: it crosses
//! module boundaries and is cloned on every request, so growing it
//! has a real cost.
//!
//! A `UserContext` is constructed once per request, right after
//! authentication succeeds, and handed downstream by value. It is
//! intentionally cheap to clone (one `String` + one `Arc`-sized
//! set of allowed functions).

use std::collections::BTreeSet;
use std::sync::Arc;

use super::namespace::UserNamespace;

/// Identity for a single authenticated request in cluster mode.
///
/// Standalone mode never constructs one of these — callers pass
/// `Option<UserContext>` where `None` signals "no tenant scoping"
/// so standalone-mode code paths stay untouched.
#[derive(Debug, Clone)]
pub struct UserContext {
    namespace: UserNamespace,
    api_key_id: String,
    /// `None` → full access (admin / unrestricted key).
    /// `Some(set)` → only these MCP / RPC function names are callable.
    /// The set is behind an `Arc` so cloning a `UserContext` is O(1)
    /// regardless of how long the allow-list gets.
    allowed_functions: Option<Arc<BTreeSet<String>>>,
}

impl UserContext {
    /// Build a context with full (unrestricted) function access.
    /// Use this for admin keys and during migration when no
    /// allow-list has been assigned yet.
    pub fn unrestricted(namespace: UserNamespace, api_key_id: impl Into<String>) -> Self {
        Self {
            namespace,
            api_key_id: api_key_id.into(),
            allowed_functions: None,
        }
    }

    /// Build a context restricted to a specific set of MCP / RPC
    /// function names. Empty set means "no functions at all" — which
    /// is different from [`Self::unrestricted`] and is used, for
    /// example, for keys that may only call `/health` endpoints.
    pub fn restricted(
        namespace: UserNamespace,
        api_key_id: impl Into<String>,
        allowed: impl IntoIterator<Item = String>,
    ) -> Self {
        let set: BTreeSet<String> = allowed.into_iter().collect();
        Self {
            namespace,
            api_key_id: api_key_id.into(),
            allowed_functions: Some(Arc::new(set)),
        }
    }

    /// Namespace this request is scoped to. The one piece every
    /// downstream layer actually cares about.
    pub fn namespace(&self) -> &UserNamespace {
        &self.namespace
    }

    /// Stable identifier of the API key that authenticated the
    /// request. Used for audit logging and usage reporting — never
    /// for authorisation decisions (those go through the permission
    /// check, not through key identity).
    pub fn api_key_id(&self) -> &str {
        &self.api_key_id
    }

    /// Whether this context may invoke a function named `name`.
    ///
    /// * Unrestricted contexts say yes to everything.
    /// * Restricted contexts say yes iff `name` is in the allow-list.
    ///
    /// Comparison is case-sensitive and exact — function registries
    /// are expected to emit canonical names (e.g. `"cypher.execute"`,
    /// not `"Cypher.Execute"`).
    pub fn may_call(&self, name: &str) -> bool {
        match self.allowed_functions.as_deref() {
            None => true,
            Some(set) => set.contains(name),
        }
    }

    /// Iterate the explicit allow-list, or `None` if the context is
    /// unrestricted. Exposed so the HTTP layer can filter the list
    /// of advertised MCP tools before handing it to the client —
    /// callers discovering only functions they can invoke means one
    /// fewer round-trip for a "you can't call that" error.
    pub fn allowed_functions(&self) -> Option<&BTreeSet<String>> {
        self.allowed_functions.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ns() -> UserNamespace {
        UserNamespace::new("alice").unwrap()
    }

    #[test]
    fn unrestricted_allows_every_function() {
        let ctx = UserContext::unrestricted(ns(), "key-1");
        assert!(ctx.may_call("anything"));
        assert!(ctx.may_call("nexus.admin.drop_database"));
        assert!(ctx.allowed_functions().is_none());
    }

    #[test]
    fn restricted_allows_only_listed() {
        let ctx = UserContext::restricted(
            ns(),
            "key-1",
            ["cypher.execute".to_string(), "kv.get".to_string()],
        );
        assert!(ctx.may_call("cypher.execute"));
        assert!(ctx.may_call("kv.get"));
        assert!(!ctx.may_call("nexus.admin.drop_database"));
    }

    #[test]
    fn empty_allow_list_denies_everything() {
        // Different from unrestricted: an empty explicit list is a
        // deliberate "may call nothing" configuration (e.g. for a
        // health-probe-only key).
        let ctx = UserContext::restricted(ns(), "key-1", std::iter::empty());
        assert!(!ctx.may_call("cypher.execute"));
        assert!(ctx.allowed_functions().is_some_and(|s| s.is_empty()));
    }

    #[test]
    fn case_sensitive_matching() {
        let ctx = UserContext::restricted(ns(), "key-1", ["cypher.execute".to_string()]);
        assert!(ctx.may_call("cypher.execute"));
        assert!(!ctx.may_call("Cypher.Execute"));
    }

    #[test]
    fn cloning_is_cheap_regardless_of_allow_list_size() {
        // Correctness check, not a perf assertion — the Arc means
        // clone does not re-walk the set. We just verify behaviour
        // is preserved across a clone of a large context.
        let funcs: Vec<String> = (0..1000).map(|i| format!("fn_{i}")).collect();
        let ctx = UserContext::restricted(ns(), "key-1", funcs);
        let cloned = ctx.clone();
        assert!(cloned.may_call("fn_0"));
        assert!(cloned.may_call("fn_999"));
        assert!(!cloned.may_call("fn_1000"));
        assert_eq!(cloned.namespace(), ctx.namespace());
    }

    #[test]
    fn namespace_and_key_id_are_preserved() {
        let ctx = UserContext::unrestricted(ns(), "key-abc");
        assert_eq!(ctx.namespace().as_id(), "alice");
        assert_eq!(ctx.api_key_id(), "key-abc");
    }
}
