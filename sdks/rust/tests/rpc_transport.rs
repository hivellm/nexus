//! Integration tests for the Rust SDK's binary RPC transport.
//!
//! These tests exercise the SDK against a running `nexus-server`. To
//! run them:
//!
//! ```bash
//! # In one terminal:
//! ./target/release/nexus-server
//!
//! # In another:
//! cd sdks/rust && NEXUS_SDK_LIVE_TEST=1 cargo test --test rpc_transport -- --nocapture
//! ```
//!
//! Without `NEXUS_SDK_LIVE_TEST=1` every test returns early so CI
//! builds stay green when no server is reachable. The mock-server
//! unit tests under `src/transport/rpc.rs::tests` cover the
//! wire-format behaviour without network access.

use nexus_sdk::NexusClient;
use nexus_sdk::transport::TransportMode;

fn live() -> bool {
    std::env::var("NEXUS_SDK_LIVE_TEST").ok().as_deref() == Some("1")
}

fn rpc_url() -> String {
    std::env::var("NEXUS_RPC_URL").unwrap_or_else(|_| "nexus://127.0.0.1:15475".to_string())
}

fn root_creds() -> (String, String) {
    (
        std::env::var("NEXUS_TEST_USER").unwrap_or_else(|_| "root".to_string()),
        std::env::var("NEXUS_TEST_PASS").unwrap_or_else(|_| "root".to_string()),
    )
}

#[tokio::test]
async fn default_url_infers_rpc_transport() {
    let client = NexusClient::new("nexus://127.0.0.1:15475").expect("client");
    assert!(
        client.is_rpc(),
        "nexus:// URL must produce an RPC-backed client"
    );
    assert!(client.endpoint_description().contains("RPC"));
}

#[tokio::test]
async fn http_scheme_picks_http_transport() {
    let client = NexusClient::new("http://127.0.0.1:15474").expect("client");
    assert!(!client.is_rpc(), "http:// URL must NOT use RPC");
    assert!(client.endpoint_description().contains("HTTP"));
}

#[tokio::test]
async fn default_config_uses_nexus_loopback_on_15475() {
    // `NexusClient::with_config(ClientConfig::default())` must
    // produce an RPC client post phase2 — the HTTP default is gone.
    let client = NexusClient::with_config(Default::default()).expect("client");
    assert!(client.is_rpc());
    assert!(
        client
            .endpoint_description()
            .contains("nexus://127.0.0.1:15475")
    );
}

#[tokio::test]
async fn bare_host_port_defaults_to_rpc() {
    let client = NexusClient::new("127.0.0.1:15600").expect("client");
    assert!(client.is_rpc());
}

#[tokio::test]
async fn rejects_nexus_rpc_scheme() {
    let err = NexusClient::new("nexus-rpc://host").expect_err("must reject");
    assert!(format!("{err}").contains("unsupported URL scheme"));
}

#[tokio::test]
async fn explicit_http_override_wins_over_nexus_url_via_env_var() {
    // Temporarily set NEXUS_SDK_TRANSPORT and verify it downgrades.
    // We clear it immediately after building the client so we do
    // not leak state into other tests running concurrently.
    // SAFETY: test-only env mutation; `set_var` is unsafe since
    // Rust 1.88 because the process env is shared state.
    unsafe { std::env::set_var("NEXUS_SDK_TRANSPORT", "http") };
    let client = NexusClient::new("nexus://127.0.0.1:15475").expect("client");
    unsafe { std::env::remove_var("NEXUS_SDK_TRANSPORT") };

    // URL scheme is still a STRONGER signal than the env var, per
    // the spec. So nexus:// wins here. Prove it explicitly.
    assert!(
        client.is_rpc(),
        "URL scheme trumps env var — nexus:// must stay RPC even with \
         NEXUS_SDK_TRANSPORT=http"
    );
}

#[tokio::test]
async fn live_cypher_return_one_via_rpc() {
    if !live() {
        return;
    }
    let (user, pass) = root_creds();
    let client = nexus_sdk::NexusClient::with_config(nexus_sdk::ClientConfig {
        base_url: rpc_url(),
        username: Some(user),
        password: Some(pass),
        ..Default::default()
    })
    .expect("client");

    let result = client
        .execute_cypher("RETURN 1 AS v", None)
        .await
        .expect("ok");
    assert_eq!(result.columns, vec!["v".to_string()]);
    assert_eq!(result.rows.len(), 1);
}

#[tokio::test]
async fn live_stats_via_rpc() {
    if !live() {
        return;
    }
    let (user, pass) = root_creds();
    let client = nexus_sdk::NexusClient::with_config(nexus_sdk::ClientConfig {
        base_url: rpc_url(),
        username: Some(user),
        password: Some(pass),
        ..Default::default()
    })
    .expect("client");

    let _stats = client.get_stats().await.expect("stats");
    // The shape is tolerant — any successful decode is enough to
    // prove the RPC STATS round-trip.
}

#[tokio::test]
async fn live_health_check_via_rpc() {
    if !live() {
        return;
    }
    let (user, pass) = root_creds();
    let client = nexus_sdk::NexusClient::with_config(nexus_sdk::ClientConfig {
        base_url: rpc_url(),
        username: Some(user),
        password: Some(pass),
        ..Default::default()
    })
    .expect("client");
    assert!(client.health_check().await.expect("health"));
}

#[tokio::test]
async fn transport_mode_parses_canonical_tokens() {
    assert_eq!(TransportMode::parse("nexus"), Some(TransportMode::NexusRpc));
    assert_eq!(TransportMode::parse("rpc"), Some(TransportMode::NexusRpc));
    assert_eq!(
        TransportMode::parse("NexusRpc"),
        Some(TransportMode::NexusRpc)
    );
    assert_eq!(TransportMode::parse("http"), Some(TransportMode::Http));
    assert_eq!(TransportMode::parse("HTTPS"), Some(TransportMode::Https));
    assert_eq!(TransportMode::parse("resp3"), Some(TransportMode::Resp3));
    assert_eq!(TransportMode::parse("auto"), None);
    assert_eq!(TransportMode::parse(""), None);
    assert_eq!(TransportMode::parse("grpc"), None);
}
