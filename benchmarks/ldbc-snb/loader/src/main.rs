//! `ldbc-load` — streams an LDBC SNB Interactive dataset
//! (`CsvCompositeMergeForeign` / `LongDateFormatter`) into a running Nexus
//! server through `POST /ingest`, then VERIFIES the result by reading the
//! counts back.
//!
//! The verification is not optional decoration: `/ingest` is best-effort per
//! row, so "the requests all returned 200" does not mean the graph is
//! complete. A load that ends with a mismatch exits non-zero and says exactly
//! which label or relationship type is short.
//!
//! ```text
//! ldbc-load --dataset ~/.cache/ldbc-snb/sf0.1/social_network-sf0.1-CsvCompositeMergeForeign-LongDateFormatter
//! ldbc-load --dataset <dir> --dry-run     # parse + resolve every edge, write nothing
//! ```

mod client;
mod csv_source;
mod load;
mod schema;

use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use client::NexusClient;
use csv_source::CsvSource;
use load::{verify, IdMap, Loader, Submitted};

const DEFAULT_URL: &str = "http://localhost:15474";
const DEFAULT_BATCH_ROWS: usize = 5_000;
/// Half the server's default 16 MiB body limit, leaving room for the JSON
/// framing the estimate does not model exactly.
const DEFAULT_BATCH_BYTES: usize = 8 * 1024 * 1024;

struct Args {
    dataset: PathBuf,
    url: String,
    batch_rows: usize,
    batch_bytes: usize,
    dry_run: bool,
    /// Skip the writes but still run the verification against a live server —
    /// the passes are executed exactly as in a dry run purely to recompute the
    /// expected counts, so an already-loaded database can be re-checked in
    /// seconds instead of being reloaded.
    verify_only: bool,
    /// Promote the per-type traversal read-back from advisory to fatal. Off by
    /// default only because that read is currently non-deterministic in the
    /// engine (phase7_opencypher-gap-closure 4.8); turn it on once that is
    /// fixed and the check becomes a regression guard.
    strict_readback: bool,
    /// Compare the CSV row counts against the pinned SF0.1 reference table
    /// before writing anything. Auto-enabled when the dataset path names
    /// sf0.1, so a truncated download is caught in seconds.
    check_reference_counts: bool,
}

fn parse_args() -> Result<Args> {
    let mut dataset = None;
    let mut url = DEFAULT_URL.to_string();
    let mut batch_rows = DEFAULT_BATCH_ROWS;
    let mut batch_bytes = DEFAULT_BATCH_BYTES;
    let mut dry_run = false;
    let mut verify_only = false;
    let mut strict_readback = false;
    let mut check_reference_counts = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = |flag: &str| -> Result<String> {
            args.next().with_context(|| format!("{flag} needs a value"))
        };
        match arg.as_str() {
            "--dataset" => dataset = Some(PathBuf::from(value("--dataset")?)),
            "--url" => url = value("--url")?,
            "--batch-rows" => batch_rows = value("--batch-rows")?.parse()?,
            "--batch-bytes" => batch_bytes = value("--batch-bytes")?.parse()?,
            "--dry-run" => dry_run = true,
            "--verify-only" => verify_only = true,
            "--strict-readback" => strict_readback = true,
            "--check-reference-counts" => check_reference_counts = Some(true),
            "--no-check-reference-counts" => check_reference_counts = Some(false),
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            other => bail!("unknown argument `{other}` (try --help)"),
        }
    }

    let dataset = dataset.context("--dataset is required (try --help)")?;
    let check_reference_counts = check_reference_counts
        .unwrap_or_else(|| dataset.to_string_lossy().to_lowercase().contains("sf0.1"));

    if batch_rows == 0 {
        bail!("--batch-rows must be at least 1");
    }
    Ok(Args {
        dataset,
        url,
        batch_rows,
        batch_bytes,
        dry_run,
        verify_only,
        strict_readback,
        check_reference_counts,
    })
}

