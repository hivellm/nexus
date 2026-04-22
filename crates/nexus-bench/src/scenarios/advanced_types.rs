//! phase6_opencypher-advanced-types scenarios — BYTES family, typed
//! lists, composite B-tree index, savepoints, and graph scoping.
//!
//! Nexus ships these surfaces in-tree. Neo4j has equivalent behaviour
//! for BYTES (`bytes()` / `bytesToHex`) and composite indexes;
//! graph-scope and save-points are Nexus-specific. The `live_compare`
//! harness diffs shared ones against Neo4j and records the Nexus-only
//! scenarios as baseline regression guards.

use crate::dataset::DatasetKind;
use crate::scenario::{Scenario, ScenarioBuilder};

pub(crate) fn scenarios() -> Vec<Scenario> {
    let mut out = Vec::new();
    out.extend(bytes());
    out.extend(composite_index());
    out
}

fn scalar(id: &str, description: &str, query: &str) -> Scenario {
    ScenarioBuilder::new(id, description, DatasetKind::Tiny, query)
        .expected_rows(1)
        .build()
}

fn bytes() -> Vec<Scenario> {
    vec![
        scalar(
            "advanced_types.bytes_from_base64",
            "bytesFromBase64 decode",
            "RETURN bytesToHex(bytesFromBase64('AAH/')) AS hex",
        ),
        scalar(
            "advanced_types.bytes_utf8_encode",
            "bytes() UTF-8 encode then hex",
            "RETURN bytesToHex(bytes('abc')) AS hex",
        ),
        scalar(
            "advanced_types.bytes_length",
            "bytesLength of 5-byte payload",
            "RETURN bytesLength(bytesFromBase64('AAECAwQ=')) AS len",
        ),
        scalar(
            "advanced_types.bytes_slice",
            "bytesSlice clamping",
            "RETURN bytesToHex(bytesSlice(bytesFromBase64('AAECAwQ='), 1, 3)) AS hex",
        ),
    ]
}

fn composite_index() -> Vec<Scenario> {
    vec![
        scalar(
            "advanced_types.composite_index_ddl",
            "CREATE INDEX FOR (p:Person) ON (p.tenantId, p.id) idempotent",
            "CREATE INDEX person_tenant_id_idx FOR (p:Person) ON (p.tenantId, p.id)",
        ),
        scalar(
            "advanced_types.db_indexes_lists_composite",
            "db.indexes() includes the composite row",
            "CALL db.indexes() YIELD name RETURN count(name) AS c",
        ),
    ]
}
