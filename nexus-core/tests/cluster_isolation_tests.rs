//! End-to-end multi-tenant isolation tests for
//! `TenantIsolationMode::CatalogPrefix`.
//!
//! Exercises the full parse → scope → plan → execute path that
//! `Engine::execute_cypher_with_context` threads together. What we
//! want to prove: two tenants who both run the literal same Cypher
//! against the same engine do NOT see each other's data.
//!
//! These tests use a single fresh engine per test, created via the
//! repo's isolation-safe `TestContext`. Cluster mode does not
//! require multi-database support — the catalog-prefix approach
//! works on a single shared engine, which is exactly the point.

use std::sync::Arc;

use nexus_core::Error;
use nexus_core::cluster::{
    LocalQuotaProvider, QuotaProvider, TenantDefaults, TenantIsolationMode, UserContext,
    UserNamespace,
};
use nexus_core::testing::setup_isolated_test_engine;

// Uses `setup_isolated_test_engine` (not `setup_test_engine`) because
// the default test engine shares an LMDB catalog directory across
// every test in the binary to sidestep a Windows TlsFull limit. That
// shared catalog is fatal for isolation tests — another test's labels
// collide with the namespace-scoped ones we register, and the
// "tenants must not see each other" assertion fires spuriously on
// pre-existing data. Isolated-catalog mode creates a fresh LMDB env
// per test, which is what we actually want for correctness checks.

fn ctx_for(tenant: &str) -> UserContext {
    UserContext::unrestricted(UserNamespace::new(tenant).unwrap(), format!("key-{tenant}"))
}

#[test]
fn two_tenants_do_not_see_each_others_nodes() -> Result<(), Error> {
    let (mut engine, _guard) = setup_isolated_test_engine()?;
    let alice = ctx_for("alice");
    let bob = ctx_for("bob");

    // Alice creates 3 Persons; Bob creates 2 Persons. Both use the
    // literal same Cypher — isolation comes purely from scope
    // rewriting.
    for name in ["Aria", "Alex", "Amir"] {
        let cypher = format!("CREATE (n:Person {{name: '{name}'}})");
        engine.execute_cypher_with_context(
            &cypher,
            Some(&alice),
            TenantIsolationMode::CatalogPrefix,
        )?;
    }
    for name in ["Bob", "Beth"] {
        let cypher = format!("CREATE (n:Person {{name: '{name}'}})");
        engine.execute_cypher_with_context(
            &cypher,
            Some(&bob),
            TenantIsolationMode::CatalogPrefix,
        )?;
    }

    // Each tenant queries `MATCH (n:Person) RETURN count(n)` and
    // must see only their own rows.
    let alice_count = engine
        .execute_cypher_with_context(
            "MATCH (n:Person) RETURN count(n) AS c",
            Some(&alice),
            TenantIsolationMode::CatalogPrefix,
        )?
        .rows[0]
        .values[0]
        .as_i64()
        .expect("count is integer");
    assert_eq!(alice_count, 3, "alice must see her 3 Persons");

    let bob_count = engine
        .execute_cypher_with_context(
            "MATCH (n:Person) RETURN count(n) AS c",
            Some(&bob),
            TenantIsolationMode::CatalogPrefix,
        )?
        .rows[0]
        .values[0]
        .as_i64()
        .expect("count is integer");
    assert_eq!(bob_count, 2, "bob must see only his 2 Persons");

    Ok(())
}

#[test]
fn relationship_types_are_also_isolated() -> Result<(), Error> {
    let (mut engine, _guard) = setup_isolated_test_engine()?;
    let alice = ctx_for("alice");
    let bob = ctx_for("bob");

    // Alice: two Persons and a KNOWS between them.
    engine.execute_cypher_with_context(
        "CREATE (a:Person {name: 'A'}), (b:Person {name: 'B'}), (a)-[:KNOWS]->(b)",
        Some(&alice),
        TenantIsolationMode::CatalogPrefix,
    )?;
    // Bob: one Person, no relationships.
    engine.execute_cypher_with_context(
        "CREATE (n:Person {name: 'Bob'})",
        Some(&bob),
        TenantIsolationMode::CatalogPrefix,
    )?;

    let alice_rels = engine
        .execute_cypher_with_context(
            "MATCH ()-[r:KNOWS]->() RETURN count(r) AS c",
            Some(&alice),
            TenantIsolationMode::CatalogPrefix,
        )?
        .rows[0]
        .values[0]
        .as_i64()
        .expect("count is integer");
    assert_eq!(alice_rels, 1, "alice must see her one KNOWS");

    let bob_rels = engine
        .execute_cypher_with_context(
            "MATCH ()-[r:KNOWS]->() RETURN count(r) AS c",
            Some(&bob),
            TenantIsolationMode::CatalogPrefix,
        )?
        .rows[0]
        .values[0]
        .as_i64()
        .expect("count is integer");
    assert_eq!(bob_rels, 0, "bob must see zero KNOWS — isolation holds");

    Ok(())
}

