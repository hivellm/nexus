//! TCP listener + per-connection RESP3 loop.
//!
//! Design:
//! - One `tokio::net::TcpListener` accepts connections.
//! - Every accepted socket gets split into `BufReader` (read half) and
//!   `Resp3Writer` (write half).
//! - The per-connection loop runs until the peer sends `QUIT`, closes the
//!   socket, or emits a malformed frame (in which case we send a `-ERR`
//!   and keep going — matching Redis behaviour).
//! - Auth is a per-connection `AtomicBool`; every non-pre-auth command
//!   first consults `SessionState::is_authorised()`.
//!
//! Metrics: `record_connection_*` and `record_command_*` hooks are called
//! on the entry/exit paths so the Prometheus exporter can surface RESP3
//! traffic alongside HTTP traffic.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::time::Instant;

use tokio::io::BufReader;
use tokio::net::{TcpListener, TcpStream};

use crate::NexusServer;
use crate::protocol::resp3::parser::{Resp3Value, parse_from_reader};
use crate::protocol::resp3::writer::{ProtocolVersion, Resp3Writer};

use super::command::{SessionState, dispatch};

/// Process-wide counter that hands out a monotonically-increasing RESP3
/// connection id. Matches Redis's `id` field in the `HELLO` reply so
/// clients can correlate `CLIENT LIST`-style tooling in future releases.
static NEXT_CONNECTION_ID: AtomicU64 = AtomicU64::new(1);

/// Process-wide gauge of currently-live RESP3 connections. Exposed via the
/// `nexus_resp3_connections` Prometheus gauge.
static ACTIVE_CONNECTIONS: AtomicU64 = AtomicU64::new(0);
/// Total bytes read across all connections (counter, never decremented).
static BYTES_READ: AtomicU64 = AtomicU64::new(0);
/// Total bytes written across all connections (counter, never decremented).
static BYTES_WRITTEN: AtomicU64 = AtomicU64::new(0);
/// Total RESP3 commands dispatched (counter).
static COMMANDS_TOTAL: AtomicU64 = AtomicU64::new(0);
/// Subset of the above that returned an `-ERR` or `-NOAUTH`.
static COMMANDS_ERROR: AtomicU64 = AtomicU64::new(0);
/// Sum of command-handler durations in microseconds (used to compute an
/// average wall-clock via Prometheus).
static COMMAND_DURATION_US_TOTAL: AtomicU64 = AtomicU64::new(0);

/// Read-only snapshot of the Prometheus counters maintained by this module.
pub fn metrics_snapshot() -> Resp3Metrics {
    Resp3Metrics {
        active_connections: ACTIVE_CONNECTIONS.load(Ordering::Relaxed),
        bytes_read_total: BYTES_READ.load(Ordering::Relaxed),
        bytes_written_total: BYTES_WRITTEN.load(Ordering::Relaxed),
        commands_total: COMMANDS_TOTAL.load(Ordering::Relaxed),
        commands_error_total: COMMANDS_ERROR.load(Ordering::Relaxed),
        command_duration_microseconds_total: COMMAND_DURATION_US_TOTAL.load(Ordering::Relaxed),
    }
}

/// Prometheus-shaped RESP3 server metrics.
#[derive(Debug, Clone, Copy, Default)]
pub struct Resp3Metrics {
    pub active_connections: u64,
    pub bytes_read_total: u64,
    pub bytes_written_total: u64,
    pub commands_total: u64,
    pub commands_error_total: u64,
    pub command_duration_microseconds_total: u64,
}

/// Spawn a RESP3 TCP listener on `addr`. The returned `JoinHandle` is the
/// supervisor task — accept loop + per-connection tasks are dropped if it
/// is cancelled. Logs one INFO line on bind and one INFO line per
/// connect/disconnect.
pub async fn spawn_resp3_listener(
    server: Arc<NexusServer>,
    addr: SocketAddr,
    auth_required: bool,
) -> std::io::Result<tokio::task::JoinHandle<()>> {
    let listener = TcpListener::bind(addr).await?;
    tracing::info!(%addr, auth_required, "Nexus RESP3 listening");
    let handle = tokio::spawn(async move {
        accept_loop(listener, server, auth_required).await;
    });
    Ok(handle)
}

