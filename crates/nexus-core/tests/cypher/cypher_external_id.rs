//! End-to-end Cypher tests for the reserved `_id` property
//! (phase9_external-node-ids §4.4).

use nexus_core::Engine;
use nexus_core::catalog::external_id::ExternalId;
use nexus_core::testing::TestContext;
use std::str::FromStr;

const SHA256_ZEROS: &str =
    "sha256:0000000000000000000000000000000000000000000000000000000000000000";

#[test]
fn create_with_string_literal_id_assigns_external_id() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");
    let q = format!(
        "CREATE (n:File {{_id: '{}', name: 'a.txt'}}) RETURN n",
        SHA256_ZEROS
    );
    let res = engine.execute_cypher(&q);
    assert!(res.is_ok(), "CREATE with _id failed: {:?}", res.err());

    let ext = ExternalId::from_str(SHA256_ZEROS).expect("parse external id");
    let txn = engine.catalog.read_txn().expect("open catalog read txn");
    let mapped = engine
        .catalog
        .external_id_index()
        .get_internal(&txn, &ext)
        .expect("index lookup");
    assert!(
        mapped.is_some(),
        "external id should map to a node id after CREATE"
    );
}

#[test]
fn create_on_conflict_match_returns_existing_node() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");
    let uuid_id = "uuid:11111111-1111-1111-1111-111111111111";
    let q = format!("CREATE (n:File {{_id: '{}'}}) ON CONFLICT MATCH", uuid_id);
    engine.execute_cypher(&q).expect("first create");
    let res = engine.execute_cypher(&q);
    assert!(
        res.is_ok(),
        "second CREATE with ON CONFLICT MATCH must not error: {:?}",
        res.err()
    );
}

// This file now builds each `Engine` with `Engine::with_isolated_catalog`
// over a fresh `testing::TestContext` directory, giving every test its own
// unshared catalog. Fuller `RETURN n._id` / `WHERE n._id = ...` projection
// coverage (plus the write-path forms fixed by issue #29 — MERGE, CREATE+SET,
// UNWIND+CREATE) lives in `tests/cypher_external_id_write_paths.rs`.

#[test]
fn create_on_conflict_default_errors_on_duplicate() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");
    let uuid_id = "uuid:22222222-2222-2222-2222-222222222222";
    let q = format!("CREATE (n:File {{_id: '{}'}})", uuid_id);
    engine.execute_cypher(&q).expect("first create");
    let res = engine.execute_cypher(&q);
    assert!(
        res.is_err(),
        "second CREATE without ON CONFLICT must fail with ExternalIdConflict"
    );
}
