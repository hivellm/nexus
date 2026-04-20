# Benchmark CI Gate Spec

## ADDED Requirements

### Requirement: Per-Scenario Budget File

The system SHALL honour a `bench/budget.toml` file declaring
per-scenario limits: `max_latency_p95_ms`, `max_ratio_vs_neo4j`,
`min_throughput_ops_per_sec`. A global `default_max_ratio` SHALL
apply when a scenario lacks an explicit entry.

#### Scenario: Global default applies
Given `default_max_ratio = 1.5` and no explicit entry for
  `traversals.one_hop_by_label`
When the budget check runs
Then the scenario SHALL be checked against `ratio ≤ 1.5`

#### Scenario: Explicit override wins
Given an explicit `max_ratio_vs_neo4j = 1.1` for
  `traversals.one_hop_by_label`
When the budget check runs
Then the scenario SHALL be checked against `ratio ≤ 1.1`

### Requirement: `nexus-bench check-budget` Command

The CLI SHALL expose `nexus-bench check-budget <budget.toml> <report.json>`
returning exit code 0 if all scenarios are within budget and exit
code 1 otherwise. Stdout SHALL list violations with scenario id,
measured value, and budget ceiling.

#### Scenario: Pass
Given all scenarios within their budget
When `check-budget` is run
Then the exit code SHALL be 0
And stdout SHALL be empty apart from a summary line

#### Scenario: Violation
Given one scenario exceeding its ratio ceiling
When `check-budget` is run
Then the exit code SHALL be 1
And stdout SHALL contain the offending scenario id and values

### Requirement: Baseline Comparison

The CLI SHALL expose `nexus-bench compare-baselines <baseline.json>
<current.json>` returning exit code 1 when more than 5% of scenarios
regress by ≥ 20% p95 latency versus the baseline.

#### Scenario: Release regression caught
Given a baseline where p95 of 100 scenarios averages 10 ms
When a new run regresses 10 scenarios by 30% each
Then `compare-baselines` SHALL exit with code 1
And the markdown output SHALL list the 10 regressed scenarios

### Requirement: PR Comment Integration

On a PR with label `perf-check`, the CI workflow SHALL post the
markdown report as a comment on the pull request.

#### Scenario: Comment posted
Given a PR labelled `perf-check`
When the workflow completes successfully
Then one new comment SHALL appear on the PR
And the comment body SHALL contain the markdown report text

### Requirement: Scheduled Baseline Update

A weekly scheduled workflow on the `main` branch SHALL run the full
suite and replace `bench/baselines/latest.json` with the new
results, committed by a bot.

#### Scenario: Weekly run updates baseline
Given the scheduled workflow runs on Monday 03:00 UTC
When the run completes
Then a new commit SHALL be pushed by `nexus-bench-bot`
And the commit message SHALL reference the weekly baseline update

### Requirement: Override via Label

A PR labelled `perf-exception` SHALL bypass the budget check, but
the PR description SHALL be required to contain a justification
section. The CI step SHALL check for that section and fail the PR
if absent.

#### Scenario: Override with justification
Given a PR labelled `perf-exception`
And the PR description contains a `## Performance exception` section
When the workflow runs
Then the budget check SHALL exit 0 regardless of measured values

#### Scenario: Override without justification
Given a PR labelled `perf-exception`
And no `## Performance exception` section in the description
When the workflow runs
Then the workflow SHALL fail with `ERR_MISSING_JUSTIFICATION`
