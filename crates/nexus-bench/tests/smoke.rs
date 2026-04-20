//! End-to-end smoke tests.
//!
//! Exercises the full harness pipeline: install the `micro` dataset
//! into a fresh in-process Nexus engine, run a subset of the seed
//! scenario catalogue with reduced iteration counts, then emit both
//! report formats. Any regression in the glue between modules lights
//! up here instead of via a flaky benchmark run.

use std::time::Duration;

use nexus_bench::{
    ComparativeRow, Dataset, MicroDataset, NexusClient, RunConfig,
    harness::run_scenario,
    report::{json::JsonReport, markdown::MarkdownReport},
    scenario::{Scenario, ScenarioBuilder},
    scenario_catalog::seed_scenarios,
};

/// Trim every seed scenario's iteration counts so the whole suite
/// runs in under a second. Smoke tests measure correctness of the
/// glue, not throughput numbers.
fn quick_scenarios() -> Vec<Scenario> {
    seed_scenarios()
        .into_iter()
        .map(|mut s| {
            s.warmup_iters = 1;
            s.measured_iters = 2;
            s
        })
        .collect()
}

#[test]
fn full_harness_end_to_end() {
    let mut client = NexusClient::new().expect("client");
    MicroDataset::default()
        .load(&mut client)
        .expect("micro dataset loads");

    let cfg = RunConfig::default();
    let scenarios = quick_scenarios();
    let mut rows = Vec::new();
    for scen in &scenarios {
        let nexus = run_scenario(scen, &mut client, &cfg).expect(&scen.id);
        rows.push(ComparativeRow::new(nexus, None));
    }

    assert_eq!(rows.len(), scenarios.len());
    for row in &rows {
        // Neo4j wasn't run — classification must be None, but the
        // Nexus side must have a valid percentile number.
        assert!(row.classification.is_none());
        assert!(row.nexus.samples_us.len() >= 2);
    }
}

#[test]
fn markdown_report_renders_without_panics() {
    let mut client = NexusClient::new().unwrap();
    MicroDataset::default().load(&mut client).unwrap();
    let scen = ScenarioBuilder::new(
        "smoke.literal",
        "RETURN 1",
        nexus_bench::dataset::DatasetKind::Micro,
        "RETURN 1 AS n",
    )
    .warmup(1)
    .measured(2)
    .expected_rows(1)
    .build();
    let result = run_scenario(&scen, &mut client, &RunConfig::default()).unwrap();
    let rows = vec![ComparativeRow::new(result, None)];
    let md = MarkdownReport::new(&rows).render();
    assert!(md.contains("smoke.literal"));
    assert!(md.contains("Nexus p50"));
}

#[test]
fn json_report_roundtrips_after_run() {
    let mut client = NexusClient::new().unwrap();
    let scen = ScenarioBuilder::new(
        "smoke.literal",
        "",
        nexus_bench::dataset::DatasetKind::Micro,
        "RETURN 1 AS n",
    )
    .warmup(1)
    .measured(2)
    .expected_rows(1)
    .build();
    let result = run_scenario(&scen, &mut client, &RunConfig::default()).unwrap();
    let report = JsonReport::new(vec![ComparativeRow::new(result, None)]);
    let s = report.to_pretty_string().unwrap();
    let back: JsonReport = serde_json::from_str(&s).unwrap();
    assert_eq!(back.scenario_count, 1);
    assert_eq!(back.rows[0].scenario_id, "smoke.literal");
}

#[test]
fn divergence_guard_catches_wrong_row_count() {
    let mut client = NexusClient::new().unwrap();
    let scen = ScenarioBuilder::new(
        "smoke.literal",
        "",
        nexus_bench::dataset::DatasetKind::Micro,
        "RETURN 1 AS n",
    )
    .warmup(1)
    .measured(2)
    .expected_rows(99) // wrong on purpose
    .build();
    let err = run_scenario(&scen, &mut client, &RunConfig::default()).unwrap_err();
    assert!(
        format!("{err}").contains("ERR_BENCH_OUTPUT_DIVERGENCE"),
        "got: {err}"
    );
}

#[test]
fn micro_dataset_loads_within_budget() {
    let mut client = NexusClient::new().unwrap();
    let start = std::time::Instant::now();
    MicroDataset::default().load(&mut client).unwrap();
    let elapsed = start.elapsed();
    // 10k nodes + 50k edges on a debug build should comfortably be
    // under 2 minutes; on release mode it's a few seconds. Cap at
    // 180 s so a pathological CI machine doesn't false-alarm but a
    // 10× regression still trips.
    assert!(
        elapsed < Duration::from_secs(180),
        "micro dataset load took {elapsed:?}, budget is 180s"
    );
}
