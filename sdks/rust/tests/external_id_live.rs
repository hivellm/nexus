//! Phase 10 — Rust SDK live external-id suite.
//!
//! Mirrors the depth of the live suites that ship with the other 5 SDKs
//! (Python, TypeScript, Go, C#, PHP) so the Rust SDK stays at parity:
//! 6 ExternalId variants, 3 ConflictPolicy values (REPLACE asserts a
//! property change to guard the fd001344 regression), Cypher `_id`
//! round-trip, length-cap rejection, absent-id resolution.
//!
//! Gated on the `NEXUS_LIVE_HOST` env var, matching the other SDKs.
//! Without it, every test marks itself ignored at runtime so unit-only
//! CI runs do not require a server.

use nexus_sdk::{NexusClient, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

static UNIQ: AtomicU64 = AtomicU64::new(0);

fn live_host() -> Option<String> {
    std::env::var("NEXUS_LIVE_HOST").ok()
}

fn client() -> NexusClient {
    let host = live_host().expect("NEXUS_LIVE_HOST must be set");
    NexusClient::new(&host).expect("client init")
}

/// Per-test unique suffix so tests don't collide on the shared catalog.
fn uniq() -> String {
    let n = UNIQ.fetch_add(1, Ordering::SeqCst);
    format!("{}-{}", std::process::id(), n)
}

fn props(name: &str, val: Value) -> HashMap<String, Value> {
    let mut p = HashMap::new();
    p.insert(name.to_string(), val);
    p
}

// ── 6 variant round-trips ──────────────────────────────────────────────

macro_rules! skip_if_no_host {
    () => {
        if live_host().is_none() {
            eprintln!("skipping — NEXUS_LIVE_HOST not set");
            return;
        }
    };
}

#[tokio::test]
async fn sha256_variant_round_trip() {
    skip_if_no_host!();
    let c = client();
    let ext = format!(
        "sha256:{}",
        format!("{:0>64}", format!("a{}", uniq()).replace("-", ""))[..64].to_string()
    );
    let r = c
        .create_node_with_external_id(
            vec!["LiveRustSha256".to_string()],
            props("name", Value::String("a".into())),
            ext.clone(),
            Some("error"),
        )
        .await
        .unwrap();
    assert!(r.error.is_none(), "create error: {:?}", r.error);
    let g = c.get_node_by_external_id(ext).await.unwrap();
    let n = g.node.expect("node present");
    assert_eq!(n.id, r.node_id);
}

#[tokio::test]
async fn blake3_variant_round_trip() {
    skip_if_no_host!();
    let c = client();
    let ext = format!(
        "blake3:{}",
        format!("{:0>64}", format!("b{}", uniq()).replace("-", ""))[..64].to_string()
    );
    let r = c
        .create_node_with_external_id(
            vec!["LiveRustBlake3".to_string()],
            HashMap::new(),
            ext.clone(),
            None,
        )
        .await
        .unwrap();
    assert!(r.error.is_none(), "create error: {:?}", r.error);
    let g = c.get_node_by_external_id(ext).await.unwrap();
    assert_eq!(g.node.expect("node").id, r.node_id);
}

#[tokio::test]
async fn sha512_variant_round_trip() {
    skip_if_no_host!();
    let c = client();
    let ext = format!(
        "sha512:{}",
        format!("{:0>128}", format!("c{}", uniq()).replace("-", ""))[..128].to_string()
    );
    let r = c
        .create_node_with_external_id(
            vec!["LiveRustSha512".to_string()],
            HashMap::new(),
            ext.clone(),
            None,
        )
        .await
        .unwrap();
    assert!(r.error.is_none(), "create error: {:?}", r.error);
    let g = c.get_node_by_external_id(ext).await.unwrap();
    assert_eq!(g.node.expect("node").id, r.node_id);
}

#[tokio::test]
async fn uuid_variant_round_trip() {
    skip_if_no_host!();
    let c = client();
    let raw = uniq();
    let padded = format!("{:0>32}", raw.replace("-", ""));
    let p = padded.as_str();
    let ext = format!(
        "uuid:{}-{}-{}-{}-{}",
        &p[0..8],
        &p[8..12],
        &p[12..16],
        &p[16..20],
        &p[20..32]
    );
    let r = c
        .create_node_with_external_id(
            vec!["LiveRustUuid".to_string()],
            HashMap::new(),
            ext.clone(),
            None,
        )
        .await
        .unwrap();
    assert!(r.error.is_none(), "create error: {:?}", r.error);
    let g = c.get_node_by_external_id(ext).await.unwrap();
    assert_eq!(g.node.expect("node").id, r.node_id);
}

#[tokio::test]
async fn str_variant_round_trip() {
    skip_if_no_host!();
    let c = client();
    let ext = format!("str:rust-live-{}", uniq());
    let r = c
        .create_node_with_external_id(
            vec!["LiveRustStr".to_string()],
            HashMap::new(),
            ext.clone(),
            None,
        )
        .await
        .unwrap();
    assert!(r.error.is_none(), "create error: {:?}", r.error);
    let g = c.get_node_by_external_id(ext).await.unwrap();
    assert_eq!(g.node.expect("node").id, r.node_id);
}

#[tokio::test]
async fn bytes_variant_round_trip() {
    skip_if_no_host!();
    let c = client();
    let hex = format!("{:0>16}", uniq().replace("-", ""))[..16].to_string();
    let ext = format!("bytes:{}", hex);
    let r = c
        .create_node_with_external_id(
            vec!["LiveRustBytes".to_string()],
            HashMap::new(),
            ext.clone(),
            None,
        )
        .await
        .unwrap();
    assert!(r.error.is_none(), "create error: {:?}", r.error);
    let g = c.get_node_by_external_id(ext).await.unwrap();
    assert_eq!(g.node.expect("node").id, r.node_id);
}

// ── 3 conflict policies ────────────────────────────────────────────────

#[tokio::test]
async fn conflict_policy_error_rejects_duplicate() {
    skip_if_no_host!();
    let c = client();
    let ext = format!("str:rust-error-{}", uniq());
    let first = c
        .create_node_with_external_id(
            vec!["LiveRustErr".to_string()],
            HashMap::new(),
            ext.clone(),
            None,
        )
        .await
        .unwrap();
    assert!(first.error.is_none());
    let dup = c
        .create_node_with_external_id(
            vec!["LiveRustErr".to_string()],
            HashMap::new(),
            ext.clone(),
            Some("error"),
        )
        .await
        .unwrap();
    assert!(dup.error.is_some(), "second create must error");
}

#[tokio::test]
async fn conflict_policy_match_returns_existing_id() {
    skip_if_no_host!();
    let c = client();
    let ext = format!("str:rust-match-{}", uniq());
    let first = c
        .create_node_with_external_id(
            vec!["LiveRustMatch".to_string()],
            props("v", Value::Int(1)),
            ext.clone(),
            None,
        )
        .await
        .unwrap();
    let again = c
        .create_node_with_external_id(
            vec!["LiveRustMatch".to_string()],
            props("v", Value::Int(999)),
            ext.clone(),
            Some("match"),
        )
        .await
        .unwrap();
    assert!(again.error.is_none());
    assert_eq!(again.node_id, first.node_id);
}

#[tokio::test]
async fn conflict_policy_replace_overwrites_properties() {
    skip_if_no_host!();
    let c = client();
    let ext = format!("str:rust-replace-{}", uniq());
    let first = c
        .create_node_with_external_id(
            vec!["LiveRustReplace".to_string()],
            props("v", Value::Int(1)),
            ext.clone(),
            None,
        )
        .await
        .unwrap();
    let again = c
        .create_node_with_external_id(
            vec!["LiveRustReplace".to_string()],
            props("v", Value::Int(999)),
            ext.clone(),
            Some("replace"),
        )
        .await
        .unwrap();
    assert!(again.error.is_none());
    assert_eq!(again.node_id, first.node_id);
    // Regression guard for fd001344 — REPLACE must update prop_ptr.
    let q = c
        .execute_cypher(
            &format!(
                "MATCH (n:LiveRustReplace) WHERE n._id = '{}' RETURN n.v",
                ext
            ),
            None,
        )
        .await
        .unwrap();
    assert!(q.error.is_none());
    let row = &q.rows[0];
    let v = row.as_array().expect("array row");
    assert_eq!(v[0].as_i64(), Some(999), "replace must overwrite v");
}

// ── Cypher _id round-trip ──────────────────────────────────────────────

#[tokio::test]
async fn cypher_create_with_id_literal_round_trip() {
    skip_if_no_host!();
    let c = client();
    let ext = format!("str:rust-cyp-{}", uniq());
    let q = format!(
        "CREATE (n:LiveRustCyp {{_id: '{}', name: 'cypher'}}) RETURN n._id",
        ext
    );
    let r = c.execute_cypher(&q, None).await.unwrap();
    assert!(r.error.is_none(), "cypher error: {:?}", r.error);
    let row = r.rows[0].as_array().expect("array row");
    assert_eq!(row[0].as_str(), Some(ext.as_str()));
}

// ── Length-cap rejection ──────────────────────────────────────────────

#[tokio::test]
async fn str_too_long_is_rejected() {
    skip_if_no_host!();
    let c = client();
    let ext = format!("str:{}", "a".repeat(257));
    let r = c
        .create_node_with_external_id(
            vec!["LiveRustCap".to_string()],
            HashMap::new(),
            ext,
            None,
        )
        .await
        .unwrap();
    assert!(r.error.is_some(), "oversize str should error");
}

#[tokio::test]
async fn bytes_too_long_is_rejected() {
    skip_if_no_host!();
    let c = client();
    let ext = format!("bytes:{}", "ff".repeat(65));
    let r = c
        .create_node_with_external_id(
            vec!["LiveRustCap".to_string()],
            HashMap::new(),
            ext,
            None,
        )
        .await
        .unwrap();
    assert!(r.error.is_some(), "oversize bytes should error");
}

#[tokio::test]
async fn uuid_empty_payload_is_rejected() {
    skip_if_no_host!();
    let c = client();
    let r = c
        .create_node_with_external_id(
            vec!["LiveRustCap".to_string()],
            HashMap::new(),
            "uuid:".to_string(),
            None,
        )
        .await
        .unwrap();
    assert!(r.error.is_some(), "empty uuid should error");
}

// ── Absent external id ────────────────────────────────────────────────

#[tokio::test]
async fn get_node_by_absent_external_id_returns_none() {
    skip_if_no_host!();
    let c = client();
    let ext = format!("str:rust-absent-{}", uniq());
    let g = c.get_node_by_external_id(ext).await.unwrap();
    assert!(g.node.is_none(), "absent external id should resolve to None");
    assert!(g.error.is_none(), "absent != error");
}
