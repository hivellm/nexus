//! Criterion bench for cluster-mode overhead (Phase 4 §13.6 / §15.4).
//!
//! Measures three things operators care about when deciding whether
//! to flip `NEXUS_CLUSTER_ENABLED` on a single-tenant deployment:
//!
//! 1. **Scope-walker cost.** `scope_query` runs on every cluster-mode
//!    query before planning. How much does it add to per-query wall
//!    time on a representative CREATE? Compares `TenantIsolationMode::
//!    None` (no-op short-circuit) against `TenantIsolationMode::
//!    CatalogPrefix` (full AST walk + prefix rewrite).
//!
//! 2. **Quota-gated write throughput.** With an `Arc<dyn QuotaProvider>`
//!    installed on the Engine, every write pays a `check_storage` +
//!    `record_usage` round-trip against the local provider's mutex.
//!    Baseline is standalone-mode CREATE (no provider, no gate);
//!    cluster-mode is the same CREATE through `execute_cypher_with_context`
//!    with a tenant scoped and the provider wired in.
//!
//! 3. **Rate-window contention under load.** Fires N concurrent-ish
//!    `check_rate` calls against a single tenant's window and times
//!    the mutex acquisition + counter update. The provider's rate
//!    windows are per-tenant `parking_lot::RwLock` slots, so the
//!    contention shape is the same regardless of whether the
//!    middleware or the engine is the caller.
//!
//! ```text
//! cargo +nightly bench -p nexus-core --bench cluster_mode_benchmark
//! ```
//!
//! In-process: no server, no HTTP layer, no Axum. Every bench drives
//! the core primitives directly so regressions surface as clean
//! deltas instead of being swamped by networking noise.
//!
//! Interpretation guidance: see `docs/specs/cluster-mode.md` §5.5
//! (known limitations — label-less MATCH, rel-count tracking).

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use std::sync::Arc;

use nexus_core::cluster::{
    LocalQuotaProvider, QuotaProvider, TenantDefaults, TenantIsolationMode, UsageDelta,
    UserContext, UserNamespace,
};
use nexus_core::executor::parser::CypherParser;
use nexus_core::testing::setup_isolated_test_engine;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn alice_ctx() -> UserContext {
    UserContext::unrestricted(UserNamespace::new("alice").unwrap(), "key-alice")
}

fn generous_defaults() -> TenantDefaults {
    // Rate + storage limits tall enough that the bench never trips
    // the gate on its own — we're measuring overhead, not denial
    // behaviour (that has its own unit test).
    TenantDefaults {
        storage_mb: 10_240,
        requests_per_minute: u32::MAX,
        requests_per_hour: u32::MAX,
    }
}

// ---------------------------------------------------------------------------
// 1. Scope-walker cost
// ---------------------------------------------------------------------------

fn bench_scope_walker(c: &mut Criterion) {
    let mut group = c.benchmark_group("cluster_scope_walker");
    let ns = UserNamespace::new("alice").unwrap();

    // One moderately complex query that exercises every rewrite
    // position the walker covers: node labels, rel types, property
    // keys in map literals, property keys in WHERE expressions,
    // property keys in RETURN.
    let query = "MATCH (a:Person)-[r:KNOWS {since: 2020}]->(b:Person) \
                 WHERE a.email = 'x' RETURN a.name, b.name, r.since";

    group.throughput(Throughput::Elements(1));

    group.bench_function(BenchmarkId::new("mode", "None"), |b| {
        b.iter_batched(
            || {
                let mut parser = CypherParser::new(query.to_string());
                parser.parse().expect("parse")
            },
            |mut ast| {
                nexus_core::cluster::scope_query(
                    &mut ast,
                    black_box(&ns),
                    TenantIsolationMode::None,
                );
                black_box(ast);
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.bench_function(BenchmarkId::new("mode", "CatalogPrefix"), |b| {
        b.iter_batched(
            || {
                let mut parser = CypherParser::new(query.to_string());
                parser.parse().expect("parse")
            },
            |mut ast| {
                nexus_core::cluster::scope_query(
                    &mut ast,
                    black_box(&ns),
                    TenantIsolationMode::CatalogPrefix,
                );
                black_box(ast);
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// 2. Quota-gated write throughput
// ---------------------------------------------------------------------------

fn bench_write_path_with_and_without_quota(c: &mut Criterion) {
    let mut group = c.benchmark_group("cluster_write_path");

    // Standalone baseline: classic execute_cypher, no provider, no
    // ctx. This is the number every cluster-mode overhead should be
    // compared against.
    {
        let (mut engine, _guard) = setup_isolated_test_engine().expect("engine");
        group.throughput(Throughput::Elements(1));
        group.bench_function(BenchmarkId::new("path", "standalone"), |b| {
            b.iter(|| {
                engine
                    .execute_cypher("CREATE (n:Person {name: 'x'})")
                    .expect("standalone CREATE");
            })
        });
    }

    // Cluster-mode with provider + CatalogPrefix scoping + tenant
    // context. Every write pays: scope walker + override install +
    // check_storage + record_usage.
    {
        let (mut engine, _guard) = setup_isolated_test_engine().expect("engine");
        let provider: Arc<dyn QuotaProvider> = LocalQuotaProvider::new(generous_defaults());
        engine.set_quota_provider(Some(provider));
        let ctx = alice_ctx();
        group.bench_function(BenchmarkId::new("path", "cluster+prefix+gate"), |b| {
            b.iter(|| {
                engine
                    .execute_cypher_with_context(
                        "CREATE (n:Person {name: 'x'})",
                        Some(&ctx),
                        TenantIsolationMode::CatalogPrefix,
                    )
                    .expect("cluster CREATE");
            })
        });
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// 3. Rate-window contention
// ---------------------------------------------------------------------------

fn bench_check_rate(c: &mut Criterion) {
    let mut group = c.benchmark_group("cluster_check_rate");
    let provider: Arc<dyn QuotaProvider> = LocalQuotaProvider::new(generous_defaults());
    let ns = UserNamespace::new("alice").unwrap();

    group.throughput(Throughput::Elements(1));
    group.bench_function(BenchmarkId::new("single_tenant", "allow"), |b| {
        b.iter(|| {
            let decision = provider.check_rate(black_box(&ns));
            debug_assert!(decision.is_allowed());
            black_box(decision);
        })
    });

    // record_usage is on the same mutex — operators pay for both
    // per request in cluster mode, so we benchmark the paired cost.
    group.bench_function(BenchmarkId::new("single_tenant", "allow+record"), |b| {
        b.iter(|| {
            let decision = provider.check_rate(black_box(&ns));
            debug_assert!(decision.is_allowed());
            provider.record_usage(
                black_box(&ns),
                UsageDelta {
                    storage_bytes: 256,
                    requests: 1,
                },
            );
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_scope_walker,
    bench_write_path_with_and_without_quota,
    bench_check_rate,
);
criterion_main!(benches);
