//! `nexus-bench` CLI — runs the seed catalogue against a
//! **running** Nexus server over HTTP, and optionally against a
//! Neo4j server in comparative mode. Never starts an engine by
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
//!
//! Compiled features:
//!
//! * `live-bench` (required by this binary) — HTTP client + scenario
//!   runner.
//! * `neo4j` (optional) — adds `--neo4j-url` + `--compare` so the
//!   CLI can benchmark a Neo4j server alongside Nexus and emit a
//!   comparative report. Without this feature the CLI is Nexus-only.

use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use nexus_bench::{
    ComparativeRow, Dataset, RunConfig, TinyDataset,
    client::{BenchClient, HttpClient},
    harness::run_scenario,
    report::{json::JsonReport, markdown::MarkdownReport},
    scenario_catalog::seed_scenarios,
};

#[cfg(feature = "neo4j")]
use nexus_bench::Neo4jBoltClient;

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Markdown,
    Json,
    Both,
}

#[derive(Debug, Parser)]
#[command(
    name = "nexus-bench",
    about = "Nexus ↔ Neo4j comparative benchmark harness (HTTP-only for Nexus, Bolt for Neo4j).",
    long_about = "Runs benchmark scenarios against a Nexus server that is already listening for HTTP requests.\n\
                  Does NOT start a server or an engine. You must pass --i-have-a-server-running + a reachable --url.\n\
                  With --compare + --neo4j-url (requires `neo4j` feature), each scenario additionally runs against Neo4j via Bolt."
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
    /// In comparative mode this loads on BOTH engines.
    #[arg(long)]
    load_dataset: bool,

    /// Column label for the Nexus side of the report.
    #[arg(long, default_value = "nexus")]
    engine_label: String,

    /// Bolt URL of a running Neo4j server (e.g. `bolt://localhost:17687`).
    /// Enables comparative mode together with `--compare`. Requires
    /// the `neo4j` feature to be compiled in.
    #[cfg(feature = "neo4j")]
    #[arg(long, env = "NEO4J_BENCH_URL")]
    neo4j_url: Option<String>,

    /// Neo4j user. Defaults to `neo4j`. Ignored when the target
    /// container runs with `NEO4J_AUTH=none`.
    #[cfg(feature = "neo4j")]
    #[arg(long, default_value = "neo4j")]
    neo4j_user: String,

    /// Neo4j password. Defaults to `neo4j`. Ignored when the target
    /// container runs with `NEO4J_AUTH=none`.
    #[cfg(feature = "neo4j")]
    #[arg(long, default_value = "neo4j", env = "NEO4J_BENCH_PASSWORD")]
    neo4j_password: String,

    /// Column label for the Neo4j side of the report.
    #[cfg(feature = "neo4j")]
    #[arg(long, default_value = "neo4j")]
    neo4j_engine_label: String,

    /// Run every scenario against both Nexus and Neo4j. Requires
    /// `--neo4j-url` to be set (or `NEO4J_BENCH_URL`).
    #[cfg(feature = "neo4j")]
    #[arg(long)]
    compare: bool,
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
    if cfg!(debug_assertions)
        && std::env::var("NEXUS_BENCH_ALLOW_DEBUG").ok().as_deref() != Some("1")
    {
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
    let mut nexus_client =
        HttpClient::connect(args.url.clone(), args.engine_label.clone(), handle.clone()).await?;
    println!("\u{2713} nexus server reachable.");

    #[cfg(feature = "neo4j")]
    let mut neo4j_client = connect_neo4j_if_requested(&args, handle.clone()).await?;

    if !args.i_have_a_server_running {
        println!(
            "\nDry run (no --i-have-a-server-running). No Cypher was sent.\n\
             Pass --i-have-a-server-running to run the actual benchmark."
        );
        return Ok(());
    }

    if args.load_dataset {
        load_tiny_dataset(&mut nexus_client, &args.engine_label)?;
        #[cfg(feature = "neo4j")]
        if let Some(ref mut c) = neo4j_client {
            load_tiny_dataset(c, &args.neo4j_engine_label)?;
        }
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

        // Nexus first, always.
        let mut c = &mut nexus_client;
        let nexus = run_scenario(&scen, &args.engine_label, &mut c, &cfg)?;
        println!(
            "  {}: p50={}\u{00b5}s p95={}\u{00b5}s ({:.0} ops/s)",
            args.engine_label, nexus.p50_us, nexus.p95_us, nexus.ops_per_second
        );

        // Neo4j second, when comparative mode is wired in and armed.
        #[cfg(feature = "neo4j")]
        let neo4j = run_neo4j_side(&scen, &cfg, neo4j_client.as_mut(), &args.neo4j_engine_label)?;
        #[cfg(not(feature = "neo4j"))]
        let neo4j = None;

        rows.push(ComparativeRow::new(nexus, neo4j));
    }

    emit(&rows, &args)?;
    Ok(())
}

/// Issue the single-CREATE tiny dataset against `client`. Shared
/// between the Nexus side and (when `neo4j` is enabled) the Neo4j
/// side so both engines see an identical seed.
fn load_tiny_dataset<C: BenchClient>(client: &mut C, label: &str) -> anyhow::Result<()> {
    println!(
        "\u{25b6} loading tiny dataset on {} (1 CREATE statement)...",
        label
    );
    let load = TinyDataset.load_statement();
    let out = client.execute(load, std::time::Duration::from_secs(30))?;
    println!(
        "\u{2713} {} loaded ({} nodes, {} edges expected; server returned {} rows)",
        label,
        TinyDataset.node_count(),
        TinyDataset.rel_count(),
        out.rows.len()
    );
    Ok(())
}

#[cfg(feature = "neo4j")]
async fn connect_neo4j_if_requested(
    args: &Args,
    handle: tokio::runtime::Handle,
) -> anyhow::Result<Option<Neo4jBoltClient>> {
    if !args.compare {
        return Ok(None);
    }
    let url = args.neo4j_url.as_deref().ok_or_else(|| {
        anyhow::anyhow!(
            "--compare requires --neo4j-url (or NEO4J_BENCH_URL) to point at a running Neo4j"
        )
    })?;
    println!(
        "\u{25b6} probing Neo4j {} for bolt HELLO + RETURN 1 ...",
        url
    );
    let client = Neo4jBoltClient::connect(
        url,
        &args.neo4j_user,
        &args.neo4j_password,
        &args.neo4j_engine_label,
        handle,
    )
    .await?;
    println!("\u{2713} neo4j server reachable.");
    Ok(Some(client))
}

#[cfg(feature = "neo4j")]
fn run_neo4j_side(
    scen: &nexus_bench::Scenario,
    cfg: &RunConfig,
    client: Option<&mut Neo4jBoltClient>,
    label: &str,
) -> anyhow::Result<Option<nexus_bench::ScenarioResult>> {
    let Some(client) = client else {
        return Ok(None);
    };
    let mut c = client;
    let result = run_scenario(scen, label, &mut c, cfg)?;
    println!(
        "  {}: p50={}\u{00b5}s p95={}\u{00b5}s ({:.0} ops/s)",
        label, result.p50_us, result.p95_us, result.ops_per_second
    );
    Ok(Some(result))
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
