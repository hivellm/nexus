//! Admin commands: PING, HELLO, AUTH, QUIT, HELP, COMMAND.
//!
//! These are always allowed pre-auth (see [`super::dispatch`]). Their job
//! is to get the connection into an authenticated state and advertise
//! server capabilities.

use std::sync::atomic::Ordering;

use crate::protocol::resp3::parser::Resp3Value;
use crate::protocol::resp3::writer::ProtocolVersion;

use super::{SessionState, err, expect_arity, expect_arity_min, expect_arity_range};

/// `PING` -> `+PONG`; `PING <msg>` echoes the message as a BulkString.
pub async fn ping(_state: &SessionState, args: &[Resp3Value]) -> Resp3Value {
    match args.len() {
        1 => Resp3Value::SimpleString("PONG".into()),
        2 => match args[1].as_bytes() {
            Some(b) => Resp3Value::BulkString(b.to_vec()),
            None => err("ERR PING argument must be a bulk string"),
        },
        _ => err("ERR wrong number of arguments for 'PING' command"),
    }
}

/// `HELLO [2|3] [AUTH <user> <pass>]` — negotiates the protocol version and
/// (optionally) authenticates the connection in the same round-trip.
///
/// On success returns a Map with server metadata:
///
/// ```text
/// %7
///   $6 server   $5 nexus
///   $7 version  $5 0.12.0
///   $5 proto    :3
///   $2 id       :<connection_id>
///   $4 mode     $10 standalone
///   $4 role     $6 master
///   $7 modules  *0
/// ```
pub async fn hello(state: &SessionState, args: &[Resp3Value]) -> Resp3Value {
    if let Some(e) = expect_arity_range(args, 1, 5, "HELLO") {
        return e;
    }

    // [HELLO] [protover] [AUTH username password]
    let mut protover: i64 = 3;
    let mut idx = 1;
    if let Some(v) = args.get(idx) {
        if let Some(n) = v.as_int() {
            if n != 2 && n != 3 {
                return err(format!(
                    "NOPROTO unsupported protocol version {n}; supported: 2, 3"
                ));
            }
            protover = n;
            idx += 1;
        }
    }

    // Optional AUTH clause.
    if let Some(maybe_auth) = args.get(idx)
        && maybe_auth.as_str().map(str::to_ascii_uppercase).as_deref() == Some("AUTH")
    {
        let username = match args.get(idx + 1).and_then(Resp3Value::as_str) {
            Some(s) => s,
            None => return err("ERR HELLO AUTH requires username and password"),
        };
        let password = match args.get(idx + 2).and_then(Resp3Value::as_str) {
            Some(s) => s,
            None => return err("ERR HELLO AUTH requires username and password"),
        };
        if !check_password_auth(state, username, password).await {
            return Resp3Value::Error("WRONGPASS invalid username-password pair".into());
        }
        state.authenticated.store(true, Ordering::Relaxed);
    }

    // Set protocol for the write side.
    state.set_protocol(match protover {
        2 => ProtocolVersion::Resp2,
        _ => ProtocolVersion::Resp3,
    });

    hello_reply(state, protover)
}

/// `AUTH <password>` or `AUTH <username> <password>`.
///
/// Single-arg form: treat the argument as an API key and verify it via the
/// `AuthManager`. Two-arg form: username + password lookup against the RBAC
/// store.
pub async fn auth(state: &SessionState, args: &[Resp3Value]) -> Resp3Value {
    match args.len() {
        2 => {
            let api_key = match args[1].as_str() {
                Some(s) => s,
                None => return err("ERR AUTH argument must be a string"),
            };
            if check_api_key_auth(state, api_key) {
                state.authenticated.store(true, Ordering::Relaxed);
                Resp3Value::SimpleString("OK".into())
            } else {
                Resp3Value::Error("WRONGPASS invalid API key".into())
            }
        }
        3 => {
            let username = args[1].as_str().unwrap_or("");
            let password = args[2].as_str().unwrap_or("");
            if check_password_auth(state, username, password).await {
                state.authenticated.store(true, Ordering::Relaxed);
                Resp3Value::SimpleString("OK".into())
            } else {
                Resp3Value::Error("WRONGPASS invalid username-password pair".into())
            }
        }
        _ => err("ERR wrong number of arguments for 'AUTH' command"),
    }
}

