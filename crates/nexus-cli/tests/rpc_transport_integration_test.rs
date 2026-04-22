//! Integration tests for the CLI's native binary RPC transport
//! (`phase2_cli-default-rpc-transport`).
//!
//! The CLI crate cannot depend on `nexus-server` (reverse-dep) so this
//! suite stands up a **minimal mock** RPC server speaking the exact
//! wire format defined in `nexus_protocol::rpc::{codec,types}`. The
//! mock:
//!
//! - binds a loopback TCP socket on an OS-assigned free port,
//! - accepts one connection,
//! - expects a `HELLO 1` handshake frame and replies with
//!   `Ok(Str("NEXUSRPC/1"))`,
//! - optionally expects an `AUTH <credentials>` frame and replies
//!   `Ok(Str("OK"))` when the provided args match, `Err("WRONGPASS …")`
//!   otherwise,
//! - then echoes every subsequent request as a CYPHER-shaped Map so
//!   the real `nexus_to_query_result` decoder in
//!   `nexus-cli/src/client.rs` is exercised.
//!
//! The assertions here prove the **client side** of the wire protocol
//! matches the server — any framing / id / handshake regression in the
//! CLI will fail loudly here without waiting for an end-to-end
//! integration against the real server.

use nexus_protocol::rpc::{NexusValue, Request, Response, read_request, write_response};
use std::net::SocketAddr;
use tokio::net::TcpListener;

/// Build a CYPHER-shape reply envelope.
fn cypher_envelope(columns: Vec<&str>, rows: Vec<Vec<NexusValue>>, elapsed_ms: i64) -> NexusValue {
    NexusValue::Map(vec![
        (
            NexusValue::Str("columns".into()),
            NexusValue::Array(
                columns
                    .into_iter()
                    .map(|s| NexusValue::Str(s.to_string()))
                    .collect(),
            ),
        ),
        (
            NexusValue::Str("rows".into()),
            NexusValue::Array(rows.into_iter().map(NexusValue::Array).collect()),
        ),
        (
            NexusValue::Str("execution_time_ms".into()),
            NexusValue::Int(elapsed_ms),
        ),
    ])
}

async fn spawn_mock(expect_auth: Option<(String, String)>, cypher_reply: NexusValue) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local_addr");

    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let (reader, writer) = stream.into_split();
        let mut reader = tokio::io::BufReader::new(reader);
        let mut writer = writer;

        // HELLO
        let hello: Request = read_request(&mut reader).await.expect("read HELLO");
        assert_eq!(hello.command, "HELLO", "first frame must be HELLO");
        write_response(
            &mut writer,
            &Response::ok(hello.id, NexusValue::Str("NEXUSRPC/1".into())),
        )
        .await
        .expect("write HELLO reply");

        // Optional AUTH step.
        if let Some((expected_user, expected_pass)) = expect_auth {
            let auth: Request = read_request(&mut reader).await.expect("read AUTH");
            assert_eq!(auth.command, "AUTH");
            let got_user = auth.args.first().and_then(|v| v.as_str()).unwrap_or("");
            let got_pass = auth.args.get(1).and_then(|v| v.as_str()).unwrap_or("");
            let resp = if got_user == expected_user && got_pass == expected_pass {
                Response::ok(auth.id, NexusValue::Str("OK".into()))
            } else {
                Response::err(auth.id, "WRONGPASS invalid username-password pair")
            };
            write_response(&mut writer, &resp)
                .await
                .expect("write AUTH reply");
        }

        // CYPHER — reply with the pre-baked envelope, then hang up.
        let req: Request = read_request(&mut reader).await.expect("read CYPHER");
        assert_eq!(req.command, "CYPHER");
        write_response(&mut writer, &Response::ok(req.id, cypher_reply))
            .await
            .expect("write CYPHER reply");
    });

    addr
}

#[tokio::test]
async fn cli_rpc_transport_round_trips_a_cypher_query() {
    let envelope = cypher_envelope(vec!["n"], vec![vec![NexusValue::Int(42)]], 3);
    let addr = spawn_mock(None, envelope).await;
    // NOTE: do not pre-connect — the mock only accepts one connection,
    // and the CLI's RpcTransport is the one under test.

    let url = format!("nexus://{}", addr);
    let client = nexus_cli::client::NexusClient::new(Some(&url), None, None, None, None)
        .expect("client build");
    assert!(client.is_rpc(), "nexus:// URL must use the RPC transport");

    let result = client
        .query("RETURN 42 AS n", None)
        .await
        .expect("query must succeed against the mock");
    assert_eq!(result.columns, vec!["n".to_string()]);
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0][0], serde_json::Value::from(42i64));
    let stats = result.stats.expect("stats present");
    assert_eq!(stats.execution_time_ms, 3.0);
}

#[tokio::test]
async fn cli_rpc_transport_sends_auth_when_credentials_supplied() {
    let envelope = cypher_envelope(vec!["x"], vec![vec![NexusValue::Int(1)]], 0);
    let addr = spawn_mock(Some(("alice".into(), "secret".into())), envelope).await;

    let url = format!("nexus://{}", addr);
    let client =
        nexus_cli::client::NexusClient::new(Some(&url), None, Some("alice"), Some("secret"), None)
            .expect("client build");

    let result = client.query("RETURN 1", None).await.expect("query ok");
    assert_eq!(result.rows.len(), 1);
}

#[tokio::test]
async fn cli_rpc_transport_surfaces_wrong_password() {
    // Mock expects "alice"/"secret" but the client sends the wrong pass.
    let envelope = cypher_envelope(vec!["x"], vec![], 0);
    let addr = spawn_mock(Some(("alice".into(), "secret".into())), envelope).await;

    let url = format!("nexus://{}", addr);
    let client =
        nexus_cli::client::NexusClient::new(Some(&url), None, Some("alice"), Some("wrong"), None)
            .expect("client build");

    let err = client
        .query("RETURN 1", None)
        .await
        .expect_err("wrong password must fail the query");
    let msg = err.to_string();
    assert!(
        msg.contains("authentication failed"),
        "error must blame authentication: {}",
        msg
    );
}

#[tokio::test]
async fn cli_transport_override_forces_http_even_on_nexus_url() {
    // Purely a unit-level check: when --transport http is set, the
    // client must not attempt RPC even if the URL scheme is nexus://.
    // The endpoint string is unused because we never actually connect.
    let client = nexus_cli::client::NexusClient::new(
        Some("nexus://example.invalid:15475"),
        None,
        None,
        None,
        Some("http"),
    )
    .expect("client build");
    assert!(!client.is_rpc(), "http override must disable RPC");
    assert_eq!(
        client.endpoint_description(),
        "http://example.invalid:15474"
    );
}
