//! EXISTS Subquery Performance Benchmark
//!
//! This benchmark measures the performance of EXISTS subqueries:
//! - EXISTS vs COUNT pattern comparison
//! - Early termination effectiveness
//! - Different dataset sizes
//! - Nested EXISTS performance
//! - EXISTS with complex patterns

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use nexus_core::{Engine, testing::setup_isolated_test_engine};

/// Setup test data with nodes and relationships
fn setup_test_data(engine: &mut Engine, node_count: usize) {
    // Create Person nodes
    for i in 0..node_count {
        let query = format!(
            "CREATE (n:Person {{id: {}, name: 'Person{}', age: {}}})",
            i,
            i,
            20 + (i % 50)
        );
        engine
            .execute_cypher(&query)
            .expect("Failed to create Person node");
    }

    // Create Company nodes
    for i in 0..(node_count / 5) {
        let query = format!("CREATE (c:Company {{id: {}, name: 'Company{}'}})", i, i);
        engine
            .execute_cypher(&query)
            .expect("Failed to create Company node");
    }

    // Create KNOWS relationships (sparse - only 30% of nodes)
    for i in 0..(node_count * 3 / 10) {
        let query = format!(
            "MATCH (a:Person {{id: {}}}), (b:Person {{id: {}}}) CREATE (a)-[:KNOWS]->(b)",
            i,
            (i + 1) % node_count
        );
        engine.execute_cypher(&query).ok();
    }

    // Create WORKS_AT relationships (very sparse - only 10% of nodes)
    for i in 0..(node_count / 10) {
        let query = format!(
            "MATCH (p:Person {{id: {}}}), (c:Company {{id: {}}}) CREATE (p)-[:WORKS_AT]->(c)",
            i,
            i % (node_count / 5).max(1)
        );
        engine.execute_cypher(&query).ok();
    }
}

/// Benchmark EXISTS subquery vs COUNT > 0 pattern
/// EXISTS should be faster due to early termination
fn benchmark_exists_vs_count(c: &mut Criterion) {
    let scales = vec![50, 100, 200];
    let mut group = c.benchmark_group("exists_vs_count");
    group.sample_size(10);

    for scale in scales {
        // Setup for EXISTS test
        let (mut engine_exists, _ctx1) = setup_isolated_test_engine().unwrap();
        setup_test_data(&mut engine_exists, scale);

        // Setup for COUNT test
        let (mut engine_count, _ctx2) = setup_isolated_test_engine().unwrap();
        setup_test_data(&mut engine_count, scale);

        // Benchmark EXISTS pattern
        group.bench_with_input(
            BenchmarkId::new("exists", format!("{}_nodes", scale)),
            &scale,
            |b, _| {
                b.iter(|| {
                    let result = engine_exists.execute_cypher(
                        "MATCH (p:Person) WHERE EXISTS { (p)-[:KNOWS]->(:Person) } RETURN p.name",
                    );
                    black_box(result)
                });
            },
        );

        // Benchmark COUNT > 0 pattern (alternative to EXISTS)
        group.bench_with_input(
            BenchmarkId::new("count_gt_zero", format!("{}_nodes", scale)),
            &scale,
            |b, _| {
                b.iter(|| {
                    let result = engine_count.execute_cypher(
                        "MATCH (p:Person) WHERE size([(p)-[:KNOWS]->(:Person) | 1]) > 0 RETURN p.name",
                    );
                    black_box(result)
                });
            },
        );
    }
    group.finish();
}

/// Benchmark EXISTS with simple pattern
fn benchmark_exists_simple(c: &mut Criterion) {
    let scales = vec![50, 100, 200];
    let mut group = c.benchmark_group("exists_simple_pattern");
    group.sample_size(10);

    for scale in scales {
        let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
        setup_test_data(&mut engine, scale);

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}_nodes", scale)),
            &scale,
            |b, _| {
                b.iter(|| {
                    let result = engine.execute_cypher(
                        "MATCH (p:Person) WHERE EXISTS { (p)-[:KNOWS]->() } RETURN p.name",
                    );
                    black_box(result)
                });
            },
        );
    }
    group.finish();
}

