//! Parameter sweep over `(M, ef_construction, ef_search)`.
//!
//! Each sweep cell builds an in-memory [`nexus_core::index::KnnIndex`],
//! inserts the corpus base set, runs every query at the chosen
//! `ef_search`, and records the recall + latency aggregate.
//!
//! `M` and `ef_construction` invalidate the index on change — those
//! get an outer loop. `ef_search` only changes the query path, so we
//! reuse the built index across the inner sweep.

use std::time::{Duration, Instant};

use nexus_core::index::{KnnConfig, KnnIndex};
use serde::Serialize;

use crate::corpus::Corpus;
use crate::groundtruth::Groundtruth;
use crate::metrics::{LatencyStats, Recall, summarise_latency, summarise_recall};

/// Knobs for [`run`].
#[derive(Debug, Clone)]
pub struct SweepConfig {
    /// HNSW `M` values to try (max outgoing edges per node).
    pub m_values: Vec<usize>,
    /// HNSW `ef_construction` values to try (build-time candidate list size).
    pub ef_construction_values: Vec<usize>,
    /// HNSW `ef_search` values to try (query-time candidate list size).
    pub ef_search_values: Vec<usize>,
    /// `k` passed to the engine on every query. Recall@1 / 10 / 100
    /// are computed from this single result list — pick at least
    /// `100`.
    pub k: usize,
    /// Optional cap on the corpus base set, useful for smoke runs.
    pub base_limit: Option<usize>,
    /// Optional cap on the query set, useful for smoke runs.
    pub query_limit: Option<usize>,
    /// Random seed for the (currently unused) jitter knobs — held
    /// here so future extensions don't break the public surface.
    pub seed: u64,
}

impl Default for SweepConfig {
    fn default() -> Self {
        Self {
            m_values: vec![8, 16, 32, 64],
            ef_construction_values: vec![100, 200, 400, 800],
            ef_search_values: vec![50, 100, 200, 400],
            k: 100,
            base_limit: None,
            query_limit: None,
            seed: 0xC0FFEE,
        }
    }
}

/// One row of the sweep result. Engineered to round-trip cleanly
/// through serde so [`crate::report::write_json`] /
/// [`crate::report::write_csv`] can serialise it without further
/// transformation.
#[derive(Debug, Clone, Serialize)]
pub struct SweepCell {
    pub m: usize,
    pub ef_construction: usize,
    pub ef_search: usize,
    pub k: usize,
    pub corpus_kind: crate::corpus::CorpusKind,
    pub base_count: usize,
    pub query_count: usize,
    pub dim: usize,
    pub build_time_seconds: f64,
    pub recall: Recall,
    pub latency: LatencyStats,
}

#[derive(Debug, thiserror::Error)]
pub enum SweepError {
    #[error("k={k} cannot exceed base size {base_count}")]
    KExceedsBase { k: usize, base_count: usize },
    #[error("nexus core rejected index parameters: {0}")]
    Index(String),
}

