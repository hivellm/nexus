//! Protocol-level benchmark: wire-codec throughput for a typical
//! point-read request/response shape.
//!
//! This is the scaffolding for the phase3 benchmark matrix described
//! in `docs/specs/rpc-wire-format.md` and
//! `.rulebook/tasks/phase3_rpc-protocol-docs-benchmarks/`. The
//! in-crate version measures the wire-format hot path that every
//! end-to-end point read has to traverse:
//!
//! 1. Encode a `Request { id, command: "CYPHER", args: [<query>, <params>] }`
//!    into a length-prefixed MessagePack frame.
//! 2. Decode the server's `Response::ok(id, <row-shaped NexusValue>)`
//!    back into a typed value.
//!
//! Combined, the two steps establish a lower bound on per-request
//! latency regardless of network, server, or storage. Production
//! point-read numbers live in `docs/PERFORMANCE.md` under "Protocol
//! benchmarks — v1.0.0"; that report runs the full end-to-end matrix
//! described in the phase3 task against a live `nexus-server`.

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use nexus_protocol::rpc::codec::{decode_frame, encode_frame};
use nexus_protocol::rpc::types::{NexusValue, Request, Response};
use std::hint::black_box;

/// Build a realistic point-read request frame: `CYPHER "MATCH (n:Person
/// {id: $id}) RETURN n" {"id": <n>}`.
fn build_request(id: u32) -> Request {
    Request {
        id,
        command: "CYPHER".to_string(),
        args: vec![
            NexusValue::Str("MATCH (n:Person {id: $id}) RETURN n".to_string()),
            NexusValue::Map(vec![(
                NexusValue::Str("id".to_string()),
                NexusValue::Int(id as i64),
            )]),
        ],
    }
}

/// Build a row-shaped response: a `columns`/`rows` map matching the
/// Neo4j-compatible `QueryResult` shape the server actually returns.
fn build_response(id: u32) -> Response {
    let node = NexusValue::Map(vec![
        (
            NexusValue::Str("id".to_string()),
            NexusValue::Int(id as i64),
        ),
        (
            NexusValue::Str("labels".to_string()),
            NexusValue::Array(vec![NexusValue::Str("Person".to_string())]),
        ),
        (
            NexusValue::Str("properties".to_string()),
            NexusValue::Map(vec![
                (
                    NexusValue::Str("name".to_string()),
                    NexusValue::Str(format!("User-{id}")),
                ),
                (
                    NexusValue::Str("age".to_string()),
                    NexusValue::Int((id as i64) % 100),
                ),
            ]),
        ),
    ]);
    let body = NexusValue::Map(vec![
        (
            NexusValue::Str("columns".to_string()),
            NexusValue::Array(vec![NexusValue::Str("n".to_string())]),
        ),
        (
            NexusValue::Str("rows".to_string()),
            NexusValue::Array(vec![NexusValue::Array(vec![node])]),
        ),
    ]);
    Response::ok(id, body)
}

fn bench_encode_request(c: &mut Criterion) {
    let req = build_request(42);
    let frame = encode_frame(&req).expect("encode");
    let mut group = c.benchmark_group("protocol_point_read/encode_request");
    group.throughput(Throughput::Bytes(frame.len() as u64));
    group.bench_function(BenchmarkId::from_parameter(frame.len()), |b| {
        b.iter(|| {
            let out = encode_frame(black_box(&req)).expect("encode");
            black_box(out);
        })
    });
    group.finish();
}

fn bench_decode_response(c: &mut Criterion) {
    let resp = build_response(42);
    let frame = encode_frame(&resp).expect("encode");
    let mut group = c.benchmark_group("protocol_point_read/decode_response");
    group.throughput(Throughput::Bytes(frame.len() as u64));
    group.bench_function(BenchmarkId::from_parameter(frame.len()), |b| {
        b.iter(|| {
            let (decoded, _): (Response, usize) = decode_frame(black_box(&frame))
                .expect("decode")
                .expect("complete");
            black_box(decoded);
        })
    });
    group.finish();
}

fn bench_roundtrip(c: &mut Criterion) {
    // Full request-encode + response-decode — the hot path a real
    // client walks on every point read (minus the network round-trip).
    let req = build_request(42);
    let resp = build_response(42);
    let resp_frame = encode_frame(&resp).expect("encode");
    let mut group = c.benchmark_group("protocol_point_read/roundtrip");
    group.throughput(Throughput::Elements(1));
    group.bench_function("encode_request+decode_response", |b| {
        b.iter(|| {
            let req_frame = encode_frame(black_box(&req)).expect("encode");
            let (decoded, _): (Response, usize) = decode_frame(black_box(&resp_frame))
                .expect("decode")
                .expect("complete");
            black_box((req_frame, decoded));
        })
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_encode_request,
    bench_decode_response,
    bench_roundtrip,
);
criterion_main!(benches);
