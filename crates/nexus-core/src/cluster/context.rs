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

use serde::{Deserialize, Serialize};

use super::namespace::UserNamespace;

/// Structured rejection returned by [`UserContext::require_may_call`].
///
/// Lives here (not in `error::Error`) because the HTTP / MCP layer
/// translates it into a 403 response body directly — the variant
/// never flows through the core `Result<T>` chain, so adding it to
/// the main error enum would just cost us a roundtrip through
/// `From` impls every time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FunctionAccessError {
    /// Canonical function name the caller tried to invoke.
    pub function: String,
    /// Stable machine-readable code. Kept in sync with the MCP /
    /// REST error-body contract so SDKs can match on it without
    /// string-matching the `message` field.
    pub code: String,
    /// Human-readable explanation safe to return to the caller.
    /// Does NOT include the namespace id or api-key id — those are
    /// internal to the server and would leak otherwise.
    pub message: String,
}

impl FunctionAccessError {
    /// Error code used when an API key's allow-list does not
    /// include the requested function. Stable wire constant —
    /// SDKs match on this, never on the `message` text.
    pub const CODE: &'static str = "FUNCTION_NOT_ALLOWED";

    fn forbidden(function: impl Into<String>) -> Self {
        let function = function.into();
        Self {
            message: format!("function '{function}' is not in this API key's allow-list"),
            function,
            code: Self::CODE.into(),
        }
    }
}

impl std::fmt::Display for FunctionAccessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for FunctionAccessError {}

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

    /// Result-returning twin of [`Self::may_call`]. Prefer this in
    /// MCP / RPC handlers: `ctx.require_may_call("cypher.execute")?`
    /// bubbles the 403 path up without a hand-rolled `if !may_call`
    /// branch at every call site.
    pub fn require_may_call(&self, name: &str) -> Result<(), FunctionAccessError> {
        if self.may_call(name) {
            Ok(())
        } else {
            Err(FunctionAccessError::forbidden(name))
        }
    }

    /// Filter an iterator of canonical function names down to those
    /// this context may actually invoke. Useful when advertising
    /// available tools on MCP / discovery endpoints so the client
    /// only ever sees callable operations. Unrestricted contexts
    /// get the full list back unchanged.
    pub fn filter_callable<'a, I, S>(&self, names: I) -> Vec<String>
    where
        I: IntoIterator<Item = &'a S>,
        S: AsRef<str> + 'a,
    {
        names
            .into_iter()
            .filter_map(|n| {
                let s = n.as_ref();
                if self.may_call(s) {
                    Some(s.to_string())
                } else {
                    None
                }
            })
            .collect()
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

    #[test]
    fn require_may_call_is_ok_when_allowed() {
        let ctx = UserContext::unrestricted(ns(), "key-1");
        ctx.require_may_call("anything")
            .expect("unrestricted must pass");

        let scoped = UserContext::restricted(ns(), "key-1", ["cypher.execute".into()]);
        scoped
            .require_may_call("cypher.execute")
            .expect("listed function must pass");
    }

    #[test]
    fn require_may_call_returns_structured_error() {
        // Contract: the error body shows the offending function
        // and the stable CODE constant. SDK decoders match on
        // `code`, never on `message` — change the message all you
        // like, do not break the code without bumping SDKs.
        let ctx = UserContext::restricted(ns(), "key-1", ["cypher.execute".into()]);
        let err = ctx
            .require_may_call("nexus.admin.drop_database")
            .expect_err("unlisted function must reject");

        assert_eq!(err.function, "nexus.admin.drop_database");
        assert_eq!(err.code, FunctionAccessError::CODE);
        assert!(
            err.message.contains("not in this API key's allow-list"),
            "message: {}",
            err.message
        );
    }

    #[test]
    fn filter_callable_trims_to_allow_list() {
        let ctx =
            UserContext::restricted(ns(), "key-1", ["cypher.execute".into(), "kv.get".into()]);
        let tools = vec![
            "cypher.execute",
            "kv.get",
            "kv.set",
            "nexus.admin.drop_database",
        ];
        let visible = ctx.filter_callable(&tools);
        assert_eq!(
            visible,
            vec!["cypher.execute".to_string(), "kv.get".to_string()]
        );
    }

    #[test]
    fn filter_callable_passes_through_on_unrestricted() {
        let ctx = UserContext::unrestricted(ns(), "key-1");
        let tools = vec!["a", "b", "c"];
        let visible = ctx.filter_callable(&tools);
        assert_eq!(visible, vec!["a", "b", "c"]);
    }

    #[test]
    fn function_access_error_round_trips_through_serde() {
        // HTTP handlers serialise this into 403 bodies; the shape
        // needs to survive `serde_json` untouched so SDKs can lean
        // on their generated types.
        let err = FunctionAccessError {
            function: "nexus.admin.drop_database".into(),
            code: FunctionAccessError::CODE.into(),
            message: "x".into(),
        };
        let json = serde_json::to_string(&err).unwrap();
        let parsed: FunctionAccessError = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, err);
    }
}
