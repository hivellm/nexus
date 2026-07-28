use super::*;
use crate::error::Error;
use crate::wal::Wal;
use crossbeam_channel::{TrySendError, bounded};
use std::thread;
use std::time::Instant;

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

                        // phase0_fix-wal-checkpoint-truncate-production: once the
                        // freshly-flushed WAL exceeds the configured size, compact
                        // it. The WAL is redundant for recovery — external-ids are
                        // committed to the LMDB catalog BEFORE their WAL entry is
                        // appended, and node/rel state lives in the fsynced record
                        // stores — so a full truncate loses nothing recoverable and
                        // keeps the file bounded well under health_check's 1 GiB
                        // gate. A checkpoint marker is written for observability,
                        // then the log is truncated to empty.
                        if wal.file_size() >= config.checkpoint_size_bytes {
                            let epoch = wal.stats().checkpoints + 1;
                            match wal.checkpoint(epoch).and_then(|()| wal.truncate()) {
                                Ok(()) => {
                                    stats.wal_checkpoints.fetch_add(1, Relaxed);
                                }
                                Err(e) => {
                                    tracing::warn!("WAL checkpoint/truncate failed: {e}");
                                }
                            }
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

        // Persist the batch to an emergency side-WAL in the main WAL's own
        // directory, in the REAL frame format (and cipher), so it is replayed
        // on the next boot rather than silently lost
        // (phase0_fix-wal-durability-gaps #4). The previous fallback wrote an
        // unparseable `[len][bincode]` frame to a CWD-relative `data/` path
        // that nothing ever read back.
        match wal.emergency_save(batch) {
            Ok(path) => tracing::error!(
                "CRITICAL: WAL flush failed after {} retries; {} entries saved to emergency file {} for boot-time replay",
                MAX_RETRIES,
                batch.len(),
                path.display()
            ),
            Err(e) => tracing::error!(
                "CRITICAL: WAL flush failed after {} retries AND emergency save failed ({e}); {} entries lost",
                MAX_RETRIES,
                batch.len()
            ),
        }

        Err(last_error.unwrap_or_else(|| {
            Error::wal(format!(
                "WAL flush failed after {MAX_RETRIES} retries with no captured error"
            ))
        }))
    }
}
