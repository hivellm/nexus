//! `nexus-bench` CLI — runs the seed catalogue against a
//! **running** Nexus server over the native RPC protocol, and
//! optionally against a Neo4j server in comparative mode. Never
//! starts an engine by itself.
//!
//! Transport: Nexus-side goes over the native length-prefixed
//! MessagePack RPC defined in `nexus_protocol::rpc`. HTTP is
//! intentionally not a transport — see the crate docs for the
//! Bolt↔HTTP fairness argument.
//!
//! Guard rails (see the crate docs for rationale):
//!
//! * **Debug-build refusal** — debug numbers are meaningless and
//!   the debug engine is 10–100× slower than release. Override with
//!   `NEXUS_BENCH_ALLOW_DEBUG=1`.
//! * **Explicit server flag** — `--i-have-a-server-running` is
//!   required before any Cypher fires against the target server.
//!   Without it the CLI does a HELLO+PING probe and exits 0 — a
//!   no-op that verifies reachability.
//! * **2 s PING probe** — fail fast if the RPC listener isn't up.
//! * **Hard per-scenario timeout** — the harness clamps both the
//!   scenario timeout and the measured-iteration count to values the
//!   shipped [`nexus_bench::scenario`] module enforces.
//!
//! Compiled features:
//!
//! * `live-bench` (required by this binary) — RPC client + scenario
//!   runner.
//! * `neo4j` (optional) — adds `--neo4j-url` + `--compare` so the
//!   CLI can benchmark a Neo4j server alongside Nexus and emit a
//!   comparative report. Without this feature the CLI is Nexus-only.

use std::path::PathBuf;

use std::collections::HashSet;

use clap::{Parser, ValueEnum};
use nexus_bench::{
    ComparativeRow, Dataset, RunConfig, SmallDataset, TinyDataset, VectorSmallDataset,
    client::{BenchClient, NexusRpcClient, NexusRpcCredentials},
    dataset::DatasetKind,
    harness::run_scenario,
    report::{json::JsonReport, markdown::MarkdownReport},
    scenario_catalog::seed_scenarios,
};

#[cfg(feature = "neo4j")]
use nexus_bench::compare_rows;

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
    about = "Nexus ↔ Neo4j comparative benchmark harness (native RPC for Nexus, Bolt for Neo4j).",
    long_about = "Runs benchmark scenarios against a Nexus RPC listener that is already bound on --rpc-addr.\n\
                  Does NOT start a server or an engine. You must pass --i-have-a-server-running + a reachable --rpc-addr.\n\
                  With --compare + --neo4j-url (requires `neo4j` feature), each scenario additionally runs against Neo4j via Bolt."
)]
struct Args {
    /// TCP address of the Nexus RPC listener (`host:port`). Required.
    #[arg(long, env = "NEXUS_BENCH_RPC_ADDR")]
    rpc_addr: String,

    /// Optional API key for `AUTH` handshake. Skipped when the
    /// server is running with authentication disabled.
    #[arg(long, env = "NEXUS_BENCH_API_KEY")]
    rpc_api_key: Option<String>,

    /// Optional username for the `AUTH user pass` handshake form.
    /// Paired with `--rpc-password`.
    #[arg(long, env = "NEXUS_BENCH_USER")]
    rpc_user: Option<String>,

    /// Optional password paired with `--rpc-user`.
    #[arg(long, env = "NEXUS_BENCH_PASSWORD")]
    rpc_password: Option<String>,

