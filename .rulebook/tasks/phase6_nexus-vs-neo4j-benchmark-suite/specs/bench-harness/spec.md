# Benchmark Harness Spec

## ADDED Requirements

### Requirement: `BenchClient` Trait

The system SHALL expose a `BenchClient` trait with methods `name`,
`reset`, `load_dataset`, `execute`, and `metrics_snapshot`. Three
implementations SHALL be provided: `NexusEmbedded`, `NexusRest`, and
`Neo4jBolt`.

#### Scenario: Three clients selectable by flag
Given the `nexus-bench` binary
When invoked with `--engine nexus-embedded`, `--engine nexus-rest`,
  and `--engine neo4j-bolt` respectively
Then each invocation SHALL succeed in running a trivial scenario
And the report SHALL identify the engine used

### Requirement: Measurement Loop

Each scenario SHALL be executed with a warmup phase followed by a
measurement phase. Measured samples SHALL be timed with nanosecond
resolution via `Instant::now`.

#### Scenario: Sample count matches config
Given a scenario with `warmup_iters = 100, measured_iters = 500`
When the harness runs the scenario
Then exactly 500 latency samples SHALL be recorded
And the report SHALL derive p50, p95, p99 from those samples

### Requirement: Output Divergence Guard

Before measurement, the harness SHALL compare the first response
from the engine to the scenario's `OutputExpectation`. Any mismatch
SHALL raise `ERR_BENCH_OUTPUT_DIVERGENCE` and abort the scenario.

#### Scenario: Row count mismatch
Given a scenario with `expected = RowCount(10)`
When the engine returns 9 rows
Then the harness SHALL abort the scenario
And the error code in the report SHALL be `ERR_BENCH_OUTPUT_DIVERGENCE`

### Requirement: Timeout Enforcement

Each scenario SHALL time out after `timeout_ms`. A timed-out run
SHALL be recorded as `class = "Timeout"` and the scenario SHALL NOT
block the rest of the suite.

#### Scenario: Scenario exceeds timeout
Given a scenario configured with `timeout_ms = 5000`
When an execution takes longer than 5 seconds
Then the execution SHALL be cancelled
And the scenario SHALL appear in the report with a timeout marker

### Requirement: CPU Pinning and Variance Sanity Check

On Linux, the harness SHALL pin itself to a fixed CPU subset via
`taskset` or `sched_setaffinity`. If the measured stdev exceeds 20%
of the mean, the scenario SHALL be flagged `HIGH_VARIANCE` in the
report.

#### Scenario: High variance flagged
Given a scenario whose samples have stdev = 0.35 × mean
When the report is generated
Then the scenario row SHALL include a `HIGH_VARIANCE` flag

### Requirement: Engine Metrics Collection

For each scenario the harness SHALL capture `EngineMetrics` from
both engines containing at least `rss_mb` and `cpu_time_ms_delta`
across the measurement window.

#### Scenario: RSS delta reported
Given any scenario
When the report is generated
Then the JSON for each scenario SHALL include `nexus.rss_mb` and
  `neo4j.rss_mb` as non-negative numbers
