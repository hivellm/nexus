//! Scenario execution harness.
//!
//! Accepts anything that implements a narrow **execute** trait (see
//! [`BenchExecute`]); the HTTP client lives behind the `live-bench`
//! feature flag. Every call is bounded by a per-scenario timeout that
//! itself can't exceed [`crate::scenario::MAX_TIMEOUT`]; the number
//! of measured iterations can't exceed
//! [`crate::scenario::MAX_MEASURED_ITERS`]. These are hard ceilings
//! — the runner cannot be configured around them.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::scenario::Scenario;

/// The narrow execute contract the harness needs. Implementations
/// (typically the HTTP [`crate::client::HttpClient`] under the
/// `live-bench` feature) enforce their own I/O-level timeout in
/// addition to whatever the harness supplies.
pub trait BenchExecute {
    /// Execute `cypher` against the underlying engine. The
    /// implementation MUST honour `timeout` and return early with an
    /// error that surfaces through [`HarnessError::Client`].
    fn execute(
        &mut self,
        cypher: &str,
        timeout: Duration,
    ) -> Result<ExecResult, Box<dyn std::error::Error + Send + Sync>>;
}

/// Minimal execute result the harness needs. Each client converts
/// its own rich result type into this shape.
#[derive(Debug, Clone, PartialEq)]
pub struct ExecResult {
    /// Number of rows the engine returned.
    pub row_count: usize,
}

/// Per-run knobs.
#[derive(Debug, Clone)]
pub struct RunConfig {
    /// Multiplier applied to `scenario.measured_iters`. Allows a dev
    /// loop at `0.1` and a baseline run at `1.0`. A ceiling of
    /// [`MAX_MULTIPLIER`] is enforced so a typo can't spin up
    /// thousands of calls.
    pub measured_multiplier: f64,
}

/// Hard upper bound on the measured-multiplier knob.
pub const MAX_MULTIPLIER: f64 = 5.0;

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            measured_multiplier: 1.0,
        }
    }
}

impl RunConfig {
    /// Clamp the multiplier to `(0, MAX_MULTIPLIER]`. Values outside
    /// the range are rewritten to the nearest valid one.
    pub fn clamped(mut self) -> Self {
        if !self.measured_multiplier.is_finite() || self.measured_multiplier <= 0.0 {
            self.measured_multiplier = 1.0;
        }
        if self.measured_multiplier > MAX_MULTIPLIER {
            self.measured_multiplier = MAX_MULTIPLIER;
        }
        self
    }
}

/// Result of a single scenario run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioResult {
    /// Scenario id.
    pub scenario_id: String,
    /// Engine label — whatever the client reported.
    pub engine: String,
    /// Per-iteration latencies in microseconds.
    pub samples_us: Vec<u64>,
    /// p50 / p95 / p99 / min / max / mean — all in µs.
    pub p50_us: u64,
    pub p95_us: u64,
    pub p99_us: u64,
    pub min_us: u64,
    pub max_us: u64,
    pub mean_us: u64,
    /// Measured iterations divided by total measured wall time.
    pub ops_per_second: f64,
    /// Row count the engine actually returned (cross-checked against
    /// the scenario's `expected_row_count`).
    pub rows_returned: usize,
}

