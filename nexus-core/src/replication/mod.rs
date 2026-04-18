//! Replication System for Nexus Graph Database
//!
//! This module implements master-replica replication with:
//! - Async WAL streaming for high performance
//! - Optional sync replication for durability
//! - Full sync (snapshot transfer) for new replicas
//! - Automatic failover with replica promotion
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────┐     WAL Stream      ┌─────────────────┐
//! │     Master      │ ──────────────────► │    Replica 1    │
//! │                 │                     └─────────────────┘
//! │  Writes go here │     WAL Stream      ┌─────────────────┐
//! │                 │ ──────────────────► │    Replica 2    │
//! └─────────────────┘                     └─────────────────┘
//! ```
//!
//! # Replication Modes
//!
//! - **Async**: Master doesn't wait for replica ACKs (default)
//! - **Sync**: Master waits for quorum ACKs before commit
//!
//! # Wire Protocol
//!
//! All messages use bincode serialization with CRC32 validation:
//!
//! ```text
//! [message_type:1][length:4][payload:N][crc32:4]
//! ```

pub mod config;
pub mod master;
pub mod protocol;
pub mod replica;
pub mod snapshot;

pub use config::{ReplicationConfig, ReplicationMode, ReplicationRole};
pub use master::{Master, MasterStats, ReplicaInfo};
pub use protocol::{ReplicationMessage, ReplicationMessageType};
pub use replica::{Replica, ReplicaStats};
pub use snapshot::{Snapshot, SnapshotConfig};

use crate::{Error, Result};

/// Replication lag threshold for warnings (in operations)
pub const LAG_WARNING_THRESHOLD: u64 = 10_000;

/// Maximum replication log size (circular buffer)
pub const MAX_REPLICATION_LOG_SIZE: usize = 1_000_000;

/// Default heartbeat interval in milliseconds
pub const DEFAULT_HEARTBEAT_MS: u64 = 5_000;

/// Number of missed heartbeats before considering master dead
pub const MISSED_HEARTBEATS_THRESHOLD: u32 = 3;

/// Default replication port
pub const DEFAULT_REPLICATION_PORT: u16 = 15475;
