//! Concurrent-load harness.
//!
//! The single-threaded [`crate::harness`] measures *engine* latency
//! under a serial driver. This module measures *system* throughput
//! under a chosen concurrency level — the metric that matters when
//! the workload has many simultaneous clients (web tier behind a
//! load balancer, RAG pipeline issuing dozens of retrieval requests
//! per LLM call, multi-tenant SaaS).
//!
//! The contract is the same shape as [`crate::harness::run_scenario`]:
//!
//! * Pure logic — `BenchExecute` is the only seam. The same
//!   [`crate::client::HttpClient`] / Bolt client / mock plug into
//!   either harness.
//! * Hard ceilings on duration and worker count so a typo cannot
//!   wedge the runner.
//! * Per-iteration row-count divergence guard.
//!
//! Output shape pairs every concurrency level with its qps,
//! p50/p95/p99 latency, and a `cpu_util_estimate_pct` that the
//! orchestrator script populates from outside the harness (we
//! cannot read the server's CPU usage from here).

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::harness::{BenchExecute, ExecResult, HarnessError};
use crate::scenario::Scenario;

/// Hard upper bound on `workers`. Above this we are testing the
/// thread scheduler, not the engine.
pub const MAX_WORKERS: usize = 256;

/// Hard upper bound on `duration`. Above this the run drifts into
/// "soak test" territory where memory / GC effects dominate the
/// signal we want.
pub const MAX_DURATION: Duration = Duration::from_secs(120);

/// Knobs for [`run_concurrent`].
#[derive(Debug, Clone)]
pub struct ConcurrentRunConfig {
    /// Number of worker threads issuing concurrent requests.
    /// Clamped to `(0, MAX_WORKERS]`.
    pub workers: usize,
    /// Total wall-clock duration of the measured loop. Clamped to
    /// `(0, MAX_DURATION]`. Each worker stops at the next
    /// scenario completion after the deadline.
    pub duration: Duration,
    /// Optional warmup window before the measured loop begins.
    /// Samples taken during warmup are discarded but the divergence
    /// guard still runs.
    pub warmup: Duration,
}

impl Default for ConcurrentRunConfig {
    fn default() -> Self {
        Self {
            workers: 4,
            duration: Duration::from_secs(15),
            warmup: Duration::from_secs(2),
        }
    }
}

impl ConcurrentRunConfig {
    /// Clamp every field to a sane range. Values outside the range
    /// are rewritten silently — same behaviour as
    /// [`crate::harness::RunConfig::clamped`].
    pub fn clamped(mut self) -> Self {
        if self.workers == 0 {
            self.workers = 1;
        }
        if self.workers > MAX_WORKERS {
            self.workers = MAX_WORKERS;
        }
        if self.duration.is_zero() {
            self.duration = Duration::from_secs(1);
        }
        if self.duration > MAX_DURATION {
            self.duration = MAX_DURATION;
        }
        if self.warmup > self.duration {
            self.warmup = self.duration / 5;
        }
        self
    }
}

/// One row of the concurrent sweep output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcurrentResult {
    /// Scenario id (one row per scenario × concurrency level).
    pub scenario_id: String,
    /// Engine label.
    pub engine: String,
    /// Configured worker count for this row.
    pub workers: usize,
    /// Measured wall-clock duration in milliseconds.
    pub wall_ms: u64,
    /// Total successful iterations across every worker.
    pub iterations: u64,
    /// Iterations divided by wall time. The headline number.
    pub qps: f64,
    /// Latency summary aggregated across every worker (microseconds).
    pub p50_us: u64,
    pub p95_us: u64,
    pub p99_us: u64,
    pub min_us: u64,
    pub max_us: u64,
    pub mean_us: u64,
    /// Number of rows the engine returned on the **last** completed
    /// iteration. The divergence guard ensures every iteration
    /// matched the scenario's expected row count, so this is also
    /// the row count for *every* completed iteration.
    pub rows_returned: usize,
    /// Server-side CPU utilisation (0..=100). Populated by the
    /// orchestrator script from outside the harness; defaults to
    /// `None` because Rust cannot read the server's `top` from here.
    pub cpu_util_estimate_pct: Option<f64>,
}

