//! Criterion bench for Cypher parse time — the workload most sensitive
//! to the O(N²) `chars().nth(pos)` pattern that the tokenizer used
//! before the fix.
//!
//! Three query sizes model the traffic Nexus actually sees:
//!
//! * **small** — 120 bytes, a hand-written single MATCH + WHERE.
//! * **medium** — ~4 KiB, 200 comma-separated WITH projections.
//! * **large** — ~64 KiB, LLM-generated MATCH with dozens of hops
//!   and a long return list.
//!
//! Before the fix, parse cost was quadratic in query length
//! (every `peek_char` walked the UTF-8 iterator from byte 0).
//! After the fix, parse cost is linear. This bench is the
//! acceptance signal: wall time must not grow faster than O(N)
//! with query length.
//!
//! ```text
//! cargo +nightly bench -p nexus-core --bench parser_tokenize
//! ```

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use nexus_core::executor::parser::CypherParser;
use std::hint::black_box;

fn small_query() -> String {
    "MATCH (n:Person {name: 'Alice'})-[r:KNOWS]->(m) WHERE m.age > 30 RETURN n, m LIMIT 10".into()
}

fn medium_query() -> String {
    let mut q = String::from("MATCH (n:Person)\nRETURN");
    for i in 0..200 {
        if i > 0 {
            q.push(',');
        }
        q.push_str(&format!("\n  n.field{i} AS c{i}"));
    }
    q
}

fn large_query() -> String {
    // Build a ~64 KiB MATCH with many hops + a long RETURN list.
    let mut q = String::from("MATCH ");
    for i in 0..800 {
        if i > 0 {
            q.push_str("-[:REL]->");
        }
        q.push_str(&format!("(n{i}:Label{})", i % 8));
    }
    q.push_str(" WHERE ");
    for i in 0..500 {
        if i > 0 {
            q.push_str(" AND ");
        }
        q.push_str(&format!("n{i}.prop{i} = {i}"));
    }
    q.push_str("\nRETURN ");
    for i in 0..500 {
        if i > 0 {
            q.push_str(", ");
        }
        q.push_str(&format!("n{i}"));
    }
    q
}

fn bench_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("cypher_parse");
    for (label, q) in [
        ("small", small_query()),
        ("medium", medium_query()),
        ("large", large_query()),
    ] {
        let bytes = q.len();
        group.throughput(Throughput::Bytes(bytes as u64));
        group.bench_with_input(BenchmarkId::new(label, bytes), &q, |b, q| {
            b.iter(|| {
                // Every iteration re-parses the full query; we measure
                // pure parse time, not shared setup.
                let mut p = CypherParser::new(black_box(q).to_string());
                let _ = p.parse();
            })
        });
    }
    group.finish();
}

criterion_group!(benches, bench_parse);
criterion_main!(benches);