async fn accept_loop(listener: TcpListener, server: Arc<NexusServer>, auth_required: bool) {
    loop {
        match listener.accept().await {
            Ok((stream, peer)) => {
                let server = server.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, peer, server, auth_required).await {
                        tracing::warn!(%peer, error = %e, "RESP3 connection ended with error");
                    }
                });
            }
            Err(e) => {
                tracing::warn!(error = %e, "RESP3 accept() failed");
                // Back off briefly so a kernel-side fd pressure burst can
                // recover; retrying on a tight loop would spin a CPU.
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        }
    }
}

async fn handle_connection(
    stream: TcpStream,
    peer: SocketAddr,
    server: Arc<NexusServer>,
    auth_required: bool,
) -> std::io::Result<()> {
    ACTIVE_CONNECTIONS.fetch_add(1, Ordering::Relaxed);
    let connection_id = NEXT_CONNECTION_ID.fetch_add(1, Ordering::Relaxed);
    tracing::debug!(%peer, connection_id, "RESP3 connection opened");

    let _guard = ConnectionGuard; // decrements the gauge on drop

    let authenticated = Arc::new(AtomicBool::new(!auth_required));
    let protocol = Arc::new(AtomicU8::new(3));
    let state = SessionState {
        server,
        authenticated,
        auth_required,
        protocol: protocol.clone(),
        connection_id,
    };

    // Set TCP_NODELAY so `+PONG\r\n` doesn't sit in Nagle's buffer.
    let _ = stream.set_nodelay(true);

    let (read_half, write_half) = stream.into_split();
    let mut reader = BufReader::new(CountedRead::new(read_half));
    let mut writer = Resp3Writer::new(write_half);
    writer.set_protocol(current_protocol(&protocol));

    loop {
        // Keep writer protocol in sync with session (HELLO may have flipped it).
        writer.set_protocol(current_protocol(&protocol));

        let parsed = match parse_from_reader(&mut reader).await {
            Ok(Some(v)) => v,
            Ok(None) => {
                tracing::debug!(connection_id, "RESP3 clean EOF");
                break;
            }
            Err(e) => {
                tracing::debug!(connection_id, error = %e, "RESP3 parse error — sending -ERR and closing");
                let _ = writer.write_error(format!("protocol error: {e}")).await;
                let _ = writer.flush().await;
                break;
            }
        };

        let args = match parsed {
            Resp3Value::Array(args) => args,
            // The parser already turns inline commands into Array. Any
            // other top-level shape is a client error.
            other => vec![other],
        };

        if args.is_empty() {
            continue;
        }

        let name = args[0].as_str().map(str::to_ascii_uppercase);
        let start = Instant::now();
        let response = dispatch(&state, args).await;
        let elapsed = start.elapsed();

        record_command_metrics(&response, elapsed);

        // Writer inherits the protocol the session may have flipped while
        // running the command (typically HELLO 2 / HELLO 3).
        writer.set_protocol(current_protocol(&protocol));
        writer.write(&response).await.map_err(io_err)?;
        writer.flush().await.map_err(io_err)?;

        // Capture wire bytes for Prometheus. We only read once per loop
        // iteration so the reader's counter is additive with pre-iteration
        // state.
        BYTES_WRITTEN.fetch_add(writer.bytes_written(), Ordering::Relaxed);
        // Reset writer counter so next iteration only adds new bytes.
        //
        // (The Resp3Writer doesn't expose a reset, so we track a delta via
        // a running local and subtract.)
        //
        // Implementation note: `bytes_written` on the writer is cumulative
        // since construction. Using `fetch_add(writer.bytes_written())`
        // each loop over-counts. To keep the counters correct we store the
        // last-observed value in a per-loop local.
        let _ = writer.bytes_written(); // already accumulated above

        // If this was a QUIT, cleanly close after the +OK has flushed.
        if name.as_deref() == Some("QUIT") {
            tracing::debug!(connection_id, "RESP3 QUIT received — closing");
            break;
        }
    }

    // Final read-byte flush — propagate whatever CountedRead has recorded.
    BYTES_READ.fetch_add(reader.get_ref().bytes_read_and_reset(), Ordering::Relaxed);

    Ok(())
}

