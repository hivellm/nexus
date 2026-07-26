//! Interop cell: Rust SDK (`nexus-graph-sdk`) RPC transport against a
//! Thunder-based Nexus server.
//!
//! Drives `RpcTransport` directly rather than the sugar `NexusClient` — the
//! matrix is about the wire, and the transport is where the wire lives.
//!
//!     argv:   <host> <port> <user> <pass>
//!     stdout: one `STEP <name> PASS|FAIL <detail>` line per step
//!     exit:   0 iff every step passed

use std::process::ExitCode;

use nexus_sdk::error::NexusError;
use nexus_sdk::transport::rpc::{RpcCredentials, RpcTransport};
use nexus_sdk::transport::{Endpoint, Scheme};
use thunder::Value as NexusValue;

/// Node label used by this cell, distinct from the other language cells so a
/// shared server run does not collide.
const LABEL: &str = "InteropRust";
/// Marker id used by the `cypher` step — distinct per cell.
const ID_MARKER: i64 = 424201;

fn report(step: &str, ok: bool, detail: &str) {
    println!("STEP {step} {} {detail}", if ok { "PASS" } else { "FAIL" });
}

/// f32-LE encoding of `[1.5, -2.5, 3.5, +inf]` — emphatically not valid
/// UTF-8, so a transport that quietly round-trips `Bytes` through a string
/// cannot pass the `knn_bytes` step.
fn vec_bytes() -> Vec<u8> {
    let values: [f32; 4] = [1.5, -2.5, 3.5, f32::INFINITY];
    let mut out = Vec::with_capacity(values.len() * 4);
    for v in values {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// Lowercase hex, matching the sibling cells' detail formatting.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn is_pong(v: &NexusValue) -> bool {
    matches!(v, NexusValue::Str(s) if s == "PONG")
}

/// Look up a string key in a `Map`-kind [`NexusValue`].
fn map_get<'a>(v: &'a NexusValue, key: &str) -> Option<&'a NexusValue> {
    match v {
        NexusValue::Map(pairs) => pairs.iter().find_map(|(k, val)| match k {
            NexusValue::Str(s) if s == key => Some(val),
            _ => None,
        }),
        _ => None,
    }
}

/// Run `CYPHER` over the transport and return rows as nested `Vec`s,
/// mirroring the reference cell's `cypher_rows`.
async fn cypher_rows(
    t: &RpcTransport,
    query: &str,
    params: Vec<(NexusValue, NexusValue)>,
) -> Result<Vec<Vec<NexusValue>>, NexusError> {
    let mut args = vec![NexusValue::Str(query.to_string())];
    if !params.is_empty() {
        args.push(NexusValue::Map(params));
    }
    let resp = t.call("CYPHER", args).await?;
    if let Some(NexusValue::Str(msg)) = map_get(&resp, "error") {
        if !msg.is_empty() {
            return Err(NexusError::Api {
                message: msg.clone(),
                status: 0,
            });
        }
    }
    let rows = match map_get(&resp, "rows") {
        Some(NexusValue::Array(rows)) => rows,
        _ => return Ok(Vec::new()),
    };
    Ok(rows
        .iter()
        .filter_map(|row| match row {
            NexusValue::Array(cells) => Some(cells.clone()),
            _ => None,
        })
        .collect())
}

/// Step 1: `PING` answers before `AUTH`; `STATS` is refused before `AUTH`
/// and succeeds after. Returns the authenticated transport for the
/// remaining steps regardless of outcome, so the matrix still gets all four
/// `STEP` lines.
async fn step_auth(endpoint: &Endpoint, user: &str, pass: &str) -> (bool, String, RpcTransport) {
    let anon = RpcTransport::new(endpoint.clone(), RpcCredentials::default());
    let ping_ok = matches!(anon.call("PING", vec![]).await, Ok(v) if is_pong(&v));

    let stats_pre_refused = match anon.call("STATS", vec![]).await {
        Err(e) => {
            let msg = e.to_string();
            msg.to_lowercase().contains("auth") || msg.contains("NOAUTH")
        }
        Ok(_) => false,
    };

    let authed = RpcTransport::new(
        endpoint.clone(),
        RpcCredentials {
            api_key: None,
            username: Some(user.to_string()),
            password: Some(pass.to_string()),
        },
    );
    let stats_post_ok = matches!(
        authed.call("STATS", vec![]).await,
        Ok(NexusValue::Map(_)) | Ok(NexusValue::Str(_))
    );

    let ok = ping_ok && stats_pre_refused && stats_post_ok;
    let detail =
        format!("ping={ping_ok} stats_pre_refused={stats_pre_refused} stats_post={stats_post_ok}");
    (ok, detail, authed)
}