#[test]
fn standalone_mode_is_unaffected_by_context_api() -> Result<(), Error> {
    // Regression guard: calling the new context-aware entry point
    // with `None` context (or `TenantIsolationMode::None`) must be
    // indistinguishable from the legacy `execute_cypher`.
    let (mut engine, _guard) = setup_isolated_test_engine()?;

    // Write via the classic path.
    engine.execute_cypher("CREATE (n:Item {name: 'W-1'})")?;

    // Read via the context-aware path with NO context at all.
    let result = engine.execute_cypher_with_context(
        "MATCH (n:Item) RETURN count(n) AS c",
        None,
        TenantIsolationMode::None,
    )?;
    assert_eq!(result.rows[0].values[0].as_i64().unwrap(), 1);

    // Also verify: passing a ctx but with mode=None → still no
    // rewrite. The mode is the authority, not the presence of ctx.
    let alice = ctx_for("alice");
    let result = engine.execute_cypher_with_context(
        "MATCH (n:Item) RETURN count(n) AS c",
        Some(&alice),
        TenantIsolationMode::None,
    )?;
    assert_eq!(
        result.rows[0].values[0].as_i64().unwrap(),
        1,
        "mode=None must NOT scope, regardless of ctx"
    );

    Ok(())
}

#[test]
fn alice_cannot_delete_bobs_data_via_label_match() -> Result<(), Error> {
    // Attack shape: Alice issues a `MATCH (n:Person) DELETE n` in
    // cluster mode. The scope rewriter turns her MATCH label into
    // `ns:alice:Person`, so the DELETE only touches her own rows.
    // Bob's rows must survive.
    let (mut engine, _guard) = setup_isolated_test_engine()?;
    let alice = ctx_for("alice");
    let bob = ctx_for("bob");

    engine.execute_cypher_with_context(
        "CREATE (n:Person {name: 'Alice'})",
        Some(&alice),
        TenantIsolationMode::CatalogPrefix,
    )?;
    engine.execute_cypher_with_context(
        "CREATE (n:Person {name: 'Bob'})",
        Some(&bob),
        TenantIsolationMode::CatalogPrefix,
    )?;

    // Alice's destructive query.
    engine.execute_cypher_with_context(
        "MATCH (n:Person) DELETE n",
        Some(&alice),
        TenantIsolationMode::CatalogPrefix,
    )?;

    // Alice sees 0; Bob still sees his row.
    let a = engine
        .execute_cypher_with_context(
            "MATCH (n:Person) RETURN count(n) AS c",
            Some(&alice),
            TenantIsolationMode::CatalogPrefix,
        )?
        .rows[0]
        .values[0]
        .as_i64()
        .unwrap();
    assert_eq!(a, 0);

    let b = engine
        .execute_cypher_with_context(
            "MATCH (n:Person) RETURN count(n) AS c",
            Some(&bob),
            TenantIsolationMode::CatalogPrefix,
        )?
        .rows[0]
        .values[0]
        .as_i64()
        .unwrap();
    assert_eq!(b, 1, "bob's row must survive alice's DELETE");

    Ok(())
}

// ---------------------------------------------------------------------------
// Write-path quota enforcement (Phase 4 §13 / §14)
// ---------------------------------------------------------------------------

/// Tight quota helper: 1 MiB storage, 1 000 req/min — big enough
/// rate-wise that these tests never hit the per-minute gate, small
/// enough storage-wise that a single `record_usage(256 bytes)` flips
/// the tenant into over-budget after ~4096 writes. We reach the
/// denial in one step by pre-populating `record_usage` with a
/// bespoke delta; see `storage_quota_blocks_write_once_tenant_past_limit`.
fn tight_storage_defaults() -> TenantDefaults {
    TenantDefaults {
        storage_mb: 1,
        requests_per_minute: 10_000,
        requests_per_hour: 100_000,
    }
}

#[test]
fn storage_quota_allows_writes_within_budget() -> Result<(), Error> {
    let (mut engine, _guard) = setup_isolated_test_engine()?;
    let provider: Arc<dyn QuotaProvider> = LocalQuotaProvider::new(tight_storage_defaults());
    engine.set_quota_provider(Some(provider.clone()));

    let alice = ctx_for("alice");

    // A handful of writes fit comfortably inside 1 MiB. The charge
    // per write is 256 bytes (see engine::execute_cypher_with_context
    // post-record block), so ten writes spend 2.5 KiB — nowhere
    // near the 1 MiB cap.
    for name in ["A", "B", "C", "D", "E"] {
        let cypher = format!("CREATE (n:Person {{name: '{name}'}})");
        engine.execute_cypher_with_context(
            &cypher,
            Some(&alice),
            TenantIsolationMode::CatalogPrefix,
        )?;
    }

    // Snapshot must show five recorded writes.
    let snap = provider.snapshot(alice.namespace()).expect("tenant known");
    assert_eq!(
        snap.storage_bytes_used,
        5 * 256,
        "five writes at 256 B each must charge 1280 B total"
    );
    assert!(snap.storage_bytes_used < snap.storage_bytes_limit);

    Ok(())
}

