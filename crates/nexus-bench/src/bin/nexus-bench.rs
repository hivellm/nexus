//! `nexus-bench` CLI — runs the seed catalogue against a
//! **running** Nexus server over HTTP. Never starts an engine by
//! itself.
//!
//! Guard rails (see the crate docs for rationale):
//!
//! * **Debug-build refusal** — debug numbers are meaningless and
//!   the debug engine is 10–100× slower than release. Override with
//!   `NEXUS_BENCH_ALLOW_DEBUG=1`.
//! * **Explicit server flag** — `--i-have-a-server-running` is
//!   required before any Cypher fires against the target URL.
//!   Without it the CLI does a `/health` probe and exits 0 — a
//!   no-op that verifies reachability.
//! * **2 s `/health` probe** — fail fast if the server isn't up.
//! * **Hard per-scenario timeout** — the harness clamps both the
//!   scenario timeout and the measured-iteration count to values the
//!   shipped [`nexus_bench::scenario`] module enforces.

use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use nexus_bench::{
    ComparativeRow, Dataset, RunConfig, TinyDataset,
    client::{BenchClient, HttpClient},
    harness::run_scenario,
    report::{json::JsonReport, markdown::MarkdownReport},
    scenario_catalog::seed_scenarios,
};

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Markdown,
    Json,
    Both,
}

#[derive(Debug, Parser)]
#[command(
    name = "nexus-bench",
    about = "Nexus ↔ Neo4j comparative benchmark harness (HTTP-only).",
    long_about = "Runs benchmark scenarios against a Nexus server that is already listening for HTTP requests.\n\
                  Does NOT start a server or an engine. You must pass --i-have-a-server-running + a reachable --url."
)]
struct Args {
    /// Base URL of a running Nexus HTTP server. Required.
    #[arg(long, env = "NEXUS_BENCH_URL")]
    url: String,

    /// Explicit acknowledgement that this run WILL send Cypher
    /// queries to the target URL. Without this flag the CLI only
    /// performs a `/health` probe + exits 0 — a dry run.
    #[arg(long)]
    i_have_a_server_running: bool,

    /// Multiplier applied to every scenario's `measured_iters`.
    /// Clamped internally to [`nexus_bench::harness::MAX_MULTIPLIER`].
    #[arg(long, default_value_t = 1.0)]
    measured_multiplier: f64,

    /// Output format. Markdown by default.
    #[arg(long, value_enum, default_value_t = OutputFormat::Markdown)]
    format: OutputFormat,

    /// Write the report here instead of stdout.
    #[arg(long)]
    output: Option<PathBuf>,

    /// Comma-separated scenario ids to run. Default: all seed scenarios.
    #[arg(long)]
    only: Option<String>,

    /// Load the tiny dataset into the server before running the
    /// scenarios. Default: off — assume the dataset is already
    /// loaded. Ships exactly one CREATE statement; never a fan-out.
    #[arg(long)]
    load_dataset: bool,

    /// Column label for the Nexus side of the report.
    #[arg(long, default_value = "nexus")]
    engine_label: String,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    enforce_release_build()?;

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(2)
        .build()?;
    rt.block_on(run(args))
}

fn enforce_release_build() -> anyhow::Result<()> {
    if cfg!(debug_assertions) && std::env::var("NEXUS_BENCH_ALLOW_DEBUG").ok().as_deref() != Some("1") {
        anyhow::bail!(
            "\nnexus-bench refuses to run in debug builds — the numbers would be meaningless.\n\
             Run with `cargo run --release --features live-bench --bin nexus-bench ...`.\n\
             Override (for structural smoke checks only) with NEXUS_BENCH_ALLOW_DEBUG=1."
        );
    }
    Ok(())
}

async fn run(args: Args) -> anyhow::Result<()> {
    let handle = tokio::runtime::Handle::current();

    println!("\u{25b6} probing {} for /health ...", args.url);
    let mut client =
        HttpClient::connect(args.url.clone(), args.engine_label.clone(), handle).await?;
    println!("\u{2713} server reachable.");

    if !args.i_have_a_server_running {
        println!(
            "\nDry run (no --i-have-a-server-running). No Cypher was sent.\n\
             Pass --i-have-a-server-running to run the actual benchmark."
        );
        return Ok(());
    }

    if args.load_dataset {
        println!("\u{25b6} loading tiny dataset (1 CREATE statement)...");
        let load = TinyDataset.load_statement();
        let out = client.execute(load, std::time::Duration::from_secs(30))?;
        println!(
            "\u{2713} dataset loaded ({} nodes, {} edges expected; server returned {} rows)",
            TinyDataset.node_count(),
            TinyDataset.rel_count(),
            out.rows.len()
        );
    }

    let cfg = RunConfig {
        measured_multiplier: args.measured_multiplier,
    }
    .clamped();

    let wanted: Option<Vec<String>> = args
        .only
        .as_deref()
        .map(|s| s.split(',').map(|x| x.trim().to_string()).collect());

    let scenarios = seed_scenarios();
    let mut rows = Vec::new();
    for scen in scenarios {
        if let Some(ref w) = wanted {
            if !w.iter().any(|id| id == &scen.id) {
                continue;
            }
        }
        println!(
            "\u{25b6} {} ({} warmup / {} measured)",
            scen.id, scen.warmup_iters, scen.measured_iters
        );
        let mut client_ref = &mut client;
        let nexus = run_scenario(&scen, &args.engine_label, &mut client_ref, &cfg)?;
        println!(
            "  {}: p50={}\u{00b5}s p95={}\u{00b5}s ({:.0} ops/s)",
            args.engine_label, nexus.p50_us, nexus.p95_us, nexus.ops_per_second
        );
        rows.push(ComparativeRow::new(nexus, None));
    }

    emit(&rows, &args)?;
    Ok(())
}

fn emit(rows: &[ComparativeRow], args: &Args) -> anyhow::Result<()> {
    let md = || MarkdownReport::new(rows).render();
    let js = || JsonReport::new(rows.to_vec()).to_pretty_string();
    match (args.format, &args.output) {
        (OutputFormat::Markdown, None) => println!("{}", md()),
        (OutputFormat::Markdown, Some(p)) => std::fs::write(p, md())?,
        (OutputFormat::Json, None) => println!("{}", js()?),
        (OutputFormat::Json, Some(p)) => std::fs::write(p, js()?)?,
        (OutputFormat::Both, None) => {
            println!("{}", md());
            println!("{}", js()?);
        }
        (OutputFormat::Both, Some(p)) => {
            std::fs::write(p.with_extension("md"), md())?;
            std::fs::write(p.with_extension("json"), js()?)?;
        }
    }
    Ok(())
}
