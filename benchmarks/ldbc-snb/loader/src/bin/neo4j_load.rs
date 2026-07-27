//! `neo4j-load` — loads an LDBC SNB Interactive dataset into the Neo4j
//! baseline so the same query can be run against both engines and the results
//! compared. Produces the identical logical graph the Nexus `ldbc-load` bin
//! does: same labels (including the `:Message` superlabel), same synthesized
//! merge-foreign edges, dates as epoch-millisecond integers.
//!
//! ```text
//! neo4j-load --dataset <dir> --url http://localhost:17474
//! ```

use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use ldbc_snb_loader::neo4j::{Neo4jClient, Neo4jLoader};
use ldbc_snb_loader::schema;

const DEFAULT_URL: &str = "http://localhost:17474";
const DEFAULT_DATABASE: &str = "neo4j";
const DEFAULT_BATCH_ROWS: usize = 5_000;

struct Args {
    dataset: PathBuf,
    url: String,
    database: String,
    batch_rows: usize,
    wipe: bool,
}

fn parse_args() -> Result<Args> {
    let mut dataset = None;
    let mut url = DEFAULT_URL.to_string();
    let mut database = DEFAULT_DATABASE.to_string();
    let mut batch_rows = DEFAULT_BATCH_ROWS;
    let mut wipe = false;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = |flag: &str| args.next().with_context(|| format!("{flag} needs a value"));
        match arg.as_str() {
            "--dataset" => dataset = Some(PathBuf::from(value("--dataset")?)),
            "--url" => url = value("--url")?,
            "--database" => database = value("--database")?,
            "--batch-rows" => batch_rows = value("--batch-rows")?.parse()?,
            "--wipe" => wipe = true,
            "-h" | "--help" => {
                println!(
                    "neo4j-load — load an LDBC SNB dataset into Neo4j\n\n\
                     USAGE:\n    neo4j-load --dataset <DIR> [OPTIONS]\n\n\
                     OPTIONS:\n\
                     \x20   --dataset <DIR>    Dataset root (contains static/ and dynamic/). Required.\n\
                     \x20   --url <URL>        Neo4j HTTP base URL [default: {DEFAULT_URL}]\n\
                     \x20   --database <NAME>  Neo4j database [default: {DEFAULT_DATABASE}]\n\
                     \x20   --batch-rows <N>   Rows per Cypher batch [default: {DEFAULT_BATCH_ROWS}]\n\
                     \x20   --wipe            delete every node/relationship (batched) before loading\n\
                     \x20   -h, --help        Print this help"
                );
                std::process::exit(0);
            }
            other => bail!("unknown argument `{other}` (try --help)"),
        }
    }

    Ok(Args {
        dataset: dataset.context("--dataset is required (try --help)")?,
        url,
        database,
        batch_rows: batch_rows.max(1),
        wipe,
    })
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

    let client = Neo4jClient::new(&args.url, &args.database);
    client.ping()?;
    println!("Neo4j at {} (db `{}`) is up", args.url, args.database);

    if args.wipe {
        println!("wiping the database (batched)…");
        client.wipe(50_000)?;
    }

    let loader = Neo4jLoader {
        client: &client,
        dataset: &args.dataset,
        batch_rows: args.batch_rows,
    };

    println!("\nCreating id indexes");
    loader.create_indexes()?;

    let started = Instant::now();
    println!("\nPass 1/3 — nodes");
    loader.load_nodes()?;
    println!("\nPass 2/3 — merge-foreign relationships");
    loader.load_foreign_key_edges()?;
    println!("\nPass 3/3 — relationship files");
    loader.load_edge_files()?;
    println!("\nLoaded in {:.1}s", started.elapsed().as_secs_f64());

    println!("\nVerifying counts against Neo4j…");
    let mut mismatches = Vec::new();
    for (label, expected) in schema::SF0_1_NODE_COUNTS {
        let actual = client.count(&format!("MATCH (n:{label}) RETURN count(n)"))?;
        if actual != *expected as i64 {
            mismatches.push(format!(
                "label {label}: expected {expected}, Neo4j holds {actual}"
            ));
        }
    }
    // Total relationship count: the ten edge files (one edge per row) plus the
    // synthesized merge-foreign edges. Computed from the CSVs so the check
    // stays honest if the dataset changes.
    let expected_rels = expected_relationship_total(&args.dataset)?;
    let actual_rels = client.count("MATCH ()-[r]->() RETURN count(r)")?;
    if actual_rels != expected_rels {
        mismatches.push(format!(
            "relationships: expected {expected_rels}, Neo4j holds {actual_rels}"
        ));
    }

    if mismatches.is_empty() {
        println!("  every label count matches, and {actual_rels} relationships are present");
        println!("\nNeo4j load complete.");
        Ok(ExitCode::SUCCESS)
    } else {
        eprintln!("\nNEO4J COUNT VERIFICATION FAILED:");
        for m in &mismatches {
            eprintln!("  - {m}");
        }
        Ok(ExitCode::FAILURE)
    }
}

/// Expected total relationship count: the edge files (one edge per row) plus
/// the merge-foreign edges, counted straight from the CSVs so the number tracks
/// the actual dataset rather than a hard-coded constant.
fn expected_relationship_total(dataset: &std::path::Path) -> Result<i64> {
    use ldbc_snb_loader::csv_source::{parse_id, CsvSource};

    let mut total: i64 = 0;

    for file in schema::EDGE_FILES {
        total += count_rows(&dataset.join(file.path))?;
    }
    // Merge-foreign edges: one per non-empty foreign-key cell.
    for file in schema::NODE_FILES {
        if file.foreign_keys.is_empty() {
            continue;
        }
        let mut source = CsvSource::open(&dataset.join(file.path))?;
        let fk_cols = file
            .foreign_keys
            .iter()
            .map(|fk| source.column(fk.column))
            .collect::<Result<Vec<_>>>()?;
        for row in source.rows() {
            let row = row?;
            for &col in &fk_cols {
                if parse_id(&row[col], "fk")?.is_some() {
                    total += 1;
                }
            }
        }
    }
    Ok(total)
}

fn count_rows(path: &std::path::Path) -> Result<i64> {
    use ldbc_snb_loader::csv_source::CsvSource;
    let mut source = CsvSource::open(path)?;
    let mut n = 0i64;
    for row in source.rows() {
        row?;
        n += 1;
    }
    Ok(n)
}
