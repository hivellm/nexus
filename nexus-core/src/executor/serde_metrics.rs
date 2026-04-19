//! Process-wide counter for `serde_json` fallback events inside the
//! executor.
//!
//! Several executor operators build dedup / group-by keys by calling
//! `serde_json::to_string(&value)`. That call can fail — most
//! realistically on non-finite floats (`NaN`, `+Inf`, `-Inf`) which the
//! JSON data model does not permit. Before phase2's error-propagation
//! pass, those failures were silently swallowed with
//! `.unwrap_or_default()`, collapsing every failing row into the empty-
//! string bucket. Phase2 splits the sites into two categories:
//!
//! - **Error-propagating sites** (`operators/{aggregate,join,union}.rs`) —
//!   GROUP BY, DISTINCT, and UNION key serialisation failures become
//!   `Error::CypherExecution` and fail the query instead of silently
//!   producing wrong groupings. `record_propagated_failure` bumps this
//!   counter so ops can alarm on the rate even when the failure is
//!   already visible to the caller.
//!
//! - **Fallback sites** (`eval/helpers.rs::update_result_set_from_rows`) —
//!   the helper's return type is `()` and changing it would cascade
//!   through 18 call sites; in practice the key is only used to drop
//!   duplicate rows when no entity id is available, so a degraded
//!   `{:?}` key is acceptable. `record_fallback` bumps this counter
//!   and the site emits a `tracing::warn!`.
//!
//! `nexus-server` exposes this value in the Prometheus output as
//! `nexus_executor_serde_fallback_total` with a `site` label so alerts
//! can distinguish the two classes.

use std::sync::atomic::{AtomicU64, Ordering};

/// One entry per site that previously relied on `unwrap_or_default` /
/// `unwrap_or_else(_ -> Vec::new())` around `serde_json::*`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerdeFallbackSite {
    /// `operators/aggregate.rs` GROUP BY key.
    AggregateGroupKey,
    /// `operators/join.rs` DISTINCT key.
    DistinctKey,
    /// `operators/union.rs` UNION DISTINCT key.
    UnionDedupKey,
    /// `eval/helpers.rs::update_result_set_from_rows` fallback dedup
    /// key (degraded to `{:?}` when `to_string` fails).
    HelperRowDedupKey,
    /// `executor::execute` background cache-warming call.
    WarmCacheLazy,
}

impl SerdeFallbackSite {
    /// Short machine-readable label used in Prometheus output.
    pub const fn as_label(self) -> &'static str {
        match self {
            Self::AggregateGroupKey => "aggregate_group_key",
            Self::DistinctKey => "distinct_key",
            Self::UnionDedupKey => "union_dedup_key",
            Self::HelperRowDedupKey => "helper_row_dedup_key",
            Self::WarmCacheLazy => "warm_cache_lazy",
        }
    }
}

static AGGREGATE_GROUP_KEY: AtomicU64 = AtomicU64::new(0);
static DISTINCT_KEY: AtomicU64 = AtomicU64::new(0);
static UNION_DEDUP_KEY: AtomicU64 = AtomicU64::new(0);
static HELPER_ROW_DEDUP_KEY: AtomicU64 = AtomicU64::new(0);
static WARM_CACHE_LAZY: AtomicU64 = AtomicU64::new(0);

fn counter(site: SerdeFallbackSite) -> &'static AtomicU64 {
    match site {
        SerdeFallbackSite::AggregateGroupKey => &AGGREGATE_GROUP_KEY,
        SerdeFallbackSite::DistinctKey => &DISTINCT_KEY,
        SerdeFallbackSite::UnionDedupKey => &UNION_DEDUP_KEY,
        SerdeFallbackSite::HelperRowDedupKey => &HELPER_ROW_DEDUP_KEY,
        SerdeFallbackSite::WarmCacheLazy => &WARM_CACHE_LAZY,
    }
}

/// Record a site that detected a serde failure and propagated it as an
/// `Error::CypherExecution`. Caller still returns `Err(...)` — this
/// function only touches the counter.
pub fn record_propagated_failure(site: SerdeFallbackSite) {
    counter(site).fetch_add(1, Ordering::Relaxed);
}

/// Record a site that detected a serde failure and chose to fall back
/// to a degraded value (log `warn!` at the call site too).
pub fn record_fallback(site: SerdeFallbackSite) {
    counter(site).fetch_add(1, Ordering::Relaxed);
}

/// Snapshot of every site's counter. Used by the Prometheus exporter.
#[derive(Debug, Clone, Copy, Default)]
pub struct SerdeFallbackSnapshot {
    pub aggregate_group_key: u64,
    pub distinct_key: u64,
    pub union_dedup_key: u64,
    pub helper_row_dedup_key: u64,
    pub warm_cache_lazy: u64,
}

impl SerdeFallbackSnapshot {
    /// Sum across every site. Handy for a single `total` counter.
    pub fn total(&self) -> u64 {
        self.aggregate_group_key
            + self.distinct_key
            + self.union_dedup_key
            + self.helper_row_dedup_key
            + self.warm_cache_lazy
    }
}

/// Read every counter atomically-ish — each load is independent, so
/// readers may see a skewed snapshot under contention, but the values
/// are monotonically non-decreasing so this is safe for Prometheus.
pub fn snapshot() -> SerdeFallbackSnapshot {
    SerdeFallbackSnapshot {
        aggregate_group_key: AGGREGATE_GROUP_KEY.load(Ordering::Relaxed),
        distinct_key: DISTINCT_KEY.load(Ordering::Relaxed),
        union_dedup_key: UNION_DEDUP_KEY.load(Ordering::Relaxed),
        helper_row_dedup_key: HELPER_ROW_DEDUP_KEY.load(Ordering::Relaxed),
        warm_cache_lazy: WARM_CACHE_LAZY.load(Ordering::Relaxed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn site_labels_are_stable_machine_readable_strings() {
        assert_eq!(
            SerdeFallbackSite::AggregateGroupKey.as_label(),
            "aggregate_group_key"
        );
        assert_eq!(
            SerdeFallbackSite::HelperRowDedupKey.as_label(),
            "helper_row_dedup_key"
        );
    }

    #[test]
    fn snapshot_total_sums_every_site() {
        let before = snapshot();
        record_fallback(SerdeFallbackSite::HelperRowDedupKey);
        record_propagated_failure(SerdeFallbackSite::AggregateGroupKey);
        let after = snapshot();

        // Counters are process-wide so we only check the delta.
        assert_eq!(
            after.total() - before.total(),
            2,
            "recording two events must increment the total by exactly 2"
        );
        assert_eq!(after.helper_row_dedup_key - before.helper_row_dedup_key, 1);
        assert_eq!(after.aggregate_group_key - before.aggregate_group_key, 1);
    }
}
