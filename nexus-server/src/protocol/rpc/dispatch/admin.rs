//! Admin commands: PING, HELLO, AUTH, QUIT.
//!
//! All four are always accepted pre-auth (see [`super::PRE_AUTH_COMMANDS`]).
//! Their job is to get the connection into an authenticated state and
//! advertise server identity / protocol version.

use crate::protocol::rpc::NexusValue;

use super::{RpcSession, arg_str};

/// RPC protocol version advertised by `HELLO`. Bumped if the wire format
/// changes in a backwards-incompatible way.
pub const RPC_PROTO_VERSION: i64 = 1;

/// Dispatch admin commands under the session. Commands that reach here
/// have already cleared [`super::run`]'s uppercasing and auth-gate checks.
pub async fn run(
    state: &RpcSession,
    command: &str,
    args: &[NexusValue],
) -> Result<NexusValue, String> {
    match command {
        "PING" => ping(args),
        "HELLO" => Ok(hello(state)),
        "AUTH" => auth(state, args).await,
        "QUIT" => quit(args),
        other => Err(format!("ERR unknown admin command '{other}'")),
    }
}

// ── PING ─────────────────────────────────────────────────────────────────────

/// `PING` returns `"PONG"`. `PING <msg>` echoes the payload back so clients
/// can match response-to-request without consulting `id`.
fn ping(args: &[NexusValue]) -> Result<NexusValue, String> {
    match args.len() {
        0 => Ok(NexusValue::Str("PONG".into())),
        1 => match &args[0] {
            NexusValue::Str(s) => Ok(NexusValue::Str(s.clone())),
            NexusValue::Bytes(b) => Ok(NexusValue::Bytes(b.clone())),
            _ => Err("ERR PING argument must be a string or bytes".into()),
        },
        n => Err(format!("ERR wrong number of arguments for 'PING' ({n})")),
    }
}

// ── HELLO ────────────────────────────────────────────────────────────────────

/// `HELLO` advertises server identity and the negotiated protocol version.
/// Unlike RESP3 this RPC has a single wire format; clients call `HELLO`
/// purely for metadata and to learn the server's `connection_id`.
fn hello(state: &RpcSession) -> NexusValue {
    NexusValue::Map(vec![
        (
            NexusValue::Str("server".into()),
            NexusValue::Str("nexus".into()),
        ),
        (
            NexusValue::Str("version".into()),
            NexusValue::Str(env!("CARGO_PKG_VERSION").into()),
        ),
        (
            NexusValue::Str("proto".into()),
            NexusValue::Int(RPC_PROTO_VERSION),
        ),
        (
            NexusValue::Str("id".into()),
            NexusValue::Int(state.connection_id as i64),
        ),
        (
            NexusValue::Str("authenticated".into()),
            NexusValue::Bool(state.is_authenticated()),
        ),
    ])
}

// ── AUTH ─────────────────────────────────────────────────────────────────────

/// `AUTH <api_key>` — verifies a Nexus API key through `AuthManager`.
/// `AUTH <username> <password>` — verifies a user via the RBAC store, with
/// a root-user fast-path that matches the REST `/login` endpoint so the
/// first ever connection can still authenticate.
///
/// On success the per-connection authentication flag is flipped and the
/// response is `"OK"`. On failure the response is a `WRONGPASS` error and
/// the flag stays off.
async fn auth(state: &RpcSession, args: &[NexusValue]) -> Result<NexusValue, String> {
    match args.len() {
        1 => {
            let api_key = arg_str(args, 0)?;
            if verify_api_key(state, &api_key) {
                state.mark_authenticated();
                Ok(NexusValue::Str("OK".into()))
            } else {
                Err("WRONGPASS invalid API key".into())
            }
        }
        2 => {
            let username = arg_str(args, 0)?;
            let password = arg_str(args, 1)?;
            if verify_user_password(state, &username, &password).await {
                state.mark_authenticated();
                Ok(NexusValue::Str("OK".into()))
            } else {
                Err("WRONGPASS invalid username-password pair".into())
            }
        }
        n => Err(format!("ERR wrong number of arguments for 'AUTH' ({n})")),
    }
}

fn verify_api_key(state: &RpcSession, api_key: &str) -> bool {
    matches!(
        state.server.auth_manager.verify_api_key(api_key),
        Ok(Some(_))
    )
}

async fn verify_user_password(state: &RpcSession, username: &str, password: &str) -> bool {
    // Root fast-path: if the configured root account is enabled, accept it
    // against the plaintext credentials from `RootUserConfig`. Mirrors how
    // the REST `/login` endpoint treats root so a freshly-booted server
    // lets the operator in over RPC before any RBAC user exists.
    let root = &state.server.root_user_config;
    if root.enabled && username == root.username && password == root.password {
        return true;
    }

    let rbac = state.server.rbac.read().await;
    let Some(user) = rbac
        .list_users()
        .into_iter()
        .find(|u| u.username == username)
    else {
        return false;
    };
    if !user.is_active {
        return false;
    }
    match &user.password_hash {
        Some(hash) => nexus_core::auth::verify_password(password, hash),
        None => false,
    }
}

