//! `impl QueryPlanner` — the bulk of planning logic: statistics-driven
//! pattern reordering, cost estimation, join algorithm selection, index
//! push-down, aggregation rewrite. Split into cohesive submodules; this
//! façade re-exports every item that was previously reachable at
//! `crate::executor::planner::queries::*`.

// ── Submodule declarations ────────────────────────────────────────────────────
mod cost;
mod expressions;
mod notifications;
mod planner_core;
mod qpp;
mod relationships;
mod spatial;
mod strategy;
mod unindexed;

// ── Imports shared across all submodules (mirrors the original `use super::*`
//    plus the extra items `queries.rs` imported explicitly). These are
//    `pub(super)` so each submodule's `use super::*` picks them up.
// ─────────────────────────────────────────────────────────────────────────────
pub(super) use super::*;
pub(super) use crate::executor::types::{NotificationCategory, NotificationSeverity};

// ── Public re-exports (preserve every path that was previously reachable) ────

// Thread-local notification helpers (used by engine/executor)
pub use notifications::drain_pending_planner_notifications;
pub use notifications::stash_planner_notifications;

// `UnindexedAccessClause` is `pub(super)` in notifications.rs because it was
// `pub(super)` in the original — visible within `planner::queries` but not
// outside. Re-export at the same visibility for sibling submodules.
pub(crate) use notifications::UnindexedAccessClause;
// warn helpers are `pub(super)` in notifications — accessible from sibling mods
pub(super) use notifications::planner_warn_interval;
pub(super) use notifications::warn_log_state;

// Unindexed-property-access entry point (used by engine write path)
pub use unindexed::compute_unindexed_property_access_notifications;

// QPP feature flag (pub(crate) in original)
pub(crate) use qpp::qpp_legacy_rewrite_enabled;
pub(crate) use qpp::set_qpp_legacy_rewrite_enabled;
