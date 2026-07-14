//! concurrent-sweep -- drives nexus_bench::concurrent::run_concurrent
//! against a running Nexus RPC listener (and, optionally, a Neo4j Bolt
//! server) across a configurable worker-count grid, replacing the
//! TODO-fill-with-real-numbers stub that
//! scripts/benchmarks/run-vs-neo4j.sh used to write before this example
//! existed.
//!
//! Emits one ConcurrentJsonReport per worker level (the shape the
//! orchestrator script already expects under bench-out/concurrent-N.json)
//! plus a combined report at --output covering every level in one file.
//!
//! Usage:
//! cargo +nightly run --release --features "live-bench neo4j" --example concurrent_sweep -- --rpc-addr 127.0.0.1:15475 --neo4j-url bolt://127.0.0.1:7687 --neo4j-password password --workers 1,4,16,64 --duration-secs 15 --warmup-secs 2 --output bench-out/concurrent-combined.json
//!
//! Guard rails mirror the nexus-bench CLI: debug builds refuse to run
//! (override with NEXUS_BENCH_ALLOW_DEBUG=1) since the numbers would be
//! meaningless.

use std::path::PathBuf;
use std::time::Duration;

use clap::Parser;
use tokio::runtime::{Handle, Runtime};

use nexus_bench::client::{BenchClient, NexusRpcClient, NexusRpcCredentials};
use nexus_bench::concurrent::{
    ClientFactory, ConcurrentResult, ConcurrentRunConfig, run_concurrent,
};
use nexus_bench::harness::{BenchExecute, ExecResult};
use nexus_bench::report::concurrent_report::{ConcurrentJsonReport, render_markdown};
use nexus_bench::scenario_catalog::seed_scenarios;

#[cfg(feature = "neo4j")]
use nexus_bench::Neo4jBoltClient;

#[derive(Debug, Parser)]
#[command(
    name = "concurrent-sweep",
    about = "Drives nexus_bench::run_concurrent across a worker-count grid against a running server."
)]
struct Args {
    #[arg(long, env = "NEXUS_BENCH_RPC_ADDR")]
    rpc_addr: String,
    #[arg(long, env = "NEXUS_BENCH_API_KEY")]
    rpc_api_key: Option<String>,
    #[arg(long, env = "NEXUS_BENCH_USER")]
    rpc_user: Option<String>,
    #[arg(long, env = "NEXUS_BENCH_PASSWORD")]
    rpc_password: Option<String>,
    #[arg(long, default_value = "nexus")]
    engine_label: String,

    #[cfg(feature = "neo4j")]
    #[arg(long, env = "NEO4J_BENCH_URL")]
    neo4j_url: Option<String>,
    #[cfg(feature = "neo4j")]
    #[arg(long, default_value = "neo4j")]
    neo4j_user: String,
    #[cfg(feature = "neo4j")]
    #[arg(long, default_value = "neo4j", env = "NEO4J_BENCH_PASSWORD")]
    neo4j_password: String,
    #[cfg(feature = "neo4j")]
    #[arg(long, default_value = "neo4j")]
    neo4j_engine_label: String,

    #[arg(long, value_delimiter = ',', default_values_t = vec![1usize, 4, 16, 64])]
    workers: Vec<usize>,
    #[arg(long, default_value_t = 15)]
    duration_secs: u64,
    #[arg(long, default_value_t = 2)]
    warmup_secs: u64,
    #[arg(
        long,
        value_delimiter = ',',
        default_values_t = vec![
            "point_read.by_id".to_string(),
            "traversal.small_two_hop_from_hub".to_string(),
            "aggregation.count_all".to_string(),
            "write.merge_singleton".to_string(),
        ]
    )]
    scenarios: Vec<String>,

    #[arg(long)]
    output: PathBuf,
    #[arg(long)]
    markdown_output: Option<PathBuf>,
    #[arg(long)]
    per_level_dir: Option<PathBuf>,
}