/// Run the full sweep. Returns one [`SweepCell`] per
/// `(M, ef_construction, ef_search)` triple.
pub fn run(
    corpus: &Corpus,
    truth: &Groundtruth,
    config: &SweepConfig,
) -> Result<Vec<SweepCell>, SweepError> {
    let base = match config.base_limit {
        Some(limit) => &corpus.base[..limit.min(corpus.base.len())],
        None => &corpus.base[..],
    };
    let queries = match config.query_limit {
        Some(limit) => &corpus.queries[..limit.min(corpus.queries.len())],
        None => &corpus.queries[..],
    };
    if config.k > base.len() {
        return Err(SweepError::KExceedsBase {
            k: config.k,
            base_count: base.len(),
        });
    }

    let mut cells = Vec::with_capacity(
        config.m_values.len() * config.ef_construction_values.len() * config.ef_search_values.len(),
    );

    for &m in &config.m_values {
        for &ef_c in &config.ef_construction_values {
            let knn_config = KnnConfig {
                max_elements: base.len().max(1),
                max_connections: m,
                max_layer: ((base.len().max(2) as f32).ln().ceil() as usize).clamp(4, 24),
                ef_construction: ef_c,
            };
            let build_start = Instant::now();
            let index = KnnIndex::with_config(corpus.dim, knn_config)
                .map_err(|e| SweepError::Index(e.to_string()))?;
            for (i, vector) in base.iter().enumerate() {
                index
                    .add_vector(i as u64, vector.clone())
                    .map_err(|e| SweepError::Index(e.to_string()))?;
            }
            let build_time = build_start.elapsed();

            for &ef_s in &config.ef_search_values {
                let mut per_query_top: Vec<Vec<u32>> = Vec::with_capacity(queries.len());
                let mut samples: Vec<Duration> = Vec::with_capacity(queries.len());
                for q in queries {
                    let start = Instant::now();
                    let results = index
                        .search_knn_with_ef(q, config.k, ef_s)
                        .map_err(|e| SweepError::Index(e.to_string()))?;
                    samples.push(start.elapsed());
                    per_query_top.push(results.into_iter().map(|(id, _)| id as u32).collect());
                }
                let recall = summarise_recall(&per_query_top, &truth.top_k);
                let latency = summarise_latency(&samples);
                cells.push(SweepCell {
                    m,
                    ef_construction: ef_c,
                    ef_search: ef_s,
                    k: config.k,
                    corpus_kind: corpus.kind,
                    base_count: base.len(),
                    query_count: queries.len(),
                    dim: corpus.dim,
                    build_time_seconds: build_time.as_secs_f64(),
                    recall,
                    latency,
                });
                tracing::info!(
                    m,
                    ef_construction = ef_c,
                    ef_search = ef_s,
                    recall_at_10 = recall.recall_at_10,
                    p95_us = latency.p95_us,
                    "sweep cell complete"
                );
            }
        }
    }
    Ok(cells)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::{Rng, SeedableRng};

    fn synthetic_corpus(seed: u64, base_count: usize, query_count: usize, dim: usize) -> Corpus {
        let mut rng = StdRng::seed_from_u64(seed);
        let base: Vec<Vec<f32>> = (0..base_count)
            .map(|_| (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect())
            .collect();
        let queries: Vec<Vec<f32>> = (0..query_count)
            .map(|_| (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect())
            .collect();
        Corpus::from_memory(dim, base, queries)
    }

    #[test]
    fn small_sweep_produces_one_cell_per_triple() {
        let corpus = synthetic_corpus(7, 64, 4, 8);
        let truth = Groundtruth::compute(&corpus.base, &corpus.queries, 8).expect("gt");
        let config = SweepConfig {
            m_values: vec![8, 16],
            ef_construction_values: vec![100],
            ef_search_values: vec![50, 100],
            k: 8,
            base_limit: None,
            query_limit: None,
            seed: 7,
        };
        let cells = run(&corpus, &truth, &config).expect("sweep");
        assert_eq!(cells.len(), 4);
        for cell in &cells {
            assert_eq!(cell.k, 8);
            assert!(cell.recall.recall_at_1 >= 0.0 && cell.recall.recall_at_1 <= 1.0);
            assert!(cell.latency.samples == corpus.queries.len());
        }
    }

    #[test]
    fn larger_ef_search_does_not_decrease_recall_on_average() {
        let corpus = synthetic_corpus(11, 256, 8, 16);
        let truth = Groundtruth::compute(&corpus.base, &corpus.queries, 16).expect("gt");
        let config = SweepConfig {
            m_values: vec![16],
            ef_construction_values: vec![200],
            ef_search_values: vec![16, 256],
            k: 16,
            base_limit: None,
            query_limit: None,
            seed: 11,
        };
        let cells = run(&corpus, &truth, &config).expect("sweep");
        // The two cells share the same index; the larger ef_search
        // must not produce strictly worse recall@10.
        let small = cells.iter().find(|c| c.ef_search == 16).unwrap();
        let large = cells.iter().find(|c| c.ef_search == 256).unwrap();
        assert!(
            large.recall.recall_at_10 + 1e-9 >= small.recall.recall_at_10,
            "recall@10 regressed from ef=16 ({:?}) to ef=256 ({:?})",
            small.recall,
            large.recall
        );
    }

    #[test]
    fn k_exceeding_base_is_rejected() {
        let corpus = synthetic_corpus(13, 4, 1, 4);
        let truth = Groundtruth::compute(&corpus.base, &corpus.queries, 4).expect("gt");
        let config = SweepConfig {
            m_values: vec![8],
            ef_construction_values: vec![50],
            ef_search_values: vec![50],
            k: 32,
            base_limit: None,
            query_limit: None,
            seed: 13,
        };
        let err = run(&corpus, &truth, &config).unwrap_err();
        assert!(matches!(err, SweepError::KExceedsBase { .. }));
    }
}
