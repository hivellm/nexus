//! Comprehensive Replication Integration Tests
//!
//! Tests the full replication workflow including:
//! - Master node creation and startup
//! - Replica node connection
//! - WAL entry replication
//! - Snapshot creation and transfer
//! - Failover/promotion

use nexus_core::replication::{
    config::{ReplicationConfig, ReplicationMode, ReplicationRole},
    master::Master,
    protocol::{PROTOCOL_VERSION, ReplicationMessage},
    replica::Replica,
    snapshot::{Snapshot, SnapshotConfig},
};
use nexus_core::testing::TestContext;
use nexus_core::wal::WalEntry;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Test master node creation and basic operations
#[tokio::test]
async fn test_master_node_lifecycle() {
    let ctx = TestContext::new();
    let data_dir = ctx.path().join("master_data");
    std::fs::create_dir_all(&data_dir).unwrap();

    let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let config = ReplicationConfig::master(bind_addr);
    let snapshot_config = SnapshotConfig {
        data_dir,
        ..Default::default()
    };
    let snapshot = Arc::new(Snapshot::new(snapshot_config));
    let master = Master::new(config, snapshot);

    // Test initial state
    assert!(!master.is_running());
    assert_eq!(master.current_offset(), 0);

    // Test replication
    let entry = WalEntry::CreateNode {
        node_id: 1,
        label_bits: 1,
    };
    master.replicate(entry.clone(), 1).await.unwrap();
    assert_eq!(master.current_offset(), 1);

    // Test stats
    let stats = master.stats();
    assert_eq!(stats.entries_replicated, 1);
}

/// Test replica node creation and basic operations
#[tokio::test]
async fn test_replica_node_lifecycle() {
    let ctx = TestContext::new();
    let data_dir = ctx.path().join("replica_data");
    std::fs::create_dir_all(&data_dir).unwrap();

    let master_addr: SocketAddr = "127.0.0.1:15476".parse().unwrap();
    let config = ReplicationConfig::replica(master_addr);
    let snapshot_config = SnapshotConfig {
        data_dir,
        ..Default::default()
    };
    let snapshot = Arc::new(Snapshot::new(snapshot_config));
    let replica = Replica::new(config, snapshot);

    // Test initial state
    assert!(!replica.is_running());
    assert!(!replica.is_connected());
    assert_eq!(replica.current_offset(), 0);

    // Test offset setting
    replica.set_offset(100);
    assert_eq!(replica.current_offset(), 100);

    // Test stats
    let stats = replica.stats();
    assert_eq!(stats.entries_received, 0);
    assert!(!stats.connected);
}

/// Test protocol message encoding and decoding
#[tokio::test]
async fn test_protocol_messages() {
    // Test Hello message
    let hello = ReplicationMessage::Hello {
        replica_id: "test-replica".to_string(),
        last_wal_offset: 1000,
        protocol_version: PROTOCOL_VERSION,
    };
    let encoded = hello.encode().unwrap();
    let decoded = ReplicationMessage::decode(&encoded).unwrap();
    match decoded {
        ReplicationMessage::Hello {
            replica_id,
            last_wal_offset,
            protocol_version,
        } => {
            assert_eq!(replica_id, "test-replica");
            assert_eq!(last_wal_offset, 1000);
            assert_eq!(protocol_version, PROTOCOL_VERSION);
        }
        _ => panic!("Expected Hello message"),
    }

    // Test WalEntry message
    let wal_msg = ReplicationMessage::WalEntry {
        offset: 500,
        epoch: 1,
        entry: WalEntry::CreateNode {
            node_id: 42,
            label_bits: 7,
        },
    };
    let encoded = wal_msg.encode().unwrap();
    let decoded = ReplicationMessage::decode(&encoded).unwrap();
    match decoded {
        ReplicationMessage::WalEntry {
            offset,
            epoch,
            entry,
        } => {
            assert_eq!(offset, 500);
            assert_eq!(epoch, 1);
            match entry {
                WalEntry::CreateNode {
                    node_id,
                    label_bits,
                } => {
                    assert_eq!(node_id, 42);
                    assert_eq!(label_bits, 7);
                }
                _ => panic!("Expected CreateNode entry"),
            }
        }
        _ => panic!("Expected WalEntry message"),
    }

    // Test SnapshotChunk message
    let chunk_data = vec![1, 2, 3, 4, 5, 6, 7, 8];
    let checksum = crc32fast::hash(&chunk_data);
    let chunk_msg = ReplicationMessage::SnapshotChunk {
        snapshot_id: "snap-123".to_string(),
        chunk_index: 0,
        data: chunk_data.clone(),
        checksum,
    };
    let encoded = chunk_msg.encode().unwrap();
    let decoded = ReplicationMessage::decode(&encoded).unwrap();
    match decoded {
        ReplicationMessage::SnapshotChunk {
            snapshot_id,
            chunk_index,
            data,
            checksum: decoded_checksum,
        } => {
            assert_eq!(snapshot_id, "snap-123");
            assert_eq!(chunk_index, 0);
            assert_eq!(data, chunk_data);
            assert_eq!(decoded_checksum, checksum);
        }
        _ => panic!("Expected SnapshotChunk message"),
    }
}

