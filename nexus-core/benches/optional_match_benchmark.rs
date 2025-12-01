//! OPTIONAL MATCH Performance Benchmark
//!
//! This benchmark measures the performance of OPTIONAL MATCH vs regular MATCH:
//! - Query execution time comparison
//! - NULL handling overhead
//! - Different dataset sizes
//! - Pattern complexity impact

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use nexus_core::{Engine, testing::setup_isolated_test_engine};

fn setup_test_data(engine: &mut Engine, node_count: usize) {
    // Create nodes with and without relationships
    for i in 0..node_count {
        let query = format!("CREATE (n:Person {{id: {}, name: 'Person{}'}})", i, i);
        engine
            .execute_cypher(&query)
            .expect("Failed to create node");
    }

    // Create relationships for only half of the nodes
    for i in 0..(node_count / 2) {
        let query = format!(
            "MATCH (a:Person {{id: {}}}), (b:Person {{id: {}}}) CREATE (a)-[:KNOWS]->(b)",
            i,
            i + 1
        );
        engine
            .execute_cypher(&query)
            .expect("Failed to create relationship");
    }
}

fn benchmark_regular_match(c: &mut Criterion) {
    let scales = vec![10, 50, 100];
    let mut group = c.benchmark_group("regular_match");
    group.sample_size(10);

    for scale in scales {
        // Create engine ONCE outside the benchmark
        let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
        setup_test_data(&mut engine, scale);

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}_nodes", scale)),
            &scale,
            |b, _| {
                b.iter(|| {
                    let result = engine.execute_cypher(
                        "MATCH (a:Person)-[r:KNOWS]->(b:Person) RETURN a.name, b.name",
                    );
                    black_box(result)
                });
            },
        );
    }
    group.finish();
}

fn benchmark_optional_match(c: &mut Criterion) {
    let scales = vec![10, 50, 100];
    let mut group = c.benchmark_group("optional_match");
    group.sample_size(10);

    for scale in scales {
        // Create engine ONCE outside the benchmark
        let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
        setup_test_data(&mut engine, scale);

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}_nodes", scale)),
            &scale,
            |b, _| {
                b.iter(|| {
                    let result = engine.execute_cypher(
                        "MATCH (a:Person) OPTIONAL MATCH (a)-[r:KNOWS]->(b:Person) RETURN a.name, b.name"
                    );
                    black_box(result)
                });
            },
        );
    }
    group.finish();
}

fn benchmark_optional_match_with_null_filtering(c: &mut Criterion) {
    let scales = vec![10, 50];
    let mut group = c.benchmark_group("optional_match_with_where");
    group.sample_size(10);

    for scale in scales {
        // Create engine ONCE outside the benchmark
        let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
        setup_test_data(&mut engine, scale);

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}_nodes", scale)),
            &scale,
            |b, _| {
                b.iter(|| {
                    let result = engine.execute_cypher(
                        "MATCH (a:Person) OPTIONAL MATCH (a)-[r:KNOWS]->(b:Person) WHERE b IS NOT NULL RETURN a.name, b.name"
                    );
                    black_box(result)
                });
            },
        );
    }
    group.finish();
}

fn benchmark_nested_optional_match(c: &mut Criterion) {
    let scales = vec![10, 25];
    let mut group = c.benchmark_group("nested_optional_match");
    group.sample_size(10);

    for scale in scales {
        // Create engine ONCE outside the benchmark
        let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
        setup_test_data(&mut engine, scale);

        // Create an additional relationship type
        for i in 0..(scale / 4) {
            let query = format!(
                "MATCH (a:Person {{id: {}}}), (b:Person {{id: {}}}) CREATE (a)-[:FRIEND]->(b)",
                i,
                i + 2
            );
            engine.execute_cypher(&query).ok();
        }

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}_nodes", scale)),
            &scale,
            |b, _| {
                b.iter(|| {
                    let result = engine.execute_cypher(
                        "MATCH (a:Person) OPTIONAL MATCH (a)-[:KNOWS]->(b:Person) OPTIONAL MATCH (b)-[:FRIEND]->(c:Person) RETURN a.name, b.name, c.name"
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
    benchmark_regular_match,
    benchmark_optional_match,
    benchmark_optional_match_with_null_filtering,
    benchmark_nested_optional_match
);
criterion_main!(benches);
