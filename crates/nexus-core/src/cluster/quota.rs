//! Quota provider abstraction.
//!
//! The server asks a [`QuotaProvider`] three questions for every
//! request in cluster mode:
//!
//! 1. *Can this tenant start another request right now?* —
//!    [`QuotaProvider::check_rate`]. Answered in-process for
//!    throughput; eventual consistency with a control plane is
//!    acceptable.
//! 2. *Does this write fit inside the storage budget?* —
//!    [`QuotaProvider::check_storage`]. Must be strongly consistent
//!    with whatever the provider considers authoritative, because
//!    accepting a write past the quota is a billable event.
//! 3. *Record that this much work actually happened.* —
//!    [`QuotaProvider::record_usage`]. Best-effort: the provider
//!    is free to batch / aggregate, but must not drop events
//!    silently under normal operation.
//!
//! Keeping all three behind a trait lets the server run against
//! [`LocalQuotaProvider`] in tests and standalone deployments, and
//! against a future `HiveHubQuotaProvider` in multi-tenant SaaS
//! deployments, without any of the hot-path code knowing which.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use super::config::TenantDefaults;
use super::namespace::UserNamespace;

/// Outcome of a quota check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuotaDecision {
    /// Operation may proceed. `remaining` is informational — callers
    /// put it in `X-RateLimit-Remaining` or similar.
    Allow { remaining: Option<u64> },
    /// Operation is over quota. `reason` is a human-readable
    /// explanation safe to return to the caller (no PII). `retry_after`
    /// is populated for rate-limit denials so the server can set a
    /// `Retry-After` header.
    Deny {
        reason: String,
        retry_after: Option<std::time::Duration>,
    },
}

impl QuotaDecision {
    /// Whether this decision allows the operation.
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allow { .. })
    }
}

/// Work a tenant just performed, for the provider to bill / track.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UsageDelta {
    /// New bytes written to storage (positive only — deletes are
    /// eventually reconciled via a full-scan, not via negative deltas
    /// here, to keep the arithmetic monotonic and race-free).
    pub storage_bytes: u64,
    /// Number of chargeable requests (reads, writes, Cypher ops).
    pub requests: u64,
}

/// Snapshot of a tenant's current quota / usage state. Used by
/// `/stats`-style endpoints and by admin tooling; the hot path uses
/// [`QuotaProvider::check_rate`] / [`QuotaProvider::check_storage`]
/// directly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaSnapshot {
    pub storage_bytes_used: u64,
    pub storage_bytes_limit: u64,
    pub requests_this_minute: u32,
    pub requests_per_minute_limit: u32,
    pub requests_this_hour: u32,
    pub requests_per_hour_limit: u32,
}

/// Abstract interface to a quota backend.
///
/// Implementations MUST be thread-safe (`Send + Sync`) because they
/// are shared across every request-handling task.
///
/// `async_trait` is deliberately NOT used — quota checks live on
/// the hot path and most impls will answer synchronously from local
/// state. The future HiveHub-backed provider will do its own
/// batched async work behind a sync façade (periodic sync task +
/// in-memory cache) to keep this trait cheap.
pub trait QuotaProvider: Send + Sync {
    /// Check whether `ns` may perform another chargeable request.
    fn check_rate(&self, ns: &UserNamespace) -> QuotaDecision;

    /// Check whether `ns` may grow its storage by `bytes`.
    fn check_storage(&self, ns: &UserNamespace, bytes: u64) -> QuotaDecision;

    /// Record `delta` against `ns`'s usage totals. Must not fail
    /// synchronously — a provider that cannot immediately commit
    /// (e.g. network hiccup on a remote backend) should buffer and
    /// retry out of band.
    fn record_usage(&self, ns: &UserNamespace, delta: UsageDelta);

    /// Return the current snapshot for `ns`, or `None` if the tenant
    /// is unknown to this provider.
    fn snapshot(&self, ns: &UserNamespace) -> Option<QuotaSnapshot>;
}

/// In-process implementation used by standalone / test deployments.
///
/// State is held in a single `RwLock<HashMap>` — fine for the
/// modest tenant counts this provider is intended for. A deployment
/// large enough to feel the lock contention should be on the
/// HiveHub-backed provider anyway.
#[derive(Debug, Default)]
pub struct LocalQuotaProvider {
    defaults: TenantDefaults,
    state: RwLock<HashMap<UserNamespace, TenantState>>,
}

#[derive(Debug)]
struct TenantState {
    storage_bytes_used: u64,
    storage_bytes_limit: u64,
    per_minute_limit: u32,
    per_hour_limit: u32,
    minute_window: RateWindow,
    hour_window: RateWindow,
}

