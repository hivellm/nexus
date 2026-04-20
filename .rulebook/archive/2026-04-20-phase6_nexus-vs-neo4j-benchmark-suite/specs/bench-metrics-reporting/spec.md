# Benchmark Metrics & Reporting Spec

## ADDED Requirements

### Requirement: Four Latency Metrics per Scenario

Every scenario run SHALL report p50, p95, p99 latency in
milliseconds and throughput in operations-per-second for each
engine.

#### Scenario: Report contains all four metrics
Given a completed run
When the JSON report is inspected
Then every scenario entry SHALL contain fields `p50_ms`, `p95_ms`,
  `p99_ms`, `ops_per_sec` for both `nexus` and `neo4j` sub-objects

### Requirement: Ratio and Classification

Every scenario SHALL be annotated with a `ratio` (Nexus p95 / Neo4j p95)
and a `class` string in `{"Lead", "Parity", "Behind", "Gap",
"HighVariance", "Timeout", "Error"}`.

#### Scenario: Parity band
Given a scenario with Nexus p95 = 10 ms and Neo4j p95 = 11 ms
When the report is generated
Then `ratio` SHALL be approximately `0.91`
And `class` SHALL be `"Parity"`

#### Scenario: Gap band
Given a scenario with Nexus p95 = 50 ms and Neo4j p95 = 20 ms
When the report is generated
Then `ratio` SHALL be `2.50`
And `class` SHALL be `"Gap"`

### Requirement: Markdown Report

The harness SHALL produce `bench/report.md` grouping scenarios by
category, with a table per category showing id, Nexus p95, Neo4j
p95, ratio, and class glyph.

#### Scenario: Markdown render
Given a completed run with scenarios in five categories
When `nexus-bench report --format markdown` is executed
Then `bench/report.md` SHALL contain five `## <category>` sections
And each section SHALL contain a table with the required columns

### Requirement: Machine-Readable JSON

The harness SHALL emit `bench/report.json` whose schema is stable
across minor releases and documented in `docs/benchmarks/SCHEMA.md`.

#### Scenario: Schema is stable
Given a report produced at version `v1`
And a report produced at version `v1.1`
When the JSON schemas are compared
Then the `v1.1` schema SHALL be a superset of `v1` (no field removal)

### Requirement: SQLite Trace History

The harness SHALL append one row per scenario per run to
`bench/trace.sqlite` with columns
`(run_id, ts, commit, scenario_id, nexus_p95, neo4j_p95, ratio)`.

#### Scenario: Trace row appended
Given an existing `trace.sqlite` with N rows
When a new run produces M scenarios
Then after the run the DB SHALL contain exactly `N + M` rows

### Requirement: Metadata Capture

Every report SHALL include a top-level `meta` object with fields
`nexus_version`, `neo4j_version`, `host`, `cpu_model`,
`kernel_version`, `docker_version`, `generated_at`.

#### Scenario: Metadata present
When `report.json` is produced
Then the top-level object SHALL contain a `meta` object with all
  seven fields populated
