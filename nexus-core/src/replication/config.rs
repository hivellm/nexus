//! Replication configuration

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::time::Duration;

/// Replication mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplicationMode {
    /// Async replication - don't wait for replica ACKs
    Async,
    /// Sync replication - wait for quorum ACKs before commit
    Sync,
}

impl Default for ReplicationMode {
    fn default() -> Self {
        Self::Async
    }
}

/// Node role in replication topology
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplicationRole {
    /// Primary node - accepts writes
    Master,
    /// Secondary node - read-only replica
    Replica,
    /// Standalone node - no replication
    Standalone,
}

impl Default for ReplicationRole {
    fn default() -> Self {
        Self::Standalone
    }
}

/// Replication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationConfig {
    /// This node's role
    pub role: ReplicationRole,

    /// Replication mode (async/sync)
    pub mode: ReplicationMode,

    /// Address to bind for replication connections
    pub bind_addr: SocketAddr,

    /// Master address (for replicas)
    pub master_addr: Option<SocketAddr>,

    /// Heartbeat interval
    pub heartbeat_interval: Duration,

    /// Connection timeout
    pub connect_timeout: Duration,

    /// Read timeout for replication stream
    pub read_timeout: Duration,

    /// Write timeout for replication stream
    pub write_timeout: Duration,

    /// Number of missed heartbeats before failover
    pub missed_heartbeats_threshold: u32,

    /// Quorum size for sync replication (including master)
    pub sync_quorum: u32,

    /// Maximum replication log size
    pub max_log_size: usize,

    /// Enable automatic failover
    pub auto_failover: bool,

    /// Snapshot compression level (0-22, zstd)
    pub snapshot_compression_level: i32,

    /// Maximum concurrent snapshot transfers
    pub max_snapshot_transfers: usize,
}

impl Default for ReplicationConfig {
    fn default() -> Self {
        Self {
            role: ReplicationRole::Standalone,
            mode: ReplicationMode::Async,
            bind_addr: "0.0.0.0:15475".parse().unwrap(),
            master_addr: None,
            heartbeat_interval: Duration::from_secs(5),
            connect_timeout: Duration::from_secs(10),
            read_timeout: Duration::from_secs(30),
            write_timeout: Duration::from_secs(30),
            missed_heartbeats_threshold: 3,
            sync_quorum: 2,
            max_log_size: 1_000_000,
            auto_failover: true,
            snapshot_compression_level: 3,
            max_snapshot_transfers: 2,
        }
    }
}

impl ReplicationConfig {
    /// Create a master configuration
    pub fn master(bind_addr: SocketAddr) -> Self {
        Self {
            role: ReplicationRole::Master,
            bind_addr,
            ..Default::default()
        }
    }

    /// Create a replica configuration
    pub fn replica(master_addr: SocketAddr) -> Self {
        Self {
            role: ReplicationRole::Replica,
            master_addr: Some(master_addr),
            ..Default::default()
        }
    }

    /// Create a standalone configuration (no replication)
    pub fn standalone() -> Self {
        Self {
            role: ReplicationRole::Standalone,
            ..Default::default()
        }
    }

    /// Set replication mode
    pub fn with_mode(mut self, mode: ReplicationMode) -> Self {
        self.mode = mode;
        self
    }

    /// Set sync quorum
    pub fn with_quorum(mut self, quorum: u32) -> Self {
        self.sync_quorum = quorum;
        self
    }

    /// Enable/disable auto failover
    pub fn with_auto_failover(mut self, enabled: bool) -> Self {
        self.auto_failover = enabled;
        self
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), String> {
        match self.role {
            ReplicationRole::Replica => {
                if self.master_addr.is_none() {
                    return Err("Replica requires master_addr".into());
                }
            }
            ReplicationRole::Master => {
                if self.sync_quorum < 1 {
                    return Err("sync_quorum must be at least 1".into());
                }
            }
            ReplicationRole::Standalone => {}
        }

        if self.missed_heartbeats_threshold == 0 {
            return Err("missed_heartbeats_threshold must be at least 1".into());
        }

        if self.max_log_size == 0 {
            return Err("max_log_size must be at least 1".into());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ReplicationConfig::default();
        assert_eq!(config.role, ReplicationRole::Standalone);
        assert_eq!(config.mode, ReplicationMode::Async);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_master_config() {
        let addr: SocketAddr = "0.0.0.0:15475".parse().unwrap();
        let config = ReplicationConfig::master(addr);
        assert_eq!(config.role, ReplicationRole::Master);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_replica_config() {
        let addr: SocketAddr = "127.0.0.1:15475".parse().unwrap();
        let config = ReplicationConfig::replica(addr);
        assert_eq!(config.role, ReplicationRole::Replica);
        assert_eq!(config.master_addr, Some(addr));
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_replica_without_master() {
        let mut config = ReplicationConfig::default();
        config.role = ReplicationRole::Replica;
        config.master_addr = None;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_sync_mode() {
        let addr: SocketAddr = "0.0.0.0:15475".parse().unwrap();
        let config = ReplicationConfig::master(addr)
            .with_mode(ReplicationMode::Sync)
            .with_quorum(2);
        assert_eq!(config.mode, ReplicationMode::Sync);
        assert_eq!(config.sync_quorum, 2);
    }
}