/// Errors the harness surfaces.
#[derive(Debug, Error)]
pub enum HarnessError {
    /// Client `execute` failed.
    #[error("client error during {phase}: {source}")]
    Client {
        phase: &'static str,
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    /// Row-count divergence — the engine returned a different row
    /// count than the scenario declared. Typically means the engine
    /// hit a bug; surfaced loudly rather than silently affecting
    /// latency numbers.
    #[error("ERR_BENCH_OUTPUT_DIVERGENCE({scenario}): expected {expected} rows, got {actual}")]
    OutputDivergence {
        scenario: String,
        expected: usize,
        actual: usize,
    },
    /// Scenario asked for 0 measured iterations after the multiplier
    /// was applied. Usually a `--measured-multiplier 0.001` typo.
    #[error("scenario {0} evaluated to 0 measured iterations")]
    NoIterations(String),
}

/// Drive a scenario to completion. Generic over the client so the
/// harness stays usable under the HTTP feature or by future non-HTTP
/// clients without any plumbing change.
pub fn run_scenario<C: BenchExecute>(
    scenario: &Scenario,
    engine_label: &str,
    client: &mut C,
    cfg: &RunConfig,
) -> Result<ScenarioResult, HarnessError> {
    let cfg = cfg.clone().clamped();
    let measured_iters = ((scenario.measured_iters as f64) * cfg.measured_multiplier)
        .round()
        .max(0.0) as u32;
    if measured_iters == 0 {
        return Err(HarnessError::NoIterations(scenario.id.clone()));
    }

    // Warmup — discard samples, still enforce the divergence guard.
    for _ in 0..scenario.warmup_iters {
        let out = client
            .execute(&scenario.query, scenario.timeout)
            .map_err(|e| HarnessError::Client {
                phase: "warmup",
                source: e,
            })?;
        assert_row_count(scenario, out.row_count)?;
    }

    // Measured loop.
    let mut samples_us = Vec::with_capacity(measured_iters as usize);
    let measured_wall_start = Instant::now();
    let mut last_rows = 0usize;
    for _ in 0..measured_iters {
        let start = Instant::now();
        let out = client
            .execute(&scenario.query, scenario.timeout)
            .map_err(|e| HarnessError::Client {
                phase: "measured",
                source: e,
            })?;
        let elapsed = start.elapsed();
        samples_us.push(duration_to_us(elapsed));
        last_rows = out.row_count;
        assert_row_count(scenario, last_rows)?;
    }
    let measured_wall = measured_wall_start.elapsed();

    let (p50, p95, p99, min, max, mean) = summarize(&samples_us);
    let ops_per_second = if measured_wall.is_zero() {
        0.0
    } else {
        f64::from(measured_iters) / measured_wall.as_secs_f64()
    };

    Ok(ScenarioResult {
        scenario_id: scenario.id.clone(),
        engine: engine_label.to_string(),
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
    if micros > u128::from(u64::MAX) {
        u64::MAX
    } else {
        micros as u64
    }
}

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
    use crate::dataset::DatasetKind;
    use crate::scenario::ScenarioBuilder;

    /// In-memory mock — zero I/O, zero engine, used to prove the
    /// harness's pure logic without crossing the process boundary.
    struct MockClient {
        rows: usize,
        calls: u32,
        fail_after: Option<u32>,
    }

    impl BenchExecute for MockClient {
        fn execute(
            &mut self,
            _cypher: &str,
            _timeout: Duration,
        ) -> Result<ExecResult, Box<dyn std::error::Error + Send + Sync>> {
            self.calls += 1;
            if matches!(self.fail_after, Some(n) if self.calls > n) {
                return Err("mock explosion".into());
            }
            Ok(ExecResult {
                row_count: self.rows,
            })
        }
    }

    fn sc(rows: usize) -> Scenario {
        ScenarioBuilder::new("unit.one", "", DatasetKind::Tiny, "RETURN 1")
            .warmup(1)
            .measured(3)
            .expected_rows(rows)
            .build()
    }

    #[test]
    fn runs_to_completion_against_mock() {
        let mut client = MockClient {
            rows: 1,
            calls: 0,
            fail_after: None,
        };
        let r = run_scenario(&sc(1), "mock", &mut client, &RunConfig::default()).unwrap();
        assert_eq!(r.engine, "mock");
        assert_eq!(r.samples_us.len(), 3);
        assert!(r.p50_us <= r.max_us);
    }

    #[test]
    fn divergence_guard_fires() {
        let mut client = MockClient {
            rows: 2,
            calls: 0,
            fail_after: None,
        };
        let err = run_scenario(&sc(1), "mock", &mut client, &RunConfig::default()).unwrap_err();
        assert!(matches!(err, HarnessError::OutputDivergence { .. }));
    }

    #[test]
    fn client_error_surfaces() {
        let mut client = MockClient {
            rows: 1,
            calls: 0,
            fail_after: Some(0),
        };
        let err = run_scenario(&sc(1), "mock", &mut client, &RunConfig::default()).unwrap_err();
        assert!(matches!(err, HarnessError::Client { .. }));
    }

    #[test]
    fn zero_iters_rejected() {
        let mut client = MockClient {
            rows: 1,
            calls: 0,
            fail_after: None,
        };
        let cfg = RunConfig {
            measured_multiplier: 0.0001,
        };
        let err = run_scenario(&sc(1), "mock", &mut client, &cfg).unwrap_err();
        assert!(matches!(err, HarnessError::NoIterations(_)));
    }

    #[test]
    fn multiplier_clamp_upper_bound() {
        let cfg = RunConfig {
            measured_multiplier: 1000.0,
        }
        .clamped();
        assert!(cfg.measured_multiplier <= MAX_MULTIPLIER);
    }

    #[test]
    fn multiplier_clamp_nan_falls_back() {
        let cfg = RunConfig {
            measured_multiplier: f64::NAN,
        }
        .clamped();
        assert_eq!(cfg.measured_multiplier, 1.0);
    }

    #[test]
    fn summarize_single_sample() {
        assert_eq!(summarize(&[42]), (42, 42, 42, 42, 42, 42));
    }

    #[test]
    fn summarize_empty_is_zeros() {
        assert_eq!(summarize(&[]), (0, 0, 0, 0, 0, 0));
    }

    #[test]
    fn summarize_percentiles_are_monotonic() {
        let s: Vec<u64> = (1..=100).collect();
        let (p50, p95, p99, _, _, _) = summarize(&s);
        assert!(p50 <= p95 && p95 <= p99);
    }

    #[test]
    fn duration_to_us_clamps_overflow() {
        assert_eq!(duration_to_us(Duration::from_secs(u64::MAX)), u64::MAX);
    }
}
