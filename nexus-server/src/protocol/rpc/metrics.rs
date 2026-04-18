//! Process-wide RPC metrics counters.
//!
//! Plain `AtomicU64` counters — no Prometheus registry needed inside the
//! hot path. [`crate::api::prometheus`] calls [`snapshot`] once per
//! scrape to render the current values into the `/metrics` response.

use std::sync::atomic::{AtomicU64, Ordering};

static CONNECTIONS_OPEN: AtomicU64 = AtomicU64::new(0);
static COMMANDS_TOTAL: AtomicU64 = AtomicU64::new(0);
static COMMANDS_ERROR_TOTAL: AtomicU64 = AtomicU64::new(0);
static COMMAND_DURATION_US_TOTAL: AtomicU64 = AtomicU64::new(0);
static FRAME_BYTES_IN_TOTAL: AtomicU64 = AtomicU64::new(0);
static FRAME_BYTES_OUT_TOTAL: AtomicU64 = AtomicU64::new(0);
static SLOW_COMMANDS_TOTAL: AtomicU64 = AtomicU64::new(0);

/// Bump the live-connection gauge on accept.
pub fn rpc_connection_open() {
    CONNECTIONS_OPEN.fetch_add(1, Ordering::Relaxed);
}

/// Decrement the live-connection gauge on close.
pub fn rpc_connection_close() {
    CONNECTIONS_OPEN.fetch_sub(1, Ordering::Relaxed);
}

/// Record one completed RPC command: increment the total counter, the
/// error counter if `ok == false`, and add the duration to the histogram-
/// friendly microseconds accumulator.
pub fn record_rpc_command(_command: &str, ok: bool, elapsed_secs: f64) {
    COMMANDS_TOTAL.fetch_add(1, Ordering::Relaxed);
    if !ok {
        COMMANDS_ERROR_TOTAL.fetch_add(1, Ordering::Relaxed);
    }
    let us = (elapsed_secs * 1_000_000.0) as u64;
    COMMAND_DURATION_US_TOTAL.fetch_add(us, Ordering::Relaxed);
}

/// Record the incoming/outgoing frame byte counts per request.
pub fn record_rpc_frame_sizes(in_bytes: usize, out_bytes: usize) {
    FRAME_BYTES_IN_TOTAL.fetch_add(in_bytes as u64, Ordering::Relaxed);
    FRAME_BYTES_OUT_TOTAL.fetch_add(out_bytes as u64, Ordering::Relaxed);
}

/// Bump the slow-command counter whenever a handler exceeds the
/// configured threshold.
pub fn record_rpc_slow_command() {
    SLOW_COMMANDS_TOTAL.fetch_add(1, Ordering::Relaxed);
}

/// Atomic snapshot of the counters at scrape time.
#[derive(Debug, Clone, Copy, Default)]
pub struct RpcMetricsSnapshot {
    pub active_connections: u64,
    pub commands_total: u64,
    pub commands_error_total: u64,
    pub command_duration_us_total: u64,
    pub bytes_in_total: u64,
    pub bytes_out_total: u64,
    pub slow_commands_total: u64,
}

/// Read every counter once (in load-order, not an atomic-group snapshot —
/// numbers may drift by at most one request between reads, which is fine
/// for per-scrape Prometheus display).
pub fn snapshot() -> RpcMetricsSnapshot {
    RpcMetricsSnapshot {
        active_connections: CONNECTIONS_OPEN.load(Ordering::Relaxed),
        commands_total: COMMANDS_TOTAL.load(Ordering::Relaxed),
        commands_error_total: COMMANDS_ERROR_TOTAL.load(Ordering::Relaxed),
        command_duration_us_total: COMMAND_DURATION_US_TOTAL.load(Ordering::Relaxed),
        bytes_in_total: FRAME_BYTES_IN_TOTAL.load(Ordering::Relaxed),
        bytes_out_total: FRAME_BYTES_OUT_TOTAL.load(Ordering::Relaxed),
        slow_commands_total: SLOW_COMMANDS_TOTAL.load(Ordering::Relaxed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_reflects_counter_updates() {
        let before = snapshot();
        record_rpc_command("PING", true, 0.001);
        record_rpc_command("CYPHER", false, 0.0005);
        record_rpc_frame_sizes(128, 256);
        let after = snapshot();

        assert!(after.commands_total > before.commands_total + 1);
        assert!(after.commands_error_total > before.commands_error_total);
        assert!(after.command_duration_us_total > before.command_duration_us_total);
        assert!(after.bytes_in_total >= before.bytes_in_total + 128);
        assert!(after.bytes_out_total >= before.bytes_out_total + 256);
    }

    #[test]
    fn connection_open_close_balance() {
        let before = snapshot().active_connections;
        rpc_connection_open();
        rpc_connection_open();
        let mid = snapshot().active_connections;
        assert_eq!(mid, before + 2);
        rpc_connection_close();
        rpc_connection_close();
        let after = snapshot().active_connections;
        assert_eq!(after, before);
    }
}
