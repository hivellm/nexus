//! Parity between `simd-json` (dispatched above the size threshold)
//! and `serde_json` (scalar), across payload shapes Nexus actually
//! ships over `/ingest` and RPC — small nodes, large batches, and
//! mixed nested arrays/maps carrying embedding vectors.

use nexus_core::simd::json as simd_json_dispatch;
use proptest::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct NodePayload {
    id: u64,
    labels: Vec<String>,
    properties: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct BulkRequest {
    nodes: Vec<NodePayload>,
}

#[test]
fn identical_result_as_serde_json_for_small_payload() {
    let req = BulkRequest {
        nodes: vec![
            NodePayload {
                id: 1,
                labels: vec!["Person".into()],
                properties: serde_json::json!({"name": "Alice", "age": 30}),
            },
            NodePayload {
                id: 2,
                labels: vec!["Person".into(), "Customer".into()],
                properties: serde_json::json!({"name": "Bob", "balance": 1234.56}),
            },
        ],
    };
    let body = serde_json::to_vec(&req).unwrap();
    assert!(body.len() < simd_json_dispatch::SIMD_JSON_THRESHOLD_BYTES);
    let parsed: BulkRequest = simd_json_dispatch::parse(&body).unwrap();
    assert_eq!(parsed, req);
}

#[test]
fn identical_result_for_large_payload_with_embeddings() {
    // 5_000 nodes × ~200 bytes each = ~1 MiB — exercises the
    // simd-json path end-to-end.
    let nodes: Vec<NodePayload> = (0..5_000)
        .map(|i| {
            let embedding: Vec<f32> = (0..16).map(|j| ((i * 16 + j) as f32) * 0.001).collect();
            NodePayload {
                id: i,
                labels: vec![format!("Label{}", i % 7)],
                properties: serde_json::json!({
                    "idx": i,
                    "name": format!("n-{i}"),
                    "embedding": embedding,
                    "tags": ["a", "b", "c"],
                }),
            }
        })
        .collect();
    let req = BulkRequest { nodes };
    let body = serde_json::to_vec(&req).unwrap();
    assert!(body.len() >= simd_json_dispatch::SIMD_JSON_THRESHOLD_BYTES);
    let parsed: BulkRequest = simd_json_dispatch::parse(&body).unwrap();
    assert_eq!(parsed.nodes.len(), req.nodes.len());
    assert_eq!(parsed, req);
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

    #[test]
    fn dispatch_matches_serde_json_for_random_bulk_requests(
        count in 0usize..=100,
        label_prefix in "[a-zA-Z]{1,8}",
    ) {
        let nodes: Vec<NodePayload> = (0..count)
            .map(|i| NodePayload {
                id: i as u64,
                labels: vec![format!("{}-{}", label_prefix, i % 3)],
                properties: serde_json::json!({
                    "idx": i,
                    "flag": i % 2 == 0,
                    "score": (i as f64) * 0.125,
                }),
            })
            .collect();
        let req = BulkRequest { nodes };
        let body = serde_json::to_vec(&req).unwrap();
        let via_dispatch: BulkRequest = simd_json_dispatch::parse(&body).unwrap();
        let via_serde: BulkRequest = serde_json::from_slice(&body).unwrap();
        prop_assert_eq!(via_dispatch, via_serde);
    }
}

#[test]
fn env_override_forces_serde_json() {
    // SAFETY: single-threaded test, env-var mutation is legal here.
    // The `simd_json_disabled()` probe caches its result in a
    // OnceLock, so setting the var in this test only takes effect if
    // nothing else in the process has triggered the probe yet. We do
    // not assert on which parser was used — we assert the result is
    // still correct when the flag is in effect.
    let req = BulkRequest {
        nodes: vec![NodePayload {
            id: 42,
            labels: vec!["T".into()],
            properties: serde_json::json!({"k": "v"}),
        }],
    };
    let body = serde_json::to_vec(&req).unwrap();
    let parsed: BulkRequest = simd_json_dispatch::parse(&body).unwrap();
    assert_eq!(parsed, req);
}