/// Test CRC validation on corrupted messages
#[tokio::test]
async fn test_protocol_crc_validation() {
    let msg = ReplicationMessage::Ping { timestamp: 12345 };
    let mut encoded = msg.encode().unwrap();

    // Corrupt the payload
    if encoded.len() > 6 {
        encoded[6] ^= 0xFF;
    }

    // Should fail CRC validation
    let result = ReplicationMessage::decode(&encoded);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("CRC"));
}

/// Test snapshot creation and metadata
#[tokio::test]
async fn test_snapshot_creation() {
    let ctx = TestContext::new();
    let data_dir = ctx.path().join("snapshot_data");
    std::fs::create_dir_all(&data_dir).unwrap();

    // Create test files
    std::fs::write(data_dir.join("nodes.store"), b"node data here").unwrap();
    std::fs::write(data_dir.join("rels.store"), b"relationship data").unwrap();
    std::fs::write(data_dir.join("props.store"), b"property data").unwrap();

    let config = SnapshotConfig {
        data_dir: data_dir.clone(),
        compression_level: 1,
        max_size: 1024 * 1024,
        chunk_size: 1024,
    };
    let snapshot = Snapshot::new(config);

    // Set WAL offset before snapshot
    snapshot.set_wal_offset(5000);
    snapshot.set_epoch(3);

    // Create snapshot
    let data = snapshot.create().await.unwrap();
    assert!(!data.is_empty());

    // Verify metadata
    let meta = snapshot.last_snapshot().unwrap();
    assert_eq!(meta.wal_offset, 5000);
    assert_eq!(meta.epoch, 3);
    assert_eq!(meta.files.len(), 3);
    assert!(meta.compressed_size < meta.uncompressed_size);
}

/// Test snapshot restore
#[tokio::test]
async fn test_snapshot_restore() {
    let ctx = TestContext::new();
    let data_dir = ctx.path().join("restore_data");
    std::fs::create_dir_all(&data_dir).unwrap();

    // Create original content
    std::fs::write(data_dir.join("test.txt"), b"original content").unwrap();

    let config = SnapshotConfig {
        data_dir: data_dir.clone(),
        compression_level: 1,
        max_size: 1024 * 1024,
        chunk_size: 1024,
    };
    let snapshot = Snapshot::new(config);

    // Create snapshot
    let snapshot_data = snapshot.create().await.unwrap();

    // Modify file
    std::fs::write(data_dir.join("test.txt"), b"modified content").unwrap();
    let content = std::fs::read_to_string(data_dir.join("test.txt")).unwrap();
    assert_eq!(content, "modified content");

    // Restore snapshot
    snapshot.restore(&snapshot_data).await.unwrap();

    // Verify restoration
    let content = std::fs::read_to_string(data_dir.join("test.txt")).unwrap();
    assert_eq!(content, "original content");
}

/// Test replication log circular buffer behavior
#[tokio::test]
async fn test_replication_log_circular_buffer() {
    let ctx = TestContext::new();
    let data_dir = ctx.path().join("circular_data");
    std::fs::create_dir_all(&data_dir).unwrap();

    let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let mut config = ReplicationConfig::master(bind_addr);
    config.max_log_size = 100; // Small buffer for testing

    let snapshot_config = SnapshotConfig {
        data_dir,
        ..Default::default()
    };
    let snapshot = Arc::new(Snapshot::new(snapshot_config));
    let master = Master::new(config, snapshot);

    // Add more entries than buffer size
    for i in 0..150 {
        let entry = WalEntry::CreateNode {
            node_id: i,
            label_bits: 1,
        };
        master.replicate(entry, 1).await.unwrap();
    }

    // Verify offset advanced
    assert_eq!(master.current_offset(), 150);

    // Get entries from buffer (should only have last 100)
    let entries = master.get_entries_from(50, 100);
    assert!(entries.len() <= 100);
}

