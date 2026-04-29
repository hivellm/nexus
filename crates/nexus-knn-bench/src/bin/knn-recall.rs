//! `knn-recall` — CLI front-end for the KNN recall + latency sweep.
//!
//! Given a corpus on disk, computes brute-force ground truth, runs the
//! configured `(M, ef_construction, ef_search)` sweep, and emits the
//! results as JSON + CSV. See `docs/performance/KNN_RECALL.md` for the
//! reproduction recipe.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};

use nexus_knn_bench::{Corpus, Groundtruth, SweepConfig, run_sweep, write_csv, write_json};

#[derive(Parser)]
#[command(
    name = "knn-recall",
    version,
    about = "Nexus KNN recall + latency benchmark"
)]
struct Cli {
    /// Directory used to cache brute-force ground truth between runs.
    #[arg(long, default_value = "data/knn-corpora/.cache")]
    cache_dir: PathBuf,
    /// JSON report destination.
    #[arg(long, default_value = "knn-recall.json")]
    json_out: PathBuf,
    /// CSV report destination (drop into a Pareto plot directly).
    #[arg(long, default_value = "knn-recall.csv")]
    csv_out: PathBuf,
    /// Override the engine `k`. Recall@1 / 10 / 100 are computed from
    /// this single result list, so it must be ≥ 100 to populate the
    /// recall@100 column.
    #[arg(long, default_value_t = 100)]
    k: usize,
    /// Cap the base set size for smoke runs.
    #[arg(long)]
    base_limit: Option<usize>,
    /// Cap the query set size for smoke runs.
    #[arg(long)]
    query_limit: Option<usize>,
    /// HNSW `M` values to sweep (comma-separated).
    #[arg(long, value_delimiter = ',', default_values_t = vec![8usize, 16, 32, 64])]
    m_values: Vec<usize>,
    /// HNSW `ef_construction` values to sweep (comma-separated).
    #[arg(long, value_delimiter = ',', default_values_t = vec![100usize, 200, 400, 800])]
    ef_construction_values: Vec<usize>,
    /// HNSW `ef_search` values to sweep (comma-separated).
    #[arg(long, value_delimiter = ',', default_values_t = vec![50usize, 100, 200, 400])]
    ef_search_values: Vec<usize>,
    /// Random seed reserved for future jitter knobs.
    #[arg(long, default_value_t = 0xC0FFEE)]
    seed: u64,

    #[command(subcommand)]
    corpus: CorpusArg,
}

#[derive(Subcommand)]
enum CorpusArg {
    /// Load a SIFT-style corpus (fvecs + optional ivecs ground truth).
    Sift {
        #[arg(long)]
        base: PathBuf,
        #[arg(long)]
        queries: PathBuf,
        #[arg(long)]
        groundtruth: Option<PathBuf>,
    },
    /// Load a GloVe-style corpus (whitespace text, one vector per line).
    Glove {
        #[arg(long)]
        path: PathBuf,
        #[arg(long, default_value_t = 1_000)]
        query_count: usize,
        #[arg(long)]
        base_limit: Option<usize>,
    },
}

#[derive(ValueEnum, Clone, Debug)]
#[allow(dead_code)]
enum CorpusKindArg {
    Sift,
    Glove,
}

fn main() -> ExitCode {
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    std::fs::create_dir_all(&cli.cache_dir).with_context(|| {
        format!(
            "creating ground-truth cache directory {}",
            cli.cache_dir.display()
        )
    })?;

    let corpus = match &cli.corpus {
        CorpusArg::Sift {
            base,
            queries,
            groundtruth,
        } => {
            tracing::info!(?base, ?queries, "loading SIFT-style corpus");
            Corpus::load_sift(base, queries, groundtruth.as_deref())
                .with_context(|| "loading SIFT corpus")?
        }
        CorpusArg::Glove {
            path,
            query_count,
            base_limit,
        } => {
            tracing::info!(?path, query_count, "loading GloVe corpus");
            Corpus::load_glove(path, *query_count, *base_limit)
                .with_context(|| "loading GloVe corpus")?
        }
    };

    tracing::info!(
        kind = ?corpus.kind,
        dim = corpus.dim,
        base = corpus.base.len(),
        queries = corpus.queries.len(),
        "corpus ready"
    );

    let truth =
        Groundtruth::compute_with_cache(&corpus.base, &corpus.queries, cli.k, &cli.cache_dir)
            .with_context(|| "computing brute-force ground truth")?;
    tracing::info!(k = truth.k, "ground truth ready");

    let config = SweepConfig {
        m_values: cli.m_values.clone(),
        ef_construction_values: cli.ef_construction_values.clone(),
        ef_search_values: cli.ef_search_values.clone(),
        k: cli.k,
        base_limit: cli.base_limit,
        query_limit: cli.query_limit,
        seed: cli.seed,
    };

    let cells = run_sweep(&corpus, &truth, &config).with_context(|| "running sweep")?;
    tracing::info!(rows = cells.len(), "sweep complete");

    write_json(&cli.json_out, &cells)
        .with_context(|| format!("writing JSON report to {}", cli.json_out.display()))?;
    write_csv(&cli.csv_out, &cells)
        .with_context(|| format!("writing CSV report to {}", cli.csv_out.display()))?;
    tracing::info!(
        json = %cli.json_out.display(),
        csv = %cli.csv_out.display(),
        "reports written"
    );

    Ok(())
}
