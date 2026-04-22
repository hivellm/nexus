//! Criterion bench for the groupless-aggregate columnar fast path.
//!
//! Sweeps `SUM` / `MIN` / `MAX` / `AVG` over both `i64` (`n.age`) and
//! `f64` (`n.score`) columns at 10 k, 100 k, and 1 M rows. Each
//! (op × dtype × size) runs twice — once with
//! `columnar_threshold = usize::MAX` (pins scalar row path) and
//! once with the default `4096` (fast path fires). Identical
//! fixture across both lines, so the Criterion report gives a
//! clean row-vs-column ratio per configuration.
//!
//! ```text
//! cargo +nightly bench -p nexus-core --bench executor_aggregate
//! ```
//!
//! See `docs/specs/executor-columnar.md` for interpretation
//! guidance and the `PREFER_COLUMNAR` / `DISABLE_COLUMNAR` hint
//! escape hatches.

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use nexus_core::executor::Aggregation;
use nexus_core::testing::create_test_executor;
use serde_json::{Map, Number, Value};
use std::hint::black_box;

const SIZES: &[usize] = &[10_000, 100_000, 1_000_000];

fn build_person(id: u64, age: i64, score: f64) -> Value {
    let mut node = Map::new();
    node.insert("_nexus_id".to_string(), Value::Number(id.into()));
    node.insert("age".to_string(), Value::Number(age.into()));
    node.insert(
        "score".to_string(),
        Value::Number(Number::from_f64(score).expect("finite score")),
    );
    Value::Object(node)
}

fn make_fixture(size: usize) -> Vec<Value> {
    (0..size as u64)
        .map(|i| build_person(i, i as i64, i as f64 * 0.5))
        .collect()
}

fn agg(op: &str, column: &str) -> Aggregation {
    let alias = format!("{}({})", op, column);
    let c = column.to_string();
    match op {
        "sum" => Aggregation::Sum { column: c, alias },
        "min" => Aggregation::Min { column: c, alias },
        "max" => Aggregation::Max { column: c, alias },
        "avg" => Aggregation::Avg { column: c, alias },
        _ => unreachable!("unsupported op in this bench: {op}"),
    }
}

fn bench_op(c: &mut Criterion, op: &'static str, column: &'static str) {
    let group_name = format!("aggregate_{}_{}", op, column.replace('.', "_"));
    let mut group = c.benchmark_group(&group_name);

    for &size in SIZES {
        let nodes = make_fixture(size);
        group.throughput(Throughput::Elements(size as u64));

        // Row path.
        {
            let (mut executor, _ctx) = create_test_executor();
            executor.set_columnar_threshold(usize::MAX);
            let agg_template = agg(op, column);
            group.bench_with_input(BenchmarkId::new("row", size), &size, |b, _| {
                b.iter(|| {
                    black_box(
                        executor
                            .run_in_memory_aggregate("n", nodes.clone(), black_box(&agg_template))
                            .expect("aggregate should succeed"),
                    )
                })
            });
        }

        // Columnar path.
        {
            let (mut executor, _ctx) = create_test_executor();
            executor.set_columnar_threshold(4096);
            let agg_template = agg(op, column);
            group.bench_with_input(BenchmarkId::new("columnar", size), &size, |b, _| {
                b.iter(|| {
                    black_box(
                        executor
                            .run_in_memory_aggregate("n", nodes.clone(), black_box(&agg_template))
                            .expect("aggregate should succeed"),
                    )
                })
            });
        }
    }

    group.finish();
}

fn bench_sum(c: &mut Criterion) {
    bench_op(c, "sum", "n.age");
    bench_op(c, "sum", "n.score");
}
fn bench_min(c: &mut Criterion) {
    bench_op(c, "min", "n.age");
    bench_op(c, "min", "n.score");
}
fn bench_max(c: &mut Criterion) {
    bench_op(c, "max", "n.age");
    bench_op(c, "max", "n.score");
}
fn bench_avg(c: &mut Criterion) {
    bench_op(c, "avg", "n.age");
    bench_op(c, "avg", "n.score");
}

criterion_group!(benches, bench_sum, bench_min, bench_max, bench_avg);
criterion_main!(benches);