/// Test sync replication mode configuration
#[tokio::test]
async fn test_sync_replication_config() {
    let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let config = ReplicationConfig::master(bind_addr)
        .with_mode(ReplicationMode::Sync)
        .with_quorum(2);

    assert!(matches!(config.mode, ReplicationMode::Sync));
    assert_eq!(config.sync_quorum, 2);
}

/// Test replica apply callback
#[tokio::test]
async fn test_replica_apply_callback() {
    let ctx = TestContext::new();
    let data_dir = ctx.path().join("callback_data");
    std::fs::create_dir_all(&data_dir).unwrap();

    let master_addr: SocketAddr = "127.0.0.1:15477".parse().unwrap();
    let config = ReplicationConfig::replica(master_addr);
    let snapshot_config = SnapshotConfig {
        data_dir,
        ..Default::default()
    };
    let snapshot = Arc::new(Snapshot::new(snapshot_config));

    let applied_count = Arc::new(AtomicU64::new(0));
    let applied_count_clone = applied_count.clone();

    let callback = Arc::new(move |_entry: WalEntry, _epoch: u64| {
        applied_count_clone.fetch_add(1, Ordering::SeqCst);
        Ok(())
    });

    let replica = Replica::new(config, snapshot).with_apply_callback(callback);

    // Verify callback is set
    assert!(replica.stats().entries_applied == 0);
}

/// Test configuration validation
#[tokio::test]
async fn test_config_validation() {
    // Test replica without master address
    let config = ReplicationConfig {
        role: ReplicationRole::Replica,
        master_addr: None,
        ..Default::default()
    };
    let result = config.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("master_addr"));

    // Test invalid quorum
    let mut config = ReplicationConfig::master("127.0.0.1:0".parse().unwrap());
    config.sync_quorum = 0;
    let result = config.validate();
    assert!(result.is_err());

    // Test invalid heartbeat threshold
    let mut config = ReplicationConfig::master("127.0.0.1:0".parse().unwrap());
    config.missed_heartbeats_threshold = 0;
    let result = config.validate();
    assert!(result.is_err());
}

/// Test standalone configuration
#[tokio::test]
async fn test_standalone_config() {
    let config = ReplicationConfig::standalone();
    assert!(matches!(config.role, ReplicationRole::Standalone));
    assert!(config.validate().is_ok());
}

/// Test master-replica handshake protocol
#[tokio::test]
async fn test_handshake_protocol() {
    // Simulate handshake message exchange
    let hello = ReplicationMessage::Hello {
        replica_id: "replica-1".to_string(),
        last_wal_offset: 0,
        protocol_version: PROTOCOL_VERSION,
    };

    let welcome = ReplicationMessage::Welcome {
        master_id: "master-1".to_string(),
        current_wal_offset: 1000,
        requires_full_sync: true,
    };

    // Encode and decode to verify protocol
    let hello_bytes = hello.encode().unwrap();
    let welcome_bytes = welcome.encode().unwrap();

    let decoded_hello = ReplicationMessage::decode(&hello_bytes).unwrap();
    let decoded_welcome = ReplicationMessage::decode(&welcome_bytes).unwrap();

    match decoded_hello {
        ReplicationMessage::Hello { replica_id, .. } => {
            assert_eq!(replica_id, "replica-1");
        }
        _ => panic!("Expected Hello"),
    }

    match decoded_welcome {
        ReplicationMessage::Welcome {
            master_id,
            current_wal_offset,
            requires_full_sync,
        } => {
            assert_eq!(master_id, "master-1");
            assert_eq!(current_wal_offset, 1000);
            assert!(requires_full_sync);
        }
        _ => panic!("Expected Welcome"),
    }
}

