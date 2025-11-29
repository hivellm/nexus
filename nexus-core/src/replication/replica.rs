//! Replica node implementation for replication
//!
//! The replica node:
//! - Connects to master via TCP
//! - Receives and applies WAL entries
//! - Sends ACKs to master
//! - Monitors master health
//! - Supports automatic failover

use crate::replication::config::{ReplicationConfig, ReplicationRole};
use crate::replication::protocol::{PROTOCOL_VERSION, ReplicationMessage};
use crate::replication::snapshot::Snapshot;
use crate::replication::{DEFAULT_HEARTBEAT_MS, MISSED_HEARTBEATS_THRESHOLD};
use crate::wal::WalEntry;
use crate::{Error, Result};
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

/// Replica node statistics
#[derive(Debug, Clone, Default)]
pub struct ReplicaStats {
    /// Total WAL entries received
    pub entries_received: u64,
    /// Total WAL entries applied
    pub entries_applied: u64,
    /// Total bytes received
    pub bytes_received: u64,
    /// Current WAL offset
    pub current_offset: u64,
    /// Replication lag (in operations)
    pub lag: u64,
    /// Number of reconnects
    pub reconnects: u32,
    /// Last successful sync time
    pub last_sync_time: Option<Instant>,
    /// Master ID
    pub master_id: Option<String>,
    /// Is connected to master
    pub connected: bool,
}

/// Callback for applying WAL entries
pub type ApplyCallback = Arc<dyn Fn(WalEntry, u64) -> Result<()> + Send + Sync>;

/// Replica node for replication
pub struct Replica {
    /// Replica ID
    id: String,
    /// Configuration
    config: ReplicationConfig,
    /// Current WAL offset (last applied)
    current_offset: AtomicU64,
    /// Master's current offset (for lag calculation)
    master_offset: AtomicU64,
    /// Statistics
    stats: Mutex<ReplicaStats>,
    /// Running flag
    running: AtomicBool,
    /// Connected flag
    connected: AtomicBool,
    /// Missed heartbeats counter
    missed_heartbeats: AtomicU32,
    /// Shutdown signal sender
    shutdown_tx: Mutex<Option<oneshot::Sender<()>>>,
    /// Callback for applying WAL entries
    apply_callback: Option<ApplyCallback>,
    /// Snapshot manager
    snapshot: Arc<Snapshot>,
    /// Channel for promoting to master
    promote_tx: Mutex<Option<mpsc::Sender<()>>>,
}