/// Benchmark NOT EXISTS pattern
fn benchmark_not_exists(c: &mut Criterion) {
    let scales = vec![50, 100, 200];
    let mut group = c.benchmark_group("not_exists_pattern");
    group.sample_size(10);

    for scale in scales {
        let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
        setup_test_data(&mut engine, scale);

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}_nodes", scale)),
            &scale,
            |b, _| {
                b.iter(|| {
                    let result = engine.execute_cypher(
                        "MATCH (p:Person) WHERE NOT EXISTS { (p)-[:KNOWS]->() } RETURN p.name",
                    );
                    black_box(result)
                });
            },
        );
    }
    group.finish();
}

/// Benchmark EXISTS with complex multi-hop pattern
fn benchmark_exists_complex_pattern(c: &mut Criterion) {
    let scales = vec![50, 100];
    let mut group = c.benchmark_group("exists_complex_pattern");
    group.sample_size(10);

    for scale in scales {
        let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
        setup_test_data(&mut engine, scale);

        // Add more relationships for complex patterns
        for i in 0..(scale / 10) {
            let query = format!(
                "MATCH (a:Person {{id: {}}}), (b:Person {{id: {}}}) CREATE (a)-[:FRIEND]->(b)",
                i,
                (i + 3) % scale
            );
            engine.execute_cypher(&query).ok();
        }

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}_nodes", scale)),
            &scale,
            |b, _| {
                b.iter(|| {
                    let result = engine.execute_cypher(
                        "MATCH (p:Person) WHERE EXISTS { (p)-[:KNOWS]->()-[:FRIEND]->() } RETURN p.name",
                    );
                    black_box(result)
                });
            },
        );
    }
    group.finish();
}

/// Benchmark EXISTS with WHERE clause inside
fn benchmark_exists_with_where(c: &mut Criterion) {
    let scales = vec![50, 100];
    let mut group = c.benchmark_group("exists_with_where");
    group.sample_size(10);

    for scale in scales {
        let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
        setup_test_data(&mut engine, scale);

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}_nodes", scale)),
            &scale,
            |b, _| {
                b.iter(|| {
                    let result = engine.execute_cypher(
                        "MATCH (p:Person) WHERE EXISTS { (p)-[:KNOWS]->(other:Person) WHERE other.age > 30 } RETURN p.name",
                    );
                    black_box(result)
                });
            },
        );
    }
    group.finish();
}

/// Benchmark multiple EXISTS conditions
fn benchmark_multiple_exists(c: &mut Criterion) {
    let scales = vec![50, 100];
    let mut group = c.benchmark_group("multiple_exists");
    group.sample_size(10);

    for scale in scales {
        let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
        setup_test_data(&mut engine, scale);

        // Benchmark AND of two EXISTS
        group.bench_with_input(
            BenchmarkId::new("and_exists", format!("{}_nodes", scale)),
            &scale,
            |b, _| {
                b.iter(|| {
                    let result = engine.execute_cypher(
                        "MATCH (p:Person) WHERE EXISTS { (p)-[:KNOWS]->() } AND EXISTS { (p)-[:WORKS_AT]->() } RETURN p.name",
                    );
                    black_box(result)
                });
            },
        );

        // Benchmark OR of two EXISTS
        group.bench_with_input(
            BenchmarkId::new("or_exists", format!("{}_nodes", scale)),
            &scale,
            |b, _| {
                b.iter(|| {
                    let result = engine.execute_cypher(
                        "MATCH (p:Person) WHERE EXISTS { (p)-[:KNOWS]->() } OR EXISTS { (p)-[:WORKS_AT]->() } RETURN p.name",
                    );
                    black_box(result)
                });
            },
        );
    }
    group.finish();
}

/// Benchmark EXISTS used in RETURN clause
fn benchmark_exists_in_return(c: &mut Criterion) {
    let scales = vec![50, 100];
    let mut group = c.benchmark_group("exists_in_return");
    group.sample_size(10);

    for scale in scales {
        let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
        setup_test_data(&mut engine, scale);

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}_nodes", scale)),
            &scale,
            |b, _| {
                b.iter(|| {
                    let result = engine.execute_cypher(
                        "MATCH (p:Person) RETURN p.name, EXISTS { (p)-[:KNOWS]->() } AS hasConnections",
                    );
                    black_box(result)
                });
            },
        );
    }
    group.finish();
}

criterion_group!(
    benches,
    benchmark_exists_vs_count,
    benchmark_exists_simple,
    benchmark_not_exists,
    benchmark_exists_complex_pattern,
    benchmark_exists_with_where,
    benchmark_multiple_exists,
    benchmark_exists_in_return,
);

criterion_main!(benches);