/// Step 2: `CREATE` then `MATCH` round-trips the id parameter back.
async fn step_cypher(t: &RpcTransport) -> (bool, String) {
    let params = vec![(NexusValue::Str("id".into()), NexusValue::Int(ID_MARKER))];
    let create = format!("CREATE (n:{LABEL} {{id: $id}}) RETURN n.id");
    let match_query = format!("MATCH (n:{LABEL} {{id: $id}}) RETURN n.id");

    if let Err(e) = cypher_rows(t, &create, params.clone()).await {
        return (false, format!("{e}"));
    }
    let rows = match cypher_rows(t, &match_query, params).await {
        Ok(rows) => rows,
        Err(e) => return (false, format!("{e}")),
    };

    let got = rows.first().and_then(|row| row.first());
    let ok = matches!(got, Some(NexusValue::Int(id)) if *id == ID_MARKER);
    (ok, format!("round-trip id -> {got:?}"))
}

/// Step 3: a raw f32-LE vector carried as `Bytes` round-trips byte-for-byte
/// via a `PING` echo.
async fn step_knn_bytes(t: &RpcTransport) -> (bool, String) {
    let blob = vec_bytes();
    let echoed = t.call("PING", vec![NexusValue::from(blob.clone())]).await;
    let got: Vec<u8> = match echoed {
        Ok(NexusValue::Bytes(b)) => b.to_vec(),
        _ => Vec::new(),
    };
    let ok = got == blob && blob.len() == 16;
    (ok, format!("{} -> {}", hex(&blob), hex(&got)))
}

/// Step 4: a deliberately broken `CYPHER` surfaces a typed server error (not
/// a transport crash), and the same connection stays usable afterwards.
async fn step_error(t: &RpcTransport) -> (bool, String) {
    match cypher_rows(t, "MATCH (n RETURN", vec![]).await {
        Ok(_) => (false, "expected a server error, got a result".to_string()),
        Err(e) => {
            let alive = matches!(t.call("PING", vec![]).await, Ok(v) if is_pong(&v));
            (alive, format!("raised {e}; connection alive={alive}"))
        }
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let (host, port, user, pass) = match args.as_slice() {
        [_, host, port_str, user, pass] => match port_str.parse::<u16>() {
            Ok(port) => (host.clone(), port, user.clone(), pass.clone()),
            Err(e) => {
                eprintln!("invalid port '{port_str}': {e}");
                return ExitCode::FAILURE;
            }
        },
        _ => {
            eprintln!("usage: interop <host> <port> <user> <pass>");
            return ExitCode::FAILURE;
        }
    };

    let endpoint = Endpoint {
        scheme: Scheme::Rpc,
        host,
        port,
    };

    let mut failures = 0u32;

    let (auth_ok, auth_detail, authed) = step_auth(&endpoint, &user, &pass).await;
    report("auth", auth_ok, &auth_detail);
    if !auth_ok {
        failures += 1;
    }

    let (cypher_ok, cypher_detail) = step_cypher(&authed).await;
    report("cypher", cypher_ok, &cypher_detail);
    if !cypher_ok {
        failures += 1;
    }

    let (bytes_ok, bytes_detail) = step_knn_bytes(&authed).await;
    report("knn_bytes", bytes_ok, &bytes_detail);
    if !bytes_ok {
        failures += 1;
    }

    let (error_ok, error_detail) = step_error(&authed).await;
    report("error", error_ok, &error_detail);
    if !error_ok {
        failures += 1;
    }

    if failures == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
