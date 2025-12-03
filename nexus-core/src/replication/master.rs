//! Master node implementation for replication
//!
//! The master node:
//! - Accepts connections from replicas
//! - Streams WAL entries to connected replicas
//! - Manages replication log (circular buffer)
//! - Handles sync/async replication modes
//! - Monitors replica health

use crate::replication::config::{ReplicationConfig, ReplicationMode};
use crate::replication::protocol::{PROTOCOL_VERSION, ReplicationMessage};
use crate::replication::snapshot::Snapshot;
use crate::replication::{LAG_WARNING_THRESHOLD, MAX_REPLICATION_LOG_SIZE};
use crate::wal::WalEntry;
use crate::{Error, Result};
use parking_lot::{Mutex, RwLock};
use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc, oneshot};
use uuid::Uuid;

/// Information about a connected replica
#[derive(Debug, Clone)]
pub struct ReplicaInfo {
    /// Replica ID
    pub id: String,
    /// Replica address
    pub addr: SocketAddr,
    /// Last acknowledged WAL offset
    pub last_ack_offset: u64,
    /// Replication lag (in operations)
    pub lag: u64,
    /// Last heartbeat time
    pub last_heartbeat: Instant,
    /// Connection time
    pub connected_at: Instant,
    /// Is replica healthy
    pub healthy: bool,
}

/// Master node statistics
#[derive(Debug, Clone, Default)]
pub struct MasterStats {
    /// Total WAL entries replicated
    pub entries_replicated: u64,
    /// Total bytes sent to replicas
    pub bytes_sent: u64,
    /// Number of connected replicas
    pub connected_replicas: u32,
    /// Number of healthy replicas
    pub healthy_replicas: u32,
    /// Current replication log size
    pub log_size: usize,
    /// Current WAL offset
    pub current_offset: u64,
    /// Number of sync ACKs received
    pub sync_acks: u64,
    /// Number of snapshot transfers
    pub snapshot_transfers: u64,
}

/// Replication log entry
#[derive(Debug, Clone)]
struct ReplicationLogEntry {
    offset: u64,
    epoch: u64,
    entry: WalEntry,
    timestamp: Instant,
}

/// Master node for replication
pub struct Master {
    /// Master ID
    id: String,
    /// Configuration
    config: ReplicationConfig,
    /// Replication log (circular buffer)
    replication_log: RwLock<VecDeque<ReplicationLogEntry>>,
    /// Current WAL offset
    current_offset: AtomicU64,
    /// Current epoch
    current_epoch: AtomicU64,
    /// Connected replicas
    replicas: RwLock<HashMap<String, ReplicaInfo>>,
    /// Statistics
    stats: Mutex<MasterStats>,
    /// Broadcast channel for new WAL entries
    entry_sender: broadcast::Sender<ReplicationLogEntry>,
    /// Running flag
    running: AtomicBool,
    /// Shutdown signal sender
    shutdown_tx: Mutex<Option<oneshot::Sender<()>>>,
    /// Snapshot manager
    snapshot: Arc<Snapshot>,
}

impl Master {
    /// Create a new master node
    pub fn new(config: ReplicationConfig, snapshot: Arc<Snapshot>) -> Self {
        let (entry_sender, _) = broadcast::channel(10_000);

        Self {
            id: Uuid::new_v4().to_string(),
            config,
            replication_log: RwLock::new(VecDeque::with_capacity(MAX_REPLICATION_LOG_SIZE)),
            current_offset: AtomicU64::new(0),
            current_epoch: AtomicU64::new(0),
            replicas: RwLock::new(HashMap::new()),
            stats: Mutex::new(MasterStats::default()),
            entry_sender,
            running: AtomicBool::new(false),
            shutdown_tx: Mutex::new(None),
            snapshot,
        }
    }

    /// Get master ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get current WAL offset
    pub fn current_offset(&self) -> u64 {
        self.current_offset.load(Ordering::SeqCst)
    }

    /// Get current epoch
    pub fn current_epoch(&self) -> u64 {
        self.current_epoch.load(Ordering::SeqCst)
    }

    /// Set current epoch
    pub fn set_epoch(&self, epoch: u64) {
        self.current_epoch.store(epoch, Ordering::SeqCst);
    }