impl Replica {
    /// Create a new replica node
    pub fn new(config: ReplicationConfig, snapshot: Arc<Snapshot>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            config,
            current_offset: AtomicU64::new(0),
            master_offset: AtomicU64::new(0),
            stats: Mutex::new(ReplicaStats::default()),
            running: AtomicBool::new(false),
            connected: AtomicBool::new(false),
            missed_heartbeats: AtomicU32::new(0),
            shutdown_tx: Mutex::new(None),
            apply_callback: None,
            snapshot,
            promote_tx: Mutex::new(None),
        }
    }

    /// Set callback for applying WAL entries
    pub fn with_apply_callback(mut self, callback: ApplyCallback) -> Self {
        self.apply_callback = Some(callback);
        self
    }

    /// Get replica ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get current WAL offset
    pub fn current_offset(&self) -> u64 {
        self.current_offset.load(Ordering::SeqCst)
    }

    /// Set current offset (e.g., after recovery)
    pub fn set_offset(&self, offset: u64) {
        self.current_offset.store(offset, Ordering::SeqCst);
    }

    /// Get replication lag
    pub fn lag(&self) -> u64 {
        self.master_offset
            .load(Ordering::SeqCst)
            .saturating_sub(self.current_offset.load(Ordering::SeqCst))
    }

    /// Check if connected to master
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    /// Start the replica node
    pub async fn start(&self) -> Result<()> {
        if self.running.swap(true, Ordering::SeqCst) {
            return Err(Error::replication("Replica already running"));
        }

        let master_addr = self
            .config
            .master_addr
            .ok_or_else(|| Error::replication("Master address not configured"))?;

        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
        *self.shutdown_tx.lock() = Some(shutdown_tx);

        let (promote_tx, mut promote_rx) = mpsc::channel(1);
        *self.promote_tx.lock() = Some(promote_tx);

        let mut reconnect_delay = Duration::from_secs(1);
        let max_reconnect_delay = Duration::from_secs(60);

        loop {
            tokio::select! {
                _ = &mut shutdown_rx => {
                    tracing::info!("Replica shutting down");
                    break;
                }
                _ = promote_rx.recv() => {
                    tracing::info!("Replica promoted to master");
                    break;
                }
                result = self.connect_and_sync(master_addr) => {
                    match result {
                        Ok(()) => {
                            // Normal disconnect, reset delay
                            reconnect_delay = Duration::from_secs(1);
                        }
                        Err(e) => {
                            tracing::error!("Replication error: {}. Reconnecting in {:?}", e, reconnect_delay);
                            self.connected.store(false, Ordering::SeqCst);

                            {
                                let mut stats = self.stats.lock();
                                stats.connected = false;
                                stats.reconnects += 1;
                            }

                            tokio::time::sleep(reconnect_delay).await;

                            // Exponential backoff
                            reconnect_delay = std::cmp::min(reconnect_delay * 2, max_reconnect_delay);
                        }
                    }
                }
            }
        }

        self.running.store(false, Ordering::SeqCst);
        self.connected.store(false, Ordering::SeqCst);
        Ok(())
    }

    /// Connect to master and start syncing
    async fn connect_and_sync(&self, master_addr: std::net::SocketAddr) -> Result<()> {
        tracing::info!("Connecting to master at {}", master_addr);

        let mut stream =
            tokio::time::timeout(self.config.connect_timeout, TcpStream::connect(master_addr))
                .await
                .map_err(|_| Error::replication("Connection timeout"))??;

        // Send Hello
        let hello = ReplicationMessage::Hello {
            replica_id: self.id.clone(),
            last_wal_offset: self.current_offset(),
            protocol_version: PROTOCOL_VERSION,
        };
        hello.write_to(&mut stream).await?;

        // Read Welcome
        let welcome = ReplicationMessage::read_from(&mut stream).await?;
        let (master_id, master_offset, requires_full_sync) = match welcome {
            ReplicationMessage::Welcome {
                master_id,
                current_wal_offset,
                requires_full_sync,
            } => (master_id, current_wal_offset, requires_full_sync),
            ReplicationMessage::Error { code, message } => {
                return Err(Error::replication(format!(
                    "Master rejected: {} (code {})",
                    message, code
                )));
            }
            _ => {
                return Err(Error::replication("Expected Welcome message"));
            }
        };

        tracing::info!(
            "Connected to master {} (offset: {}, full_sync: {})",
            master_id,
            master_offset,
            requires_full_sync
        );

        self.connected.store(true, Ordering::SeqCst);
        self.master_offset.store(master_offset, Ordering::SeqCst);
        self.missed_heartbeats.store(0, Ordering::SeqCst);

        {
            let mut stats = self.stats.lock();
            stats.master_id = Some(master_id.clone());
            stats.connected = true;
            stats.last_sync_time = Some(Instant::now());
        }

        // Handle full sync if needed
        if requires_full_sync {
            tracing::info!("Receiving full sync from master");
            self.receive_snapshot(&mut stream).await?;
        }

        // Start receiving WAL entries
        self.receive_wal_entries(&mut stream).await
    }

    /// Receive snapshot from master
    async fn receive_snapshot(&self, stream: &mut TcpStream) -> Result<()> {
        // Read snapshot metadata
        let meta = ReplicationMessage::read_from(stream).await?;
        let (snapshot_id, total_size, chunk_count, expected_checksum, wal_offset) = match meta {
            ReplicationMessage::SnapshotMeta {
                snapshot_id,
                total_size,
                chunk_count,
                checksum,
                wal_offset,
            } => (snapshot_id, total_size, chunk_count, checksum, wal_offset),
            _ => {
                return Err(Error::replication("Expected SnapshotMeta message"));
            }
        };

        tracing::info!(
            "Receiving snapshot {} ({} bytes, {} chunks)",
            snapshot_id,
            total_size,
            chunk_count
        );

        // Receive chunks
        let mut data = Vec::with_capacity(total_size as usize);
        for expected_index in 0..chunk_count {
            let chunk = ReplicationMessage::read_from(stream).await?;
            match chunk {
                ReplicationMessage::SnapshotChunk {
                    snapshot_id: chunk_snap_id,
                    chunk_index,
                    data: chunk_data,
                    checksum,
                } => {
                    if chunk_snap_id != snapshot_id {
                        return Err(Error::replication("Snapshot ID mismatch"));
                    }
                    if chunk_index != expected_index {
                        return Err(Error::replication(format!(
                            "Chunk index mismatch: expected {}, got {}",
                            expected_index, chunk_index
                        )));
                    }
                    let computed_checksum = crc32fast::hash(&chunk_data);
                    if computed_checksum != checksum {
                        return Err(Error::replication("Chunk checksum mismatch"));
                    }
                    data.extend_from_slice(&chunk_data);
                }
                _ => {
                    return Err(Error::replication("Expected SnapshotChunk message"));
                }
            }
        }

        // Verify total checksum
        let computed_checksum = crc32fast::hash(&data);
        if computed_checksum != expected_checksum {
            return Err(Error::replication("Snapshot checksum mismatch"));
        }

        // Read completion message
        let complete = ReplicationMessage::read_from(stream).await?;
        match complete {
            ReplicationMessage::SnapshotComplete { success, .. } => {
                if !success {
                    return Err(Error::replication("Snapshot transfer failed on master"));
                }
            }
            _ => {
                return Err(Error::replication("Expected SnapshotComplete message"));
            }
        }

        // Apply snapshot
        self.snapshot.restore(&data).await?;
        self.current_offset.store(wal_offset, Ordering::SeqCst);

        tracing::info!(
            "Snapshot {} applied successfully, offset now {}",
            snapshot_id,
            wal_offset
        );

        {
            let mut stats = self.stats.lock();
            stats.bytes_received += data.len() as u64;
            stats.current_offset = wal_offset;
        }

        Ok(())
    }

    /// Receive WAL entries from master
    async fn receive_wal_entries(&self, stream: &mut TcpStream) -> Result<()> {
        let heartbeat_timeout =
            Duration::from_millis(DEFAULT_HEARTBEAT_MS * (MISSED_HEARTBEATS_THRESHOLD as u64 + 1));

        loop {
            let result =
                tokio::time::timeout(heartbeat_timeout, ReplicationMessage::read_from(stream))
                    .await;

            let msg = match result {
                Ok(Ok(msg)) => msg,
                Ok(Err(e)) => {
                    return Err(e);
                }
                Err(_) => {
                    // Timeout - missed heartbeats
                    let missed = self.missed_heartbeats.fetch_add(1, Ordering::SeqCst) + 1;
                    if missed >= self.config.missed_heartbeats_threshold {
                        tracing::warn!("Master appears dead ({} missed heartbeats)", missed);
                        if self.config.auto_failover {
                            return Err(Error::replication("Master dead, triggering failover"));
                        }
                    }
                    continue;
                }
            };

            // Reset missed heartbeats on any message
            self.missed_heartbeats.store(0, Ordering::SeqCst);

            match msg {
                ReplicationMessage::WalEntry {
                    offset,
                    epoch,
                    entry,
                } => {
                    // Apply entry
                    if let Some(ref callback) = self.apply_callback {
                        if let Err(e) = callback(entry.clone(), epoch) {
                            tracing::error!(
                                "Failed to apply WAL entry at offset {}: {}",
                                offset,
                                e
                            );
                            // Continue anyway - we'll get out of sync but can recover later
                        }
                    }

                    self.current_offset.store(offset, Ordering::SeqCst);

                    {
                        let mut stats = self.stats.lock();
                        stats.entries_received += 1;
                        stats.entries_applied += 1;
                        stats.current_offset = offset;
                        stats.lag = self
                            .master_offset
                            .load(Ordering::SeqCst)
                            .saturating_sub(offset);
                    }

                    // Send ACK if in sync mode
                    // Note: For now, we always send ACKs
                    let ack = ReplicationMessage::WalAck {
                        offset,
                        success: true,
                    };
                    ack.write_to(stream).await?;
                }

                ReplicationMessage::Ping { timestamp } => {
                    // Respond with Pong
                    let pong = ReplicationMessage::Pong { timestamp };
                    pong.write_to(stream).await?;
                }

                ReplicationMessage::Pong { .. } => {
                    // Master responded to our ping (if we sent one)
                }

                ReplicationMessage::Error { code, message } => {
                    return Err(Error::replication(format!(
                        "Master error: {} (code {})",
                        message, code
                    )));
                }

                _ => {
                    tracing::warn!("Unexpected message type from master");
                }
            }
        }
    }

    /// Stop the replica node
    pub fn stop(&self) {
        if let Some(tx) = self.shutdown_tx.lock().take() {
            let _ = tx.send(());
        }
    }

    /// Promote replica to master
    pub async fn promote(&self) -> Result<()> {
        if !self.connected.load(Ordering::SeqCst) {
            // Already disconnected from master, can promote
        }

        if let Some(tx) = self.promote_tx.lock().take() {
            let _ = tx.send(()).await;
        }

        tracing::info!("Replica {} promoted to master", self.id);
        Ok(())
    }

    /// Get statistics
    pub fn stats(&self) -> ReplicaStats {
        let mut stats = self.stats.lock().clone();
        stats.current_offset = self.current_offset();
        stats.lag = self.lag();
        stats.connected = self.is_connected();
        stats
    }

    /// Check if replica is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Get missed heartbeats count
    pub fn missed_heartbeats(&self) -> u32 {
        self.missed_heartbeats.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> ReplicationConfig {
        ReplicationConfig::replica("127.0.0.1:15475".parse().unwrap())
    }

    #[test]
    fn test_replica_creation() {
        let config = create_test_config();
        let snapshot = Arc::new(Snapshot::new(Default::default()));
        let replica = Replica::new(config, snapshot);

        assert_eq!(replica.current_offset(), 0);
        assert!(!replica.is_running());
        assert!(!replica.is_connected());
    }

    #[test]
    fn test_replica_offset() {
        let config = create_test_config();
        let snapshot = Arc::new(Snapshot::new(Default::default()));
        let replica = Replica::new(config, snapshot);

        replica.set_offset(1000);
        assert_eq!(replica.current_offset(), 1000);
    }

    #[test]
    fn test_replica_lag() {
        let config = create_test_config();
        let snapshot = Arc::new(Snapshot::new(Default::default()));
        let replica = Replica::new(config, snapshot);

        replica.set_offset(100);
        replica.master_offset.store(200, Ordering::SeqCst);

        assert_eq!(replica.lag(), 100);
    }

    #[test]
    fn test_replica_stats() {
        let config = create_test_config();
        let snapshot = Arc::new(Snapshot::new(Default::default()));
        let replica = Replica::new(config, snapshot);

        let stats = replica.stats();
        assert_eq!(stats.entries_received, 0);
        assert_eq!(stats.reconnects, 0);
        assert!(!stats.connected);
    }

    #[test]
    fn test_apply_callback() {
        let config = create_test_config();
        let snapshot = Arc::new(Snapshot::new(Default::default()));
        let applied = Arc::new(AtomicU64::new(0));
        let applied_clone = applied.clone();

        let callback: ApplyCallback = Arc::new(move |_entry, _epoch| {
            applied_clone.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });

        let replica = Replica::new(config, snapshot).with_apply_callback(callback);
        assert!(replica.apply_callback.is_some());
    }
}
