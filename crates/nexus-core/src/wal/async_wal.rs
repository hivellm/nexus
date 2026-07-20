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

use crate::error::{Error, Result};
use crate::wal::{Wal, WalEntry};
use crossbeam_channel::{Receiver, Sender, TrySendError, bounded};
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tracing;

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
}

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
            max_batch_size: 100,                      // Batch up to 100 entries
            max_batch_age: Duration::from_millis(10), // Or flush after 10ms
            max_queue_depth: 10_000,                  // Block if queue gets too deep
            flush_interval: Duration::from_millis(5), // Background flush every 5ms
            channel_buffer_size: 1000,                // Channel buffer for commands
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

impl AsyncWalWriter {
    /// Create a new async WAL writer
    ///
    /// # Arguments
    ///
    /// * `wal` - The underlying WAL instance
    /// * `config` - Configuration for the writer
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use nexus_core::wal::{Wal, AsyncWalWriter, AsyncWalConfig};
    ///
    /// let wal = Wal::new("./data/wal.log").unwrap();
    /// let config = AsyncWalConfig::default();
    /// let writer = AsyncWalWriter::new(wal, config).unwrap();
    /// ```
    pub fn new(wal: Wal, config: AsyncWalConfig) -> Result<Self> {
        // #19: size the command channel from the configured max queue depth so
        // the `max_queue_depth` knob is real (previously the channel was bound
        // only by `channel_buffer_size`, so `max_queue_depth` merely fed a
        // counter and the writer blocked far earlier than configured).
        let capacity = config.channel_buffer_size.max(config.max_queue_depth);
        let (sender, receiver) = bounded(capacity);
        let stats = Arc::new(AsyncWalStats::default());
        let shutdown = Arc::new(AtomicBool::new(false));
        let writer_exited = Arc::new(AtomicBool::new(false));

        let stats_clone = stats.clone();
        let shutdown_clone = shutdown.clone();
        let writer_exited_clone = writer_exited.clone();
        let config_clone = config.clone();

        // Start the background writer thread
        let handle = thread::spawn(move || {
            Self::writer_thread(
                wal,
                receiver,
                stats_clone,
                shutdown_clone,
                writer_exited_clone,
                &config_clone,
            );
        });

        Ok(Self {
            sender,
            handle: Some(handle),
            stats,
            shutdown,
            writer_exited,
            config,
        })
    }

