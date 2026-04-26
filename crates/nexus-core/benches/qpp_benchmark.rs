//! Quantified Path Pattern bench — slice 3b §9.
//!
//! Three workloads, all against the same fixture (a 50-node
//! `Person:KNOWS:Person:KNOWS:…` chain):
//!
//! - `legacy_var_length` — hand-written `*1..5`. Baseline for the
//!   slice-1 lowering parity check.
//! - `qpp_anonymous_body` — `( ()-[:KNOWS]->() ){1,5}`. Slice-1
//!   collapses this at parse time, so it must hit the same
//!   `VariableLengthPath` operator as the baseline. Runtime should
//!   be within Criterion's sample noise.
//! - `qpp_named_body` — `( (x:Person)-[:KNOWS]->() ){1,5}`.
//!   Drives the slice-2/3a `QuantifiedExpand` operator with a
//!   list-promoted inner var. Compared against the baseline by
//!   the §9.4 "≤ 1.3× legacy runtime" gate.
//!
//! ```text
//! cargo +nightly bench -p nexus-core --bench qpp_benchmark
//! ```
//!
//! In-process — no server, no RPC. Engine reused across iterations.

use criterion::{Criterion, criterion_group, criterion_main};
use nexus_core::{Engine, testing::setup_isolated_test_engine};
use std::hint::black_box;

const CHAIN_SIZE: usize = 50;

/// Build a `:Person` chain
/// `(p0)-[:KNOWS]->(p1)-[:KNOWS]->…->(pN-1)`. Slice-1's lowering
/// path and the slice-3a operator both walk this fixture; the
/// chain depth (50) outruns any sensible hop count, so the
/// `*1..5` and `{1,5}` workloads are not artificially bounded by
/// the data.
fn build_chain(engine: &mut Engine, n: usize) {
    for i in 0..n {
        let query = format!("CREATE (n:Person {{id: {}, name: 'Person{}'}})", i, i);
        engine.execute_cypher(&query).expect("create chain node");
    }
    for i in 0..(n - 1) {
        let query = format!(
            "MATCH (a:Person {{id: {}}}), (b:Person {{id: {}}}) CREATE (a)-[:KNOWS]->(b)",
            i,
            i + 1
        );
        engine.execute_cypher(&query).expect("create chain edge");
    }
}

fn bench_legacy_var_length(c: &mut Criterion) {
    let (mut engine, _ctx) = setup_isolated_test_engine().expect("setup engine");
    build_chain(&mut engine, CHAIN_SIZE);

    let mut group = c.benchmark_group("qpp/legacy_var_length");
    group.sample_size(10);
    group.bench_function("knows_*1..5", |b| {
        b.iter(|| {
            let result = engine
                .execute_cypher("MATCH (a:Person {id: 0})-[:KNOWS*1..5]->(b:Person) RETURN b.id");
            black_box(result)
        });
    });
    group.finish();
}

fn bench_qpp_anonymous_body(c: &mut Criterion) {
    let (mut engine, _ctx) = setup_isolated_test_engine().expect("setup engine");
    build_chain(&mut engine, CHAIN_SIZE);

    let mut group = c.benchmark_group("qpp/anonymous_body");
    group.sample_size(10);
    group.bench_function("knows_{1,5}_lowered", |b| {
        b.iter(|| {
            let result = engine.execute_cypher(
                "MATCH (a:Person {id: 0})( ()-[:KNOWS]->() ){1,5}(b:Person) RETURN b.id",
            );
            black_box(result)
        });
    });
    group.finish();
}

fn bench_qpp_named_body(c: &mut Criterion) {
    let (mut engine, _ctx) = setup_isolated_test_engine().expect("setup engine");
    build_chain(&mut engine, CHAIN_SIZE);

    let mut group = c.benchmark_group("qpp/named_body");
    group.sample_size(10);
    group.bench_function("knows_{1,5}_named_inner", |b| {
        b.iter(|| {
            let result = engine.execute_cypher(
                "MATCH (a:Person {id: 0})( (x:Person)-[:KNOWS]->() ){1,5}(b:Person) RETURN b.id",
            );
            black_box(result)
        });
    });
    group.finish();
}

/// Build a small fan-out tree: every node has `FANOUT` outgoing
/// `:KNOWS` edges to children, depth `DEPTH`. Total nodes =
/// `FANOUT^0 + FANOUT^1 + … + FANOUT^DEPTH`. Picking
/// `FANOUT=3, DEPTH=6` gives 1093 nodes — large enough that BFS
/// frontier work dominates over fixture build, small enough to
/// fit comfortably in a bench iteration.
fn build_fanout_tree(engine: &mut Engine, fanout: u32, depth: u32) {
    // Root.
    engine
        .execute_cypher("CREATE (n:TreeNode {id: 0})")
        .expect("create root");
    let mut next_id: u64 = 1;
    let mut frontier: Vec<u64> = vec![0];
    for _ in 0..depth {
        let mut next_frontier = Vec::with_capacity(frontier.len() * fanout as usize);
        for parent in &frontier {
            for _ in 0..fanout {
                let id = next_id;
                next_id += 1;
                let create_child = format!("CREATE (n:TreeNode {{id: {id}}})");
                engine.execute_cypher(&create_child).expect("create child");
                let link = format!(
                    "MATCH (a:TreeNode {{id: {parent}}}), (b:TreeNode {{id: {id}}}) \
                     CREATE (a)-[:KNOWS]->(b)"
                );
                engine.execute_cypher(&link).expect("link child");
                next_frontier.push(id);
            }
        }
        frontier = next_frontier;
    }
}

/// Slice 3b §9.3 — worst-case cycle-free traversal at depth 10.
/// The fan-out tree has only outgoing edges, so the BFS frontier
/// grows monotonically with each iteration; at depth 6 (`FANOUT^6
/// = 729` leaves) we already exercise the operator's per-frame
/// allocator more than the linear chain in the other benches.
/// Depth 10 stays out of reach because `FANOUT^10` would be 59049
/// nodes — too heavy for an in-process bench. Slice 4 lifts this
/// when the operator is run against a persisted fixture.
fn bench_qpp_dense_fanout(c: &mut Criterion) {
    let (mut engine, _ctx) = setup_isolated_test_engine().expect("setup engine");
    build_fanout_tree(&mut engine, 3, 6);

    let mut group = c.benchmark_group("qpp/dense_fanout");
    group.sample_size(10);
    group.bench_function("knows_{1,6}_root_to_leaves", |b| {
        b.iter(|| {
            let result = engine.execute_cypher(
                "MATCH (root:TreeNode {id: 0})( ()-[:KNOWS]->() ){1,6}(leaf:TreeNode) \
                 RETURN count(leaf)",
            );
            black_box(result)
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_legacy_var_length,
    bench_qpp_anonymous_body,
    bench_qpp_named_body,
    bench_qpp_dense_fanout,
);
criterion_main!(benches);