fn enforce_release_build() -> anyhow::Result<()> {
    if cfg!(debug_assertions)
        && std::env::var("NEXUS_BENCH_ALLOW_DEBUG").ok().as_deref() != Some("1")
    {
        anyhow::bail!(
            "concurrent-sweep refuses to run in debug builds -- the numbers would be meaningless. \
             Run with: cargo run --release --features live-bench,neo4j --example concurrent_sweep ... \
             Override (structural smoke checks only) with NEXUS_BENCH_ALLOW_DEBUG=1."
        );
    }
    Ok(())
}

// Adapts an owned BenchClient (which only implements BenchExecute
// through a &mut T blanket impl) into a value that can be boxed as
// Box<dyn BenchExecute + Send> for ClientFactory::build. Re-enters the
// tokio Handle on every call -- the RPC/Bolt clients bridge their
// internal async I/O via tokio::task::block_in_place, which panics
// unless the calling thread currently holds an active runtime context.
// run_concurrent drives each worker from a plain std::thread::spawn OS
// thread with no such context, so each call re-enters explicitly rather
// than assuming one.
struct OwnedClient<C: BenchClient> {
    inner: C,
    handle: Handle,
}

impl<C: BenchClient> BenchExecute for OwnedClient<C> {
    fn execute(
        &mut self,
        cypher: &str,
        timeout: Duration,
    ) -> Result<ExecResult, Box<dyn std::error::Error + Send + Sync>> {
        let _guard = self.handle.enter();
        let out = self
            .inner
            .execute(cypher, timeout)
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;
        Ok(ExecResult {
            row_count: out.rows.len(),
        })
    }
}

struct RpcFactory {
    addr: String,
    creds: NexusRpcCredentials,
    label: String,
    handle: Handle,
}

impl ClientFactory for RpcFactory {
    fn build(
        &self,
        _worker_id: usize,
    ) -> Result<Box<dyn BenchExecute + Send>, Box<dyn std::error::Error + Send + Sync>> {
        let client = self.handle.block_on(NexusRpcClient::connect(
            &self.addr,
            self.creds.clone(),
            self.label.clone(),
            self.handle.clone(),
        ))?;
        Ok(Box::new(OwnedClient {
            inner: client,
            handle: self.handle.clone(),
        }))
    }
}

#[cfg(feature = "neo4j")]
struct Neo4jFactory {
    url: String,
    user: String,
    password: String,
    label: String,
    handle: Handle,
}

#[cfg(feature = "neo4j")]
impl ClientFactory for Neo4jFactory {
    fn build(
        &self,
        _worker_id: usize,
    ) -> Result<Box<dyn BenchExecute + Send>, Box<dyn std::error::Error + Send + Sync>> {
        let client = self.handle.block_on(Neo4jBoltClient::connect(
            &self.url,
            &self.user,
            &self.password,
            self.label.clone(),
            self.handle.clone(),
        ))?;
        Ok(Box::new(OwnedClient {
            inner: client,
            handle: self.handle.clone(),
        }))
    }
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    enforce_release_build()?;

    let rt = Runtime::new()?;
    let handle = rt.handle().clone();

    println!("probing Nexus RPC {} (HELLO + PING) ...", args.rpc_addr);
    let creds = NexusRpcCredentials {
        api_key: args.rpc_api_key.clone(),
        username: args.rpc_user.clone(),
        password: args.rpc_password.clone(),
    };
    handle.block_on(NexusRpcClient::connect(
        &args.rpc_addr,
        creds.clone(),
        args.engine_label.clone(),
        handle.clone(),
    ))?;
    println!("nexus reachable over RPC.");

    #[cfg(feature = "neo4j")]
    if let Some(url) = &args.neo4j_url {
        println!("probing Neo4j {url} ...");
        handle.block_on(Neo4jBoltClient::connect(
            url.clone(),
            args.neo4j_user.clone(),
            args.neo4j_password.clone(),
            args.neo4j_engine_label.clone(),
            handle.clone(),
        ))?;
        println!("neo4j reachable over bolt.");
    }