/// `QUIT` — acknowledges and the connection loop closes the socket after
/// it sees this command name.
pub async fn quit(_state: &SessionState, args: &[Resp3Value]) -> Resp3Value {
    if let Some(e) = expect_arity(args, 1, "QUIT") {
        return e;
    }
    Resp3Value::SimpleString("OK".into())
}

/// `HELP` returns a short array of human-readable command summaries.
/// Clients like `redis-cli` render this line-by-line.
pub async fn help(_state: &SessionState, args: &[Resp3Value]) -> Resp3Value {
    if let Some(e) = expect_arity(args, 1, "HELP") {
        return e;
    }
    let lines = [
        "Nexus RESP3 command reference (see docs/specs/resp3-nexus-commands.md).",
        "",
        "Admin:",
        "  PING [msg]                                 — health check.",
        "  HELLO [2|3] [AUTH user pass]               — negotiate protocol / auth.",
        "  AUTH <api-key> | AUTH <user> <pass>        — authenticate.",
        "  QUIT                                       — close the connection.",
        "  HELP                                       — this text.",
        "  COMMAND                                    — machine-readable command list.",
        "",
        "Cypher:",
        "  CYPHER <query>                             — run a query.",
        "  CYPHER.WITH <query> <params-json>          — run with parameters.",
        "  CYPHER.EXPLAIN <query>                     — planner output.",
        "",
        "Graph CRUD:",
        "  NODE.CREATE <labels-csv> <props-json>      — returns node id.",
        "  NODE.GET <id>                              — full node as Map.",
        "  NODE.UPDATE <id> <props-json>              — replace properties.",
        "  NODE.DELETE <id> [DETACH]                  — returns deleted count.",
        "  NODE.MATCH <label> <props-json> [LIMIT n]  — filtered node scan.",
        "  REL.CREATE <src> <dst> <type> <props-json> — returns rel id.",
        "  REL.GET <id>                               — full relationship.",
        "  REL.DELETE <id>                            — returns deleted count.",
        "",
        "KNN / ingest:",
        "  KNN.SEARCH <label> <vector> <k> [FILTER <json>]",
        "  KNN.TRAVERSE <seeds-csv> <depth> [FILTER <json>]",
        "  INGEST.NODES <ndjson-bulk>",
        "  INGEST.RELS <ndjson-bulk>",
        "",
        "Schema, indexes, databases:",
        "  INDEX.CREATE <label> <property> [UNIQUE]",
        "  INDEX.DROP <label> <property>",
        "  INDEX.LIST",
        "  DB.LIST | DB.CREATE <name> | DB.DROP <name> | DB.USE <name>",
        "  LABELS | REL_TYPES | PROPERTY_KEYS | STATS | HEALTH",
    ];
    Resp3Value::Array(
        lines
            .iter()
            .map(|l| Resp3Value::bulk(l.to_string()))
            .collect(),
    )
}

/// `COMMAND` — machine-readable list of supported commands. Each entry is
/// `[name, arity, flags]` where `arity` is a signed integer (positive ==
/// exact, negative == minimum) matching the Redis convention.
pub async fn command(_state: &SessionState, args: &[Resp3Value]) -> Resp3Value {
    if let Some(e) = expect_arity_min(args, 1, "COMMAND") {
        return e;
    }
    let commands: &[(&str, i64, &[&str])] = &[
        ("PING", -1, &["fast", "loading"]),
        ("HELLO", -1, &["fast", "no-auth"]),
        ("AUTH", -2, &["fast", "no-auth"]),
        ("QUIT", 1, &["fast", "no-auth"]),
        ("HELP", 1, &["fast"]),
        ("COMMAND", -1, &["fast"]),
        ("CYPHER", 2, &["readonly"]),
        ("CYPHER.WITH", 3, &["readonly"]),
        ("CYPHER.EXPLAIN", 2, &["readonly"]),
        ("NODE.CREATE", 3, &["write"]),
        ("NODE.GET", 2, &["readonly"]),
        ("NODE.UPDATE", 3, &["write"]),
        ("NODE.DELETE", -2, &["write"]),
        ("NODE.MATCH", -3, &["readonly"]),
        ("REL.CREATE", 5, &["write"]),
        ("REL.GET", 2, &["readonly"]),
        ("REL.DELETE", 2, &["write"]),
        ("KNN.SEARCH", -4, &["readonly"]),
        ("KNN.TRAVERSE", -3, &["readonly"]),
        ("INGEST.NODES", 2, &["write"]),
        ("INGEST.RELS", 2, &["write"]),
        ("INDEX.CREATE", -3, &["write"]),
        ("INDEX.DROP", 3, &["write"]),
        ("INDEX.LIST", 1, &["readonly"]),
        ("DB.LIST", 1, &["readonly"]),
        ("DB.CREATE", 2, &["write"]),
        ("DB.DROP", 2, &["write"]),
        ("DB.USE", 2, &["readonly"]),
        ("LABELS", 1, &["readonly"]),
        ("REL_TYPES", 1, &["readonly"]),
        ("PROPERTY_KEYS", 1, &["readonly"]),
        ("STATS", 1, &["readonly"]),
        ("HEALTH", 1, &["readonly"]),
    ];
    let entries: Vec<Resp3Value> = commands
        .iter()
        .map(|(name, arity, flags)| {
            Resp3Value::Array(vec![
                Resp3Value::bulk(*name),
                Resp3Value::Integer(*arity),
                Resp3Value::Array(flags.iter().map(|f| Resp3Value::bulk(*f)).collect()),
            ])
        })
        .collect();
    Resp3Value::Array(entries)
}

