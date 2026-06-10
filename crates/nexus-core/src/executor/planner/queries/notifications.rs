//! Per-thread planner notification sink, `UnindexedAccessClause`, and the
//! process-global rate-limiter for the unindexed-property WARN log.

use super::*;
use std::cell::RefCell;
use std::fmt;
use std::sync::{Mutex, OnceLock};

thread_local! {
    /// Per-thread sink for notifications that the planner produced
    /// while building a plan. The planner is constructed deep inside
    /// `Executor::plan_ast` / `Executor::parse_and_plan`, so the
    /// per-call `QueryPlanner::notifications` accumulator is dropped
    /// before the wrapper has a chance to attach them to the
    /// `ResultSet`. This thread-local bridges the gap: the planner
    /// flushes its accumulator into here right before being dropped,
    /// and `Executor::execute` drains here after the operators run.
    ///
    /// Cleared at the start of every `Executor::execute` so a panic
    /// that aborts a prior query cannot leak its notifications onto
    /// an unrelated follow-up query.
    static PENDING_PLANNER_NOTIFICATIONS: RefCell<Vec<Notification>> =
        const { RefCell::new(Vec::new()) };
}

/// Drain the per-thread pending notifications. Call from
/// `Executor::execute` after the operators run; the returned vector
/// is appended to the resulting `ResultSet`. Safe to call when the
/// list is empty.
pub fn drain_pending_planner_notifications() -> Vec<Notification> {
    PENDING_PLANNER_NOTIFICATIONS.with(|c| std::mem::take(&mut *c.borrow_mut()))
}

/// Push a planner's notifications into the per-thread sink. Used by
/// `Executor::plan_ast` immediately before dropping the planner.
pub fn stash_planner_notifications(notifications: Vec<Notification>) {
    if notifications.is_empty() {
        return;
    }
    PENDING_PLANNER_NOTIFICATIONS.with(|c| c.borrow_mut().extend(notifications));
}

/// Origin clause for an unindexed-property-access notification — used
/// in the human-readable description so operators can locate the
/// offending pattern in their query. `Display` produces `MERGE` /
/// `MATCH` exactly so it inlines cleanly into the notification body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnindexedAccessClause {
    Match,
    Merge,
}

impl fmt::Display for UnindexedAccessClause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            UnindexedAccessClause::Match => "MATCH",
            UnindexedAccessClause::Merge => "MERGE",
        })
    }
}

/// Process-global rate limiter for the WARN log mirror of every
/// `Nexus.Performance.UnindexedPropertyAccess` notification. Keyed by
/// `(label_id, key_id)`; value is the `Instant` of the last emission.
///
/// A `QueryPlanner` is constructed fresh per query (`Engine::execute_*`),
/// so per-planner state would never deduplicate. The rate limiter
/// therefore lives in process-global storage. Stored as
/// `OnceLock<Mutex<HashMap<...>>>` rather than `Lazy` to avoid pulling
/// `once_cell` into `nexus-core`'s already-large dependency closure.
pub(in crate::executor::planner) fn warn_log_state() -> &'static Mutex<HashMap<(u32, u32), Instant>>
{
    static STATE: OnceLock<Mutex<HashMap<(u32, u32), Instant>>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Window size for the rate-limited unindexed-property WARN log,
/// configurable via `NEXUS_PLANNER_WARN_INTERVAL_SECS`. Default: 60s.
pub(in crate::executor::planner) fn planner_warn_interval() -> Duration {
    let secs = std::env::var("NEXUS_PLANNER_WARN_INTERVAL_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(60);
    Duration::from_secs(secs)
}
