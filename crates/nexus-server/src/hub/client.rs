//! [`HubClient`] — opinionated wrapper around the Hub SDK
//! ([`hivehub-internal-sdk`]).
//!
//! Responsibilities:
//!
//! 1. **Lifetime**: hold the SDK client behind an `Arc` so request
//!    handlers can clone the handle without re-doing the TLS / HTTP
//!    client warmup.
//! 2. **Configuration**: read `HIVEHUB_CLOUD_SERVICE_API_KEY` /
//!    `HIVEHUB_CLOUD_BASE_URL` from the environment via the SDK's
//!    own `from_env`, with an explicit opt-in switch
//!    (`HIVEHUB_DISABLED`) so local / single-tenant deployments
//!    skip Hub entirely.
//! 3. **Health check**: a `ping()` round-trip against the Hub's
//!    configured base URL so startup can fail fast on a wrong
//!    URL or revoked key.
//!
//! Higher-level features (auth middleware, database routing, credit
//! consumption) live in their own modules and call the methods this
//! struct exposes — no other code in `nexus-server` should
//! `use hivehub_internal_sdk::*` directly.

use std::sync::Arc;
use std::time::Duration;

use hivehub_internal_sdk::HiveHubCloudClient;
use hivehub_internal_sdk::error::HiveHubCloudError;
use thiserror::Error;
use tracing::{info, warn};

/// Wrapper error surfaced to the rest of the server. Keeps the SDK
/// error type internal so future SDK upgrades (e.g. adding new
/// variants) don't ripple through call sites.
#[derive(Debug, Error)]
pub enum HubClientError {
    /// `HIVEHUB_CLOUD_SERVICE_API_KEY` was unset while
    /// `HIVEHUB_API_URL` told us to enable Hub integration.
    #[error(
        "Hub integration enabled but missing service API key (set HIVEHUB_CLOUD_SERVICE_API_KEY)"
    )]
    MissingApiKey,
    /// Underlying SDK / network error.
    #[error("Hub SDK error: {0}")]
    Sdk(String),
}

impl From<HiveHubCloudError> for HubClientError {
    fn from(e: HiveHubCloudError) -> Self {
        HubClientError::Sdk(e.to_string())
    }
}

/// Reported by the `/health` route + startup probe.
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HubHealthStatus {
    /// Hub integration is wired and the configured URL responds.
    Connected,
    /// Hub integration is configured but the last probe failed.
    /// `nexus-server` keeps running; per-request handlers degrade
    /// gracefully or return 503 to the user as appropriate.
    Disconnected,
    /// `HIVEHUB_DISABLED=1` or no Hub URL configured. The standalone
    /// path is the documented mode for single-tenant deployments.
    Disabled,
}

/// Opinionated Hub client handle. Cloneable and `Send + Sync` so it
/// can live inside Axum's `State<…>`.
#[derive(Clone)]
pub struct HubClient {
    inner: Arc<HiveHubCloudClient>,
    base_url: String,
}

impl HubClient {
    /// Build a client from `HIVEHUB_*` environment variables. Returns
    /// `Ok(None)` when integration is opted out of (no
    /// `HIVEHUB_CLOUD_BASE_URL` configured, or
    /// `HIVEHUB_DISABLED=1`).
    pub fn from_env() -> Result<Option<Self>, HubClientError> {
        if std::env::var("HIVEHUB_DISABLED")
            .map(|v| v == "1")
            .unwrap_or(false)
        {
            info!("Hub integration disabled by HIVEHUB_DISABLED=1");
            return Ok(None);
        }
        if std::env::var("HIVEHUB_CLOUD_BASE_URL").is_err() {
            // No Hub URL → standalone mode. Surface as info so ops
            // can grep startup logs and see whether multi-tenant
            // mode was active.
            info!("Hub integration disabled (HIVEHUB_CLOUD_BASE_URL unset)");
            return Ok(None);
        }
        if std::env::var("HIVEHUB_CLOUD_SERVICE_API_KEY").is_err() {
            return Err(HubClientError::MissingApiKey);
        }

        let sdk = HiveHubCloudClient::from_env()?;
        let base_url = std::env::var("HIVEHUB_CLOUD_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:12000".to_string());

        info!(
            target = "nexus_server::hub",
            base_url = %base_url,
            "Hub integration enabled"
        );
        Ok(Some(Self {
            inner: Arc::new(sdk),
            base_url,
        }))
    }

    /// Build a client from explicit credentials. Used by tests and
    /// any caller that wants to bypass env discovery.
    pub fn new(api_key: String, base_url: String) -> Result<Self, HubClientError> {
        let sdk = HiveHubCloudClient::new(api_key, base_url.clone())?;
        Ok(Self {
            inner: Arc::new(sdk),
            base_url,
        })
    }