/// Factory that returns one `BenchExecute` per worker. The factory
/// is called `workers` times before the measured loop begins —
/// clients are not shared between workers. This keeps client-side
/// state (HTTP keep-alive, RPC connection) isolated and avoids
/// surfacing client-internal contention as engine concurrency.
pub trait ClientFactory {
    /// Build a fresh client for one worker. May fail if the worker
    /// can't connect.
    fn build(
        &self,
        worker_id: usize,
    ) -> Result<Box<dyn BenchExecute + Send>, Box<dyn std::error::Error + Send + Sync>>;
}

/// Drive a scenario at the chosen concurrency. Returns one
/// [`ConcurrentResult`] aggregating every worker's samples.
pub fn run_concurrent<F: ClientFactory + Sync>(
    scenario: &Scenario,
    engine_label: &str,
    factory: &F,
    cfg: &ConcurrentRunConfig,
) -> Result<ConcurrentResult, HarnessError> {
    let cfg = cfg.clone().clamped();

    // Build one client per worker before the measurement begins so
    // factory failures surface immediately and aren't conflated with
    // engine errors.
    let mut clients: Vec<Box<dyn BenchExecute + Send>> = Vec::with_capacity(cfg.workers);
    for w in 0..cfg.workers {
        let c = factory.build(w).map_err(|e| HarnessError::Client {
            phase: "factory",
            source: e,
        })?;
        clients.push(c);
    }

    // Shared state across worker threads. Workers push their
    // per-iteration latencies and the shared row count; the parent
    // thread drains everything after the deadline.
    let stop = Arc::new(AtomicBool::new(false));
    let warmup_done = Arc::new(AtomicBool::new(cfg.warmup.is_zero()));
    let total_iters = Arc::new(AtomicU64::new(0));
    let last_rows = Arc::new(AtomicUsize::new(0));
    let any_divergence = Arc::new(AtomicBool::new(false));
    let any_client_error: Arc<Mutex<Option<HarnessError>>> = Arc::new(Mutex::new(None));

    let scenario_arc = Arc::new(scenario.clone());
    let measure_start = Instant::now();
    let warmup_until = if cfg.warmup.is_zero() {
        measure_start
    } else {
        measure_start + cfg.warmup
    };
    let deadline = warmup_until + cfg.duration;

    let mut handles = Vec::with_capacity(cfg.workers);
    for (worker_id, mut client) in clients.into_iter().enumerate() {
        let stop = Arc::clone(&stop);
        let warmup_done = Arc::clone(&warmup_done);
        let total_iters = Arc::clone(&total_iters);
        let last_rows = Arc::clone(&last_rows);
        let any_divergence = Arc::clone(&any_divergence);
        let any_client_error = Arc::clone(&any_client_error);
        let scenario = Arc::clone(&scenario_arc);

        let handle = thread::Builder::new()
            .name(format!("nexus-bench-conc-{worker_id}"))
            .spawn(move || -> Vec<u64> {
                let mut local_samples = Vec::with_capacity(1024);
                let scenario = scenario.as_ref();
                while !stop.load(Ordering::Relaxed) {
                    let now = Instant::now();
                    if now >= deadline {
                        break;
                    }
                    let in_warmup = now < warmup_until;
                    let start = Instant::now();
                    let res = client.execute(&scenario.query, scenario.timeout);
                    let elapsed = start.elapsed();
                    match res {
                        Ok(ExecResult { row_count }) => {
                            if scenario.expected_row_count != row_count {
                                any_divergence.store(true, Ordering::Relaxed);
                                stop.store(true, Ordering::Relaxed);
                                break;
                            }
                            last_rows.store(row_count, Ordering::Relaxed);
                            if in_warmup {
                                continue;
                            }
                            // Mark warmup-complete on the first
                            // measured iteration so cross-worker
                            // observers can switch their reading
                            // semantics.
                            if !warmup_done.load(Ordering::Acquire) {
                                warmup_done.store(true, Ordering::Release);
                            }
                            local_samples.push(duration_to_us(elapsed));
                            total_iters.fetch_add(1, Ordering::Relaxed);
                        }
                        Err(e) => {
                            // Capture the first client error so the
                            // parent can surface it. Subsequent
                            // workers also notice `stop` and exit.
                            if let Ok(mut slot) = any_client_error.lock()
                                && slot.is_none()
                            {
                                *slot = Some(HarnessError::Client {
                                    phase: "concurrent_measured",
                                    source: e,
                                });
                            }
                            stop.store(true, Ordering::Relaxed);
                            break;
                        }
                    }
                }
                local_samples
            })
            .expect("spawn worker thread");
        handles.push(handle);
    }

    // Park the main thread until the deadline; workers exit on their
    // own. We don't `park_timeout` because the deadline is computed
    // off the same `Instant`.
    while Instant::now() < deadline && !stop.load(Ordering::Relaxed) {
        thread::sleep(Duration::from_millis(50));
    }
    stop.store(true, Ordering::Relaxed);

    let mut all_samples: Vec<u64> = Vec::new();
    for handle in handles {
        match handle.join() {
            Ok(samples) => all_samples.extend(samples),
            Err(_) => {
                return Err(HarnessError::Client {
                    phase: "join",
                    source: "worker thread panicked".into(),
                });
            }
        }
    }

    if any_divergence.load(Ordering::Relaxed) {
        return Err(HarnessError::OutputDivergence {
            scenario: scenario.id.clone(),
            expected: scenario.expected_row_count,
            actual: last_rows.load(Ordering::Relaxed),
        });
    }
    if let Ok(mut slot) = any_client_error.lock()
        && let Some(e) = slot.take()
    {
        return Err(e);
    }

    let wall = Instant::now().saturating_duration_since(warmup_until);
    let iterations = total_iters.load(Ordering::Relaxed);
    let qps = if wall.is_zero() {
        0.0
    } else {
        iterations as f64 / wall.as_secs_f64()
    };
    let (p50, p95, p99, min, max, mean) = summarise(&all_samples);

    Ok(ConcurrentResult {
        scenario_id: scenario.id.clone(),
        engine: engine_label.to_string(),
        workers: cfg.workers,
        wall_ms: wall.as_millis() as u64,
        iterations,
        qps,
        p50_us: p50,
        p95_us: p95,
        p99_us: p99,
        min_us: min,
        max_us: max,
        mean_us: mean,
        rows_returned: last_rows.load(Ordering::Relaxed),
        cpu_util_estimate_pct: None,
    })
}

