//! Scenario execution harness.
//!
//! Given a [`Scenario`] + a [`BenchClient`], the harness:
//!
//! 1. Runs `warmup_iters` iterations and discards the samples.
//! 2. Runs `measured_iters` iterations, collecting per-iteration
//!    latency + row count.
//! 3. On any row-count disagreement with `scenario.expected_row_count`
//!    surfaces a [`HarnessError::OutputDivergence`].
//! 4. Returns a [`ScenarioResult`] with p50 / p95 / p99 / min / max +
//!    throughput + the first timeout iteration (if any).
//!
//! The harness does NOT install the dataset — that's the caller's
//! responsibility, so multiple scenarios can share the same loaded
//! state. See [`crate::bin::nexus_bench`] for the end-to-end flow.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::client::{BenchClient, ClientError};
use crate::scenario::Scenario;

/// Result of a single [`Scenario`] run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioResult {
    /// Scenario id.
    pub scenario_id: String,
    /// Engine name (`"nexus"` / `"neo4j"`).
    pub engine: String,
    /// Per-iteration latencies in microseconds, in order.
    pub samples_us: Vec<u64>,
    /// Median.
    pub p50_us: u64,
    /// 95th percentile.
    pub p95_us: u64,
    /// 99th percentile.
    pub p99_us: u64,
    /// Fastest run.
    pub min_us: u64,
    /// Slowest run.
    pub max_us: u64,
    /// Arithmetic mean.
    pub mean_us: u64,
    /// Measured-iterations / total-measured-time.
    pub ops_per_second: f64,
    /// Row count the engine returned (cross-checked against
    /// scenario.expected_row_count).
    pub rows_returned: usize,
}

/// Errors the harness produces.
#[derive(Debug, Error)]
pub enum HarnessError {
    /// Client `execute` failed.
    #[error("client error during {phase}: {source}")]
    Client {
        phase: &'static str,
        #[source]
        source: ClientError,
    },
    /// Row-count disagreement.
    #[error("ERR_BENCH_OUTPUT_DIVERGENCE({scenario}): expected {expected} rows, got {actual}")]
    OutputDivergence {
        scenario: String,
        expected: usize,
        actual: usize,
    },
    /// Scenario asked for 0 measured iterations.
    #[error("scenario {0} declared 0 measured iterations")]
    NoIterations(String),
}

/// Per-run configuration toggles the CLI exposes.
#[derive(Debug, Clone)]
pub struct RunConfig {
    /// Multiplier applied to `scenario.measured_iters`. A quick dev
    /// loop uses 0.1, a canonical run uses 1.0, a release baseline
    /// might use 5.0.
    pub measured_multiplier: f64,
    /// Whether to call `BenchClient::reset` between measured runs.
    /// Off for read-only scenarios (cheaper); on for writes.
    pub reset_between: bool,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            measured_multiplier: 1.0,
            reset_between: false,
        }
    }
}

/// Drive a scenario against a client.
pub fn run_scenario(
    scenario: &Scenario,
    client: &mut dyn BenchClient,
    cfg: &RunConfig,
) -> Result<ScenarioResult, HarnessError> {
    let measured_iters =
        ((scenario.measured_iters as f64) * cfg.measured_multiplier).round() as u32;
    if measured_iters == 0 {
        return Err(HarnessError::NoIterations(scenario.id.clone()));
    }

    // Warmup — discard samples.
    for _ in 0..scenario.warmup_iters {
        let out = client
            .execute(&scenario.query, &scenario.parameters, scenario.timeout)
            .map_err(|e| HarnessError::Client {
                phase: "warmup",
                source: e,
            })?;
        assert_row_count(scenario, out.row_count())?;
        if cfg.reset_between {
            client.reset().map_err(|e| HarnessError::Client {
                phase: "warmup-reset",
                source: e,
            })?;
        }
    }

    // Measured loop.
    let mut samples_us = Vec::with_capacity(measured_iters as usize);
    let measured_wall_start = Instant::now();
    let mut last_rows = 0usize;
    for _ in 0..measured_iters {
        let start = Instant::now();
        let out = client
            .execute(&scenario.query, &scenario.parameters, scenario.timeout)
            .map_err(|e| HarnessError::Client {
                phase: "measured",
                source: e,
            })?;
        let elapsed = start.elapsed();
        samples_us.push(duration_to_us(elapsed));
        last_rows = out.row_count();
        assert_row_count(scenario, last_rows)?;
        if cfg.reset_between {
            client.reset().map_err(|e| HarnessError::Client {
                phase: "measured-reset",
                source: e,
            })?;
        }
    }
    let measured_wall = measured_wall_start.elapsed();

    let (p50, p95, p99, min, max, mean) = summarize(&samples_us);
    let ops_per_second = if measured_wall.is_zero() {
        0.0
    } else {
        measured_iters as f64 / measured_wall.as_secs_f64()
    };

    Ok(ScenarioResult {
        scenario_id: scenario.id.clone(),
        engine: client.engine_name().to_string(),
        samples_us,
        p50_us: p50,
        p95_us: p95,
        p99_us: p99,
        min_us: min,
        max_us: max,
        mean_us: mean,
        ops_per_second,
        rows_returned: last_rows,
    })
}