/// Fixed-size sliding window counter. "Sliding" is approximated with
/// a reset-on-expiry: simpler than a ring buffer, accurate enough
/// for rate-limiting where the goal is just to cap long-run abuse.
#[derive(Debug)]
struct RateWindow {
    limit: u32,
    duration: std::time::Duration,
    opened_at: Instant,
    count: u32,
}

impl RateWindow {
    fn new(limit: u32, duration: std::time::Duration) -> Self {
        Self {
            limit,
            duration,
            opened_at: Instant::now(),
            count: 0,
        }
    }

    /// Try to consume one slot. Returns `None` on success (with the
    /// new remaining count) or `Some(retry_after)` on denial.
    fn try_consume(&mut self) -> Result<u32, std::time::Duration> {
        let now = Instant::now();
        if now.duration_since(self.opened_at) >= self.duration {
            self.opened_at = now;
            self.count = 0;
        }
        if self.count >= self.limit {
            // Time left in the current window — the caller's hint
            // for Retry-After.
            let elapsed = now.duration_since(self.opened_at);
            let remaining = self.duration.saturating_sub(elapsed);
            return Err(remaining);
        }
        self.count += 1;
        Ok(self.limit.saturating_sub(self.count))
    }

    fn current(&self) -> u32 {
        // Callers using this for reporting don't care about window
        // expiry — they want the raw counter so the snapshot is
        // consistent with the one check_rate would produce.
        self.count
    }
}

impl LocalQuotaProvider {
    /// Build a new provider seeded with `defaults` as the baseline
    /// for any tenant that has not been registered explicitly.
    pub fn new(defaults: TenantDefaults) -> Arc<Self> {
        Arc::new(Self {
            defaults,
            state: RwLock::new(HashMap::new()),
        })
    }

    /// Pre-register `ns` with specific quota overrides. Useful in
    /// tests and for ops tooling that wants tighter-than-default
    /// limits for a known tenant.
    pub fn register_tenant(&self, ns: UserNamespace, quotas: TenantDefaults) {
        self.state.write().insert(ns, TenantState::new(&quotas));
    }

    fn ensure_tenant<'a>(
        &'a self,
        ns: &UserNamespace,
        map: &'a mut HashMap<UserNamespace, TenantState>,
    ) -> &'a mut TenantState {
        if !map.contains_key(ns) {
            map.insert(ns.clone(), TenantState::new(&self.defaults));
        }
        // SAFETY: just inserted above if absent.
        map.get_mut(ns)
            .expect("tenant state must exist after ensure_tenant insertion")
    }
}

impl TenantState {
    fn new(q: &TenantDefaults) -> Self {
        Self {
            storage_bytes_used: 0,
            storage_bytes_limit: q.storage_mb.saturating_mul(1024 * 1024),
            per_minute_limit: q.requests_per_minute,
            per_hour_limit: q.requests_per_hour,
            minute_window: RateWindow::new(
                q.requests_per_minute,
                std::time::Duration::from_secs(60),
            ),
            hour_window: RateWindow::new(
                q.requests_per_hour,
                std::time::Duration::from_secs(60 * 60),
            ),
        }
    }
}

impl QuotaProvider for LocalQuotaProvider {
    fn check_rate(&self, ns: &UserNamespace) -> QuotaDecision {
        let mut guard = self.state.write();
        let tenant = self.ensure_tenant(ns, &mut guard);

        // The per-minute window is the stricter of the two for any
        // realistic quota pair, so we check it first and only fall
        // through to the per-hour check on success. This preserves
        // the Retry-After hint from whichever window actually denied.
        match tenant.minute_window.try_consume() {
            Err(retry) => QuotaDecision::Deny {
                reason: "per-minute rate limit exceeded".into(),
                retry_after: Some(retry),
            },
            Ok(minute_remaining) => match tenant.hour_window.try_consume() {
                Err(retry) => QuotaDecision::Deny {
                    reason: "per-hour rate limit exceeded".into(),
                    retry_after: Some(retry),
                },
                Ok(hour_remaining) => QuotaDecision::Allow {
                    remaining: Some(minute_remaining.min(hour_remaining) as u64),
                },
            },
        }
    }

    fn check_storage(&self, ns: &UserNamespace, bytes: u64) -> QuotaDecision {
        let mut guard = self.state.write();
        let tenant = self.ensure_tenant(ns, &mut guard);

        let projected = tenant.storage_bytes_used.saturating_add(bytes);
        if projected > tenant.storage_bytes_limit {
            return QuotaDecision::Deny {
                reason: format!(
                    "storage quota exceeded ({} B would push tenant past {} B limit)",
                    bytes, tenant.storage_bytes_limit
                ),
                retry_after: None,
            };
        }
        QuotaDecision::Allow {
            remaining: Some(tenant.storage_bytes_limit.saturating_sub(projected)),
        }
    }