// --------------------------------------------------------------------------
// Internal helpers.
// --------------------------------------------------------------------------

fn hello_reply(state: &SessionState, protover: i64) -> Resp3Value {
    let entries: Vec<(Resp3Value, Resp3Value)> = vec![
        (Resp3Value::bulk("server"), Resp3Value::bulk("nexus")),
        (
            Resp3Value::bulk("version"),
            Resp3Value::bulk(env!("CARGO_PKG_VERSION")),
        ),
        (Resp3Value::bulk("proto"), Resp3Value::Integer(protover)),
        (
            Resp3Value::bulk("id"),
            Resp3Value::Integer(state.connection_id as i64),
        ),
        (Resp3Value::bulk("mode"), Resp3Value::bulk("standalone")),
        (Resp3Value::bulk("role"), Resp3Value::bulk("master")),
        (Resp3Value::bulk("modules"), Resp3Value::Array(vec![])),
    ];
    Resp3Value::Map(entries)
}

/// Look up the user by `username`, then verify the supplied password
/// matches the stored hash. Runs under the tokio RwLock on `rbac`.
async fn check_password_auth(state: &SessionState, username: &str, password: &str) -> bool {
    // Root fast path — if the configured root user is enabled, match first
    // against the plaintext config credential. This mirrors how the REST
    // login endpoint handles the root account, so a freshly-booted server
    // lets the operator get in over RESP3 before any other user exists.
    let root = &state.server.root_user_config;
    if root.enabled && username == root.username && password == root.password {
        return true;
    }

    let rbac = state.server.rbac.read().await;
    let user = match rbac
        .list_users()
        .into_iter()
        .find(|u| u.username == username)
    {
        Some(u) => u.clone(),
        None => return false,
    };
    if !user.is_active {
        return false;
    }
    match &user.password_hash {
        Some(hash) => nexus_core::auth::verify_password(password, hash),
        None => false,
    }
}

/// Single-argument AUTH verifies a Nexus API key (`nx_...`).
fn check_api_key_auth(state: &SessionState, api_key: &str) -> bool {
    matches!(
        state.server.auth_manager.verify_api_key(api_key),
        Ok(Some(_))
    )
}

