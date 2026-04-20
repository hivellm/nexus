//! Criterion bench for the filter-operator columnar fast path.
//!
//! Runs a single `variable.property OP numeric-literal` WHERE against
//! the same 100 000-row fixture twice — once with
//! `columnar_threshold = usize::MAX` (pins the scalar row path) and
//! once with the production default `4096` (columnar fires at
//! 100 000 >> 4096). Identical fixture on both lines, so the delta
//! in Criterion's report is a clean row-vs-column ratio.
//!
//! ```text
//! cargo +nightly bench -p nexus-core --bench executor_filter
//! ```
//!
//! In-process — no server, no RPC, no catalog I/O beyond what
//! `create_test_executor` touches to warm a temp dir once. See
//! `docs/specs/executor-columnar.md` for interpretation guidance.

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use nexus_core::testing::create_test_executor;
use serde_json::{Map, Number, Value};

const FIXTURE_SIZE: usize = 100_000;

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

fn bench_case(c: &mut Criterion, group_name: &str, predicate: &'static str, nodes: &[Value]) {
    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Elements(nodes.len() as u64));

    // Row path — threshold above the fixture size pins the scalar
    // loop. Executor reused across iterations; `run_in_memory_filter`
    // internally rebuilds the lightweight per-call context because
    // `execute_filter` consumes the working set.
    {
        let (mut executor, _ctx) = create_test_executor();
        executor.set_columnar_threshold(usize::MAX);
        group.bench_with_input(
            BenchmarkId::new("row", nodes.len()),
            &nodes.len(),
            |b, _| {
                b.iter(|| {
                    black_box(
                        executor
                            .run_in_memory_filter("n", nodes.to_vec(), black_box(predicate))
                            .expect("filter should succeed"),
                    )
                })
            },
        );
    }

    // Columnar path — default threshold (4096). 100 000 >> 4096 so
    // the fast path fires for every iteration.
    {
        let (mut executor, _ctx) = create_test_executor();
        executor.set_columnar_threshold(4096);
        group.bench_with_input(
            BenchmarkId::new("columnar", nodes.len()),
            &nodes.len(),
            |b, _| {
                b.iter(|| {
                    black_box(
                        executor
                            .run_in_memory_filter("n", nodes.to_vec(), black_box(predicate))
                            .expect("filter should succeed"),
                    )
                })
            },
        );
    }

    group.finish();
}

fn bench_filter_i64_gt(c: &mut Criterion) {
    let nodes = make_fixture(FIXTURE_SIZE);
    bench_case(c, "filter_i64_gt", "n.age > 50000", &nodes);
}

fn bench_filter_f64_lt(c: &mut Criterion) {
    let nodes = make_fixture(FIXTURE_SIZE);
    bench_case(c, "filter_f64_lt", "n.score < 25000.0", &nodes);
}

criterion_group!(benches, bench_filter_i64_gt, bench_filter_f64_lt);
criterion_main!(benches);