#[test]
fn storage_quota_blocks_write_once_tenant_past_limit() -> Result<(), Error> {
    let (mut engine, _guard) = setup_isolated_test_engine()?;
    let provider: Arc<dyn QuotaProvider> = LocalQuotaProvider::new(tight_storage_defaults());
    engine.set_quota_provider(Some(provider.clone()));

    let alice = ctx_for("alice");

    // Push the tenant AT its storage cap by recording a usage
    // delta equal to the whole budget. The check is
    // `used > limit`, so exactly-at-limit is still allowed; we
    // also record one extra byte so the check deterministically
    // denies.
    let limit_bytes = tight_storage_defaults().storage_mb * 1024 * 1024;
    provider.record_usage(
        alice.namespace(),
        nexus_core::cluster::UsageDelta {
            storage_bytes: limit_bytes + 1,
            requests: 0,
        },
    );

    // Any write from alice must now be rejected BEFORE it reaches
    // storage — no partial data, just an immediate QuotaExceeded.
    let err = engine
        .execute_cypher_with_context(
            "CREATE (n:Person {name: 'will-fail'})",
            Some(&alice),
            TenantIsolationMode::CatalogPrefix,
        )
        .expect_err("over-budget tenant must be denied");
    match err {
        Error::QuotaExceeded(reason) => {
            assert!(
                reason.contains("storage quota exceeded"),
                "reason should identify the storage quota: {reason}"
            );
        }
        other => panic!("expected QuotaExceeded, got {other:?}"),
    }

    Ok(())
}

#[test]
fn reads_are_never_quota_gated_even_over_budget() -> Result<(), Error> {
    // An over-budget tenant must still be able to READ their data
    // — otherwise quota exhaustion bricks the tenant's ability to
    // observe what they have. Reads are not write queries, so the
    // `is_write_query` check short-circuits and the provider is
    // never consulted.
    let (mut engine, _guard) = setup_isolated_test_engine()?;
    let provider: Arc<dyn QuotaProvider> = LocalQuotaProvider::new(tight_storage_defaults());
    engine.set_quota_provider(Some(provider.clone()));

    let alice = ctx_for("alice");

    // Seed a node before hitting the limit.
    engine.execute_cypher_with_context(
        "CREATE (n:Person {name: 'Alice'})",
        Some(&alice),
        TenantIsolationMode::CatalogPrefix,
    )?;

    // Blow past the cap.
    let limit_bytes = tight_storage_defaults().storage_mb * 1024 * 1024;
    provider.record_usage(
        alice.namespace(),
        nexus_core::cluster::UsageDelta {
            storage_bytes: limit_bytes + 1,
            requests: 0,
        },
    );

    // Reads still work — the tenant can see their own data.
    let r = engine.execute_cypher_with_context(
        "MATCH (n:Person) RETURN count(n) AS c",
        Some(&alice),
        TenantIsolationMode::CatalogPrefix,
    )?;
    assert_eq!(r.rows[0].values[0].as_i64().unwrap(), 1);

    // And DELETE is allowed — it is a write in accounting terms,
    // but the quota provider's storage check happens BEFORE the
    // record-usage step, so an over-limit tenant can still shed
    // load. That's the right semantics for a tenant trying to
    // bring themselves back under quota.
    //
    // Wait — but `is_write_query` returns true for DELETE, so the
    // check_storage gate DOES fire. For now the test just asserts
    // the current behaviour (DELETE refused). If operators later
    // ask for DELETE-always-allowed, the place to change is the
    // `is_write_query` helper; this test is a guard on the
    // contract as it stands.
    let err = engine
        .execute_cypher_with_context(
            "MATCH (n:Person) DELETE n",
            Some(&alice),
            TenantIsolationMode::CatalogPrefix,
        )
        .expect_err("over-budget DELETE must reject at the gate");
    assert!(matches!(err, Error::QuotaExceeded(_)));

    Ok(())
}

#[test]
fn standalone_mode_ignores_quota_provider_on_writes() -> Result<(), Error> {
    // Regression guard: legacy `execute_cypher` callers (no
    // UserContext) MUST never hit the quota gate, even if an
    // engine has a provider installed. The pre-cluster-mode
    // contract is "one tenant, no quotas, just work", and nothing
    // on the provider path should change that.
    let (mut engine, _guard) = setup_isolated_test_engine()?;
    let provider: Arc<dyn QuotaProvider> = LocalQuotaProvider::new(tight_storage_defaults());
    engine.set_quota_provider(Some(provider.clone()));

    // Even with a provider attached, writes without a context
    // succeed — the ctx-presence check at the top of the gate is
    // the short-circuit.
    for _ in 0..10 {
        engine.execute_cypher("CREATE (n:Item {name: 'x'})")?;
    }
    // Provider remains untouched — snapshot for any ns is None.
    let anything = UserNamespace::new("some-tenant").unwrap();
    assert!(
        provider.snapshot(&anything).is_none(),
        "standalone writes must never touch the provider"
    );

    Ok(())
}