// --------------------------------------------------------------------------
// Tests.
// --------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Build a minimal SessionState that doesn't touch any real server state.
    // Commands that actually need the server (CYPHER, etc.) get integration-
    // tested separately; admin commands only need the auth flags.
    fn empty_state() -> SessionState {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, AtomicU8};

        // Build a throwaway NexusServer using the same initialisation path
        // as main.rs's test suite. The admin tests only actually reach for
        // `rbac`, `auth_manager`, and `root_user_config`; everything else
        // just has to exist with a consistent type.
        let ctx = nexus_core::testing::TestContext::new();
        let engine = nexus_core::Engine::with_data_dir(ctx.path())
            .expect("engine init for resp3 admin test");
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

        // The TestContext must out-live the test (on-disk files need to
        // stay valid while the Engine/AuditLogger are alive). Leaking it
        // is the simplest way; tests run in a fresh process.
        let _leaked = Box::leak(Box::new(ctx));

        SessionState {
            server,
            authenticated: Arc::new(AtomicBool::new(false)),
            auth_required: false,
            protocol: Arc::new(AtomicU8::new(3)),
            connection_id: 1,
        }
    }

    #[tokio::test]
    async fn ping_no_arg_returns_pong() {
        let s = empty_state();
        let args = vec![Resp3Value::bulk("PING")];
        assert_eq!(
            ping(&s, &args).await,
            Resp3Value::SimpleString("PONG".into())
        );
    }

    #[tokio::test]
    async fn ping_echoes_payload() {
        let s = empty_state();
        let args = vec![Resp3Value::bulk("PING"), Resp3Value::bulk("hi")];
        assert_eq!(
            ping(&s, &args).await,
            Resp3Value::BulkString(b"hi".to_vec())
        );
    }

    #[tokio::test]
    async fn hello_default_returns_map_with_proto_3() {
        let s = empty_state();
        let args = vec![Resp3Value::bulk("HELLO")];
        let v = hello(&s, &args).await;
        match v {
            Resp3Value::Map(entries) => {
                let proto = entries
                    .iter()
                    .find_map(|(k, v)| match k.as_str() {
                        Some("proto") => v.as_int(),
                        _ => None,
                    })
                    .expect("proto key missing");
                assert_eq!(proto, 3);
            }
            other => panic!("expected Map, got {other:?}"),
        }
        assert_eq!(s.protocol(), ProtocolVersion::Resp3);
    }

    #[tokio::test]
    async fn hello_version_2_flips_protocol() {
        let s = empty_state();
        let args = vec![Resp3Value::bulk("HELLO"), Resp3Value::bulk("2")];
        let v = hello(&s, &args).await;
        assert!(matches!(v, Resp3Value::Map(_)));
        assert_eq!(s.protocol(), ProtocolVersion::Resp2);
    }

    #[tokio::test]
    async fn hello_noproto_for_unsupported_version() {
        let s = empty_state();
        let args = vec![Resp3Value::bulk("HELLO"), Resp3Value::bulk("4")];
        match hello(&s, &args).await {
            Resp3Value::Error(msg) => assert!(msg.contains("NOPROTO")),
            other => panic!("expected NOPROTO error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn auth_with_wrong_api_key_returns_wrongpass() {
        let s = empty_state();
        let args = vec![Resp3Value::bulk("AUTH"), Resp3Value::bulk("nx_invalid")];
        match auth(&s, &args).await {
            Resp3Value::Error(msg) => assert!(msg.starts_with("WRONGPASS")),
            other => panic!("expected WRONGPASS, got {other:?}"),
        }
        assert!(!s.authenticated.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn auth_with_root_user_password_succeeds() {
        let s = empty_state();
        // Default RootUserConfig: username "root", password "root".
        let args = vec![
            Resp3Value::bulk("AUTH"),
            Resp3Value::bulk("root"),
            Resp3Value::bulk("root"),
        ];
        match auth(&s, &args).await {
            Resp3Value::SimpleString(ref msg) if msg == "OK" => {}
            other => panic!("expected +OK, got {other:?}"),
        }
        assert!(s.authenticated.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn auth_with_wrong_arity_fails() {
        let s = empty_state();
        let args = vec![Resp3Value::bulk("AUTH")];
        match auth(&s, &args).await {
            Resp3Value::Error(msg) => assert!(msg.contains("wrong number of arguments")),
            other => panic!("expected ERR, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn quit_returns_ok() {
        let s = empty_state();
        let args = vec![Resp3Value::bulk("QUIT")];
        assert_eq!(quit(&s, &args).await, Resp3Value::SimpleString("OK".into()));
    }

    #[tokio::test]
    async fn help_returns_nonempty_array() {
        let s = empty_state();
        let args = vec![Resp3Value::bulk("HELP")];
        match help(&s, &args).await {
            Resp3Value::Array(v) => assert!(!v.is_empty()),
            other => panic!("expected Array, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn command_returns_list_of_triples() {
        let s = empty_state();
        let args = vec![Resp3Value::bulk("COMMAND")];
        match command(&s, &args).await {
            Resp3Value::Array(items) => {
                assert!(items.len() >= 30);
                match &items[0] {
                    Resp3Value::Array(spec) => {
                        assert_eq!(spec.len(), 3);
                        assert!(matches!(&spec[0], Resp3Value::BulkString(_)));
                        assert!(matches!(&spec[1], Resp3Value::Integer(_)));
                        assert!(matches!(&spec[2], Resp3Value::Array(_)));
                    }
                    other => panic!("expected inner Array, got {other:?}"),
                }
            }
            other => panic!("expected Array, got {other:?}"),
        }
    }
}
