//! Asynchronous WAL Writer
//!
//! This module provides an asynchronous WAL writer that batches and flushes WAL entries
//! in the background to improve write performance while maintaining durability guarantees.
//!
//! The async writer uses:
//! - Channel-based communication between main thread and WAL writer thread
//! - Batching of WAL entries with configurable batch size and timeout
//! - Background fsync with configurable intervals
//! - Graceful shutdown handling

use crate::error::Result;
use crate::wal::WalEntry;
use crossbeam_channel::{Receiver, Sender};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

mod writer;

#[cfg(test)]
mod tests;

/// Commands sent to the WAL writer thread
#[derive(Debug)]
enum WalCommand {
    /// Append a WAL entry
    Append(WalEntry),
    /// Force flush all pending entries. Carries a completion handshake
    /// (see `phase0_fix-async-wal-flush-durability` §2.1): a fresh,
    /// single-use `std::sync::mpsc::Sender` that `writer_thread` signals
    /// with `flush_batch`'s real `Result` once the flush this command
    /// requested has actually executed. `AsyncWalWriter::flush()` blocks
    /// on the paired receiver so the barrier it documents is real.
    Flush(mpsc::Sender<Result<()>>),
    /// Shutdown the writer thread
    Shutdown,
}

/// Thread-safe statistics for the async WAL writer.
///
/// Every counter is an `AtomicU64`; writes use `fetch_add` / `store`,
/// reads use `load(Ordering::Relaxed)`. [`AsyncWalStats::snapshot`]
/// returns an owned [`AsyncWalStatsSnapshot`] (the plain-data view
/// consumers actually want to inspect).
///
/// This replaces the pre-v1.0 pattern of casting `Arc<AsyncWalStats>`
/// through a `*mut` (`unsafe { &mut *(Arc::as_ptr(&self.stats) as
/// *mut AsyncWalStats) }`), which aliased `&mut` across threads and
/// was a data race under the Rust memory model. The fields here are
/// exposed as atomics so `Arc` sharing is sound.
#[derive(Debug, Default)]
pub struct AsyncWalStats {
    /// Total entries submitted to writer
    pub entries_submitted: std::sync::atomic::AtomicU64,
    /// Total entries actually written
    pub entries_written: std::sync::atomic::AtomicU64,
    /// Total batches flushed
    pub batches_flushed: std::sync::atomic::AtomicU64,
    /// Total force flushes requested
    pub force_flushes: std::sync::atomic::AtomicU64,
    /// Total write latency (in microseconds)
    pub total_write_latency_us: std::sync::atomic::AtomicU64,
    /// Total flush latency (in microseconds)
    pub total_flush_latency_us: std::sync::atomic::AtomicU64,
    /// Number of batches that timed out (vs size-based)
    pub timeout_batches: std::sync::atomic::AtomicU64,
    /// Number of batches that hit max size (vs timeout-based)
    pub size_batches: std::sync::atomic::AtomicU64,
    /// Current queue depth
    pub current_queue_depth: std::sync::atomic::AtomicU64,
    /// Max queue depth seen
    pub max_queue_depth: std::sync::atomic::AtomicU64,
    /// Total WAL I/O errors encountered
    pub wal_errors: std::sync::atomic::AtomicU64,
    /// Number of `append` calls that had to block on a full channel (#19).
    pub backpressure_blocks: std::sync::atomic::AtomicU64,
    /// Number of times the WAL was checkpoint-truncated for size
    /// (phase0_fix-wal-checkpoint-truncate-production).
    pub wal_checkpoints: std::sync::atomic::AtomicU64,
}

impl AsyncWalStats {
    /// Load every counter under `Ordering::Relaxed` into an owned
    /// plain-data snapshot. Consumers should read via this method
    /// rather than poking the atomics directly.
    pub fn snapshot(&self) -> AsyncWalStatsSnapshot {
        use std::sync::atomic::Ordering::Relaxed;
        AsyncWalStatsSnapshot {
            entries_submitted: self.entries_submitted.load(Relaxed),
            entries_written: self.entries_written.load(Relaxed),
            batches_flushed: self.batches_flushed.load(Relaxed),
            force_flushes: self.force_flushes.load(Relaxed),
            total_write_latency_us: self.total_write_latency_us.load(Relaxed),
            total_flush_latency_us: self.total_flush_latency_us.load(Relaxed),
            timeout_batches: self.timeout_batches.load(Relaxed),
            size_batches: self.size_batches.load(Relaxed),
            current_queue_depth: self.current_queue_depth.load(Relaxed),
            max_queue_depth: self.max_queue_depth.load(Relaxed),
            wal_errors: self.wal_errors.load(Relaxed),
            backpressure_blocks: self.backpressure_blocks.load(Relaxed),
            wal_checkpoints: self.wal_checkpoints.load(Relaxed),
        }
    }
}

/// Plain-data snapshot of [`AsyncWalStats`], safe to clone and expose
/// through public APIs. Values are consistent per-field but the
/// snapshot as a whole is not atomic across all counters.
#[derive(Debug, Clone, Default)]
pub struct AsyncWalStatsSnapshot {
    pub entries_submitted: u64,
    pub entries_written: u64,
    pub batches_flushed: u64,
    pub force_flushes: u64,
    pub total_write_latency_us: u64,
    pub total_flush_latency_us: u64,
    pub timeout_batches: u64,
    pub size_batches: u64,
    pub current_queue_depth: u64,
    pub max_queue_depth: u64,
    pub wal_errors: u64,
    pub backpressure_blocks: u64,
    pub wal_checkpoints: u64,
}

