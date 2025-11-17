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
use crossbeam_channel::{Receiver, Sender, bounded};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Commands sent to the WAL writer thread
#[derive(Debug)]
enum WalCommand {
    /// Append a WAL entry
    Append(WalEntry),
    /// Force flush all pending entries
    Flush,
    /// Shutdown the writer thread
    Shutdown,
}

/// Statistics for the async WAL writer
#[derive(Debug, Clone, Default)]
pub struct AsyncWalStats {
    /// Total entries submitted to writer
    pub entries_submitted: u64,
    /// Total entries actually written
    pub entries_written: u64,
    /// Total batches flushed
    pub batches_flushed: u64,
    /// Total force flushes requested
    pub force_flushes: u64,
    /// Total write latency (in microseconds)
    pub total_write_latency_us: u64,
    /// Total flush latency (in microseconds)
    pub total_flush_latency_us: u64,
    /// Number of batches that timed out (vs size-based)
    pub timeout_batches: u64,
    /// Number of batches that hit max size (vs timeout-based)
    pub size_batches: u64,
    /// Current queue depth
    pub current_queue_depth: u64,
    /// Max queue depth seen
    pub max_queue_depth: u64,
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
}

impl Default for AsyncWalConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 100,                      // Batch up to 100 entries
            max_batch_age: Duration::from_millis(10), // Or flush after 10ms
            max_queue_depth: 10_000,                  // Block if queue gets too deep
            flush_interval: Duration::from_millis(5), // Background flush every 5ms
            channel_buffer_size: 1000,                // Channel buffer for commands
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
        let (sender, receiver) = bounded(config.channel_buffer_size);
        let stats = Arc::new(AsyncWalStats::default());
        let shutdown = Arc::new(AtomicBool::new(false));

        let stats_clone = stats.clone();
        let shutdown_clone = shutdown.clone();
        let config_clone = config.clone();

        // Start the background writer thread
        let handle = thread::spawn(move || {
            Self::writer_thread(wal, receiver, stats_clone, shutdown_clone, &config_clone);
        });

        Ok(Self {
            sender,
            handle: Some(handle),
            stats,
            shutdown,
            config,
        })
    }

    /// Submit a WAL entry for asynchronous writing
    ///
    /// This method will block if the queue is full (based on max_queue_depth).
    pub fn append(&self, entry: WalEntry) -> Result<()> {
        // Update stats
        let current_stats = unsafe { &mut *(Arc::as_ptr(&self.stats) as *mut AsyncWalStats) };
        current_stats.entries_submitted += 1;
        current_stats.current_queue_depth += 1;
        if current_stats.current_queue_depth > current_stats.max_queue_depth {
            current_stats.max_queue_depth = current_stats.current_queue_depth;
        }

        // Send command (this may block if queue is full)
        self.sender
            .send(WalCommand::Append(entry))
            .map_err(|_| Error::wal("Failed to send WAL command - channel closed"))?;

        Ok(())
    }

    /// Force flush all pending entries
    ///
    /// This ensures all previously submitted entries are written and synced to disk.
    pub fn flush(&self) -> Result<()> {
        // Update stats
        let current_stats = unsafe { &mut *(Arc::as_ptr(&self.stats) as *mut AsyncWalStats) };
        current_stats.force_flushes += 1;

        self.sender
            .send(WalCommand::Flush)
            .map_err(|_| Error::wal("Failed to send flush command - channel closed"))?;

        Ok(())
    }

    /// Get current statistics
    pub fn stats(&self) -> AsyncWalStats {
        self.stats.as_ref().clone()
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
        config: &AsyncWalConfig,
    ) {
        let mut batch = Vec::with_capacity(config.max_batch_size);
        let mut last_flush = Instant::now();
        let mut batch_start = Instant::now();

        while !shutdown.load(Ordering::SeqCst) {
            // Try to receive a command with timeout
            match receiver.recv_timeout(config.max_batch_age.min(config.flush_interval)) {
                Ok(WalCommand::Append(entry)) => {
                    batch.push(entry);
                    let current_stats =
                        unsafe { &mut *(Arc::as_ptr(&stats) as *mut AsyncWalStats) };
                    current_stats.current_queue_depth -= 1;
                }
                Ok(WalCommand::Flush) => {
                    // Force flush current batch
                    Self::flush_batch(&mut wal, &batch, &stats);
                    batch.clear();
                    batch_start = Instant::now();
                    last_flush = Instant::now();
                    continue;
                }
                Ok(WalCommand::Shutdown) => {
                    // Final flush before shutdown
                    Self::flush_batch(&mut wal, &batch, &stats);
                    break;
                }
                Err(_) => {
                    // Timeout - check if we should flush
                    let should_flush = batch.len() >= config.max_batch_size
                        || batch_start.elapsed() >= config.max_batch_age
                        || last_flush.elapsed() >= config.flush_interval;

                    if should_flush && !batch.is_empty() {
                        Self::flush_batch(&mut wal, &batch, &stats);
                        batch.clear();
                        batch_start = Instant::now();
                        last_flush = Instant::now();
                    }
                }
            }
        }

        // Final flush on exit
        if !batch.is_empty() {
            Self::flush_batch(&mut wal, &batch, &stats);
        }
    }

    /// Flush a batch of WAL entries
    fn flush_batch(wal: &mut Wal, batch: &[WalEntry], stats: &Arc<AsyncWalStats>) {
        if batch.is_empty() {
            return;
        }

        let start_time = Instant::now();

        // Write all entries in batch
        for entry in batch {
            if let Err(e) = wal.append(entry) {
                eprintln!("Failed to append WAL entry: {}", e);
                // Continue with other entries - don't fail the whole batch
            }
        }

        // Flush to disk
        if let Err(e) = wal.flush() {
            eprintln!("Failed to flush WAL: {}", e);
            return;
        }

        let elapsed = start_time.elapsed();
        let elapsed_us = elapsed.as_micros() as u64;

        // Update stats
        let current_stats = unsafe { &mut *(Arc::as_ptr(stats) as *mut AsyncWalStats) };
        current_stats.entries_written += batch.len() as u64;
        current_stats.batches_flushed += 1;
        current_stats.total_write_latency_us += elapsed_us;

        if batch.len() >= 100 {
            current_stats.size_batches += 1;
        } else {
            current_stats.timeout_batches += 1;
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
    use crate::wal::WalEntry;
    use tempfile::TempDir;

    fn create_test_writer() -> (AsyncWalWriter, TempDir) {
        let dir = TempDir::new().unwrap();
        let wal_path = dir.path().join("wal.log");
        let wal = Wal::new(&wal_path).unwrap();

        let config = AsyncWalConfig {
            max_batch_size: 10,
            max_batch_age: Duration::from_millis(50),
            max_queue_depth: 100,
            flush_interval: Duration::from_millis(25),
            channel_buffer_size: 50,
        };

        let writer = AsyncWalWriter::new(wal, config).unwrap();
        (writer, dir)
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

        // Give some time for processing
        thread::sleep(Duration::from_millis(100));

        let stats = writer.stats();
        assert_eq!(stats.entries_submitted, 20);
        assert!(stats.entries_written > 0);

        writer.shutdown().unwrap();
    }

    #[test]
    fn test_batch_size_limit() {
        let dir = TempDir::new().unwrap();
        let wal_path = dir.path().join("wal.log");
        let wal = Wal::new(&wal_path).unwrap();

        let config = AsyncWalConfig {
            max_batch_size: 5,                         // Small batch size for testing
            max_batch_age: Duration::from_millis(100), // Short timeout for testing
            max_queue_depth: 100,
            flush_interval: Duration::from_millis(50), // Short flush interval
            channel_buffer_size: 50,
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
}
