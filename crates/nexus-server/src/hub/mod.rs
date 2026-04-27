//! HiveHub.Cloud integration module
//! (phase5_hub-integration).
//!
//! Wraps the [`hivehub-internal-sdk`] so the rest of the server has a
//! single, opinionated entry-point for talking to the Hub control
//! plane. Today the wrapper covers §1 of the task — SDK lifetime,
//! env-driven configuration, and a simple liveness probe. The
//! authentication middleware (§2), database-per-user routing (§3),
//! credit consumption (§5), quota enforcement (§6), and usage
//! reporting (§7) build on top of the [`HubClient`] handle this
//! module exposes.
//!
//! ## Disabled-by-default
//!
//! Hub integration is opt-in: when the `HIVEHUB_API_URL` env var is
//! unset (or `HIVEHUB_DISABLED=1`), [`HubClient::from_env`] returns
//! `Ok(None)`. The server then runs in the original standalone mode.
//! When set, a missing/invalid `HIVEHUB_SERVICE_API_KEY` is a
//! configuration error and surfaces at startup instead of at first
//! API call.

pub mod client;

pub use client::{HubClient, HubClientError, HubHealthStatus};
