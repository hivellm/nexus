//! Native binary RPC listener, backed by the shared Thunder transport.
//!
//! phase10 replaced the hand-rolled TCP accept loop / per-connection
//! writer task / framing with `thunder::server::spawn_listener`. Thunder
//! owns the wire (v1 == the historical Nexus wire), the pre-auth allowlist
//! (`PING`/`HELLO`/`AUTH`/`QUIT`), the frame-cap and connection bounds, and
//! the idle/slow timers. This module only supplies:
//!
//! - [`NexusDispatch`] — a [`thunder::server::Dispatch`] that authenticates
//!   against the existing auth manager and delegates every command to the
//!   unchanged [`super::dispatch`] tree.
//! - [`NexusMetrics`] — a [`thunder::server::MetricsObserver`] feeding the
//!   existing `nexus_rpc_*` Prometheus series.
//! - [`spawn_rpc_listener`] — binds the listener and returns its
//!   [`ListenerHandle`] (which `main` must hold for the process lifetime;
//!   dropping it shuts the listener down).

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use thunder::Value;
use thunder::server::{
    AuthError, Credentials, Dispatch, ListenerConfig, ListenerHandle, MetricsObserver, Principal,
    ServerInfo, Session, spawn_listener,
};

use super::config::nexus_thunder_config;
use super::dispatch::{self, RpcSession};
use super::metrics;
use crate::NexusServer;
use crate::config::RpcConfig;

/// A command dispatcher over the shared `NexusServer` state.
///
/// `type Identity = ()`: the RPC port does not carry a resolved principal
/// through the session — the command tree only ever reads
/// [`Session::is_authenticated`], which Thunder flips when
/// [`Self::authenticate`] succeeds.
struct NexusDispatch {
    server: Arc<NexusServer>,
    auth_required: bool,
}

impl Dispatch for NexusDispatch {
    type Identity = ();

    async fn dispatch(
        &self,
        session: &Session<()>,
        command: &str,
        args: Vec<Value>,
    ) -> Result<Value, String> {
        // Rebuild the per-request `RpcSession` the command tree expects.
        // Authentication is owned by the Thunder listener — it answers
        // pre-auth `PING`, drives `AUTH` via `authenticate`, and gates
        // `NOAUTH` before a command ever reaches here — so this flag is a
        // read-only mirror of the listener's session state (the tree's own
        // `NOAUTH` re-check is then belt-and-suspenders).
        let rpc = RpcSession {
            server: Arc::clone(&self.server),
            authenticated: Arc::new(AtomicBool::new(session.is_authenticated())),
            auth_required: self.auth_required,
            connection_id: session.connection_id(),
        };
        dispatch::run(&rpc, command, args).await
    }

    async fn authenticate(&self, creds: Credentials) -> Result<Principal<()>, AuthError> {
        let ok = match creds {
            // `AUTH <api_key>` and a `HELLO`-carried token both resolve to
            // the API-key check (Nexus has no separate bearer-token store).
            Credentials::ApiKey(key) | Credentials::Token(key) => {
                dispatch::admin::verify_api_key(&self.server, &key)
            }
            Credentials::UserPass(user, pass) => {
                dispatch::admin::verify_user_password(&self.server, &user, &pass).await
            }
            Credentials::None => false,
        };
        if ok {
            Ok(Principal::new("rpc".to_string()))
        } else {
            Err(AuthError::InvalidCredentials)
        }
    }
}

/// Feeds Thunder's post-write metric callbacks into the existing
/// `nexus_rpc_*` counters (see [`super::metrics`]).
struct NexusMetrics {
    slow_threshold: Duration,
}

impl MetricsObserver for NexusMetrics {
    fn command_completed(
        &self,
        command: &str,
        in_bytes: usize,
        out_bytes: usize,
        duration: Duration,
        is_error: bool,
    ) {
        metrics::record_rpc_command(command, !is_error, duration.as_secs_f64());
        metrics::record_rpc_frame_sizes(in_bytes, out_bytes);
        if duration > self.slow_threshold {
            metrics::record_rpc_slow_command();
            tracing::warn!(
                cmd = %command,
                elapsed_ms = duration.as_secs_f64() * 1_000.0,
                threshold_ms = self.slow_threshold.as_secs_f64() * 1_000.0,
                "RPC slow command"
            );
        }
    }

    fn connection_opened(&self) {
        metrics::rpc_connection_open();
    }

    fn connection_closed(&self) {
        metrics::rpc_connection_close();
    }
}

/// Bind the native binary RPC listener on `addr` and return its handle.
///
/// The caller MUST keep the returned [`ListenerHandle`] alive for as long
/// as the listener should serve — dropping it triggers a graceful shutdown
/// (fire-and-forget). The wire profile is [`nexus_thunder_config`], with the
/// runtime frame cap and per-connection concurrency taken from
/// [`RpcConfig`] (env `NEXUS_RPC_MAX_FRAME_BYTES` / `NEXUS_RPC_MAX_IN_FLIGHT`
/// still override at that layer). `auth_required = false` opens the listener
/// (`ListenerConfig::open`), preserving `NEXUS_RPC_REQUIRE_AUTH` semantics.
pub async fn spawn_rpc_listener(
    server: Arc<NexusServer>,
    addr: SocketAddr,
    config: RpcConfig,
    auth_required: bool,
) -> std::io::Result<Arc<ListenerHandle>> {
    let dispatch = Arc::new(NexusDispatch {
        server,
        auth_required,
    });
    let slow_threshold = Duration::from_millis(config.slow_threshold_ms);
    let observer = Arc::new(NexusMetrics { slow_threshold });

    // The wire profile carries the frame cap and the in-flight bound; the
    // env-overridable runtime values from `RpcConfig` win over the pinned
    // defaults. (`max_in_flight_per_conn` maps onto Thunder's per-connection
    // bounded spawn-per-request.)
    let profile = nexus_thunder_config()
        .max_frame_bytes(config.max_frame_bytes)
        .max_in_flight(config.max_in_flight_per_conn);

    let mut listener_config = ListenerConfig::new(addr).with_observer(observer);
    listener_config.slow_threshold = slow_threshold;
    if !auth_required {
        listener_config = listener_config.open();
    }

    let info = ServerInfo {
        name: "nexus".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };

    let handle = spawn_listener(dispatch, profile, info, listener_config).await?;
    tracing::info!("Nexus RPC listening on {}", handle.local_addr());
    Ok(Arc::new(handle))
}