    fn record_usage(&self, ns: &UserNamespace, delta: UsageDelta) {
        let mut guard = self.state.write();
        let tenant = self.ensure_tenant(ns, &mut guard);
        tenant.storage_bytes_used = tenant
            .storage_bytes_used
            .saturating_add(delta.storage_bytes);
        // Request counts live inside the rate windows — they're
        // incremented by check_rate, not by record_usage. We leave
        // delta.requests as a future-compat hook for the HiveHub
        // provider which bills on every request.
        let _ = delta.requests;
    }

    fn snapshot(&self, ns: &UserNamespace) -> Option<QuotaSnapshot> {
        let guard = self.state.read();
        let tenant = guard.get(ns)?;
        Some(QuotaSnapshot {
            storage_bytes_used: tenant.storage_bytes_used,
            storage_bytes_limit: tenant.storage_bytes_limit,
            requests_this_minute: tenant.minute_window.current(),
            requests_per_minute_limit: tenant.per_minute_limit,
            requests_this_hour: tenant.hour_window.current(),
            requests_per_hour_limit: tenant.per_hour_limit,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ns(id: &str) -> UserNamespace {
        UserNamespace::new(id).unwrap()
    }

    fn tight_defaults() -> TenantDefaults {
        TenantDefaults {
            storage_mb: 1, // 1 MiB budget
            requests_per_minute: 3,
            requests_per_hour: 10,
        }
    }

    #[test]
    fn rate_allows_until_minute_limit() {
        let p = LocalQuotaProvider::new(tight_defaults());
        let alice = ns("alice");
        for _ in 0..3 {
            assert!(p.check_rate(&alice).is_allowed());
        }
        match p.check_rate(&alice) {
            QuotaDecision::Deny {
                reason,
                retry_after,
            } => {
                assert!(reason.contains("per-minute"), "reason: {reason}");
                assert!(retry_after.is_some());
            }
            other => panic!("expected deny, got {other:?}"),
        }
    }

    #[test]
    fn rate_windows_are_per_tenant() {
        let p = LocalQuotaProvider::new(tight_defaults());
        let alice = ns("alice");
        let bob = ns("bob");
        for _ in 0..3 {
            assert!(p.check_rate(&alice).is_allowed());
        }
        // Alice is tapped; Bob is untouched.
        assert!(!p.check_rate(&alice).is_allowed());
        assert!(p.check_rate(&bob).is_allowed());
    }

    #[test]
    fn storage_check_blocks_at_the_boundary() {
        let p = LocalQuotaProvider::new(tight_defaults());
        let alice = ns("alice");

        // 512 KiB leaves 512 KiB remaining — still allowed.
        assert!(p.check_storage(&alice, 512 * 1024).is_allowed());
        p.record_usage(
            &alice,
            UsageDelta {
                storage_bytes: 512 * 1024,
                ..Default::default()
            },
        );

        // 600 KiB more would push past the 1 MiB cap → deny.
        match p.check_storage(&alice, 600 * 1024) {
            QuotaDecision::Deny { reason, .. } => {
                assert!(
                    reason.contains("storage quota exceeded"),
                    "reason: {reason}"
                );
            }
            other => panic!("expected deny, got {other:?}"),
        }
    }

    #[test]
    fn snapshot_reports_committed_usage() {
        let p = LocalQuotaProvider::new(tight_defaults());
        let alice = ns("alice");

        // Snapshot-before-activity is None so callers can tell the
        // difference between "tenant seen, at zero" and "tenant
        // never seen".
        assert!(p.snapshot(&alice).is_none());

        assert!(p.check_rate(&alice).is_allowed());
        p.record_usage(
            &alice,
            UsageDelta {
                storage_bytes: 123,
                requests: 1,
            },
        );

        let snap = p.snapshot(&alice).expect("tenant is now known");
        assert_eq!(snap.storage_bytes_used, 123);
        assert_eq!(snap.requests_this_minute, 1);
        assert_eq!(snap.requests_per_minute_limit, 3);
    }

    #[test]
    fn explicit_registration_overrides_defaults() {
        let p = LocalQuotaProvider::new(tight_defaults());
        let alice = ns("alice");
        p.register_tenant(
            alice.clone(),
            TenantDefaults {
                storage_mb: 10,
                requests_per_minute: 1,
                requests_per_hour: 10,
            },
        );
        // First request allowed, second denied — per-minute limit
        // is now 1, not 3.
        assert!(p.check_rate(&alice).is_allowed());
        assert!(!p.check_rate(&alice).is_allowed());
    }
}
