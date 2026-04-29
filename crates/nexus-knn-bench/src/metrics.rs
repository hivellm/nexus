//! Recall + latency metrics.
//!
//! Recall@k is the fraction of true top-k neighbours surfaced by the
//! approximate index. Latency is summarised as the standard
//! p50/p95/p99 plus mean — same shape every other vector DB
//! publishes, so cross-comparisons stay apples-to-apples.

use std::time::Duration;

/// Aggregated recall numbers for a single sweep cell. The trio
/// matches the conventional "recall@1 / 10 / 100" reported by
/// Pinecone, Weaviate, Qdrant and Milvus — pinning the column set
/// matters more than the exact `k` values for cross-DB comparison.
#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize)]
pub struct Recall {
    pub recall_at_1: f64,
    pub recall_at_10: f64,
    pub recall_at_100: f64,
    /// Number of queries that produced the numbers above. Useful for
    /// downstream weighted aggregations and as a self-check.
    pub query_count: usize,
}

/// Latency summary for a single sweep cell.
#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize)]
pub struct LatencyStats {
    pub mean_us: f64,
    pub p50_us: f64,
    pub p95_us: f64,
    pub p99_us: f64,
    pub min_us: f64,
    pub max_us: f64,
    pub samples: usize,
}

/// Compute recall@k for a single approximate result against ground
/// truth. Both inputs may be longer than `k`; only the first `k`
/// elements of `approx` are considered (the engine returns at most
/// `k` IDs, and ground truth is sorted nearest-first).
pub fn recall_at_k(approx: &[u32], truth: &[u32], k: usize) -> f64 {
    if k == 0 || truth.is_empty() {
        return 0.0;
    }
    let truth_set: std::collections::HashSet<u32> = truth.iter().take(k).copied().collect();
    let take = approx
        .iter()
        .take(k)
        .filter(|id| truth_set.contains(id))
        .count();
    take as f64 / truth_set.len() as f64
}

/// Aggregate per-query recall into a [`Recall`] summary. Each entry
/// of `per_query` is the approximate result for one query, sorted
/// nearest-first.
pub fn summarise_recall(per_query: &[Vec<u32>], truth: &[Vec<u32>]) -> Recall {
    if per_query.is_empty() || truth.is_empty() {
        return Recall::default();
    }
    let n = per_query.len().min(truth.len());
    let mut r1 = 0.0;
    let mut r10 = 0.0;
    let mut r100 = 0.0;
    for i in 0..n {
        r1 += recall_at_k(&per_query[i], &truth[i], 1);
        r10 += recall_at_k(&per_query[i], &truth[i], 10);
        r100 += recall_at_k(&per_query[i], &truth[i], 100);
    }
    let denom = n as f64;
    Recall {
        recall_at_1: r1 / denom,
        recall_at_10: r10 / denom,
        recall_at_100: r100 / denom,
        query_count: n,
    }
}

/// Reduce a slice of per-query timings to a [`LatencyStats`].
///
/// Percentiles use the nearest-rank method (NIST). For 1k samples
/// this matches the linear-interpolation method to within one
/// sample — close enough for the headline numbers.
pub fn summarise_latency(samples: &[Duration]) -> LatencyStats {
    if samples.is_empty() {
        return LatencyStats::default();
    }
    let mut us: Vec<f64> = samples
        .iter()
        .map(|d| d.as_secs_f64() * 1_000_000.0)
        .collect();
    us.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = us.len();
    let pct = |p: f64| -> f64 {
        if n == 1 {
            return us[0];
        }
        let rank = ((p / 100.0) * n as f64).ceil() as usize;
        let idx = rank.saturating_sub(1).min(n - 1);
        us[idx]
    };
    let mean = us.iter().sum::<f64>() / n as f64;
    LatencyStats {
        mean_us: mean,
        p50_us: pct(50.0),
        p95_us: pct(95.0),
        p99_us: pct(99.0),
        min_us: us[0],
        max_us: us[n - 1],
        samples: n,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recall_at_k_perfect_match() {
        let approx = vec![5, 6, 7, 8];
        let truth = vec![5, 6, 7, 8];
        assert_eq!(recall_at_k(&approx, &truth, 4), 1.0);
    }

    #[test]
    fn recall_at_k_partial_overlap() {
        // Approx finds 2 of the 4 true neighbours among its top-4.
        let approx = vec![5, 99, 7, 100];
        let truth = vec![5, 6, 7, 8];
        assert_eq!(recall_at_k(&approx, &truth, 4), 0.5);
    }

    #[test]
    fn recall_at_k_truncates_to_k() {
        // Approx surfaces all four truths but only the first three
        // count when k=3.
        let approx = vec![5, 6, 7, 8];
        let truth = vec![5, 6, 7, 8];
        assert_eq!(recall_at_k(&approx, &truth, 3), 1.0);
    }

    #[test]
    fn summarise_latency_percentile_orders() {
        let samples: Vec<Duration> = (1..=100).map(Duration::from_micros).collect();
        let stats = summarise_latency(&samples);
        assert_eq!(stats.samples, 100);
        assert_eq!(stats.min_us, 1.0);
        assert_eq!(stats.max_us, 100.0);
        assert!(stats.p50_us <= stats.p95_us);
        assert!(stats.p95_us <= stats.p99_us);
        assert!((stats.mean_us - 50.5).abs() < 1e-6);
    }

    #[test]
    fn summarise_latency_empty_returns_default() {
        let stats = summarise_latency(&[]);
        assert_eq!(stats.samples, 0);
        assert_eq!(stats.mean_us, 0.0);
    }

    #[test]
    fn summarise_recall_aggregates_queries() {
        let per_query = vec![vec![1, 2, 3], vec![10, 20, 30]];
        let truth = vec![vec![1, 2, 3], vec![10, 99, 30]];
        let r = summarise_recall(&per_query, &truth);
        // First query: 3/3 = 1.0 at every k.
        // Second query: at k=1, 1/1 = 1.0. At k=3, 2/3.
        assert_eq!(r.query_count, 2);
        assert!((r.recall_at_1 - 1.0).abs() < 1e-9);
        assert!((r.recall_at_10 - ((1.0 + 2.0 / 3.0) / 2.0)).abs() < 1e-9);
    }
}
