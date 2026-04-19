//! Cluster-mode primitives for multi-tenant deployments.
//!
//! Nexus has two deployment shapes:
//!
//! * **Standalone** — the default. One tenant, no namespacing,
//!   authentication optional. The existing `auth` / `storage` /
//!   `executor` layers behave exactly as before when cluster mode
//!   is off, so upgrading an existing deployment is a no-op.
//!
//! * **Cluster** — multiple tenants share one server. Every request
//!   is authenticated, every piece of data lives under a validated
//!   [`UserNamespace`], and quota enforcement gates writes. The
//!   quota backend is pluggable via [`QuotaProvider`] so the server
//!   can run against a local in-memory implementation in tests and
//!   against a remote control-plane (HiveHub) in production without
//!   any of the hot-path code knowing which one is wired in.
//!
//! This module owns only the primitives — the concrete integration
//! points (middleware, storage prefixes, query scoping) live with
//! the subsystems they modify and consume types from here.

pub mod config;
pub mod context;
pub mod namespace;
pub mod quota;

#[cfg(feature = "axum")]
pub mod middleware;

pub use config::{ClusterConfig, TenantDefaults, TenantIsolationMode};
pub use context::{FunctionAccessError, UserContext};
pub use namespace::UserNamespace;
pub use quota::{LocalQuotaProvider, QuotaDecision, QuotaProvider, QuotaSnapshot, UsageDelta};

#[cfg(feature = "axum")]
pub use middleware::{QuotaError, QuotaMiddlewareState, quota_middleware_handler};