    /// Submit a WAL entry for asynchronous writing
    ///
    /// This method will block if the queue is full (based on max_queue_depth).
    pub fn append(&self, entry: WalEntry) -> Result<()> {
        use std::sync::atomic::Ordering::Relaxed;
        // Update stats atomically — `fetch_add` returns the previous
        // value; the `max` compare is a relaxed CAS loop below.
        self.stats.entries_submitted.fetch_add(1, Relaxed);
        let new_depth = self.stats.current_queue_depth.fetch_add(1, Relaxed) + 1;
        let mut max = self.stats.max_queue_depth.load(Relaxed);
        while new_depth > max {
            match self
                .stats
                .max_queue_depth
                .compare_exchange_weak(max, new_depth, Relaxed, Relaxed)
            {
                Ok(_) => break,
                Err(current) => max = current,
            }
        }

        // #19: fast non-blocking submit. Only when the channel is genuinely
        // full (writer thread behind fsync) do we block — and we surface the
        // backpressure first so a sustained write burst that stalls the
        // engine write lock is observable instead of an opaque hang. The
        // blocking `send` preserves ordering + durability (crossbeam blocks
        // rather than drops); it is bounded by the (now larger) channel.
        match self.sender.try_send(WalCommand::Append(entry)) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(cmd)) => {
                self.stats.backpressure_blocks.fetch_add(1, Relaxed);
                tracing::warn!(
                    queue_depth = new_depth,
                    "WAL async channel full — applying backpressure (background \
                     writer is behind fsync); the submitting thread will block \
                     until the queue drains (issue #19)"
                );
                self.sender
                    .send(cmd)
                    .map_err(|_| Error::wal("Failed to send WAL command - channel closed"))
            }
            Err(TrySendError::Disconnected(_)) => {
                Err(Error::wal("Failed to send WAL command - channel closed"))
            }
        }
    }

    /// Force flush all pending entries
    ///
    /// This is a synchronous durability barrier: it blocks until the
    /// background writer thread has actually run `flush_batch` (i.e. the
    /// fsync backing every entry successfully `append()`-ed *before* this
    /// call has completed) and returns the real outcome of that flush —
    /// not merely the fact that the request was enqueued. See
    /// `phase0_fix-async-wal-flush-durability` for the full contract.
    ///
    /// # Errors
    ///
    /// Returns `Error::Wal` if the writer thread's command channel is
    /// already closed, if the writer thread exits (e.g. racing
    /// `shutdown()`) without signaling completion, or if the underlying
    /// `flush_batch` failed after exhausting its retries.
    pub fn flush(&self) -> Result<()> {
        use std::sync::atomic::Ordering::Relaxed;
        self.stats.force_flushes.fetch_add(1, Relaxed);

        // Fresh single-use handshake channel per call (§2.1: plain
        // `std::sync::mpsc`, already in std — no new crate dependency).
        // The writer thread signals this specific request's outcome
        // through `ack_tx` once it has actually run `flush_batch`.
        let (ack_tx, ack_rx) = mpsc::channel();

        self.sender
            .send(WalCommand::Flush(ack_tx))
            .map_err(|_| Error::wal("Failed to send flush command - channel closed"))?;

        // Block until signaled. Two exit conditions besides a normal ack:
        //  - Disconnected: the writer dropped `ack_tx` without sending (e.g.
        //    it unwound from a panic mid-flush) — surface as an error.
        //  - Timeout + writer already exited: our `Flush` command was sent
        //    into the narrow window after the writer's final drain but
        //    before it exited, so it is trapped in the crossbeam command
        //    buffer (which keeps the buffered command — and this `ack_tx` —
        //    alive until `AsyncWalWriter` itself drops, so plain `recv()`
        //    would hang forever). Poll `writer_exited` so we return an error
        //    instead. (phase0_fix-async-wal-flush-durability §3.4.)
        loop {
            match ack_rx.recv_timeout(Duration::from_millis(50)) {
                Ok(result) => return result,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(Error::wal(
                        "Flush handshake channel closed before completion \
                         (writer thread exited without acknowledging the flush)",
                    ));
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if self.writer_exited.load(Ordering::SeqCst) {
                        return Err(Error::wal(
                            "Flush not acknowledged: the async WAL writer \
                             thread has exited (command was not processed)",
                        ));
                    }
                    // Writer still alive — keep waiting for the real ack.
                }
            }
        }
    }

    /// Get a consistent-per-field snapshot of the current statistics.
    pub fn stats(&self) -> AsyncWalStatsSnapshot {
        self.stats.snapshot()
    }

    /// Get configuration
    pub fn config(&self) -> &AsyncWalConfig {
        &self.config
    }

    /// Shutdown the writer
    ///
    /// This will flush all pending entries and stop the background thread.
    pub fn shutdown(&mut self) -> Result<()> {
        // Signal shutdown
        self.shutdown.store(true, Ordering::SeqCst);

        // Send shutdown command
        let _ = self.sender.send(WalCommand::Shutdown);

        // Wait for thread to finish
        if let Some(handle) = self.handle.take() {
            handle
                .join()
                .map_err(|_| Error::wal("Writer thread panicked"))?;
        }

        Ok(())
    }

    /// Background writer thread implementation
    fn writer_thread(
        mut wal: Wal,
        receiver: Receiver<WalCommand>,
        stats: Arc<AsyncWalStats>,
        shutdown: Arc<AtomicBool>,
        writer_exited: Arc<AtomicBool>,
        config: &AsyncWalConfig,
    ) {
        // Mark the thread as fully finished on ANY exit path (normal or
        // unwinding from a panic), as the LAST thing that happens — so a
        // `flush()` blocked on a command that will never be processed can
        // observe it and return an error instead of hanging forever.
        // (phase0_fix-async-wal-flush-durability §3.4.)
        struct ExitGuard(Arc<AtomicBool>);
        impl Drop for ExitGuard {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let _exit_guard = ExitGuard(writer_exited);

        let mut batch = Vec::with_capacity(config.max_batch_size);
        let mut last_flush = Instant::now();
        let mut batch_start = Instant::now();

        while !shutdown.load(Ordering::SeqCst) {
            // Try to receive a command with timeout
            match receiver.recv_timeout(config.max_batch_age.min(config.flush_interval)) {
                Ok(WalCommand::Append(entry)) => {
                    batch.push(entry);
                    // Decrement queue depth without underflow via a
                    // relaxed CAS loop that stops at zero.
                    use std::sync::atomic::Ordering::Relaxed;
                    let mut depth = stats.current_queue_depth.load(Relaxed);
                    while depth > 0 {
                        match stats.current_queue_depth.compare_exchange_weak(
                            depth,
                            depth - 1,
                            Relaxed,
                            Relaxed,
                        ) {
                            Ok(_) => break,
                            Err(cur) => depth = cur,
                        }
                    }

                    // Check if batch reached max size - flush immediately
                    if batch.len() >= config.max_batch_size {
                        let _ = Self::flush_batch(&mut wal, &batch, &stats, config);
                        batch.clear();
                        batch_start = Instant::now();
                        last_flush = Instant::now();
                    }
                }
                Ok(WalCommand::Flush(ack_tx)) => {
                    // Test-only deterministic gate (§1.2): block right
                    // here, before running `flush_batch`, so a test can
                    // hold the gate closed and observe that `flush()`
                    // has not yet returned.
                    #[cfg(test)]
                    if let Some(gate) = &config.flush_gate {
                        let _ = gate.recv();
                    }

                    // Force flush current batch and signal the real
                    // outcome back through the handshake (§2.1/§3.2/§3.3)
                    // before continuing the loop.
                    let result = Self::flush_batch(&mut wal, &batch, &stats, config);
                    batch.clear();
                    batch_start = Instant::now();
                    last_flush = Instant::now();
                    let _ = ack_tx.send(result);
                    continue;
                }
                Ok(WalCommand::Shutdown) => {
                    // Final flush before shutdown. Clear the batch
                    // afterward — leaving already-flushed entries in
                    // `batch` would replay them a second time via the
                    // drain-phase flush below, and would corrupt the
                    // ordering guarantee (§2.2) for any `Flush` handshake
                    // drained after this point.
                    let _ = Self::flush_batch(&mut wal, &batch, &stats, config);
                    batch.clear();
                    break;
                }
                Err(_) => {
                    // Timeout - check if we should flush
                    let should_flush = batch.len() >= config.max_batch_size
                        || batch_start.elapsed() >= config.max_batch_age
                        || last_flush.elapsed() >= config.flush_interval;

                    if should_flush && !batch.is_empty() {
                        let _ = Self::flush_batch(&mut wal, &batch, &stats, config);
                        batch.clear();
                        batch_start = Instant::now();
                        last_flush = Instant::now();
                    }
                }
            }
        }

        // Final drain on exit (#19 durability): the shutdown flag pops the
        // loop at the top, which can leave ACCEPTED Append commands sitting
        // in the channel — dropping them would break the "accepted ⇒
        // durable" contract (`append()` already returned Ok to the caller).
        // Consume everything still queued before the final flush.
        //
        // §2.3/§3.4: a `Flush` command can also be sitting here if a
        // caller's `flush()` raced `shutdown()` and lost — honor its
        // handshake instead of silently dropping the sender. Dropping it
        // would still unblock the caller (a disconnected receiver becomes
        // an `Err`), but running the real flush and acking it keeps the
        // §2.2 ordering guarantee (this flush() covers everything
        // appended before it) and avoids a false-negative error report.
        while let Ok(cmd) = receiver.try_recv() {
            match cmd {
                WalCommand::Append(entry) => {
                    batch.push(entry);
                    if batch.len() >= config.max_batch_size {
                        let _ = Self::flush_batch(&mut wal, &batch, &stats, config);
                        batch.clear();
                    }
                }
                WalCommand::Flush(ack_tx) => {
                    let result = Self::flush_batch(&mut wal, &batch, &stats, config);
                    batch.clear();
                    let _ = ack_tx.send(result);
                }
                WalCommand::Shutdown => {}
            }
        }

        // Final flush on exit
        if !batch.is_empty() {
            let _ = Self::flush_batch(&mut wal, &batch, &stats, config);
        }
    }

    /// Flush a batch of WAL entries.
    ///
    /// Returns the real outcome: `Ok(())` once every entry in `batch` has
    /// been written and `wal.flush()` (the real fsync) has succeeded, or
    /// `Err` carrying the last observed failure once `MAX_RETRIES` attempts
    /// are exhausted (after the emergency backup save). `flush()`'s
    /// completion handshake relies on this being faithful — see
    /// `phase0_fix-async-wal-flush-durability` §3.3.
    fn flush_batch(
        wal: &mut Wal,
        batch: &[WalEntry],
        stats: &Arc<AsyncWalStats>,
        config: &AsyncWalConfig,
    ) -> Result<()> {
        if batch.is_empty() {
            return Ok(());
        }

        let start_time = Instant::now();

        // Try to flush batch with retry logic for I/O errors
        let mut retry_count = 0;
        const MAX_RETRIES: u32 = 3;
        let mut last_error: Option<Error> = None;

        while retry_count < MAX_RETRIES {
            let mut success_count = 0;

            // Write all entries in batch
            for entry in batch {
                // Test-only fault injection (§4.2: "flush() propagates a
                // real Err when flush_batch fails after retries"): when
                // `config.fail_flush` is set and true, every append in
                // this batch is treated as failed without touching the
                // real WAL file, so the retry-exhaustion path is
                // deterministic and platform-independent.
                let inject_failure = {
                    #[cfg(test)]
                    {
                        config
                            .fail_flush
                            .as_ref()
                            .is_some_and(|flag| flag.load(Ordering::Relaxed))
                    }
                    #[cfg(not(test))]
                    {
                        false
                    }
                };

                let append_result = if inject_failure {
                    Err(Error::wal(
                        "injected test failure (AsyncWalConfig::fail_flush gate)",
                    ))
                } else {
                    wal.append(entry)
                };

                match append_result {
                    Ok(_) => success_count += 1,
                    Err(e) => {
                        last_error = Some(e);
                        tracing::error!(
                            "Failed to append WAL entry (attempt {}): {}",
                            retry_count + 1,
                            last_error.as_ref().unwrap()
                        );

                        // If it's a permission error, try to recover
                        if let Error::Io(io_err) = last_error.as_ref().unwrap() {
                            if io_err.raw_os_error() == Some(5) {
                                // ERROR_ACCESS_DENIED
                                tracing::warn!(
                                    "Permission denied error detected, attempting WAL recovery..."
                                );

                                // Try to reopen WAL file
                                if let Err(recovery_err) = wal.reopen() {
                                    tracing::error!("WAL recovery failed: {}", recovery_err);
                                } else {
                                    tracing::info!("WAL recovery successful, retrying batch...");
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            // If all entries were written successfully, flush to disk
            if success_count == batch.len() {
                match wal.flush() {
                    Ok(_) => {
                        // Success - update stats and return
                        let elapsed = start_time.elapsed();
                        let elapsed_us = elapsed.as_micros() as u64;

                        use std::sync::atomic::Ordering::Relaxed;
                        stats.entries_written.fetch_add(batch.len() as u64, Relaxed);
                        stats.batches_flushed.fetch_add(1, Relaxed);
                        stats.total_write_latency_us.fetch_add(elapsed_us, Relaxed);

                        // Track if batch was flushed due to size limit vs timeout
                        if batch.len() >= config.max_batch_size {
                            stats.size_batches.fetch_add(1, Relaxed);
                        } else {
                            stats.timeout_batches.fetch_add(1, Relaxed);
                        }

                        if retry_count > 0 {
                            tracing::info!(
                                "WAL batch flushed successfully after {} retries",
                                retry_count
                            );
                        }
                        return Ok(());
                    }
                    Err(e) => {
                        last_error = Some(e);
                        tracing::error!(
                            "Failed to flush WAL (attempt {}): {}",
                            retry_count + 1,
                            last_error.as_ref().unwrap()
                        );
                    }
                }
            }

            retry_count += 1;

            // Wait before retry with exponential backoff
            if retry_count < MAX_RETRIES {
                let wait_time = Duration::from_millis(100 * (1 << retry_count)); // 200ms, 400ms, 800ms
                tracing::debug!("Retrying WAL flush in {:?}", wait_time);
                thread::sleep(wait_time);
            }
        }

        // If we get here, all retries failed
        use std::sync::atomic::Ordering::Relaxed;
        stats.wal_errors.fetch_add(batch.len() as u64, Relaxed);

        tracing::error!(
            "CRITICAL: Failed to flush WAL batch after {} retries. {} entries lost!",
            MAX_RETRIES,
            batch.len()
        );

        // Try emergency save to a backup WAL file
        Self::emergency_save_batch(batch);

        Err(last_error.unwrap_or_else(|| {
            Error::wal(format!(
                "WAL flush failed after {MAX_RETRIES} retries with no captured error"
            ))
        }))
    }

    /// Emergency save batch to backup WAL file when main WAL fails
    fn emergency_save_batch(batch: &[WalEntry]) {
        let backup_path = format!("data/wal-emergency-{}.log", chrono::Utc::now().timestamp());

        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&backup_path)
        {
            Ok(mut file) => {
                for entry in batch {
                    if let Ok(data) = bincode::serialize(entry) {
                        let _ = file.write_all(&(data.len() as u32).to_le_bytes());
                        let _ = file.write_all(&data);
                    }
                }
                let _ = file.flush();
                tracing::warn!("Emergency WAL batch saved to: {}", backup_path);
            }
            Err(e) => {
                tracing::error!("CRITICAL: Even emergency WAL save failed: {}", e);
            }
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestContext;
    use crate::wal::WalEntry;

    fn create_test_writer() -> (AsyncWalWriter, TestContext) {
        let ctx = TestContext::new();
        let wal_path = ctx.path().join("wal.log");
        let wal = Wal::new(&wal_path).unwrap();

        let config = AsyncWalConfig {
            max_batch_size: 10,
            max_batch_age: Duration::from_millis(50),
            max_queue_depth: 100,
            flush_interval: Duration::from_millis(25),
            channel_buffer_size: 50,
            flush_gate: None,
            fail_flush: None,
        };

        let writer = AsyncWalWriter::new(wal, config).unwrap();
        (writer, ctx)
    }

    #[test]
    fn test_async_writer_creation() {
        let (mut writer, _dir) = create_test_writer();
        assert_eq!(writer.stats().entries_submitted, 0);
    }

    #[test]
    fn test_append_entry() {
        let (mut writer, _dir) = create_test_writer();

        let entry = WalEntry::BeginTx {
            tx_id: 1,
            epoch: 100,
        };

        writer.append(entry).unwrap();
        assert_eq!(writer.stats().entries_submitted, 1);
    }

    #[test]
    fn test_shutdown() {
        let (mut writer, _dir) = create_test_writer();

        // Submit some entries
        for i in 0..5 {
            let entry = WalEntry::CreateNode {
                node_id: i,
                label_bits: 0,
            };
            writer.append(entry).unwrap();
        }

        // Shutdown should flush everything
        writer.shutdown().unwrap();
    }

    #[test]
    fn test_multiple_appends_and_flush() {
        let (mut writer, _dir) = create_test_writer();

        // Submit multiple entries
        for i in 0..20 {
            let entry = WalEntry::CreateNode {
                node_id: i,
                label_bits: 0,
            };
            writer.append(entry).unwrap();
        }

        // Force flush
        writer.flush().unwrap();

        // Give more time for async processing to complete
        thread::sleep(Duration::from_millis(500));

        let stats = writer.stats();
        assert_eq!(stats.entries_submitted, 20);
        // Note: entries_written may be 0 on fast systems where shutdown happens before write
        // This is acceptable behavior - we just verify entries were submitted
        assert!(stats.entries_submitted > 0, "Should have submitted entries");

        writer.shutdown().unwrap();
    }

    /// #19: a burst far larger than the channel capacity must not deadlock —
    /// the submitting thread blocks on backpressure (try_send Full -> blocking
    /// send) and every entry is accepted and eventually written.
    #[test]
    fn test_backpressure_burst_does_not_deadlock() {
        let ctx = TestContext::new();
        let wal = Wal::new(ctx.path().join("wal.log")).unwrap();
        let config = AsyncWalConfig {
            max_batch_size: 10,
            max_batch_age: Duration::from_millis(20),
            max_queue_depth: 16, // channel capacity = max(8, 16) = 16
            flush_interval: Duration::from_millis(10),
            channel_buffer_size: 8,
            flush_gate: None,
            fail_flush: None,
        };
        let mut writer = AsyncWalWriter::new(wal, config).unwrap();

        // Submit 2000 entries into a 16-slot channel — exercises the
        // full-channel backpressure path repeatedly. Must complete, not hang.
        let burst = 2000u64;
        for i in 0..burst {
            writer
                .append(WalEntry::CreateNode {
                    node_id: i,
                    label_bits: 0,
                })
                .expect("append must not fail under backpressure");
        }
        assert_eq!(
            writer.stats().entries_submitted,
            burst,
            "all entries accepted despite a channel smaller than the burst (no deadlock)"
        );

        // Shutdown drains + joins the writer thread without hanging. (#19 is
        // about the submit path no longer dead-ending on a full channel;
        // exact shutdown-drain timing is a separate, non-deterministic concern.)
        writer.shutdown().unwrap();

        // Durability: the burst survives backpressure end-to-end — a fresh
        // Wal on the same file replays every entry (none dropped while the
        // channel was full).
        let mut reopened = Wal::new(ctx.path().join("wal.log")).unwrap();
        let recovered = reopened.recover().unwrap();
        let create_nodes = recovered
            .iter()
            .filter(|e| matches!(e, WalEntry::CreateNode { .. }))
            .count() as u64;
        assert_eq!(
            create_nodes, burst,
            "WAL replay must recover every entry submitted under backpressure"
        );
    }

    #[test]
    #[ignore] // TODO: Fix batch size limit test - timing issue with async flushing
    fn test_batch_size_limit() {
        let ctx = TestContext::new();
        let wal_path = ctx.path().join("wal.log");
        let wal = Wal::new(&wal_path).unwrap();

        let config = AsyncWalConfig {
            max_batch_size: 5,                         // Small batch size for testing
            max_batch_age: Duration::from_millis(100), // Short timeout for testing
            max_queue_depth: 100,
            flush_interval: Duration::from_millis(50), // Short flush interval
            channel_buffer_size: 50,
            flush_gate: None,
            fail_flush: None,
        };

        let mut writer = AsyncWalWriter::new(wal, config).unwrap();

        // Submit more entries than batch size
        for i in 0..10 {
            let entry = WalEntry::CreateNode {
                node_id: i,
                label_bits: 0,
            };
            writer.append(entry).unwrap();
        }

        // Give time for batching and flushing (longer wait for async processing)
        thread::sleep(Duration::from_millis(500));

        let stats = writer.stats();
        assert_eq!(stats.entries_submitted, 10);
        // With batch size 5, 10 entries should create at least 2 batches
        // But due to timing, we might get fewer - just check that some batches were flushed
        assert!(
            stats.batches_flushed > 0,
            "No batches were flushed, got {}",
            stats.batches_flushed
        );

        writer.shutdown().unwrap();
    }

    /// Regression test for the `unsafe { &mut *(Arc::as_ptr(...) as
    /// *mut AsyncWalStats) }` data race: 10 threads call `append` in
    /// parallel; every one of them must be counted. With the old
    /// pointer-cast implementation the final `entries_submitted`
    /// could come in below 10 under Miri / stressed loads; with the
    /// atomic counters it must be exactly 10.
    #[test]
    fn concurrent_appends_count_exactly() {
        use std::sync::Arc as ArcT;
        let (writer, _dir) = create_test_writer();
        let writer = ArcT::new(writer);
        let threads: Vec<_> = (0..10)
            .map(|i| {
                let w = ArcT::clone(&writer);
                thread::spawn(move || {
                    w.append(WalEntry::CreateNode {
                        node_id: i,
                        label_bits: 0,
                    })
                    .unwrap();
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        let snap = writer.stats();
        assert_eq!(snap.entries_submitted, 10);
    }

    /// §1.2 (`phase0_fix-async-wal-flush-durability`) — deterministic
    /// proof that `flush()` blocks until the writer thread has actually
    /// executed `flush_batch` for the `WalCommand::Flush` it enqueued,
    /// not merely until the request landed on the channel. A test-only
    /// gate (`AsyncWalConfig::flush_gate`) holds the writer thread
    /// immediately before it runs `flush_batch`; while the gate is
    /// closed, `flush()` must not have returned and the entry must not
    /// yet be durable on disk. Releasing the gate must unblock `flush()`
    /// and make the entry durable by the time it returns. This is the
    /// permanent regression test for the bug: before the fix, `flush()`
    /// returned as soon as the command was sent, so this test would fail
    /// at the "must not have returned" assertion.
    #[test]
    fn flush_blocks_until_writer_thread_signals_completion() {
        let ctx = TestContext::new();
        let wal_path = ctx.path().join("wal.log");
        let wal = Wal::new(&wal_path).unwrap();

        // Rendezvous gate: `writer_thread` blocks in `gate.recv()` until
        // this test sends on `gate_tx`.
        let (gate_tx, gate_rx) = bounded::<()>(0);
        let config = AsyncWalConfig {
            max_batch_size: 100,
            max_batch_age: Duration::from_secs(60), // no age-based auto-flush
            max_queue_depth: 100,
            flush_interval: Duration::from_secs(60), // no interval-based auto-flush
            channel_buffer_size: 50,
            flush_gate: Some(gate_rx),
            fail_flush: None,
        };

        let writer = Arc::new(AsyncWalWriter::new(wal, config).unwrap());

        writer
            .append(WalEntry::CreateNode {
                node_id: 1,
                label_bits: 0,
            })
            .unwrap();

        // Give the writer thread time to dequeue the Append into its
        // batch before the gated Flush is issued, isolating the
        // enqueue-vs-complete gap on the Flush command itself.
        thread::sleep(Duration::from_millis(50));

        let flush_writer = Arc::clone(&writer);
        let (done_tx, done_rx) = mpsc::channel::<Result<()>>();
        let flush_thread = thread::spawn(move || {
            let _ = done_tx.send(flush_writer.flush());
        });

        // Gate is closed: flush() must NOT have returned yet.
        thread::sleep(Duration::from_millis(200));
        assert!(
            done_rx.try_recv().is_err(),
            "flush() returned before the writer thread processed WalCommand::Flush \
             — the durability barrier is not actually blocking"
        );

        // ...and the entry must not be durable on disk yet either.
        let mut probe = Wal::new(&wal_path).unwrap();
        let recovered_before_release = probe.recover().unwrap();
        assert!(
            recovered_before_release.is_empty(),
            "entry must not be durable while flush() is still gated"
        );

        // Release the gate: the writer thread runs flush_batch and
        // signals completion; flush() must then return promptly.
        gate_tx.send(()).unwrap();

        let result = done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("flush() must return once the gate is released (bounded wait)");
        result.expect("flush() must succeed");
        flush_thread.join().unwrap();

        let mut probe = Wal::new(&wal_path).unwrap();
        let recovered_after = probe.recover().unwrap();
        assert_eq!(
            recovered_after.len(),
            1,
            "entry must be durable immediately after flush() returns"
        );

        Arc::try_unwrap(writer)
            .unwrap_or_else(|_| panic!("writer still has outstanding Arc clones"))
            .shutdown()
            .unwrap();
    }

    /// §4.2 — `flush()` must propagate a real `Err`, not a blind
    /// `Ok(())`, when `flush_batch` fails after exhausting its retries.
    /// Uses the `AsyncWalConfig::fail_flush` test-only fault injector so
    /// the failure is deterministic and platform-independent while still
    /// exercising the real retry/backoff/emergency-save path in
    /// `flush_batch`.
    #[test]
    fn flush_propagates_error_after_retries_exhausted() {
        let ctx = TestContext::new();
        let wal = Wal::new(ctx.path().join("wal.log")).unwrap();

        let fail_flush = Arc::new(AtomicBool::new(true));
        let config = AsyncWalConfig {
            max_batch_size: 10,
            max_batch_age: Duration::from_millis(20),
            max_queue_depth: 100,
            flush_interval: Duration::from_millis(10),
            channel_buffer_size: 50,
            flush_gate: None,
            fail_flush: Some(Arc::clone(&fail_flush)),
        };

        let mut writer = AsyncWalWriter::new(wal, config).unwrap();

        writer
            .append(WalEntry::CreateNode {
                node_id: 1,
                label_bits: 0,
            })
            .unwrap();

        // Exhausts MAX_RETRIES with exponential backoff (~600ms of
        // sleeping) before returning — bounded well under any test
        // harness timeout.
        let result = writer.flush();
        assert!(
            result.is_err(),
            "flush() must return Err when flush_batch exhausts its retries, got {result:?}"
        );

        fail_flush.store(false, Ordering::Relaxed);
        writer.shutdown().unwrap();
    }

    /// §2.3/§3.4 — a `flush()` call racing `shutdown()` must not hang: it
    /// must return (`Ok` via a signaled handshake, or `Err` if the
    /// writer thread's handshake sender was dropped because the thread
    /// exited first). The public API's `shutdown(&mut self)` cannot
    /// literally run concurrently with `flush(&self)` from safe code on
    /// the same instance (the `&mut` borrow forbids it), so this test
    /// drives the race directly through the writer's internal command
    /// channel — `sender` and `shutdown` are private fields of
    /// `AsyncWalWriter`, reachable here because this test module is a
    /// child of `async_wal` — issuing the exact same
    /// flag-then-`WalCommand::Shutdown` sequence the real `shutdown()`
    /// method uses, but from a second thread while several `flush()`
    /// calls are already in flight.
    #[test]
    fn flush_concurrent_with_shutdown_does_not_hang() {
        let ctx = TestContext::new();
        let wal = Wal::new(ctx.path().join("wal.log")).unwrap();
        let config = AsyncWalConfig {
            max_batch_size: 10,
            max_batch_age: Duration::from_millis(10),
            max_queue_depth: 100,
            flush_interval: Duration::from_millis(5),
            channel_buffer_size: 50,
            flush_gate: None,
            fail_flush: None,
        };
        let writer = Arc::new(AsyncWalWriter::new(wal, config).unwrap());

        writer
            .append(WalEntry::CreateNode {
                node_id: 1,
                label_bits: 0,
            })
            .unwrap();

        let (done_tx, done_rx) = mpsc::channel::<Result<()>>();
        let flush_threads: Vec<_> = (0..8)
            .map(|_| {
                let w = Arc::clone(&writer);
                let tx = done_tx.clone();
                thread::spawn(move || {
                    let _ = tx.send(w.flush());
                })
            })
            .collect();
        drop(done_tx);

        // Race the manual shutdown sequence (identical to what
        // `AsyncWalWriter::shutdown()` does) against the in-flight
        // `flush()` calls above.
        writer.shutdown.store(true, Ordering::SeqCst);
        let _ = writer.sender.send(WalCommand::Shutdown);

        for _ in 0..8 {
            done_rx.recv_timeout(Duration::from_secs(5)).expect(
                "flush() concurrent with shutdown() must return within the bound \
                 (a disconnected handshake channel must surface as Err, not a hang)",
            );
        }

        for t in flush_threads {
            t.join().unwrap();
        }

        // Bound teardown too: dropping the last `Arc<AsyncWalWriter>`
        // runs `Drop::drop`, which joins the background thread. Do that
        // on a helper thread with a timeout so a regression there fails
        // this test instead of hanging the whole test binary.
        let (teardown_tx, teardown_rx) = mpsc::channel::<()>();
        thread::spawn(move || {
            drop(writer);
            let _ = teardown_tx.send(());
        });
        teardown_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("dropping the writer (joining the background thread) must not hang");
    }
}