    /// Explicit acknowledgement that this run WILL send Cypher
    /// queries to the target server. Without this flag the CLI only
    /// performs the HELLO + PING probe + exits 0 — a dry run.
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

fn rpc_credentials(args: &Args) -> NexusRpcCredentials {
    NexusRpcCredentials {
        api_key: args.rpc_api_key.clone(),
        username: args.rpc_user.clone(),
        password: args.rpc_password.clone(),
    }
}

async fn run(args: Args) -> anyhow::Result<()> {
    let handle = tokio::runtime::Handle::current();

    println!(
        "\u{25b6} probing Nexus RPC {} (HELLO + PING) ...",
        args.rpc_addr
    );
    let mut nexus_client = NexusRpcClient::connect(
        &args.rpc_addr,
        rpc_credentials(&args),
        args.engine_label.clone(),
        handle.clone(),
    )
    .await?;
    println!("\u{2713} nexus server reachable over RPC.");

    #[cfg(feature = "neo4j")]
    let mut neo4j_client = connect_neo4j_if_requested(&args, handle.clone()).await?;

    if !args.i_have_a_server_running {
        println!(
            "\nDry run (no --i-have-a-server-running). No Cypher was sent.\n\
             Pass --i-have-a-server-running to run the actual benchmark."
        );
        return Ok(());
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
    let selected: Vec<_> = scenarios
        .iter()
        .filter(|s| match &wanted {
            None => true,
            Some(list) => list.iter().any(|id| id == &s.id),
        })
        .collect();

    if args.load_dataset {
        // Load every dataset kind referenced by the selected
        // scenarios, once per engine. `HashSet` collapses
        // duplicates so each dataset loads at most once per
        // engine regardless of how many scenarios back-reference
        // it.
        //
        // A load failure (e.g. a list-typed property an engine
        // has not shipped support for) logs and continues —
        // same per-step tolerance the scenario loop applies.
        // Scenarios that depend on the missing dataset will
        // error in turn, which is the right signal; aborting
        // the whole run would be worse.
        let kinds: HashSet<DatasetKind> = selected.iter().map(|s| s.dataset).collect();
        for kind in kinds {
            if let Err(e) = load_dataset_kind(&mut nexus_client, kind, &args.engine_label) {
                eprintln!("  !! {kind:?}: {} load skipped: {e}", args.engine_label);
            }
            #[cfg(feature = "neo4j")]
            if let Some(ref mut c) = neo4j_client {
                if let Err(e) = load_dataset_kind(c, kind, &args.neo4j_engine_label) {
                    eprintln!(
                        "  !! {kind:?}: {} load skipped: {e}",
                        args.neo4j_engine_label
                    );
                }
            }
        }
    }
    let mut rows = Vec::new();
    for scen in &selected {
        let scen = *scen;
        println!(
            "\u{25b6} {} ({} warmup / {} measured)",
            scen.id, scen.warmup_iters, scen.measured_iters
        );

        // Nexus first, always. A single scenario's failure logs and
        // skips to the next entry rather than aborting the whole
        // run — one broken query (unsupported syntax, a driver
        // timeout, a transient network blip) should not destroy
        // the report for the other 27 scenarios.
        let mut c = &mut nexus_client;
        let nexus = match run_scenario(scen, &args.engine_label, &mut c, &cfg) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("  !! {}: {} error: {e}", scen.id, args.engine_label);
                continue;
            }
        };
        println!(
            "  {}: p50={}\u{00b5}s p95={}\u{00b5}s ({:.0} ops/s)",
            args.engine_label, nexus.p50_us, nexus.p95_us, nexus.ops_per_second
        );

        // Neo4j second, when comparative mode is wired in and armed.
        #[cfg(feature = "neo4j")]
        let neo4j =
            match run_neo4j_side(scen, &cfg, neo4j_client.as_mut(), &args.neo4j_engine_label) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("  !! {}: {} error: {e}", scen.id, args.neo4j_engine_label);
                    None
                }
            };
        #[cfg(not(feature = "neo4j"))]
        let neo4j = None;

        // §3.4 — cross-engine row-content divergence guard. One
        // extra execute per side after the scenario so latency
        // numbers stay untainted by the comparison probe. Warn but
        // don't fail: a content mismatch is a signal the operator
        // should investigate, not a reason to abort an otherwise
        // useful latency report.
        #[cfg(feature = "neo4j")]
        if let Some(ref mut nc) = neo4j_client {
            compare_engines_for_scenario(
                scen,
                &mut nexus_client,
                &args.engine_label,
                nc,
                &args.neo4j_engine_label,
            );
        }

        rows.push(ComparativeRow::new(nexus, neo4j));
    }

    emit(&rows, &args)?;
    Ok(())
}

/// Issue the single-CREATE literal for `kind` against `client`.
/// Shared between the Nexus side and (when `neo4j` is enabled)
/// the Neo4j side so both engines see identical seed data.
fn load_dataset_kind<C: BenchClient>(
    client: &mut C,
    kind: DatasetKind,
    label: &str,
) -> anyhow::Result<()> {
    let (name, load, nodes, rels) = match kind {
        DatasetKind::Tiny => (
            TinyDataset.name(),
            TinyDataset.load_statement(),
            TinyDataset.node_count(),
            TinyDataset.rel_count(),
        ),
        DatasetKind::Small => (
            SmallDataset.name(),
            SmallDataset.load_statement(),
            SmallDataset.node_count(),
            SmallDataset.rel_count(),
        ),
        DatasetKind::VectorSmall => (
            VectorSmallDataset.name(),
            VectorSmallDataset.load_statement(),
            VectorSmallDataset.node_count(),
            VectorSmallDataset.rel_count(),
        ),
    };
    println!("\u{25b6} loading {name} dataset on {label} (1 CREATE statement)...");
    let out = client.execute(load, std::time::Duration::from_secs(30))?;
    println!(
        "\u{2713} {label} / {name} loaded ({nodes} nodes, {rels} edges expected; \
         server returned {} rows)",
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
    println!("\u{25b6} probing Neo4j {url} for bolt HELLO + RETURN 1 ...");
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
fn compare_engines_for_scenario(
    scen: &nexus_bench::Scenario,
    nexus: &mut NexusRpcClient,
    nexus_label: &str,
    neo4j: &mut Neo4jBoltClient,
    neo4j_label: &str,
) {
    // Probe both engines once each. Fail soft — any error is
    // reported to stderr but does not abort the report.
    let nexus_out = match nexus.execute(&scen.query, scen.timeout) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("  !! {}: {nexus_label} probe failed: {e}", scen.id);
            return;
        }
    };
    let neo4j_out = match neo4j.execute(&scen.query, scen.timeout) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("  !! {}: {neo4j_label} probe failed: {e}", scen.id);
            return;
        }
    };
    if let Err(div) = compare_rows(nexus_label, &nexus_out.rows, neo4j_label, &neo4j_out.rows) {
        eprintln!("  !! {}: content divergence — {div}", scen.id);
    }
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