// ── QUIT ─────────────────────────────────────────────────────────────────────

/// `QUIT` acknowledges and the connection loop closes the socket after it
/// sees this command name (see `server.rs`, added in Phase 7).
fn quit(args: &[NexusValue]) -> Result<NexusValue, String> {
    if !args.is_empty() {
        return Err(format!(
            "ERR wrong number of arguments for 'QUIT' ({})",
            args.len()
        ));
    }
    Ok(NexusValue::Str("OK".into()))
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Build a real `RpcSession` backed by a temporary data directory and a
    /// default-constructed RBAC + AuthManager + AuditLogger chain. Mirrors
    /// the RESP3 admin tests' setup so we exercise exactly the same layers
    /// that production code does.
    fn session(auth_required: bool) -> RpcSession {
        let ctx = nexus_core::testing::TestContext::new();
        let engine =
            nexus_core::Engine::with_data_dir(ctx.path()).expect("engine init for rpc admin test");
        let engine_arc = Arc::new(tokio::sync::RwLock::new(engine));

        let executor_arc = Arc::new(nexus_core::executor::Executor::default());
        let dbm_arc = Arc::new(parking_lot::RwLock::new(
            nexus_core::database::DatabaseManager::new(ctx.path().to_path_buf()).expect("dbm init"),
        ));
        let rbac_arc = Arc::new(tokio::sync::RwLock::new(
            nexus_core::auth::RoleBasedAccessControl::new(),
        ));
        let audit_logger = Arc::new(
            nexus_core::auth::AuditLogger::new(nexus_core::auth::AuditConfig {
                enabled: false,
                log_dir: ctx.path().join("audit"),
                retention_days: 1,
                compress_logs: false,
            })
            .expect("audit init"),
        );
        let auth_manager = Arc::new(nexus_core::auth::AuthManager::new(
            nexus_core::auth::AuthConfig::default(),
        ));
        let jwt_manager = Arc::new(nexus_core::auth::JwtManager::new(
            nexus_core::auth::JwtConfig::default(),
        ));

        let server = Arc::new(crate::NexusServer::new(
            executor_arc,
            engine_arc,
            dbm_arc,
            rbac_arc,
            auth_manager,
            jwt_manager,
            audit_logger,
            crate::config::RootUserConfig::default(),
        ));

        // Keep the temp directory alive for the whole test-process lifetime;
        // on-disk files need to stay valid while the Engine/AuditLogger hold
        // references to them. Leaking is the simplest way since tests run in
        // a fresh process every time.
        let _leaked = Box::leak(Box::new(ctx));

        RpcSession {
            server,
            authenticated: Arc::new(AtomicBool::new(false)),
            auth_required,
            connection_id: 1,
        }
    }

    #[tokio::test]
    async fn ping_no_arg_returns_pong() {
        let s = session(false);
        let out = run(&s, "PING", &[]).await.unwrap();
        assert_eq!(out, NexusValue::Str("PONG".into()));
    }

    #[tokio::test]
    async fn ping_echoes_string_payload() {
        let s = session(false);
        let out = run(&s, "PING", &[NexusValue::Str("hi".into())])
            .await
            .unwrap();
        assert_eq!(out, NexusValue::Str("hi".into()));
    }

    #[tokio::test]
    async fn ping_echoes_bytes_payload() {
        let s = session(false);
        let out = run(&s, "PING", &[NexusValue::Bytes(vec![1, 2, 3])])
            .await
            .unwrap();
        assert_eq!(out, NexusValue::Bytes(vec![1, 2, 3]));
    }

    #[tokio::test]
    async fn ping_rejects_non_string_non_bytes() {
        let s = session(false);
        let err = run(&s, "PING", &[NexusValue::Int(1)]).await.unwrap_err();
        assert!(err.contains("must be a string or bytes"));
    }

    #[tokio::test]
    async fn ping_rejects_too_many_args() {
        let s = session(false);
        let err = run(
            &s,
            "PING",
            &[NexusValue::Str("a".into()), NexusValue::Str("b".into())],
        )
        .await
        .unwrap_err();
        assert!(err.contains("wrong number of arguments"));
    }

    #[tokio::test]
    async fn hello_returns_map_with_expected_keys() {
        let s = session(false);
        let out = run(&s, "HELLO", &[]).await.unwrap();
        match out {
            NexusValue::Map(entries) => {
                let lookup = |name: &str| -> Option<NexusValue> {
                    entries.iter().find_map(|(k, v)| {
                        if k.as_str() == Some(name) {
                            Some(v.clone())
                        } else {
                            None
                        }
                    })
                };
                assert_eq!(
                    lookup("server").and_then(|v| v.as_str().map(String::from)),
                    Some("nexus".to_string())
                );
                assert_eq!(
                    lookup("proto").and_then(|v| v.as_int()),
                    Some(RPC_PROTO_VERSION)
                );
                assert_eq!(lookup("id").and_then(|v| v.as_int()), Some(1));
                // `version` is set from CARGO_PKG_VERSION — just assert it exists.
                assert!(matches!(lookup("version"), Some(NexusValue::Str(_))));
                assert_eq!(lookup("authenticated"), Some(NexusValue::Bool(false)));
            }
            other => panic!("expected Map, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn hello_reflects_authenticated_flag() {
        let s = session(false);
        s.mark_authenticated();
        let out = run(&s, "HELLO", &[]).await.unwrap();
        match out {
            NexusValue::Map(entries) => {
                let flag = entries.iter().find_map(|(k, v)| {
                    if k.as_str() == Some("authenticated") {
                        Some(v.clone())
                    } else {
                        None
                    }
                });
                assert_eq!(flag, Some(NexusValue::Bool(true)));
            }
            other => panic!("expected Map, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn auth_with_wrong_api_key_returns_wrongpass() {
        let s = session(false);
        let err = run(&s, "AUTH", &[NexusValue::Str("nx_invalid".into())])
            .await
            .unwrap_err();
        assert!(err.starts_with("WRONGPASS"));
        assert!(!s.is_authenticated());
    }

    #[tokio::test]
    async fn auth_with_root_credentials_succeeds() {
        let s = session(false);
        // Default RootUserConfig: username "root", password "root".
        let out = run(
            &s,
            "AUTH",
            &[
                NexusValue::Str("root".into()),
                NexusValue::Str("root".into()),
            ],
        )
        .await
        .unwrap();
        assert_eq!(out, NexusValue::Str("OK".into()));
        assert!(s.is_authenticated());
    }

    #[tokio::test]
    async fn auth_with_wrong_user_password_returns_wrongpass() {
        let s = session(false);
        let err = run(
            &s,
            "AUTH",
            &[
                NexusValue::Str("root".into()),
                NexusValue::Str("not-the-password".into()),
            ],
        )
        .await
        .unwrap_err();
        assert!(err.starts_with("WRONGPASS"));
        assert!(!s.is_authenticated());
    }

    #[tokio::test]
    async fn auth_with_no_args_rejected() {
        let s = session(false);
        let err = run(&s, "AUTH", &[]).await.unwrap_err();
        assert!(err.contains("wrong number of arguments"));
    }

    #[tokio::test]
    async fn auth_with_too_many_args_rejected() {
        let s = session(false);
        let err = run(
            &s,
            "AUTH",
            &[
                NexusValue::Str("a".into()),
                NexusValue::Str("b".into()),
                NexusValue::Str("c".into()),
            ],
        )
        .await
        .unwrap_err();
        assert!(err.contains("wrong number of arguments"));
    }

    #[tokio::test]
    async fn quit_returns_ok() {
        let s = session(false);
        let out = run(&s, "QUIT", &[]).await.unwrap();
        assert_eq!(out, NexusValue::Str("OK".into()));
    }

    #[tokio::test]
    async fn quit_rejects_args() {
        let s = session(false);
        let err = run(&s, "QUIT", &[NexusValue::Str("a".into())])
            .await
            .unwrap_err();
        assert!(err.contains("wrong number of arguments"));
    }

    #[tokio::test]
    async fn dispatch_routes_ping_through_run() {
        use super::super::run as top_run;
        let s = session(false);
        let out = top_run(&s, "ping", vec![]).await.unwrap();
        assert_eq!(out, NexusValue::Str("PONG".into()));
    }

    #[tokio::test]
    async fn dispatch_rejects_unknown_command() {
        use super::super::run as top_run;
        let s = session(false);
        let err = top_run(&s, "BOGUS", vec![]).await.unwrap_err();
        assert!(err.contains("unknown command 'BOGUS'"));
    }

    #[tokio::test]
    async fn dispatch_blocks_unknown_command_when_auth_required() {
        use super::super::run as top_run;
        let s = session(true);
        let err = top_run(&s, "CYPHER", vec![]).await.unwrap_err();
        assert!(err.starts_with("NOAUTH"));
    }

    #[tokio::test]
    async fn dispatch_allows_pre_auth_commands_when_auth_required() {
        use super::super::run as top_run;
        let s = session(true);
        let out = top_run(&s, "PING", vec![]).await.unwrap();
        assert_eq!(out, NexusValue::Str("PONG".into()));
    }

    #[tokio::test]
    async fn dispatch_allows_anything_after_auth_even_when_required() {
        use super::super::run as top_run;
        let s = session(true);
        s.mark_authenticated();
        // After auth, CYPHER reaches its real handler; with no args it
        // fails on wrong-arity rather than being blocked by NOAUTH.
        let err = top_run(&s, "CYPHER", vec![]).await.unwrap_err();
        assert!(!err.contains("NOAUTH"));
        assert!(err.contains("wrong number of arguments"));
    }

    #[tokio::test]
    async fn auth_stores_ordering_visible_to_is_authenticated() {
        let s = session(false);
        // A successful AUTH must be observable by the read loop on the
        // very next frame; check the flag is readable with Relaxed.
        let out = run(
            &s,
            "AUTH",
            &[
                NexusValue::Str("root".into()),
                NexusValue::Str("root".into()),
            ],
        )
        .await
        .unwrap();
        assert_eq!(out, NexusValue::Str("OK".into()));
        assert!(s.authenticated.load(Ordering::Relaxed));
    }
}