fn duration_to_us(d: Duration) -> u64 {
    let micros = d.as_micros();
    if micros > u128::from(u64::MAX) {
        u64::MAX
    } else {
        micros as u64
    }
}

fn summarise(samples: &[u64]) -> (u64, u64, u64, u64, u64, u64) {
    if samples.is_empty() {
        return (0, 0, 0, 0, 0, 0);
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let pct = |p: f64| {
        let rank = ((p / 100.0) * (sorted.len() as f64 - 1.0)).round() as usize;
        sorted[rank.min(sorted.len() - 1)]
    };
    let min = sorted[0];
    let max = *sorted.last().unwrap();
    let mean = sorted.iter().sum::<u64>() / (sorted.len() as u64);
    (pct(50.0), pct(95.0), pct(99.0), min, max, mean)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dataset::DatasetKind;
    use crate::scenario::ScenarioBuilder;
    use std::sync::atomic::AtomicU32;

    /// Mock client backed by an atomic call counter so concurrent
    /// workers can coexist without locks.
    struct MockClient {
        rows: usize,
        latency: Duration,
        calls: Arc<AtomicU32>,
        fail_after: Option<u32>,
    }

    impl BenchExecute for MockClient {
        fn execute(
            &mut self,
            _cypher: &str,
            _timeout: Duration,
        ) -> Result<ExecResult, Box<dyn std::error::Error + Send + Sync>> {
            let now = self.calls.fetch_add(1, Ordering::Relaxed);
            if matches!(self.fail_after, Some(n) if now > n) {
                return Err("mock explosion".into());
            }
            if !self.latency.is_zero() {
                thread::sleep(self.latency);
            }
            Ok(ExecResult {
                row_count: self.rows,
            })
        }
    }

    struct MockFactory {
        rows: usize,
        latency: Duration,
        calls: Arc<AtomicU32>,
        fail_after: Option<u32>,
    }

    impl ClientFactory for MockFactory {
        fn build(
            &self,
            _worker_id: usize,
        ) -> Result<Box<dyn BenchExecute + Send>, Box<dyn std::error::Error + Send + Sync>>
        {
            Ok(Box::new(MockClient {
                rows: self.rows,
                latency: self.latency,
                calls: Arc::clone(&self.calls),
                fail_after: self.fail_after,
            }))
        }
    }

    fn sc(rows: usize) -> Scenario {
        ScenarioBuilder::new("conc.unit", "", DatasetKind::Tiny, "RETURN 1")
            .warmup(0)
            .measured(1)
            .expected_rows(rows)
            .build()
    }

    #[test]
    fn config_clamp_handles_silly_values() {
        let cfg = ConcurrentRunConfig {
            workers: 0,
            duration: Duration::ZERO,
            warmup: Duration::from_secs(60),
        }
        .clamped();
        assert_eq!(cfg.workers, 1);
        assert!(cfg.duration > Duration::ZERO);
        assert!(cfg.warmup <= cfg.duration);

        let cfg = ConcurrentRunConfig {
            workers: 10_000,
            duration: Duration::from_secs(10_000),
            warmup: Duration::ZERO,
        }
        .clamped();
        assert_eq!(cfg.workers, MAX_WORKERS);
        assert_eq!(cfg.duration, MAX_DURATION);
    }

    #[test]
    fn run_concurrent_aggregates_samples_across_workers() {
        let calls = Arc::new(AtomicU32::new(0));
        let factory = MockFactory {
            rows: 1,
            latency: Duration::from_micros(100),
            calls: Arc::clone(&calls),
            fail_after: None,
        };
        let cfg = ConcurrentRunConfig {
            workers: 4,
            duration: Duration::from_millis(150),
            warmup: Duration::ZERO,
        };
        let r = run_concurrent(&sc(1), "mock", &factory, &cfg).expect("run");
        assert_eq!(r.engine, "mock");
        assert_eq!(r.workers, 4);
        assert!(r.iterations > 0, "concurrent loop produced no samples");
        // qps must be positive given the loop ran for >= 1ms.
        assert!(r.qps > 0.0);
        // The reported rows_returned must match the scenario's expectation.
        assert_eq!(r.rows_returned, 1);
        // Latency percentiles obey ordering.
        assert!(r.p50_us <= r.p95_us);
        assert!(r.p95_us <= r.p99_us);
    }

    #[test]
    fn divergence_guard_fires_in_concurrent_mode() {
        let factory = MockFactory {
            rows: 2, // scenario expects 1 — divergence
            latency: Duration::from_micros(50),
            calls: Arc::new(AtomicU32::new(0)),
            fail_after: None,
        };
        let cfg = ConcurrentRunConfig {
            workers: 2,
            duration: Duration::from_millis(100),
            warmup: Duration::ZERO,
        };
        let err = run_concurrent(&sc(1), "mock", &factory, &cfg).unwrap_err();
        assert!(matches!(err, HarnessError::OutputDivergence { .. }));
    }

    #[test]
    fn client_error_surfaces_in_concurrent_mode() {
        let factory = MockFactory {
            rows: 1,
            latency: Duration::from_micros(50),
            calls: Arc::new(AtomicU32::new(0)),
            fail_after: Some(2),
        };
        let cfg = ConcurrentRunConfig {
            workers: 2,
            duration: Duration::from_millis(200),
            warmup: Duration::ZERO,
        };
        let err = run_concurrent(&sc(1), "mock", &factory, &cfg).unwrap_err();
        assert!(matches!(err, HarnessError::Client { .. }));
    }

    #[test]
    fn warmup_samples_are_excluded() {
        let calls = Arc::new(AtomicU32::new(0));
        let factory = MockFactory {
            rows: 1,
            latency: Duration::from_millis(2),
            calls: Arc::clone(&calls),
            fail_after: None,
        };
        let cfg = ConcurrentRunConfig {
            workers: 2,
            duration: Duration::from_millis(100),
            warmup: Duration::from_millis(50),
        };
        let r = run_concurrent(&sc(1), "mock", &factory, &cfg).expect("run");
        // Total client calls (`calls` atomic) covers both warmup and
        // measured iterations; the reported `iterations` field is
        // measured-only and must be strictly less.
        let total = calls.load(Ordering::Relaxed) as u64;
        assert!(
            r.iterations < total,
            "iterations {} must be < total client calls {}",
            r.iterations,
            total
        );
    }
}