    /// Add WAL entry to replication log
    ///
    /// For async mode: returns immediately
    /// For sync mode: waits for quorum ACKs
    pub async fn replicate(&self, entry: WalEntry, epoch: u64) -> Result<u64> {
        let offset = self.current_offset.fetch_add(1, Ordering::SeqCst);

        let log_entry = ReplicationLogEntry {
            offset,
            epoch,
            entry,
            timestamp: Instant::now(),
        };

        // Add to replication log
        {
            let mut log = self.replication_log.write();
            if log.len() >= self.config.max_log_size {
                log.pop_front();
            }
            log.push_back(log_entry.clone());
        }

        // Update stats
        {
            let mut stats = self.stats.lock();
            stats.entries_replicated += 1;
            stats.current_offset = offset;
            stats.log_size = self.replication_log.read().len();
        }

        // Broadcast to replicas
        let _ = self.entry_sender.send(log_entry);

        // For sync mode, wait for quorum
        if self.config.mode == ReplicationMode::Sync {
            self.wait_for_quorum(offset).await?;
        }

        Ok(offset)
    }

    /// Wait for quorum of replicas to acknowledge
    async fn wait_for_quorum(&self, offset: u64) -> Result<()> {
        let quorum = self.config.sync_quorum as usize;
        let timeout = self.config.write_timeout;
        let start = Instant::now();

        loop {
            let ack_count = self
                .replicas
                .read()
                .values()
                .filter(|r| r.last_ack_offset >= offset && r.healthy)
                .count();

            // +1 for master itself
            if ack_count + 1 >= quorum {
                return Ok(());
            }

            if start.elapsed() > timeout {
                return Err(Error::replication(format!(
                    "Quorum timeout: got {} ACKs, need {}",
                    ack_count + 1,
                    quorum
                )));
            }

            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Start the master node
    pub async fn start(&self) -> Result<()> {
        if self.running.swap(true, Ordering::SeqCst) {
            return Err(Error::replication("Master already running"));
        }

        let listener = TcpListener::bind(self.config.bind_addr).await?;
        tracing::info!("Master {} listening on {}", self.id, self.config.bind_addr);

        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
        *self.shutdown_tx.lock() = Some(shutdown_tx);

        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((stream, addr)) => {
                            self.handle_replica_connection(stream, addr).await;
                        }
                        Err(e) => {
                            tracing::error!("Accept error: {}", e);
                        }
                    }
                }
                _ = &mut shutdown_rx => {
                    tracing::info!("Master shutting down");
                    break;
                }
            }
        }

        self.running.store(false, Ordering::SeqCst);
        Ok(())
    }

    /// Stop the master node
    pub fn stop(&self) {
        if let Some(tx) = self.shutdown_tx.lock().take() {
            let _ = tx.send(());
        }
    }

    /// Handle a new replica connection
    async fn handle_replica_connection(&self, mut stream: TcpStream, addr: SocketAddr) {
        tracing::info!("Replica connected from {}", addr);

        // Read Hello message
        let hello = match ReplicationMessage::read_from(&mut stream).await {
            Ok(msg) => msg,
            Err(e) => {
                tracing::error!("Failed to read Hello from {}: {}", addr, e);
                return;
            }
        };

        let (replica_id, last_offset) = match hello {
            ReplicationMessage::Hello {
                replica_id,
                last_wal_offset,
                protocol_version,
            } => {
                if protocol_version != PROTOCOL_VERSION {
                    let error = ReplicationMessage::Error {
                        code: 1,
                        message: format!(
                            "Protocol version mismatch: expected {}, got {}",
                            PROTOCOL_VERSION, protocol_version
                        ),
                    };
                    let _ = error.write_to(&mut stream).await;
                    return;
                }
                (replica_id, last_wal_offset)
            }
            _ => {
                tracing::error!("Expected Hello message from {}", addr);
                return;
            }
        };

        // Check if full sync is needed
        let current_offset = self.current_offset();
        let log = self.replication_log.read();
        let oldest_offset = log.front().map(|e| e.offset).unwrap_or(current_offset);
        let requires_full_sync = last_offset < oldest_offset;
        drop(log);

        // Send Welcome
        let welcome = ReplicationMessage::Welcome {
            master_id: self.id.clone(),
            current_wal_offset: current_offset,
            requires_full_sync,
        };

        if let Err(e) = welcome.write_to(&mut stream).await {
            tracing::error!("Failed to send Welcome to {}: {}", addr, e);
            return;
        }

        // Register replica
        let replica_info = ReplicaInfo {
            id: replica_id.clone(),
            addr,
            last_ack_offset: last_offset,
            lag: current_offset.saturating_sub(last_offset),
            last_heartbeat: Instant::now(),
            connected_at: Instant::now(),
            healthy: true,
        };

        self.replicas
            .write()
            .insert(replica_id.clone(), replica_info);
        self.update_replica_count();

        // Handle full sync if needed
        if requires_full_sync {
            tracing::info!("Starting full sync for replica {}", replica_id);
            if let Err(e) = self.send_snapshot(&mut stream, &replica_id).await {
                tracing::error!("Snapshot transfer failed for {}: {}", replica_id, e);
                self.replicas.write().remove(&replica_id);
                self.update_replica_count();
                return;
            }
        }

        // Start streaming WAL entries
        self.stream_wal_entries(stream, replica_id.clone(), last_offset)
            .await;

        // Cleanup on disconnect
        self.replicas.write().remove(&replica_id);
        self.update_replica_count();
        tracing::info!("Replica {} disconnected", replica_id);
    }

    /// Stream WAL entries to a replica
    async fn stream_wal_entries(
        &self,
        mut stream: TcpStream,
        replica_id: String,
        mut last_offset: u64,
    ) {
        // Subscribe to new entries
        let mut entry_rx = self.entry_sender.subscribe();

        // First, send any entries we have in the log that are after last_offset
        {
            let log = self.replication_log.read();
            for entry in log.iter() {
                if entry.offset > last_offset {
                    let msg = ReplicationMessage::WalEntry {
                        offset: entry.offset,
                        epoch: entry.epoch,
                        entry: entry.entry.clone(),
                    };
                    if let Err(e) = msg.write_to(&mut stream).await {
                        tracing::error!("Failed to send WAL entry to {}: {}", replica_id, e);
                        return;
                    }
                    last_offset = entry.offset;
                }
            }
        }

        // Now stream new entries as they arrive
        let heartbeat_interval = self.config.heartbeat_interval;
        let mut heartbeat_timer = tokio::time::interval(heartbeat_interval);

        loop {
            tokio::select! {
                result = entry_rx.recv() => {
                    match result {
                        Ok(entry) => {
                            if entry.offset > last_offset {
                                let msg = ReplicationMessage::WalEntry {
                                    offset: entry.offset,
                                    epoch: entry.epoch,
                                    entry: entry.entry,
                                };
                                if let Err(e) = msg.write_to(&mut stream).await {
                                    tracing::error!("Failed to send WAL entry to {}: {}", replica_id, e);
                                    return;
                                }
                                last_offset = entry.offset;

                                // Update stats
                                let mut stats = self.stats.lock();
                                stats.bytes_sent += 100; // Approximate
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!("Replica {} lagged by {} entries", replica_id, n);
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            tracing::info!("Entry channel closed");
                            return;
                        }
                    }
                }

                _ = heartbeat_timer.tick() => {
                    let ping = ReplicationMessage::Ping {
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_millis() as u64,
                    };
                    if let Err(e) = ping.write_to(&mut stream).await {
                        tracing::error!("Failed to send heartbeat to {}: {}", replica_id, e);
                        return;
                    }
                }

                // TODO: Read ACKs from replica
            }
        }
    }

    /// Send snapshot to replica
    async fn send_snapshot(&self, stream: &mut TcpStream, replica_id: &str) -> Result<()> {
        // Create snapshot
        let snapshot_data = self.snapshot.create().await?;
        let checksum = crc32fast::hash(&snapshot_data);
        let chunk_size = 1024 * 1024; // 1MB chunks
        let chunk_count = (snapshot_data.len() + chunk_size - 1) / chunk_size;
        let snapshot_id = Uuid::new_v4().to_string();

        // Send metadata
        let meta = ReplicationMessage::SnapshotMeta {
            snapshot_id: snapshot_id.clone(),
            total_size: snapshot_data.len() as u64,
            chunk_count: chunk_count as u32,
            checksum,
            wal_offset: self.current_offset(),
        };
        meta.write_to(stream).await?;

        // Send chunks
        for (i, chunk) in snapshot_data.chunks(chunk_size).enumerate() {
            let chunk_checksum = crc32fast::hash(chunk);
            let chunk_msg = ReplicationMessage::SnapshotChunk {
                snapshot_id: snapshot_id.clone(),
                chunk_index: i as u32,
                data: chunk.to_vec(),
                checksum: chunk_checksum,
            };
            chunk_msg.write_to(stream).await?;
        }

        // Send complete
        let complete = ReplicationMessage::SnapshotComplete {
            snapshot_id,
            success: true,
        };
        complete.write_to(stream).await?;

        // Update stats
        {
            let mut stats = self.stats.lock();
            stats.snapshot_transfers += 1;
            stats.bytes_sent += snapshot_data.len() as u64;
        }

        tracing::info!(
            "Snapshot sent to replica {} ({} bytes)",
            replica_id,
            snapshot_data.len()
        );

        Ok(())
    }

    /// Update replica ACK
    pub fn update_replica_ack(&self, replica_id: &str, offset: u64) {
        if let Some(replica) = self.replicas.write().get_mut(replica_id) {
            replica.last_ack_offset = offset;
            replica.lag = self.current_offset().saturating_sub(offset);
            replica.last_heartbeat = Instant::now();

            if replica.lag > LAG_WARNING_THRESHOLD {
                tracing::warn!(
                    "Replica {} lag warning: {} operations behind",
                    replica_id,
                    replica.lag
                );
            }
        }

        let mut stats = self.stats.lock();
        stats.sync_acks += 1;
    }

    /// Update replica count stats
    fn update_replica_count(&self) {
        let replicas = self.replicas.read();
        let mut stats = self.stats.lock();
        stats.connected_replicas = replicas.len() as u32;
        stats.healthy_replicas = replicas.values().filter(|r| r.healthy).count() as u32;
    }

    /// Get replica info
    pub fn get_replica(&self, replica_id: &str) -> Option<ReplicaInfo> {
        self.replicas.read().get(replica_id).cloned()
    }

    /// Get all replicas
    pub fn get_replicas(&self) -> Vec<ReplicaInfo> {
        self.replicas.read().values().cloned().collect()
    }

    /// Get statistics
    pub fn stats(&self) -> MasterStats {
        self.stats.lock().clone()
    }

    /// Get entry from replication log
    pub fn get_entry(&self, offset: u64) -> Option<WalEntry> {
        let log = self.replication_log.read();
        log.iter()
            .find(|e| e.offset == offset)
            .map(|e| e.entry.clone())
    }

    /// Get entries from replication log starting at offset
    pub fn get_entries_from(&self, offset: u64, limit: usize) -> Vec<(u64, WalEntry)> {
        let log = self.replication_log.read();
        log.iter()
            .filter(|e| e.offset >= offset)
            .take(limit)
            .map(|e| (e.offset, e.entry.clone()))
            .collect()
    }

    /// Check if master is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Get list of connected replicas
    pub fn replicas(&self) -> Vec<ReplicaInfo> {
        self.replicas.read().values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> ReplicationConfig {
        ReplicationConfig::master("127.0.0.1:0".parse().unwrap())
    }

    #[test]
    fn test_master_creation() {
        let config = create_test_config();
        let snapshot = Arc::new(Snapshot::new(Default::default()));
        let master = Master::new(config, snapshot);

        assert_eq!(master.current_offset(), 0);
        assert!(!master.is_running());
    }

    #[tokio::test]
    async fn test_replicate_async() {
        let config = create_test_config();
        let snapshot = Arc::new(Snapshot::new(Default::default()));
        let master = Master::new(config, snapshot);

        let entry = WalEntry::CreateNode {
            node_id: 42,
            label_bits: 7,
        };

        let offset = master.replicate(entry, 1).await.unwrap();
        assert_eq!(offset, 0);
        assert_eq!(master.current_offset(), 1);

        let stats = master.stats();
        assert_eq!(stats.entries_replicated, 1);
    }

    #[tokio::test]
    async fn test_replicate_multiple() {
        let config = create_test_config();
        let snapshot = Arc::new(Snapshot::new(Default::default()));
        let master = Master::new(config, snapshot);

        for i in 0..10 {
            let entry = WalEntry::CreateNode {
                node_id: i,
                label_bits: 0,
            };
            master.replicate(entry, 1).await.unwrap();
        }

        assert_eq!(master.current_offset(), 10);

        let stats = master.stats();
        assert_eq!(stats.entries_replicated, 10);
        assert_eq!(stats.log_size, 10);
    }

    #[test]
    fn test_get_entries() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let config = create_test_config();
            let snapshot = Arc::new(Snapshot::new(Default::default()));
            let master = Master::new(config, snapshot);

            for i in 0..5 {
                let entry = WalEntry::CreateNode {
                    node_id: i,
                    label_bits: 0,
                };
                master.replicate(entry, 1).await.unwrap();
            }

            let entries = master.get_entries_from(2, 10);
            assert_eq!(entries.len(), 3); // offsets 2, 3, 4
        });
    }

    #[test]
    fn test_replication_log_circular() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut config = create_test_config();
            config.max_log_size = 5;
            let snapshot = Arc::new(Snapshot::new(Default::default()));
            let master = Master::new(config, snapshot);

            for i in 0..10 {
                let entry = WalEntry::CreateNode {
                    node_id: i,
                    label_bits: 0,
                };
                master.replicate(entry, 1).await.unwrap();
            }

            let stats = master.stats();
            assert_eq!(stats.log_size, 5); // Should be capped at 5
        });
    }
}