fn assert_row_count(scenario: &Scenario, actual: usize) -> Result<(), HarnessError> {
    if scenario.expected_row_count != actual {
        return Err(HarnessError::OutputDivergence {
            scenario: scenario.id.clone(),
            expected: scenario.expected_row_count,
            actual,
        });
    }
    Ok(())
}

fn duration_to_us(d: Duration) -> u64 {
    let micros = d.as_micros();
    if micros > u64::MAX as u128 {
        u64::MAX
    } else {
        micros as u64
    }
}

/// Returns `(p50, p95, p99, min, max, mean)` in µs.
fn summarize(samples: &[u64]) -> (u64, u64, u64, u64, u64, u64) {
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
    use crate::client::NexusClient;
    use crate::dataset::DatasetKind;
    use crate::scenario::ScenarioBuilder;

    fn scalar_scenario() -> Scenario {
        ScenarioBuilder::new(
            "scalar.one",
            "RETURN 1",
            DatasetKind::Micro,
            "RETURN 1 AS n",
        )
        .warmup(2)
        .measured(5)
        .expected_rows(1)
        .build()
    }

    #[test]
    fn run_succeeds_on_scalar_scenario() {
        let mut client = NexusClient::new().unwrap();
        let result = run_scenario(&scalar_scenario(), &mut client, &RunConfig::default()).unwrap();
        assert_eq!(result.engine, "nexus");
        assert_eq!(result.samples_us.len(), 5);
        assert!(result.p50_us >= result.min_us);
        assert!(result.p99_us >= result.p95_us);
        assert_eq!(result.rows_returned, 1);
    }

    #[test]
    fn run_flags_output_divergence() {
        let mut scen = scalar_scenario();
        scen.expected_row_count = 99; // wrong
        let mut client = NexusClient::new().unwrap();
        let err = run_scenario(&scen, &mut client, &RunConfig::default()).unwrap_err();
        match err {
            HarnessError::OutputDivergence {
                expected, actual, ..
            } => {
                assert_eq!(expected, 99);
                assert_eq!(actual, 1);
            }
            other => panic!("expected OutputDivergence, got {other:?}"),
        }
    }

    #[test]
    fn run_rejects_zero_measured_iters() {
        let scen = ScenarioBuilder::new("a.b", "", DatasetKind::Micro, "RETURN 1")
            .measured(1)
            .expected_rows(1)
            .build();
        let cfg = RunConfig {
            measured_multiplier: 0.0,
            ..Default::default()
        };
        let mut client = NexusClient::new().unwrap();
        let err = run_scenario(&scen, &mut client, &cfg).unwrap_err();
        assert!(matches!(err, HarnessError::NoIterations(_)));
    }

    #[test]
    fn summarize_empty_returns_zero() {
        assert_eq!(summarize(&[]), (0, 0, 0, 0, 0, 0));
    }

    #[test]
    fn summarize_single_sample() {
        let (p50, p95, p99, min, max, mean) = summarize(&[42]);
        assert_eq!(p50, 42);
        assert_eq!(p95, 42);
        assert_eq!(p99, 42);
        assert_eq!(min, 42);
        assert_eq!(max, 42);
        assert_eq!(mean, 42);
    }

    #[test]
    fn summarize_percentiles_are_monotonic() {
        let samples: Vec<u64> = (1..=100).collect();
        let (p50, p95, p99, min, max, mean) = summarize(&samples);
        assert!(p50 <= p95 && p95 <= p99);
        assert_eq!(min, 1);
        assert_eq!(max, 100);
        assert!((40..=60).contains(&mean));
    }

    #[test]
    fn run_config_default_does_not_reset() {
        let cfg = RunConfig::default();
        assert!(!cfg.reset_between);
        assert!((cfg.measured_multiplier - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn duration_to_us_clamps_overflow() {
        let huge = Duration::from_secs(u64::MAX);
        assert_eq!(duration_to_us(huge), u64::MAX);
    }
}
