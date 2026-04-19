//! Cluster-mode configuration.
//!
//! A [`ClusterConfig`] with `enabled = false` is a total no-op —
//! every middleware / storage check that consults it short-circuits
//! on that flag, so the standalone deployment shape has zero
//! overhead from cluster support being compiled in.

use serde::{Deserialize, Serialize};

/// Default monthly storage budget per tenant, in MiB. Matches the
/// "free tier" number used by the reference HiveHub plans so that
/// local-only deployments behave the same way the control-plane
/// managed ones will once the integration lands.
const DEFAULT_STORAGE_MB: u64 = 1_024;

/// Default per-minute rate limit per tenant. Deliberately generous
/// so it never trips on synthetic load tests; production deployments
/// are expected to override this from the outside.
const DEFAULT_REQUESTS_PER_MINUTE: u32 = 6_000;

/// Default per-hour rate limit per tenant.
const DEFAULT_REQUESTS_PER_HOUR: u32 = 300_000;

/// How the server isolates tenant data at the storage / catalog layer.
///
/// Cluster mode bolts authentication and quota enforcement on top of
/// the existing engine regardless of isolation mode — the enum only
/// controls whether labels / types / property keys get per-tenant
/// prefixes in the catalog.
///
/// Modelled after Vectorizer's `TenantIsolationMode` (`hub/mod.rs`),
/// which has been running in production with HiveHub and validated
/// the "prefix the addressable name" approach over full storage-
/// layer isolation. The difference here is unit: Vectorizer prefixes
/// collection names, Nexus prefixes catalog names.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TenantIsolationMode {
    /// No catalog-level isolation. Tenants still authenticate and
    /// each request still carries a `UserContext`, but all requests
    /// share the same catalog / label index / record store. Useful
    /// in tests, local-only dev, and when isolation is enforced by
    /// an external layer (e.g. one Nexus instance per tenant).
    #[default]
    None,
    /// Every label / relationship-type / property key is registered
    /// into the catalog as `ns:<tenant>:<name>`, so two tenants who
    /// both create a `Person` label get distinct catalog IDs and
    /// therefore distinct label bitmap indexes. Record stores (32 B
    /// node records, 48 B relationship records) do not change —
    /// isolation is implicit because every query reaches storage
    /// via a label ID, and label IDs are tenant-scoped.
    CatalogPrefix,
}

/// Top-level cluster-mode configuration.
///
/// When `enabled` is `false`, every field below is ignored and
/// Nexus behaves exactly as a standalone single-tenant server.
/// The quota fields only take effect when the local provider is
/// the one answering quota questions; a future external provider
/// (HiveHub) will ignore them and consult the control plane.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterConfig {
    /// Master switch. `false` keeps legacy single-tenant semantics
    /// for the whole process.
    pub enabled: bool,

    /// How tenant data is isolated at the catalog / storage layer.
    /// Defaults to [`TenantIsolationMode::None`] even when `enabled`
    /// is flipped — operators must explicitly opt into prefixing,
    /// because existing data will need a one-shot catalog rewrite
    /// if they ever switch modes mid-deployment.
    #[serde(default)]
    pub isolation: TenantIsolationMode,

    /// Default quotas used when the in-process [`QuotaProvider`]
    /// has no explicit record for a tenant.
    ///
    /// [`QuotaProvider`]: super::quota::QuotaProvider
    pub default_quotas: TenantDefaults,
}

impl Default for ClusterConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            isolation: TenantIsolationMode::None,
            default_quotas: TenantDefaults::default(),
        }
    }
}

impl ClusterConfig {
    /// Build a config with cluster mode explicitly on and defaults
    /// otherwise. Convenience for tests and for operators that want
    /// "cluster mode with sane defaults" in one call.
    pub fn enabled_with_defaults() -> Self {
        Self {
            enabled: true,
            isolation: TenantIsolationMode::None,
            default_quotas: TenantDefaults::default(),
        }
    }
}

/// Per-tenant quotas served by the local provider when no external
/// control-plane is configured. These are the numbers a tenant
/// inherits on first contact — subsequent updates (via the quota
/// provider's own mutators or the eventual HiveHub sync) win.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantDefaults {
    /// Monthly storage budget in MiB. Writes that would push the
    /// tenant above this line are rejected with a quota error.
    pub storage_mb: u64,
    /// Soft rate limit, per minute, per tenant.
    pub requests_per_minute: u32,
    /// Soft rate limit, per hour, per tenant.
    pub requests_per_hour: u32,
}

impl Default for TenantDefaults {
    fn default() -> Self {
        Self {
            storage_mb: DEFAULT_STORAGE_MB,
            requests_per_minute: DEFAULT_REQUESTS_PER_MINUTE,
            requests_per_hour: DEFAULT_REQUESTS_PER_HOUR,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_disabled() {
        let cfg = ClusterConfig::default();
        assert!(!cfg.enabled, "cluster mode must be opt-in");
    }

    #[test]
    fn enabled_constructor_flips_master_switch() {
        let cfg = ClusterConfig::enabled_with_defaults();
        assert!(cfg.enabled);
        assert_eq!(cfg.default_quotas.storage_mb, DEFAULT_STORAGE_MB);
    }

    #[test]
    fn defaults_are_non_zero() {
        // Guards against a future refactor that accidentally drops
        // a tenant into a zero-quota state on first contact.
        let d = TenantDefaults::default();
        assert!(d.storage_mb > 0);
        assert!(d.requests_per_minute > 0);
        assert!(d.requests_per_hour > 0);
    }

    #[test]
    fn roundtrips_through_serde() {
        let cfg = ClusterConfig::enabled_with_defaults();
        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: ClusterConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.enabled, cfg.enabled);
        assert_eq!(
            parsed.default_quotas.storage_mb,
            cfg.default_quotas.storage_mb
        );
    }

    #[test]
    fn isolation_defaults_to_none() {
        // Flipping `enabled` alone must NOT silently start
        // rewriting catalog names — that would invalidate every
        // existing piece of data in an upgrade. Isolation is an
        // explicit opt-in even once cluster mode is on.
        assert_eq!(
            ClusterConfig::default().isolation,
            TenantIsolationMode::None
        );
        assert_eq!(
            ClusterConfig::enabled_with_defaults().isolation,
            TenantIsolationMode::None
        );
    }

    #[test]
    fn isolation_mode_round_trips_through_serde() {
        let cfg = ClusterConfig {
            enabled: true,
            isolation: TenantIsolationMode::CatalogPrefix,
            default_quotas: TenantDefaults::default(),
        };
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(
            json.contains("\"catalogprefix\""),
            "serde rename_all=lowercase must flatten the variant: {json}"
        );
        let parsed: ClusterConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.isolation, TenantIsolationMode::CatalogPrefix);
    }

    #[test]
    fn isolation_mode_deserialises_missing_field_as_none() {
        // Config files written before this field existed must keep
        // working without a migration — `#[serde(default)]` is what
        // lets that happen. Regression guard.
        let json = r#"{"enabled": true, "default_quotas": {"storage_mb": 1, "requests_per_minute": 1, "requests_per_hour": 1}}"#;
        let parsed: ClusterConfig = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.isolation, TenantIsolationMode::None);
    }
}
