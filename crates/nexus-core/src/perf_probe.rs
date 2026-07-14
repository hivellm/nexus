//! Temporary diagnostic counters for
//! `phase9_store-lock-read-concurrency` §1 (profile-before-structural-
//! change mandate). Each [`Probe`] tracks how many times a candidate
//! serialization point was hit and how much wall time was spent
//! *waiting to acquire* it (not holding it) — the direct signal for
//! "is 64 concurrent readers queuing here".
//!
//! Zero-cost when disabled: [`enabled`] caches a single env-var read
//! behind a `OnceLock`, and every call site branches on that cached
//! bool before touching a probe, so the disabled path is one relaxed
//! load + branch. Enable with `NEXUS_PERF_PROBE=1`.
//!
//! This module is intentionally temporary instrumentation for the §1
//! evidence-gathering pass — it is not wired into any public API and
//! carries no stability guarantee. Remove (or leave inert) once the
//! read-ceiling root cause is closed and documented.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static ENABLED: OnceLock<bool> = OnceLock::new();

/// True when `NEXUS_PERF_PROBE=1` was set at process start. Cached
/// after the first call so steady-state cost is a single relaxed
/// atomic-adjacent `OnceLock` load.
#[inline]
pub fn enabled() -> bool {
    *ENABLED.get_or_init(|| std::env::var("NEXUS_PERF_PROBE").as_deref() == Ok("1"))
}

/// One candidate serialization point: hit count + accumulated wait
/// time, both relaxed atomics (approximate under contention by
/// design — exact ordering does not matter for a coarse cost
/// breakdown, and `Ordering::SeqCst` would itself distort the very
/// contention we are trying to measure).
pub struct Probe {
    count: AtomicU64,
    wait_nanos: AtomicU64,
}

impl Probe {
    pub const fn new() -> Self {
        Self {
            count: AtomicU64::new(0),
            wait_nanos: AtomicU64::new(0),
        }
    }

    /// Record one occurrence that waited `wait` before proceeding.
    #[inline]
    pub fn record(&self, wait: Duration) {
        self.count.fetch_add(1, Ordering::Relaxed);
        self.wait_nanos
            .fetch_add(wait.as_nanos() as u64, Ordering::Relaxed);
    }

    /// `(count, total_wait_nanos)` since the last [`Self::reset`].
    pub fn snapshot(&self) -> (u64, u64) {
        (
            self.count.load(Ordering::Relaxed),
            self.wait_nanos.load(Ordering::Relaxed),
        )
    }

    pub fn reset(&self) {
        self.count.store(0, Ordering::Relaxed);
        self.wait_nanos.store(0, Ordering::Relaxed);
    }
}

impl Default for Probe {
    fn default() -> Self {
        Self::new()
    }
}

/// Wraps `f` (expected to acquire some lock/resource) with a
/// wait-time measurement recorded into `probe`, but only when
/// [`enabled`] — the `Instant::now()` calls themselves are not free,
/// so skip them entirely on the hot path when profiling is off.
#[inline]
pub fn timed<T>(probe: &Probe, f: impl FnOnce() -> T) -> T {
    if !enabled() {
        return f();
    }
    let start = Instant::now();
    let out = f();
    probe.record(start.elapsed());
    out
}

/// `ExecutorShared.store` `parking_lot::RwLock` read-guard acquisition
/// wait time (`Executor::store()`).
pub static STORE_READ: Probe = Probe::new();
/// Same lock's write-guard acquisition (`Executor::store_mut()`).
pub static STORE_WRITE: Probe = Probe::new();
/// `ExecutorShared.label_index` `parking_lot::RwLock` read-guard
/// acquisition (`Executor::label_index()`).
pub static LABEL_INDEX_READ: Probe = Probe::new();
/// `SessionManager::get_session` — takes an exclusive `.write()` on
/// the global sessions map for what is logically a read (expiry
/// sweep + defensive clone). Hit once per autocommit query on both
/// the HTTP and RPC dispatch paths.
pub static SESSION_GET: Probe = Probe::new();
/// The server-side `Arc<tokio::sync::RwLock<Engine>>` read-guard
/// acquired by the RPC/HTTP read-only dispatch branch just to clone
/// the lock-free `Executor` and check transaction state.
pub static ENGINE_TOKIO_READ: Probe = Probe::new();
/// Wall time between scheduling a `tokio::task::spawn_blocking`
/// closure and the closure's first instruction running — i.e. how
/// long the blocking-thread-pool queue made the task wait.
pub static SPAWN_BLOCKING_QUEUE: Probe = Probe::new();
/// Total wall time inside `Executor::execute` (includes the query's
/// own re-parse — see `dispatch/cypher.rs`'s read-only branch, which
/// hands `Executor::execute` the raw Cypher string rather than the
/// AST the caller already parsed for routing).
pub static EXECUTOR_EXECUTE: Probe = Probe::new();

/// Reset every probe. Call at the start of a measurement window so a
/// warmup period does not pollute the numbers.
pub fn reset_all() {
    STORE_READ.reset();
    STORE_WRITE.reset();
    LABEL_INDEX_READ.reset();
    SESSION_GET.reset();
    ENGINE_TOKIO_READ.reset();
    SPAWN_BLOCKING_QUEUE.reset();
    EXECUTOR_EXECUTE.reset();
}

/// Render a one-line-per-probe snapshot for log output.
pub fn render_snapshot() -> String {
    let rows = [
        ("store_read", STORE_READ.snapshot()),
        ("store_write", STORE_WRITE.snapshot()),
        ("label_index_read", LABEL_INDEX_READ.snapshot()),
        ("session_get", SESSION_GET.snapshot()),
        ("engine_tokio_read", ENGINE_TOKIO_READ.snapshot()),
        ("spawn_blocking_queue", SPAWN_BLOCKING_QUEUE.snapshot()),
        ("executor_execute", EXECUTOR_EXECUTE.snapshot()),
    ];
    let mut out = String::from("PERF_PROBE ");
    for (name, (count, wait_nanos)) in rows {
        let avg_us = if count > 0 {
            (wait_nanos as f64 / count as f64) / 1000.0
        } else {
            0.0
        };
        out.push_str(&format!(
            "{name}[n={count} total_ms={:.3} avg_us={avg_us:.3}] ",
            wait_nanos as f64 / 1_000_000.0
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_records_count_and_wait() {
        let p = Probe::new();
        p.record(Duration::from_micros(10));
        p.record(Duration::from_micros(20));
        let (count, wait_nanos) = p.snapshot();
        assert_eq!(count, 2);
        assert_eq!(wait_nanos, 30_000);
        p.reset();
        assert_eq!(p.snapshot(), (0, 0));
    }

    #[test]
    fn timed_skips_instant_calls_when_disabled() {
        // enabled() is process-global/cached; this test only asserts
        // that `timed` still returns the closure's value regardless
        // of the enabled flag (the skip-vs-measure branch is covered
        // by the module's own doc contract, not independently
        // observable from a unit test without env-var control over a
        // OnceLock that other tests in this binary may have already
        // initialized).
        let probe = Probe::new();
        let out = timed(&probe, || 42);
        assert_eq!(out, 42);
    }
}