fn print_help() {
    println!(
        "ldbc-load — load an LDBC SNB Interactive dataset into Nexus

USAGE:
    ldbc-load --dataset <DIR> [OPTIONS]

OPTIONS:
    --dataset <DIR>       Dataset root (the `social_network-sfN-CsvCompositeMergeForeign-…`
                          directory containing `static/` and `dynamic/`). Required.
    --url <URL>           Nexus base URL [default: {DEFAULT_URL}]
    --batch-rows <N>      Max rows per /ingest request [default: {DEFAULT_BATCH_ROWS}]
    --batch-bytes <N>     Approximate max payload bytes per request [default: {DEFAULT_BATCH_BYTES}]
    --dry-run             Parse every file and resolve every edge endpoint without writing.
                          Proves the dataset is internally consistent; needs no server.
    --verify-only         Re-run the count verification against an already-loaded database
                          without writing anything (the passes still run, to recompute the
                          expected counts).
    --strict-readback     Treat a per-type traversal read-back shortfall as fatal. Off by
                          default while phase7_opencypher-gap-closure 4.8 (non-deterministic
                          expand) is open — the shortfall is reported either way.
    --check-reference-counts / --no-check-reference-counts
                          Compare CSV row counts against the pinned SF0.1 table before
                          loading [default: on when the path names sf0.1]
    -h, --help            Print this help

The database must be EMPTY and its indexes created (`schema/create-schema.sh`)
before running: the loader verifies absolute counts, so pre-existing rows are
reported as a mismatch."
    );
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("\nerror: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<ExitCode> {
    let args = parse_args()?;
    if !args.dataset.is_dir() {
        bail!("--dataset {} is not a directory", args.dataset.display());
    }

    let client = NexusClient::new(&args.url);
    if args.dry_run && !args.verify_only {
        println!("DRY RUN — parsing and resolving only, nothing will be written\n");
    } else {
        client.health()?;
        println!("Nexus at {} is up", args.url);
    }

    if args.check_reference_counts {
        check_reference_counts(&args.dataset)?;
    }

    let loader = Loader {
        client: &client,
        dataset: &args.dataset,
        batch_rows: args.batch_rows,
        batch_bytes: args.batch_bytes,
        dry_run: args.dry_run || args.verify_only,
    };

    let started = Instant::now();
    let mut ids = IdMap::default();
    let mut submitted = Submitted::default();

    println!("\nPass 1/3 — nodes");
    loader.load_nodes(&mut ids, &mut submitted)?;
    println!("  {} node ids mapped", ids.len());

    println!("\nPass 2/3 — merge-foreign relationships");
    loader.load_foreign_key_edges(&ids, &mut submitted)?;

    println!("\nPass 3/3 — relationship files");
    loader.load_edge_files(&ids, &mut submitted)?;

    let nodes: usize = submitted.nodes.values().sum();
    let relationships: usize = submitted.relationships.values().sum();
    if args.verify_only {
        println!("\nDataset holds {nodes} nodes and {relationships} relationships");
    } else {
        println!(
            "\nSubmitted {nodes} nodes and {relationships} relationships in {:.1}s",
            started.elapsed().as_secs_f64()
        );
    }

    if args.dry_run && !args.verify_only {
        println!("\nDry run complete: every foreign key and edge endpoint resolved.");
        return Ok(ExitCode::SUCCESS);
    }

    println!("\nVerifying counts against the database…");
    let verification = verify(&client, &submitted)?;

    let mut fatal: Vec<String> = verification.node_mismatches.clone();
    fatal.extend(verification.total_relationship_mismatch.clone());
    if args.strict_readback {
        fatal.extend(verification.type_readback_mismatches.clone());
    }

    if verification.node_mismatches.is_empty() && verification.total_relationship_mismatch.is_none()
    {
        println!("  every label count matches, and the engine's write counter agrees on {relationships} relationships");
    }

    if !verification.type_readback_mismatches.is_empty() && !args.strict_readback {
        println!(
            "\nTRAVERSAL READ-BACK DISCREPANCY — reported, NOT counted as a load failure:\n\
             \x20 The relationships were created (per-batch acknowledgements and the engine's\n\
             \x20 own write counter both agree), but `MATCH ()-[r:TYPE]->()` sees fewer of them,\n\
             \x20 and a different number on each run of the same read-only database. That is\n\
             \x20 an engine bug, filed as phase7_opencypher-gap-closure item 4.8; re-run with\n\
             \x20 --strict-readback to treat it as fatal once that is closed."
        );
        for mismatch in &verification.type_readback_mismatches {
            println!("  - {mismatch}");
        }
    }

    if fatal.is_empty() {
        println!("\nLoad complete: {nodes} nodes, {relationships} relationships.");
        Ok(ExitCode::SUCCESS)
    } else {
        eprintln!("\nCOUNT VERIFICATION FAILED — the graph is incomplete:");
        for mismatch in &fatal {
            eprintln!("  - {mismatch}");
        }
        eprintln!(
            "\nDo not benchmark this database. `/ingest` is best-effort per row, so a \n\
             partial failure leaves a graph that looks loaded but answers queries wrong."
        );
        Ok(ExitCode::FAILURE)
    }
}

/// Dataset-integrity check against the counts pinned in the harness README.
/// Catches a truncated or half-extracted download before a single row is
/// written, instead of surfacing as a puzzling mismatch after a full load.
fn check_reference_counts(dataset: &std::path::Path) -> Result<()> {
    println!("Checking CSV row counts against the pinned SF0.1 reference table…");
    let mut problems = Vec::new();

    for file in schema::NODE_FILES {
        let Some((_, expected)) = schema::SF0_1_NODE_COUNTS
            .iter()
            .find(|(label, _)| *label == file.label)
        else {
            continue;
        };
        let actual = count_rows(&dataset.join(file.path))?;
        if actual != *expected {
            problems.push(format!("{}: {actual} rows, expected {expected}", file.path));
        }
    }
    for (path, expected) in schema::SF0_1_EDGE_FILE_ROWS {
        let actual = count_rows(&dataset.join(path))?;
        if actual != *expected {
            problems.push(format!("{path}: {actual} rows, expected {expected}"));
        }
    }

    if problems.is_empty() {
        println!("  all 18 files match the reference counts");
        Ok(())
    } else {
        bail!(
            "the dataset does not match the pinned SF0.1 reference counts:\n  - {}\n\
             Re-run fetch-dataset.sh, or pass --no-check-reference-counts if this is \
             deliberately a different dataset.",
            problems.join("\n  - ")
        )
    }
}

fn count_rows(path: &std::path::Path) -> Result<usize> {
    let mut source = CsvSource::open(path)?;
    let mut rows = 0usize;
    for row in source.rows() {
        row?;
        rows += 1;
    }
    Ok(rows)
}
