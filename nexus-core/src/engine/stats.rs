//! Engine-level statistics and health reporting types.
//!
//! [`EngineStats`] is the aggregate counter payload `Engine::stats`
//! produces; it is serialisable and surfaced through `GET /stats`
//! alongside the SIMD kernel tiers. [`HealthStatus`] + [`HealthState`]
//! are the summary `Engine::health_check` produces for liveness /
//! readiness probes.

use crate::cache;
use std::collections::HashMap;

/// Engine statistics
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EngineStats {
    pub nodes: u64,
    pub relationships: u64,
    pub labels: u64,
    pub rel_types: u64,
    pub page_cache_hits: u64,
    pub page_cache_misses: u64,
    pub wal_entries: u64,
    pub active_transactions: u64,
    pub cache_stats: cache::CacheStats,
}

/// Health status
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HealthStatus {
    pub overall: HealthState,
    pub components: HashMap<String, HealthState>,
}

/// Health state
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum HealthState {
    Healthy,
    Unhealthy,
    Degraded,
}
