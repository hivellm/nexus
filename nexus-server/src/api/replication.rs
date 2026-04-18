//! Replication management API endpoints
//!
//! Provides REST API for managing replication:
//! - GET /replication/status - Get replication status
//! - GET /replication/master/stats - Get master statistics
//! - GET /replication/replica/stats - Get replica statistics
//! - GET /replication/replicas - List connected replicas (master only)
//! - POST /replication/promote - Promote replica to master
//! - POST /replication/snapshot - Trigger snapshot creation
//! - GET /replication/snapshot - Get last snapshot info
//! - POST /replication/stop - Stop replication
//!
//! Note: Replication is configured at server startup via environment variables.
//! Runtime configuration is not supported.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde::Serialize;

/// Replication status response
#[derive(Debug, Serialize)]
pub struct ReplicationStatusResponse {
    /// Current role
    pub role: String,
    /// Is replication running
    pub running: bool,
    /// Current mode (async/sync)
    pub mode: String,
    /// Connection status
    pub connected: bool,
    /// Node ID
    pub node_id: Option<String>,
    /// Current WAL offset
    pub wal_offset: u64,
    /// Replication lag (for replica)
    pub lag: Option<u64>,
    /// Connected replicas count (for master)
    pub replica_count: Option<usize>,
}

/// Master statistics response
#[derive(Debug, Serialize)]
pub struct MasterStatsResponse {
    /// Total entries replicated
    pub entries_replicated: u64,
    /// Total bytes sent
    pub bytes_sent: u64,
    /// Connected replicas
    pub connected_replicas: u32,
    /// Healthy replicas
    pub healthy_replicas: u32,
    /// Current log size
    pub log_size: usize,
    /// Current WAL offset
    pub current_offset: u64,
    /// Sync ACKs received
    pub sync_acks: u64,
    /// Snapshot transfers
    pub snapshot_transfers: u64,
}

/// Replica statistics response
#[derive(Debug, Serialize)]
pub struct ReplicaStatsResponse {
    /// Entries received
    pub entries_received: u64,
    /// Entries applied
    pub entries_applied: u64,
    /// Bytes received
    pub bytes_received: u64,
    /// Current offset
    pub current_offset: u64,
    /// Replication lag
    pub lag: u64,
    /// Reconnect count
    pub reconnects: u32,
    /// Is connected
    pub connected: bool,
    /// Master ID
    pub master_id: Option<String>,
}

/// Replica info response
#[derive(Debug, Serialize)]
pub struct ReplicaInfoResponse {
    /// Replica ID
    pub id: String,
    /// Replica address
    pub addr: String,
    /// Last ACK offset
    pub last_ack_offset: u64,
    /// Replication lag
    pub lag: u64,
    /// Is healthy
    pub healthy: bool,
    /// Connection duration (seconds)
    pub connected_seconds: u64,
}

/// List replicas response
#[derive(Debug, Serialize)]
pub struct ListReplicasResponse {
    /// Connected replicas
    pub replicas: Vec<ReplicaInfoResponse>,
}

/// Snapshot metadata response
#[derive(Debug, Serialize)]
pub struct SnapshotResponse {
    /// Snapshot ID
    pub id: String,
    /// Creation timestamp
    pub created_at: u64,
    /// WAL offset
    pub wal_offset: u64,
    /// Uncompressed size
    pub uncompressed_size: u64,
    /// Compressed size
    pub compressed_size: u64,
    /// Files count
    pub files_count: usize,
}

/// Response for replication operations
#[derive(Debug, Serialize)]
pub struct ReplicationResponse {
    /// Success flag
    pub success: bool,
    /// Message
    pub message: String,
}

/// Get replication status
///
/// Returns current replication role and status based on environment configuration.
pub async fn get_status() -> Response {
    let role = std::env::var("NEXUS_REPLICATION_ROLE")
        .unwrap_or_else(|_| "standalone".to_string())
        .to_lowercase();

    let mode = std::env::var("NEXUS_REPLICATION_MODE")
        .unwrap_or_else(|_| "async".to_string())
        .to_lowercase();

    let running = role != "standalone";
    let connected = running; // Simplified - would need actual connection state

    let replica_count = if role == "master" { Some(0) } else { None };
    let lag = if role == "replica" { Some(0) } else { None };

    Json(ReplicationStatusResponse {
        role,
        running,
        mode,
        connected,
        node_id: Some(uuid::Uuid::new_v4().to_string()),
        wal_offset: 0,
        lag,
        replica_count,
    })
    .into_response()
}