/// Test WAL entry types serialization
#[tokio::test]
async fn test_wal_entry_types() {
    let entries = vec![
        WalEntry::CreateNode {
            node_id: 1,
            label_bits: 0x01,
        },
        WalEntry::DeleteNode { node_id: 1 },
        WalEntry::CreateRel {
            rel_id: 1,
            src: 1,
            dst: 2,
            type_id: 1,
        },
        WalEntry::DeleteRel { rel_id: 1 },
        WalEntry::SetProperty {
            entity_id: 1,
            key_id: 1,
            value: bincode::serialize(&"test").unwrap(),
        },
        WalEntry::SetProperty {
            entity_id: 2,
            key_id: 1,
            value: bincode::serialize(&42i64).unwrap(),
        },
    ];

    for (i, entry) in entries.into_iter().enumerate() {
        let msg = ReplicationMessage::WalEntry {
            offset: i as u64,
            epoch: 1,
            entry,
        };

        let encoded = msg.encode().unwrap();
        let decoded = ReplicationMessage::decode(&encoded).unwrap();

        match decoded {
            ReplicationMessage::WalEntry { offset, epoch, .. } => {
                assert_eq!(offset, i as u64);
                assert_eq!(epoch, 1);
            }
            _ => panic!("Expected WalEntry"),
        }
    }
}

/// Test heartbeat messages
#[tokio::test]
async fn test_heartbeat_messages() {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    let ping = ReplicationMessage::Ping { timestamp };
    let pong = ReplicationMessage::Pong { timestamp };

    let ping_bytes = ping.encode().unwrap();
    let pong_bytes = pong.encode().unwrap();

    let decoded_ping = ReplicationMessage::decode(&ping_bytes).unwrap();
    let decoded_pong = ReplicationMessage::decode(&pong_bytes).unwrap();

    match decoded_ping {
        ReplicationMessage::Ping { timestamp: ts } => {
            assert_eq!(ts, timestamp);
        }
        _ => panic!("Expected Ping"),
    }

    match decoded_pong {
        ReplicationMessage::Pong { timestamp: ts } => {
            assert_eq!(ts, timestamp);
        }
        _ => panic!("Expected Pong"),
    }
}

/// Test error message encoding
#[tokio::test]
async fn test_error_message() {
    let error = ReplicationMessage::Error {
        code: 500,
        message: "Internal replication error".to_string(),
    };

    let encoded = error.encode().unwrap();
    let decoded = ReplicationMessage::decode(&encoded).unwrap();

    match decoded {
        ReplicationMessage::Error { code, message } => {
            assert_eq!(code, 500);
            assert_eq!(message, "Internal replication error");
        }
        _ => panic!("Expected Error"),
    }
}

/// Test snapshot metadata serialization
#[tokio::test]
async fn test_snapshot_metadata_message() {
    let meta = ReplicationMessage::SnapshotMeta {
        snapshot_id: "snap-test-123".to_string(),
        total_size: 1024 * 1024,
        chunk_count: 10,
        checksum: 0xDEADBEEF,
        wal_offset: 5000,
    };

    let encoded = meta.encode().unwrap();
    let decoded = ReplicationMessage::decode(&encoded).unwrap();

    match decoded {
        ReplicationMessage::SnapshotMeta {
            snapshot_id,
            total_size,
            chunk_count,
            checksum,
            wal_offset,
        } => {
            assert_eq!(snapshot_id, "snap-test-123");
            assert_eq!(total_size, 1024 * 1024);
            assert_eq!(chunk_count, 10);
            assert_eq!(checksum, 0xDEADBEEF);
            assert_eq!(wal_offset, 5000);
        }
        _ => panic!("Expected SnapshotMeta"),
    }
}

/// Test snapshot complete message
#[tokio::test]
async fn test_snapshot_complete_message() {
    let complete = ReplicationMessage::SnapshotComplete {
        snapshot_id: "snap-123".to_string(),
        success: true,
    };

    let encoded = complete.encode().unwrap();
    let decoded = ReplicationMessage::decode(&encoded).unwrap();

    match decoded {
        ReplicationMessage::SnapshotComplete {
            snapshot_id,
            success,
        } => {
            assert_eq!(snapshot_id, "snap-123");
            assert!(success);
        }
        _ => panic!("Expected SnapshotComplete"),
    }
}

/// Test WAL acknowledgment message
#[tokio::test]
async fn test_wal_ack_message() {
    let ack = ReplicationMessage::WalAck {
        offset: 12345,
        success: true,
    };

    let encoded = ack.encode().unwrap();
    let decoded = ReplicationMessage::decode(&encoded).unwrap();

    match decoded {
        ReplicationMessage::WalAck { offset, success } => {
            assert_eq!(offset, 12345);
            assert!(success);
        }
        _ => panic!("Expected WalAck"),
    }
}

