//! `write.*` seed scenarios.
//!
//! Every query in this file is either idempotent under repetition
//! (MERGE / SET on a known node / literal-return CREATE) or shaped
//! so the per-iteration row count stays stable (UNWIND that
//! collapses through `count(*)`, CREATE-then-DELETE cycles that
//! return a literal). Non-stable writes would trip the harness's
//! `expected_row_count` divergence guard on iteration 2.
//! `BenchClient::reset()` is not called between iterations —
//! that would dominate latency.

use crate::dataset::DatasetKind;
use crate::scenario::{Scenario, ScenarioBuilder};

pub(crate) fn scenarios() -> Vec<Scenario> {
    vec![
        ScenarioBuilder::new(
            "write.create_singleton",
            "CREATE a new :BenchTemp node and return a literal mark",
            DatasetKind::Tiny,
            // Return a literal instead of `id(n)` — Nexus and Neo4j
            // allocate node ids independently, so the divergence
            // guard would otherwise flag this row on every run
            // even though both engines did the same work.
            "CREATE (n:BenchTemp {mark: 'bench'}) RETURN n.mark AS mark",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "write.merge_singleton",
            "MERGE a singleton :BenchSingleton — idempotent",
            DatasetKind::Tiny,
            "MERGE (n:BenchSingleton {key: 'bench'}) RETURN n.key AS k",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "write.set_property",
            "SET n.bench_visited = true on n0:A — idempotent",
            DatasetKind::Tiny,
            "MATCH (n:A {id: 0}) SET n.bench_visited = true RETURN n.id AS id",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "write.unwind_create_batch",
            "UNWIND range CREATE — 10 nodes per iteration, count collapses to 1 row",
            DatasetKind::Tiny,
            "UNWIND range(1, 10) AS i CREATE (:BenchBatch {i: i}) RETURN count(*) AS c",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "write.create_delete_cycle",
            "CREATE-then-DELETE in one query — net-zero, iteration-safe",
            DatasetKind::Tiny,
            // Each iteration creates one node and deletes it in the
            // same statement. Row count stays at 1 across the
            // measured loop, so the divergence guard stays useful
            // without a per-iteration reset hook. Also exercises
            // the `DELETE` code path the other write scenarios
            // avoid.
            "CREATE (n:BenchCycle) WITH n DELETE n RETURN 'done' AS status",
        )
        .expected_rows(1)
        .build(),
    ]
}