    let catalogue = seed_scenarios();
    let mut all_rows: Vec<ConcurrentResult> = Vec::new();

    for scenario_id in &args.scenarios {
        let Some(scenario) = catalogue.iter().find(|s| &s.id == scenario_id) else {
            eprintln!("  !! unknown scenario id {scenario_id} -- skipped");
            continue;
        };

        for &workers in &args.workers {
            let cfg = ConcurrentRunConfig {
                workers,
                duration: Duration::from_secs(args.duration_secs),
                warmup: Duration::from_secs(args.warmup_secs),
            }
            .clamped();

            println!(
                "{} workers={workers} duration={}s ({})",
                scenario.id, args.duration_secs, args.engine_label
            );
            let factory = RpcFactory {
                addr: args.rpc_addr.clone(),
                creds: creds.clone(),
                label: args.engine_label.clone(),
                handle: handle.clone(),
            };
            match run_concurrent(scenario, &args.engine_label, &factory, &cfg) {
                Ok(r) => {
                    println!(
                        "  {}: qps={:.1} p50={}us p95={}us p99={}us (n={})",
                        args.engine_label, r.qps, r.p50_us, r.p95_us, r.p99_us, r.iterations
                    );
                    all_rows.push(r);
                }
                Err(e) => eprintln!("  !! {}: {} error: {e}", scenario.id, args.engine_label),
            }

            #[cfg(feature = "neo4j")]
            if let Some(url) = &args.neo4j_url {
                println!(
                    "{} workers={workers} duration={}s ({})",
                    scenario.id, args.duration_secs, args.neo4j_engine_label
                );
                let factory = Neo4jFactory {
                    url: url.clone(),
                    user: args.neo4j_user.clone(),
                    password: args.neo4j_password.clone(),
                    label: args.neo4j_engine_label.clone(),
                    handle: handle.clone(),
                };
                match run_concurrent(scenario, &args.neo4j_engine_label, &factory, &cfg) {
                    Ok(r) => {
                        println!(
                            "  {}: qps={:.1} p50={}us p95={}us p99={}us (n={})",
                            args.neo4j_engine_label,
                            r.qps,
                            r.p50_us,
                            r.p95_us,
                            r.p99_us,
                            r.iterations
                        );
                        all_rows.push(r);
                    }
                    Err(e) => eprintln!(
                        "  !! {}: {} error: {e}",
                        scenario.id, args.neo4j_engine_label
                    ),
                }
            }

            if let Some(dir) = &args.per_level_dir {
                std::fs::create_dir_all(dir)?;
                let level_rows: Vec<ConcurrentResult> = all_rows
                    .iter()
                    .filter(|r| r.workers == workers)
                    .cloned()
                    .collect();
                let neo4j_label = {
                    #[cfg(feature = "neo4j")]
                    {
                        args.neo4j_engine_label.clone()
                    }
                    #[cfg(not(feature = "neo4j"))]
                    {
                        "none".to_string()
                    }
                };
                let report = ConcurrentJsonReport::new(
                    format!("{}-vs-{}-workers-{workers}", args.engine_label, neo4j_label),
                    level_rows,
                );
                std::fs::write(
                    dir.join(format!("concurrent-{workers}.json")),
                    report.to_pretty_string()?,
                )?;
            }
        }
    }

    let combined = ConcurrentJsonReport::new(
        format!("{}-concurrent-sweep", args.engine_label),
        all_rows.clone(),
    );
    std::fs::write(&args.output, combined.to_pretty_string()?)?;
    println!("wrote {}", args.output.display());

    if let Some(md_path) = &args.markdown_output {
        std::fs::write(md_path, render_markdown(&all_rows))?;
        println!("wrote {}", md_path.display());
    }

    Ok(())
}