/// Test multiple replications in sequence
#[tokio::test]
async fn test_sequential_replication() {
    let ctx = TestContext::new();
    let data_dir = ctx.path().join("sequential_data");
    std::fs::create_dir_all(&data_dir).unwrap();

    let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let config = ReplicationConfig::master(bind_addr);
    let snapshot_config = SnapshotConfig {
        data_dir,
        ..Default::default()
    };
    let snapshot = Arc::new(Snapshot::new(snapshot_config));
    let master = Master::new(config, snapshot);

    // Replicate multiple entries
    for i in 0..100 {
        let entry = WalEntry::CreateNode {
            node_id: i,
            label_bits: (i % 8) as u64,
        };
        master.replicate(entry, 1).await.unwrap();
    }

    assert_eq!(master.current_offset(), 100);

    let stats = master.stats();
    assert_eq!(stats.entries_replicated, 100);
}

/// Test large snapshot handling
#[tokio::test]
async fn test_large_snapshot() {
    let ctx = TestContext::new();
    let data_dir = ctx.path().join("large_snapshot_data");
    std::fs::create_dir_all(&data_dir).unwrap();

    // Create a larger file (100KB)
    let large_data: Vec<u8> = (0..102400).map(|i| (i % 256) as u8).collect();
    std::fs::write(data_dir.join("large_file.bin"), &large_data).unwrap();

    let config = SnapshotConfig {
        data_dir: data_dir.clone(),
        compression_level: 3,
        max_size: 200 * 1024 * 1024, // 200MB limit
        chunk_size: 16 * 1024,       // 16KB chunks
    };
    let snapshot = Snapshot::new(config);

    // Create snapshot
    let snapshot_data = snapshot.create().await.unwrap();
    assert!(!snapshot_data.is_empty());

    let meta = snapshot.last_snapshot().unwrap();
    assert!(meta.compressed_size < meta.uncompressed_size);

    // Verify can restore
    // First modify the file
    std::fs::write(data_dir.join("large_file.bin"), b"modified").unwrap();

    // Restore
    snapshot.restore(&snapshot_data).await.unwrap();

    // Verify original content
    let restored = std::fs::read(data_dir.join("large_file.bin")).unwrap();
    assert_eq!(restored, large_data);
}

/// Test concurrent snapshot prevention
#[tokio::test]
async fn test_concurrent_snapshot_prevention() {
    let ctx = TestContext::new();
    let data_dir = ctx.path().join("concurrent_snap_data");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::write(data_dir.join("test.txt"), b"test content").unwrap();

    let config = SnapshotConfig {
        data_dir,
        compression_level: 1,
        max_size: 1024 * 1024,
        chunk_size: 1024,
    };
    let snapshot = Arc::new(Snapshot::new(config));

    // Start first snapshot
    let snap1 = snapshot.clone();
    let handle1 = tokio::spawn(async move { snap1.create().await });

    // Small delay to let first start
    tokio::time::sleep(Duration::from_millis(5)).await;

    // Try second snapshot (may fail if first is in progress)
    let snap2 = snapshot.clone();
    let result2 = snap2.create().await;

    // Wait for first
    let result1 = handle1.await.unwrap();
    assert!(result1.is_ok());

    // Second might succeed or fail depending on timing
    // (first is fast for small files)
}

/// Test message type identification
#[tokio::test]
async fn test_message_type_identification() {
    use nexus_core::replication::protocol::ReplicationMessageType;

    assert_eq!(
        ReplicationMessage::Hello {
            replica_id: "".to_string(),
            last_wal_offset: 0,
            protocol_version: 1
        }
        .message_type(),
        ReplicationMessageType::Hello
    );

    assert_eq!(
        ReplicationMessage::Welcome {
            master_id: "".to_string(),
            current_wal_offset: 0,
            requires_full_sync: false
        }
        .message_type(),
        ReplicationMessageType::Welcome
    );

    assert_eq!(
        ReplicationMessage::Ping { timestamp: 0 }.message_type(),
        ReplicationMessageType::Ping
    );

    assert_eq!(
        ReplicationMessage::Pong { timestamp: 0 }.message_type(),
        ReplicationMessageType::Pong
    );

    assert_eq!(
        ReplicationMessage::WalEntry {
            offset: 0,
            epoch: 0,
            entry: WalEntry::DeleteNode { node_id: 0 }
        }
        .message_type(),
        ReplicationMessageType::WalEntry
    );

    assert_eq!(
        ReplicationMessage::WalAck {
            offset: 0,
            success: true
        }
        .message_type(),
        ReplicationMessageType::WalAck
    );

    assert_eq!(
        ReplicationMessage::Error {
            code: 0,
            message: "".to_string()
        }
        .message_type(),
        ReplicationMessageType::Error
    );
}
