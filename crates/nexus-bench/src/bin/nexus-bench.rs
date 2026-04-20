//! `nexus-bench` CLI.
//!
//! Runs the seed catalogue against an in-process Nexus, emits a
//! Markdown or JSON report. Neo4j comparison requires `--features
//! neo4j` + a running Bolt endpoint; when it's missing the Markdown
//! report still renders, with `—` in the Neo4j columns.

use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use nexus_bench::{
    ComparativeRow, Dataset, MicroDataset, NexusClient, RunConfig,
    dataset::DatasetKind,
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
#[command(name = "nexus-bench")]
#[command(about = "Nexus ↔ Neo4j comparative benchmark harness", long_about = None)]
struct Args {
    /// Multiplier applied to every scenario's measured iteration
    /// count. 0.1 for a fast dev loop, 1.0 for canonical, 5.0 for a
    /// release baseline.
    #[arg(long, default_value_t = 1.0)]
    measured_multiplier: f64,

    /// Output format.
    #[arg(long, value_enum, default_value_t = OutputFormat::Markdown)]
    format: OutputFormat,

    /// Write the report here instead of stdout. Markdown format by
    /// default; pass `--format json` to get JSON or `--format both`
    /// to emit both files (`.md` + `.json`).
    #[arg(long)]
    output: Option<PathBuf>,

    /// Comma-separated scenario ids to run. Default: everything from
    /// the seed catalogue.
    #[arg(long)]
    only: Option<String>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let cfg = RunConfig {
        measured_multiplier: args.measured_multiplier,
        reset_between: false,
    };

    let mut client = NexusClient::new()?;
    // Install the micro dataset once per run — the seed scenarios all
    // target it. Future dataset-switching is a per-scenario concern.
    MicroDataset::default().load(&mut client)?;

    let wanted: Option<Vec<String>> = args
        .only
        .as_deref()
        .map(|s| s.split(',').map(|x| x.trim().to_string()).collect());

    let scenarios = seed_scenarios();
    let mut rows = Vec::with_capacity(scenarios.len());
    for scen in scenarios {
        if let Some(ref w) = wanted {
            if !w.iter().any(|id| id == &scen.id) {
                continue;
            }
        }
        // Dataset consistency check.
        debug_assert_eq!(scen.dataset, DatasetKind::Micro);

        println!(
            "▶ {} ({} warmup / {} measured)",
            scen.id, scen.warmup_iters, scen.measured_iters
        );
        let nexus_result = run_scenario(&scen, &mut client, &cfg)?;
        println!(
            "  nexus: p50={}µs p95={}µs ({:.0} ops/s)",
            nexus_result.p50_us, nexus_result.p95_us, nexus_result.ops_per_second
        );
        rows.push(ComparativeRow::new(nexus_result, None));
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
            let md_path = p.with_extension("md");
            let json_path = p.with_extension("json");
            std::fs::write(md_path, md())?;
            std::fs::write(json_path, js()?)?;
        }
    }
    Ok(())
}