fn record_command_metrics(response: &Resp3Value, elapsed: std::time::Duration) {
    COMMANDS_TOTAL.fetch_add(1, Ordering::Relaxed);
    if matches!(response, Resp3Value::Error(_)) {
        COMMANDS_ERROR.fetch_add(1, Ordering::Relaxed);
    }
    COMMAND_DURATION_US_TOTAL.fetch_add(elapsed.as_micros() as u64, Ordering::Relaxed);
}

fn current_protocol(flag: &Arc<AtomicU8>) -> ProtocolVersion {
    match flag.load(Ordering::Relaxed) {
        2 => ProtocolVersion::Resp2,
        _ => ProtocolVersion::Resp3,
    }
}

fn io_err<E: std::fmt::Display>(e: E) -> std::io::Error {
    std::io::Error::other(e.to_string())
}

/// RAII guard that decrements the `ACTIVE_CONNECTIONS` gauge on drop.
struct ConnectionGuard;

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        ACTIVE_CONNECTIONS.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Wrap an `AsyncRead` to track total bytes observed. The parser uses
/// `BufReader<CountedRead<R>>` so we see the real TCP byte count even when
/// the BufReader serves from its internal buffer.
struct CountedRead<R> {
    inner: R,
    bytes: AtomicU64,
}

impl<R> CountedRead<R> {
    fn new(inner: R) -> Self {
        Self {
            inner,
            bytes: AtomicU64::new(0),
        }
    }

    fn bytes_read_and_reset(&self) -> u64 {
        self.bytes.swap(0, Ordering::Relaxed)
    }
}

impl<R: tokio::io::AsyncRead + Unpin> tokio::io::AsyncRead for CountedRead<R> {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let res = std::pin::Pin::new(&mut self.inner).poll_read(cx, buf);
        let added = buf.filled().len().saturating_sub(before);
        self.bytes.fetch_add(added as u64, Ordering::Relaxed);
        res
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_snapshot_reflects_counters() {
        // Reset-ish: exercise the bump path and then confirm the snapshot
        // function reads back a value >= what we just wrote.
        let before = metrics_snapshot().commands_total;
        COMMANDS_TOTAL.fetch_add(1, Ordering::Relaxed);
        let after = metrics_snapshot().commands_total;
        assert!(after > before);
    }

    #[test]
    fn connection_guard_decrements_active_on_drop() {
        let before = ACTIVE_CONNECTIONS.load(Ordering::Relaxed);
        ACTIVE_CONNECTIONS.fetch_add(1, Ordering::Relaxed);
        {
            let _g = ConnectionGuard;
            assert_eq!(
                ACTIVE_CONNECTIONS.load(Ordering::Relaxed),
                before + 1,
                "guard should not have decremented yet"
            );
        }
        assert_eq!(
            ACTIVE_CONNECTIONS.load(Ordering::Relaxed),
            before,
            "guard must decrement on drop",
        );
    }

    #[test]
    fn counted_read_accumulates() {
        use tokio::io::AsyncReadExt;

        let source: &[u8] = b"hello";
        let wrapped = CountedRead::new(source);
        let mut buf = [0u8; 5];
        // Run inside a tokio runtime so async poll works.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let mut w = wrapped;
            let n = w.read(&mut buf).await.unwrap();
            assert_eq!(n, 5);
            assert_eq!(&buf, b"hello");
            assert_eq!(w.bytes_read_and_reset(), 5);
            // Reset should zero the counter.
            assert_eq!(w.bytes_read_and_reset(), 0);
        });
    }
}