/// Default WAL size (bytes) at which the async writer compacts the log:
/// 64 MiB, far under `Wal::health_check`'s 1 GiB hard gate.
pub const DEFAULT_WAL_CHECKPOINT_SIZE_BYTES: u64 = 64 * 1024 * 1024;

/// Configuration for the async WAL writer
#[derive(Debug, Clone)]
pub struct AsyncWalConfig {
    /// Maximum batch size (number of entries)
    pub max_batch_size: usize,
    /// Maximum batch age before flush
    pub max_batch_age: Duration,
    /// Maximum queue depth before blocking
    pub max_queue_depth: usize,
    /// Flush interval for background fsync
    pub flush_interval: Duration,
    /// Channel buffer size
    pub channel_buffer_size: usize,
    /// WAL size (bytes) at which the writer thread checkpoint-truncates the WAL
    /// after a successful flush, keeping it bounded well under
    /// `health_check`'s 1 GiB gate. The WAL is redundant for recovery —
    /// external-ids are committed to the LMDB catalog *before* their WAL entry
    /// is appended, and node/rel state lives in the fsynced record stores — so
    /// a full truncate loses nothing recoverable. `u64::MAX` disables it.
    /// See phase0_fix-wal-checkpoint-truncate-production.
    pub checkpoint_size_bytes: u64,
    /// Test-only hook (`phase0_fix-async-wal-flush-durability` §1.2):
    /// when set, `writer_thread` blocks on this receiver immediately
    /// before running `flush_batch` for a `WalCommand::Flush`, so a test
    /// can hold the gate closed to deterministically observe that
    /// `flush()` has not yet returned, then release it to observe the
    /// unblock + durability. Not part of the public configuration
    /// surface — only compiled in test builds of this crate.
    #[cfg(test)]
    pub(crate) flush_gate: Option<Receiver<()>>,
    /// Test-only hook: when set and loaded `true`, `flush_batch` treats
    /// every `wal.append` in the batch as a failure (instead of touching
    /// the real WAL file), so tests can deterministically exercise the
    /// retry-exhaustion / emergency-save error path without depending on
    /// platform-specific I/O failure injection.
    #[cfg(test)]
    pub(crate) fail_flush: Option<Arc<AtomicBool>>,
}

impl Default for AsyncWalConfig {
    fn default() -> Self {
        Self {
            // Sustained durable WAL throughput ~= max_batch_size /
            // per-batch fsync latency (group commit already does exactly
            // one fsync per batch — this knob does not change that
            // mechanism, only how many entries share it). Measured via
            // `benches/wal_throughput.rs` on real hardware: at 100, the
            // writer sustains ~54-59k entries/s (~1.8ms/batch fsync), which
            // sits right at the ~52-68k/s bulk-ingest submit rate (e.g.
            // LDBC SF0.1 relationship loads) — the async queue fills and
            // the producer backpressures. At 1000, sustained throughput is
            // ~71-112k entries/s (~12-14ms/batch fsync; disk write-back
            // noise on this class of hardware is wide, but every run clears
            // the bar), giving headroom over that submit rate so the queue
            // drains instead of filling; a full 10k `max_queue_depth` still
            // drains in ~140ms, well under any client timeout. Under light load,
            // `max_batch_age`/`flush_interval` (unchanged below) still
            // flush partial batches within ~10ms, so latency-to-durable
            // for small workloads is unaffected. Durability is unchanged:
            // still exactly one fsync per batch, recovery replay and
            // `flush()`'s barrier semantics are untouched.
            max_batch_size: 1000,
            max_batch_age: Duration::from_millis(10), // Or flush after 10ms
            max_queue_depth: 10_000,                  // Block if queue gets too deep
            flush_interval: Duration::from_millis(5), // Background flush every 5ms
            channel_buffer_size: 1000,                // Channel buffer for commands
            checkpoint_size_bytes: DEFAULT_WAL_CHECKPOINT_SIZE_BYTES,
            #[cfg(test)]
            flush_gate: None,
            #[cfg(test)]
            fail_flush: None,
        }
    }
}

/// Asynchronous WAL writer
pub struct AsyncWalWriter {
    /// Command sender to the writer thread
    sender: Sender<WalCommand>,
    /// Writer thread handle
    handle: Option<JoinHandle<()>>,
    /// Statistics
    stats: Arc<AsyncWalStats>,
    /// Shutdown flag
    shutdown: Arc<AtomicBool>,
    /// Set true by the writer thread as its VERY LAST action, after its
    /// final drain + flush, right before it returns. Distinct from
    /// `shutdown` (which is set at the START of `shutdown()`): this marks
    /// that the writer will process no further commands. `flush()` uses it
    /// to avoid hanging forever on a `Flush` command that was sent into the
    /// narrow window after the writer's final drain but before it exited —
    /// crossbeam keeps such a buffered command (and its handshake sender)
    /// alive until `AsyncWalWriter` itself drops, so `ack_rx.recv()` would
    /// otherwise never return (phase0_fix-async-wal-flush-durability §3.4).
    writer_exited: Arc<AtomicBool>,
    /// Configuration
    config: AsyncWalConfig,
}

impl Drop for AsyncWalWriter {
    fn drop(&mut self) {
        // Attempt graceful shutdown
        self.shutdown.store(true, Ordering::SeqCst);
        let _ = self.sender.send(WalCommand::Shutdown);

        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