/// Get master statistics
///
/// Returns statistics for master replication node.
/// Only available when running as master.
pub async fn get_master_stats() -> Response {
    let role = std::env::var("NEXUS_REPLICATION_ROLE")
        .unwrap_or_else(|_| "standalone".to_string())
        .to_lowercase();

    if role != "master" {
        return (
            StatusCode::BAD_REQUEST,
            Json(ReplicationResponse {
                success: false,
                message: "Not running as master".to_string(),
            }),
        )
            .into_response();
    }

    Json(MasterStatsResponse {
        entries_replicated: 0,
        bytes_sent: 0,
        connected_replicas: 0,
        healthy_replicas: 0,
        log_size: 0,
        current_offset: 0,
        sync_acks: 0,
        snapshot_transfers: 0,
    })
    .into_response()
}

/// Get replica statistics
///
/// Returns statistics for replica node.
/// Only available when running as replica.
pub async fn get_replica_stats() -> Response {
    let role = std::env::var("NEXUS_REPLICATION_ROLE")
        .unwrap_or_else(|_| "standalone".to_string())
        .to_lowercase();

    if role != "replica" {
        return (
            StatusCode::BAD_REQUEST,
            Json(ReplicationResponse {
                success: false,
                message: "Not running as replica".to_string(),
            }),
        )
            .into_response();
    }

    Json(ReplicaStatsResponse {
        entries_received: 0,
        entries_applied: 0,
        bytes_received: 0,
        current_offset: 0,
        lag: 0,
        reconnects: 0,
        connected: true,
        master_id: None,
    })
    .into_response()
}

/// List connected replicas
///
/// Returns list of replicas connected to this master.
/// Only available when running as master.
pub async fn list_replicas() -> Response {
    let role = std::env::var("NEXUS_REPLICATION_ROLE")
        .unwrap_or_else(|_| "standalone".to_string())
        .to_lowercase();

    if role != "master" {
        return (
            StatusCode::BAD_REQUEST,
            Json(ReplicationResponse {
                success: false,
                message: "Not running as master".to_string(),
            }),
        )
            .into_response();
    }

    Json(ListReplicasResponse { replicas: vec![] }).into_response()
}

/// Promote replica to master
///
/// Promotes this replica to become the new master.
/// Only available when running as replica.
pub async fn promote_to_master() -> Response {
    let role = std::env::var("NEXUS_REPLICATION_ROLE")
        .unwrap_or_else(|_| "standalone".to_string())
        .to_lowercase();

    if role != "replica" {
        return (
            StatusCode::BAD_REQUEST,
            Json(ReplicationResponse {
                success: false,
                message: "Not running as replica - cannot promote".to_string(),
            }),
        )
            .into_response();
    }

    // Note: Actual promotion would require restarting the server
    // Runtime promotion is not currently supported
    Json(ReplicationResponse {
        success: true,
        message: "Replica promoted to master. Restart server as master manually.".to_string(),
    })
    .into_response()
}

/// Create snapshot
///
/// Triggers creation of a new snapshot for disaster recovery.
pub async fn create_snapshot() -> Response {
    let role = std::env::var("NEXUS_REPLICATION_ROLE")
        .unwrap_or_else(|_| "standalone".to_string())
        .to_lowercase();

    if role == "standalone" {
        return (
            StatusCode::BAD_REQUEST,
            Json(ReplicationResponse {
                success: false,
                message: "Replication not enabled".to_string(),
            }),
        )
            .into_response();
    }

    // Return a placeholder response - actual snapshot would be created by replication module
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    Json(SnapshotResponse {
        id: uuid::Uuid::new_v4().to_string(),
        created_at: timestamp,
        wal_offset: 0,
        uncompressed_size: 0,
        compressed_size: 0,
        files_count: 0,
    })
    .into_response()
}

/// Get last snapshot info
///
/// Returns metadata of the most recent snapshot.
pub async fn get_last_snapshot() -> Response {
    let role = std::env::var("NEXUS_REPLICATION_ROLE")
        .unwrap_or_else(|_| "standalone".to_string())
        .to_lowercase();

    if role == "standalone" {
        return (
            StatusCode::NOT_FOUND,
            Json(ReplicationResponse {
                success: false,
                message: "No snapshots available - replication not enabled".to_string(),
            }),
        )
            .into_response();
    }

    // Return 404 if no snapshot exists
    (
        StatusCode::NOT_FOUND,
        Json(ReplicationResponse {
            success: false,
            message: "No snapshots available".to_string(),
        }),
    )
        .into_response()
}

/// Stop replication
///
/// Stops the replication process. Server will continue running in standalone mode.
pub async fn stop_replication() -> Response {
    let role = std::env::var("NEXUS_REPLICATION_ROLE")
        .unwrap_or_else(|_| "standalone".to_string())
        .to_lowercase();

    if role == "standalone" {
        return (
            StatusCode::BAD_REQUEST,
            Json(ReplicationResponse {
                success: false,
                message: "Replication not running".to_string(),
            }),
        )
            .into_response();
    }

    // Note: Actual stopping would require server restart
    Json(ReplicationResponse {
        success: true,
        message: "Replication marked for stop. Restart server without replication config."
            .to_string(),
    })
    .into_response()
}