    /// Borrow the underlying SDK handle. Higher-level modules that
    /// implement §3-§7 of the task call this to get the SDK's
    /// per-service clients (`nexus()`, `access_keys()`).
    pub fn sdk(&self) -> &HiveHubCloudClient {
        &self.inner
    }

    /// Hub base URL as configured at construction time. Surfaced in
    /// the `/health` response so ops can confirm the cluster is
    /// pointed at the right control plane.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Liveness probe. Tries a lightweight HTTP HEAD against the
    /// Hub's base URL with a 3-second timeout. Returns
    /// [`HubHealthStatus::Connected`] on any 2xx/3xx/4xx response —
    /// receiving *anything* from the Hub is sufficient for "the URL
    /// resolves and the service is up". 5xx and network errors
    /// register as [`HubHealthStatus::Disconnected`].
    pub async fn ping(&self) -> HubHealthStatus {
        let client = match reqwest::Client::builder()
            .timeout(Duration::from_secs(3))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                warn!(target: "nexus_server::hub", err = %e, "Hub probe client failed to build");
                return HubHealthStatus::Disconnected;
            }
        };
        match client.head(&self.base_url).send().await {
            Ok(resp) if !resp.status().is_server_error() => HubHealthStatus::Connected,
            Ok(resp) => {
                warn!(
                    target: "nexus_server::hub",
                    status = %resp.status(),
                    "Hub probe returned 5xx"
                );
                HubHealthStatus::Disconnected
            }
            Err(e) => {
                warn!(target: "nexus_server::hub", err = %e, "Hub probe failed");
                HubHealthStatus::Disconnected
            }
        }
    }
}

impl std::fmt::Debug for HubClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HubClient")
            .field("base_url", &self.base_url)
            .field("api_key", &"<redacted>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_env_disabled_when_url_missing() {
        // Save and clear envs the test cares about so this is hermetic
        // even when run after another test that left them set.
        let saved = (
            std::env::var("HIVEHUB_CLOUD_BASE_URL").ok(),
            std::env::var("HIVEHUB_CLOUD_SERVICE_API_KEY").ok(),
            std::env::var("HIVEHUB_DISABLED").ok(),
        );
        unsafe {
            std::env::remove_var("HIVEHUB_CLOUD_BASE_URL");
            std::env::remove_var("HIVEHUB_CLOUD_SERVICE_API_KEY");
            std::env::remove_var("HIVEHUB_DISABLED");
        }

        let res = HubClient::from_env();
        assert!(matches!(res, Ok(None)), "no URL → standalone mode");

        unsafe {
            if let Some(v) = saved.0 {
                std::env::set_var("HIVEHUB_CLOUD_BASE_URL", v);
            }
            if let Some(v) = saved.1 {
                std::env::set_var("HIVEHUB_CLOUD_SERVICE_API_KEY", v);
            }
            if let Some(v) = saved.2 {
                std::env::set_var("HIVEHUB_DISABLED", v);
            }
        }
    }

    #[test]
    fn from_env_missing_key_is_an_error() {
        let saved = (
            std::env::var("HIVEHUB_CLOUD_BASE_URL").ok(),
            std::env::var("HIVEHUB_CLOUD_SERVICE_API_KEY").ok(),
        );
        unsafe {
            std::env::set_var("HIVEHUB_CLOUD_BASE_URL", "http://localhost:12000");
            std::env::remove_var("HIVEHUB_CLOUD_SERVICE_API_KEY");
        }

        let res = HubClient::from_env();
        assert!(matches!(res, Err(HubClientError::MissingApiKey)));

        unsafe {
            if let Some(v) = saved.0 {
                std::env::set_var("HIVEHUB_CLOUD_BASE_URL", v);
            } else {
                std::env::remove_var("HIVEHUB_CLOUD_BASE_URL");
            }
            if let Some(v) = saved.1 {
                std::env::set_var("HIVEHUB_CLOUD_SERVICE_API_KEY", v);
            }
        }
    }

    #[test]
    fn explicit_constructor_yields_a_handle() {
        let client =
            HubClient::new("test_key".to_string(), "http://localhost:12000".to_string()).unwrap();
        assert_eq!(client.base_url(), "http://localhost:12000");
    }

    #[test]
    fn debug_redacts_api_key() {
        let client = HubClient::new(
            "super-secret".to_string(),
            "http://localhost:12000".to_string(),
        )
        .unwrap();
        let printed = format!("{client:?}");
        assert!(!printed.contains("super-secret"));
        assert!(printed.contains("<redacted>"));
    }
}
