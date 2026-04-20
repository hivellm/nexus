//! `constraint.*` seed scenarios — UNIQUE / NOT NULL / NODE KEY
//! runtime enforcement.
//!
//! Every scenario here is the **insert-overhead** side of the
//! constraint contract: each CREATE either enters a row that a
//! constraint would reject (so the engine does the constraint
//! check), or enters a row that passes (measuring the
//! fast-path). When constraint runtime enforcement lands in
//! `nexus-core`, the latency delta between constraint-scoped
//! CREATE and plain CREATE tells the operator how much the
//! check costs. Before that lands, the scenario runs as plain
//! CREATE and simply contributes a baseline row.
//!
//! Tracks §6.1-§6.3 of `phase6_bench-scenario-expansion`.

use crate::dataset::DatasetKind;
use crate::scenario::{Scenario, ScenarioBuilder};

pub(crate) fn scenarios() -> Vec<Scenario> {
    vec![
        ScenarioBuilder::new(
            "constraint.unique_insert",
            "CREATE with UNIQUE constraint on :BenchUnique.key (§6.1)",
            DatasetKind::Tiny,
            // Idempotent at the row-count level: we MERGE so the
            // first iteration inserts, subsequent ones find. When
            // the UNIQUE constraint is enforced at runtime, the
            // MERGE path is the canonical insert-under-UNIQUE
            // latency the bench wants to measure.
            "MERGE (n:BenchUnique {key: 'singleton'}) RETURN n.key AS k",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "constraint.not_null_set",
            "SET property on :BenchNotNull with a NOT NULL expectation (§6.2)",
            DatasetKind::Tiny,
            // Each iteration sets the same value on the same
            // singleton node, so the row count is stable. When
            // NOT NULL enforcement lands the SET path measures
            // the extra nullability check.
            "MERGE (n:BenchNotNull {key: 'row'}) \
             SET n.required_field = 'value' \
             RETURN n.required_field AS v",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "constraint.node_key_composite",
            "Composite (type, id) NODE KEY insert on :BenchKey (§6.3)",
            DatasetKind::Tiny,
            // The composite key is (`type`, `id`). The MERGE
            // matches on both properties so iterations 2..N find
            // the row without a duplicate-key violation.
            "MERGE (n:BenchKey {type: 'bench', id: 1}) \
             RETURN n.type AS t, n.id AS i",
        )
        .expected_rows(1)
        .build(),
    ]
}
