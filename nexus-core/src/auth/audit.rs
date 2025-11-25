//! Audit logging system for Nexus
//!
//! This module provides comprehensive audit logging for all security-sensitive operations,
//! including user management, permission changes, API key operations, authentication failures,
//! and write operations.

use anyhow::Result;
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::warn;

/// Audit log entry structure (JSON format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    /// Timestamp of the operation
    pub timestamp: DateTime<Utc>,
    /// Type of operation
    pub operation: AuditOperation,
    /// User ID who performed the operation (if authenticated)
    pub user_id: Option<String>,
    /// Username who performed the operation (if authenticated)
    pub username: Option<String>,
    /// API key ID used (if applicable)
    pub api_key_id: Option<String>,
    /// Result of the operation
    pub result: AuditResult,
    /// Additional metadata
    pub metadata: serde_json::Value,
    /// IP address (if available)
    pub ip_address: Option<String>,
}

/// Types of audit operations
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AuditOperation {
    /// User creation
    UserCreated {
        target_username: String,
        target_user_id: String,
    },
    /// User deletion
    UserDeleted {
        target_username: String,
        target_user_id: String,
    },
    /// Permission granted
    PermissionGranted {
        target_username: String,
        target_user_id: String,
        permissions: Vec<String>,
    },
    /// Permission revoked
    PermissionRevoked {
        target_username: String,
        target_user_id: String,
        permissions: Vec<String>,
    },
    /// API key created
    ApiKeyCreated {
        key_id: String,
        key_name: String,
        user_id: Option<String>,
    },
    /// API key revoked
    ApiKeyRevoked {
        key_id: String,
        reason: Option<String>,
    },
    /// API key deleted
    ApiKeyDeleted { key_id: String },
    /// Authentication failure
    AuthenticationFailed {
        username: Option<String>,
        reason: String,
        ip_address: Option<String>,
    },
    /// Authentication success
    AuthenticationSuccess {
        username: String,
        user_id: String,
        method: String, // "api_key", "jwt", "password"
    },
    /// Write operation (CREATE, SET, DELETE)
    WriteOperation {
        operation_type: String, // "CREATE", "SET", "DELETE"
        entity_type: String,    // "NODE", "RELATIONSHIP", "PROPERTY"
        entity_id: Option<String>,
        cypher_query: Option<String>,
    },
}

/// Result of an audit operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditResult {
    /// Operation succeeded
    Success,
    /// Operation failed
    Failure { error: String },
}

/// Configuration for audit logging
#[derive(Debug, Clone)]
pub struct AuditConfig {
    /// Whether audit logging is enabled
    pub enabled: bool,
    /// Directory where audit logs are stored
    pub log_dir: PathBuf,
    /// Retention period in days (0 = keep forever)
    pub retention_days: u32,
    /// Whether to compress old logs
    pub compress_logs: bool,
}

/// Parameters for write operation logging
#[derive(Debug, Clone)]
pub struct WriteOperationParams {
    pub actor_user_id: Option<String>,
    pub actor_username: Option<String>,
    pub api_key_id: Option<String>,
    pub operation_type: String,
    pub entity_type: String,
    pub entity_id: Option<String>,
    pub cypher_query: Option<String>,
    pub result: AuditResult,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            log_dir: PathBuf::from("./logs/audit"),
            retention_days: 90,
            compress_logs: true,
        }
    }
}

/// Audit logger instance
#[derive(Debug)]
pub struct AuditLogger {
    config: AuditConfig,
    current_log_file: Arc<RwLock<Option<BufWriter<File>>>>,
    current_date: Arc<RwLock<String>>,
}

impl AuditLogger {
    /// Create a new audit logger
    pub fn new(config: AuditConfig) -> Result<Self> {
        let logger = Self {
            config: config.clone(),
            current_log_file: Arc::new(RwLock::new(None)),
            current_date: Arc::new(RwLock::new(String::new())),
        };

        // Only initialize log file if logging is enabled
        if config.enabled {
            // Create log directory if it doesn't exist
            std::fs::create_dir_all(&config.log_dir)?;

            // Initialize current log file synchronously
            let today = Utc::now().format("%Y-%m-%d").to_string();
            let log_filename = format!("audit-{}.log", today);
            let log_path = logger.config.log_dir.join(&log_filename);
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)?;
            let writer = BufWriter::new(file);
            *logger.current_log_file.write() = Some(writer);
            *logger.current_date.write() = today;
        }

        Ok(logger)
    }

    /// Log an audit entry
    pub async fn log(&self, entry: AuditLogEntry) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        // Rotate log file if needed (daily rotation)
        self.rotate_if_needed().await?;

        // Serialize entry to JSON
        let json = serde_json::to_string(&entry)?;

        // Write to current log file
        let mut log_file = self.current_log_file.write();
        if let Some(ref mut writer) = *log_file {
            writeln!(writer, "{}", json)?;
            writer.flush()?;
        } else {
            // Fallback: write to stderr if file is not available
            tracing::error!("AUDIT LOG: {}", json);
        }

        Ok(())
    }

    /// Log user creation
    pub async fn log_user_created(
        &self,
        actor_user_id: Option<String>,
        actor_username: Option<String>,
        target_username: String,
        target_user_id: String,
        result: AuditResult,
    ) -> Result<()> {
        self.log(AuditLogEntry {
            timestamp: Utc::now(),
            operation: AuditOperation::UserCreated {
                target_username,
                target_user_id,
            },
            user_id: actor_user_id,
            username: actor_username,
            api_key_id: None,
            result,
            metadata: serde_json::json!({}),
            ip_address: None,
        })
        .await
    }

    /// Log user deletion
    pub async fn log_user_deleted(
        &self,
        actor_user_id: Option<String>,
        actor_username: Option<String>,
        target_username: String,
        target_user_id: String,
        result: AuditResult,
    ) -> Result<()> {
        self.log(AuditLogEntry {
            timestamp: Utc::now(),
            operation: AuditOperation::UserDeleted {
                target_username,
                target_user_id,
            },
            user_id: actor_user_id,
            username: actor_username,
            api_key_id: None,
            result,
            metadata: serde_json::json!({}),
            ip_address: None,
        })
        .await
    }

    /// Log permission grant
    pub async fn log_permission_granted(
        &self,
        actor_user_id: Option<String>,
        actor_username: Option<String>,
        target_username: String,
        target_user_id: String,
        permissions: Vec<String>,
        result: AuditResult,
    ) -> Result<()> {
        self.log(AuditLogEntry {
            timestamp: Utc::now(),
            operation: AuditOperation::PermissionGranted {
                target_username,
                target_user_id,
                permissions,
            },
            user_id: actor_user_id,
            username: actor_username,
            api_key_id: None,
            result,
            metadata: serde_json::json!({}),
            ip_address: None,
        })
        .await
    }

    /// Log permission revocation
    pub async fn log_permission_revoked(
        &self,
        actor_user_id: Option<String>,
        actor_username: Option<String>,
        target_username: String,
        target_user_id: String,
        permissions: Vec<String>,
        result: AuditResult,
    ) -> Result<()> {
        self.log(AuditLogEntry {
            timestamp: Utc::now(),
            operation: AuditOperation::PermissionRevoked {
                target_username,
                target_user_id,
                permissions,
            },
            user_id: actor_user_id,
            username: actor_username,
            api_key_id: None,
            result,
            metadata: serde_json::json!({}),
            ip_address: None,
        })
        .await
    }

    /// Log API key creation
    pub async fn log_api_key_created(
        &self,
        actor_user_id: Option<String>,
        actor_username: Option<String>,
        key_id: String,
        key_name: String,
        user_id: Option<String>,
        result: AuditResult,
    ) -> Result<()> {
        self.log(AuditLogEntry {
            timestamp: Utc::now(),
            operation: AuditOperation::ApiKeyCreated {
                key_id,
                key_name,
                user_id,
            },
            user_id: actor_user_id,
            username: actor_username,
            api_key_id: None,
            result,
            metadata: serde_json::json!({}),
            ip_address: None,
        })
        .await
    }

    /// Log API key revocation
    pub async fn log_api_key_revoked(
        &self,
        actor_user_id: Option<String>,
        actor_username: Option<String>,
        key_id: String,
        reason: Option<String>,
        result: AuditResult,
    ) -> Result<()> {
        self.log(AuditLogEntry {
            timestamp: Utc::now(),
            operation: AuditOperation::ApiKeyRevoked { key_id, reason },
            user_id: actor_user_id,
            username: actor_username,
            api_key_id: None,
            result,
            metadata: serde_json::json!({}),
            ip_address: None,
        })
        .await
    }

    /// Log API key deletion
    pub async fn log_api_key_deleted(
        &self,
        actor_user_id: Option<String>,
        actor_username: Option<String>,
        key_id: String,
        result: AuditResult,
    ) -> Result<()> {
        self.log(AuditLogEntry {
            timestamp: Utc::now(),
            operation: AuditOperation::ApiKeyDeleted { key_id },
            user_id: actor_user_id,
            username: actor_username,
            api_key_id: None,
            result,
            metadata: serde_json::json!({}),
            ip_address: None,
        })
        .await
    }

    /// Log authentication failure
    pub async fn log_authentication_failed(
        &self,
        username: Option<String>,
        reason: String,
        ip_address: Option<String>,
    ) -> Result<()> {
        self.log(AuditLogEntry {
            timestamp: Utc::now(),
            operation: AuditOperation::AuthenticationFailed {
                username,
                reason,
                ip_address: ip_address.clone(),
            },
            user_id: None,
            username: None,
            api_key_id: None,
            result: AuditResult::Failure {
                error: "Authentication failed".to_string(),
            },
            metadata: serde_json::json!({}),
            ip_address,
        })
        .await
    }

    /// Log authentication success
    pub async fn log_authentication_success(
        &self,
        username: String,
        user_id: String,
        method: String,
    ) -> Result<()> {
        self.log(AuditLogEntry {
            timestamp: Utc::now(),
            operation: AuditOperation::AuthenticationSuccess {
                username: username.clone(),
                user_id: user_id.clone(),
                method,
            },
            user_id: Some(user_id),
            username: Some(username),
            api_key_id: None,
            result: AuditResult::Success,
            metadata: serde_json::json!({}),
            ip_address: None,
        })
        .await
    }

    /// Log write operation
    pub async fn log_write_operation(&self, params: WriteOperationParams) -> Result<()> {
        self.log(AuditLogEntry {
            timestamp: Utc::now(),
            operation: AuditOperation::WriteOperation {
                operation_type: params.operation_type,
                entity_type: params.entity_type,
                entity_id: params.entity_id,
                cypher_query: params.cypher_query,
            },
            user_id: params.actor_user_id,
            username: params.actor_username,
            api_key_id: params.api_key_id,
            result: params.result,
            metadata: serde_json::json!({}),
            ip_address: None,
        })
        .await
    }

    /// Rotate log file if needed (daily rotation)
    async fn rotate_if_needed(&self) -> Result<()> {
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let mut current_date = self.current_date.write();

        if *current_date != today {
            // Close current log file if open
            {
                let mut log_file = self.current_log_file.write();
                if let Some(mut writer) = log_file.take() {
                    writer.flush()?;
                    drop(writer);
                }
            }

            // Open new log file
            let log_filename = format!("audit-{}.log", today);
            let log_path = self.config.log_dir.join(&log_filename);

            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)?;

            let writer = BufWriter::new(file);
            *self.current_log_file.write() = Some(writer);
            let today_clone = today.clone();
            *current_date = today;

            // Compress old logs if enabled (run in background)
            if self.config.compress_logs {
                let log_dir = self.config.log_dir.clone();
                tokio::task::spawn_blocking(move || {
                    Self::compress_old_logs_static(&log_dir, &today_clone).ok();
                });
            }

            // Clean up old logs based on retention period (run in background)
            if self.config.retention_days > 0 {
                let log_dir = self.config.log_dir.clone();
                let retention_days = self.config.retention_days;
                tokio::task::spawn_blocking(move || {
                    Self::cleanup_old_logs_static(&log_dir, retention_days).ok();
                });
            }
        }

        Ok(())
    }

    /// Compress old log files (older than today) - static version for background tasks
    fn compress_old_logs_static(log_dir: &Path, today: &str) -> Result<()> {
        let entries = std::fs::read_dir(log_dir)?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            let filename = path.file_name().and_then(|n| n.to_str());

            // Skip if already compressed or is today's log
            if let Some(name) = filename {
                if name.ends_with(".gz") || name.contains(today) {
                    continue;
                }

                if name.starts_with("audit-") && name.ends_with(".log") {
                    // Compress the log file
                    Self::compress_file_static(&path)?;
                }
            }
        }

        Ok(())
    }

    /// Compress a single log file using gzip - static version
    fn compress_file_static(file_path: &Path) -> Result<()> {
        use std::io::Read;

        // Read the file
        let mut file = File::open(file_path)?;
        let mut contents = Vec::new();
        file.read_to_end(&mut contents)?;

        // Compress using flate2
        use flate2::Compression;
        use flate2::write::GzEncoder;

        let compressed_path = file_path.with_extension("log.gz");
        let compressed_file = File::create(&compressed_path)?;
        let mut encoder = GzEncoder::new(compressed_file, Compression::default());
        encoder.write_all(&contents)?;
        encoder.finish()?;

        // Remove original file
        std::fs::remove_file(file_path)?;

        Ok(())
    }

    /// Clean up old log files based on retention period - static version for background tasks
    fn cleanup_old_logs_static(log_dir: &Path, retention_days: u32) -> Result<()> {
        let cutoff_date = Utc::now()
            .checked_sub_signed(chrono::Duration::days(retention_days as i64))
            .unwrap_or(Utc::now())
            .format("%Y-%m-%d")
            .to_string();

        let entries = std::fs::read_dir(log_dir)?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            let filename = path.file_name().and_then(|n| n.to_str());

            if let Some(name) = filename {
                // Extract date from filename (audit-YYYY-MM-DD.log or audit-YYYY-MM-DD.log.gz)
                if let Some(date_str) = name.strip_prefix("audit-") {
                    let date_str = date_str
                        .strip_suffix(".log")
                        .or_else(|| date_str.strip_suffix(".log.gz"))
                        .unwrap_or(date_str);

                    if date_str < cutoff_date.as_str() {
                        // Delete old log file
                        if let Err(e) = std::fs::remove_file(&path) {
                            warn!("Failed to delete old audit log file {:?}: {}", path, e);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Get the current configuration
    pub fn config(&self) -> &AuditConfig {
        &self.config
    }
}

// Note: This implementation uses async/await for the RwLock, but the actual file I/O
// is synchronous. In a production system, you might want to use a background task
// to handle log writes asynchronously.

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_audit_logger_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = AuditConfig {
            enabled: true,
            log_dir: temp_dir.path().to_path_buf(),
            retention_days: 30,
            compress_logs: false,
        };

        let logger = AuditLogger::new(config).unwrap();
        assert!(logger.config().enabled);
    }

    #[tokio::test]
    async fn test_log_user_created() {
        let temp_dir = TempDir::new().unwrap();
        let config = AuditConfig {
            enabled: true,
            log_dir: temp_dir.path().to_path_buf(),
            retention_days: 30,
            compress_logs: false,
        };

        let logger = AuditLogger::new(config).unwrap();
        logger
            .log_user_created(
                Some("actor-123".to_string()),
                Some("admin".to_string()),
                "newuser".to_string(),
                "user-456".to_string(),
                AuditResult::Success,
            )
            .await
            .unwrap();

        // Verify log file was created
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let log_file = temp_dir.path().join(format!("audit-{}.log", today));
        assert!(log_file.exists());
    }

    #[tokio::test]
    async fn test_log_authentication_failure() {
        let temp_dir = TempDir::new().unwrap();
        let config = AuditConfig {
            enabled: true,
            log_dir: temp_dir.path().to_path_buf(),
            retention_days: 30,
            compress_logs: false,
        };

        let logger = AuditLogger::new(config).unwrap();
        logger
            .log_authentication_failed(
                Some("testuser".to_string()),
                "Invalid password".to_string(),
                Some("127.0.0.1".to_string()),
            )
            .await
            .unwrap();

        // Verify log file was created
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let log_file = temp_dir.path().join(format!("audit-{}.log", today));
        assert!(log_file.exists());
    }

    #[tokio::test]
    async fn test_log_disabled() {
        let temp_dir = TempDir::new().unwrap();
        let config = AuditConfig {
            enabled: false,
            log_dir: temp_dir.path().to_path_buf(),
            retention_days: 30,
            compress_logs: false,
        };

        let logger = AuditLogger::new(config).unwrap();
        // Should not create log file when disabled
        logger
            .log_user_created(
                None,
                None,
                "testuser".to_string(),
                "user-123".to_string(),
                AuditResult::Success,
            )
            .await
            .unwrap();

        // Verify no log file was created
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let log_file = temp_dir.path().join(format!("audit-{}.log", today));
        assert!(!log_file.exists());
    }

    #[tokio::test]
    async fn test_log_api_key_operations() {
        let temp_dir = TempDir::new().unwrap();
        let config = AuditConfig {
            enabled: true,
            log_dir: temp_dir.path().to_path_buf(),
            retention_days: 30,
            compress_logs: false,
        };

        let logger = AuditLogger::new(config).unwrap();
        // Test API key creation
        logger
            .log_api_key_created(
                Some("user-123".to_string()),
                Some("admin".to_string()),
                "key-456".to_string(),
                "test-key".to_string(),
                Some("user-123".to_string()),
                AuditResult::Success,
            )
            .await
            .unwrap();

        // Test API key revocation
        logger
            .log_api_key_revoked(
                Some("user-123".to_string()),
                Some("admin".to_string()),
                "key-456".to_string(),
                Some("Security breach".to_string()),
                AuditResult::Success,
            )
            .await
            .unwrap();

        // Test API key deletion
        logger
            .log_api_key_deleted(
                Some("user-123".to_string()),
                Some("admin".to_string()),
                "key-456".to_string(),
                AuditResult::Success,
            )
            .await
            .unwrap();

        // Verify log file was created
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let log_file = temp_dir.path().join(format!("audit-{}.log", today));
        assert!(log_file.exists());
    }

    #[tokio::test]
    async fn test_log_permission_operations() {
        let temp_dir = TempDir::new().unwrap();
        let config = AuditConfig {
            enabled: true,
            log_dir: temp_dir.path().to_path_buf(),
            retention_days: 30,
            compress_logs: false,
        };

        let logger = AuditLogger::new(config).unwrap();
        // Test permission grant
        logger
            .log_permission_granted(
                Some("admin-123".to_string()),
                Some("admin".to_string()),
                "targetuser".to_string(),
                "user-456".to_string(),
                vec!["READ".to_string(), "WRITE".to_string()],
                AuditResult::Success,
            )
            .await
            .unwrap();

        // Test permission revocation
        logger
            .log_permission_revoked(
                Some("admin-123".to_string()),
                Some("admin".to_string()),
                "targetuser".to_string(),
                "user-456".to_string(),
                vec!["WRITE".to_string()],
                AuditResult::Success,
            )
            .await
            .unwrap();

        // Verify log file was created
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let log_file = temp_dir.path().join(format!("audit-{}.log", today));
        assert!(log_file.exists());
    }

    #[tokio::test]
    async fn test_log_write_operations() {
        let temp_dir = TempDir::new().unwrap();
        let config = AuditConfig {
            enabled: true,
            log_dir: temp_dir.path().to_path_buf(),
            retention_days: 30,
            compress_logs: false,
        };

        let logger = AuditLogger::new(config).unwrap();
        // Test CREATE operation
        logger
            .log_write_operation(WriteOperationParams {
                actor_user_id: Some("user-123".to_string()),
                actor_username: Some("testuser".to_string()),
                api_key_id: Some("key-456".to_string()),
                operation_type: "CREATE".to_string(),
                entity_type: "NODE".to_string(),
                entity_id: Some("12345".to_string()),
                cypher_query: Some("CREATE (n:Person {name: 'Test'})".to_string()),
                result: AuditResult::Success,
            })
            .await
            .unwrap();

        // Test SET operation
        logger
            .log_write_operation(WriteOperationParams {
                actor_user_id: Some("user-123".to_string()),
                actor_username: Some("testuser".to_string()),
                api_key_id: Some("key-456".to_string()),
                operation_type: "SET".to_string(),
                entity_type: "PROPERTY".to_string(),
                entity_id: Some("12345".to_string()),
                cypher_query: Some("SET n.name = 'Updated'".to_string()),
                result: AuditResult::Success,
            })
            .await
            .unwrap();

        // Test DELETE operation
        logger
            .log_write_operation(WriteOperationParams {
                actor_user_id: Some("user-123".to_string()),
                actor_username: Some("testuser".to_string()),
                api_key_id: Some("key-456".to_string()),
                operation_type: "DELETE".to_string(),
                entity_type: "NODE".to_string(),
                entity_id: Some("12345".to_string()),
                cypher_query: Some("DELETE n".to_string()),
                result: AuditResult::Success,
            })
            .await
            .unwrap();

        // Test failed operation
        logger
            .log_write_operation(WriteOperationParams {
                actor_user_id: Some("user-123".to_string()),
                actor_username: Some("testuser".to_string()),
                api_key_id: Some("key-456".to_string()),
                operation_type: "CREATE".to_string(),
                entity_type: "NODE".to_string(),
                entity_id: None,
                cypher_query: Some("CREATE (n:Person)".to_string()),
                result: AuditResult::Failure {
                    error: "Invalid syntax".to_string(),
                },
            })
            .await
            .unwrap();

        // Verify log file was created
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let log_file = temp_dir.path().join(format!("audit-{}.log", today));
        assert!(log_file.exists());
    }

    #[tokio::test]
    async fn test_log_authentication_success() {
        let temp_dir = TempDir::new().unwrap();
        let config = AuditConfig {
            enabled: true,
            log_dir: temp_dir.path().to_path_buf(),
            retention_days: 30,
            compress_logs: false,
        };

        let logger = AuditLogger::new(config).unwrap();
        logger
            .log_authentication_success(
                "testuser".to_string(),
                "user-123".to_string(),
                "api_key".to_string(),
            )
            .await
            .unwrap();

        // Verify log file was created
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let log_file = temp_dir.path().join(format!("audit-{}.log", today));
        assert!(log_file.exists());
    }

    #[tokio::test]
    async fn test_log_user_deleted() {
        let temp_dir = TempDir::new().unwrap();
        let config = AuditConfig {
            enabled: true,
            log_dir: temp_dir.path().to_path_buf(),
            retention_days: 30,
            compress_logs: false,
        };

        let logger = AuditLogger::new(config).unwrap();
        logger
            .log_user_deleted(
                Some("admin-123".to_string()),
                Some("admin".to_string()),
                "targetuser".to_string(),
                "user-456".to_string(),
                AuditResult::Success,
            )
            .await
            .unwrap();

        // Verify log file was created
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let log_file = temp_dir.path().join(format!("audit-{}.log", today));
        assert!(log_file.exists());
    }

    #[tokio::test]
    async fn test_log_rotation_edge_case() {
        let temp_dir = TempDir::new().unwrap();
        let config = AuditConfig {
            enabled: true,
            log_dir: temp_dir.path().to_path_buf(),
            retention_days: 30,
            compress_logs: false,
        };

        let logger = AuditLogger::new(config).unwrap();

        // Log multiple entries
        for i in 0..10 {
            logger
                .log_user_created(
                    Some("actor-123".to_string()),
                    Some("admin".to_string()),
                    format!("user{}", i),
                    format!("user-id-{}", i),
                    AuditResult::Success,
                )
                .await
                .unwrap();
        }

        // Verify log file exists and has content
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let log_file = temp_dir.path().join(format!("audit-{}.log", today));
        assert!(log_file.exists());

        // Verify file has multiple lines
        let content = std::fs::read_to_string(&log_file).unwrap();
        let lines: Vec<&str> = content.lines().filter(|l| !l.is_empty()).collect();
        assert!(lines.len() >= 10);
    }

    #[tokio::test]
    async fn test_log_compression_failure_scenario() {
        let temp_dir = TempDir::new().unwrap();
        let config = AuditConfig {
            enabled: true,
            log_dir: temp_dir.path().to_path_buf(),
            retention_days: 30,
            compress_logs: true,
        };

        let logger = AuditLogger::new(config).unwrap();

        // Create a log entry
        logger
            .log_user_created(
                Some("actor-123".to_string()),
                Some("admin".to_string()),
                "testuser".to_string(),
                "user-456".to_string(),
                AuditResult::Success,
            )
            .await
            .unwrap();

        // Verify log file was created (compression happens in background)
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let log_file = temp_dir.path().join(format!("audit-{}.log", today));
        assert!(log_file.exists());
    }

    #[tokio::test]
    async fn test_retention_period_cleanup() {
        let temp_dir = TempDir::new().unwrap();
        let config = AuditConfig {
            enabled: true,
            log_dir: temp_dir.path().to_path_buf(),
            retention_days: 1, // Very short retention for testing
            compress_logs: false,
        };

        let logger = AuditLogger::new(config).unwrap();

        // Create a log entry
        logger
            .log_user_created(
                Some("actor-123".to_string()),
                Some("admin".to_string()),
                "testuser".to_string(),
                "user-456".to_string(),
                AuditResult::Success,
            )
            .await
            .unwrap();

        // Verify log file was created
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let log_file = temp_dir.path().join(format!("audit-{}.log", today));
        assert!(log_file.exists());

        // Test cleanup function directly
        let old_date = Utc::now()
            .checked_sub_signed(chrono::Duration::days(2))
            .unwrap()
            .format("%Y-%m-%d")
            .to_string();

        // Create an old log file
        let old_log_file = temp_dir.path().join(format!("audit-{}.log", old_date));
        std::fs::write(&old_log_file, "old log content").unwrap();
        assert!(old_log_file.exists());

        // Run cleanup
        AuditLogger::cleanup_old_logs_static(temp_dir.path(), 1).unwrap();

        // Old file should be deleted
        assert!(!old_log_file.exists());
        // Today's file should still exist
        assert!(log_file.exists());
    }

    #[tokio::test]
    async fn test_audit_log_with_none_actor() {
        let temp_dir = TempDir::new().unwrap();
        let config = AuditConfig {
            enabled: true,
            log_dir: temp_dir.path().to_path_buf(),
            retention_days: 30,
            compress_logs: false,
        };

        let logger = AuditLogger::new(config).unwrap();

        // Test logging with None actor (unauthenticated operation)
        logger
            .log_user_created(
                None,
                None,
                "testuser".to_string(),
                "user-456".to_string(),
                AuditResult::Success,
            )
            .await
            .unwrap();

        // Verify log file was created
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let log_file = temp_dir.path().join(format!("audit-{}.log", today));
        assert!(log_file.exists());
    }

    #[tokio::test]
    async fn test_audit_log_failure_result() {
        let temp_dir = TempDir::new().unwrap();
        let config = AuditConfig {
            enabled: true,
            log_dir: temp_dir.path().to_path_buf(),
            retention_days: 30,
            compress_logs: false,
        };

        let logger = AuditLogger::new(config).unwrap();

        // Test logging with failure result
        logger
            .log_user_created(
                Some("actor-123".to_string()),
                Some("admin".to_string()),
                "testuser".to_string(),
                "user-456".to_string(),
                AuditResult::Failure {
                    error: "User already exists".to_string(),
                },
            )
            .await
            .unwrap();

        // Verify log file was created
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let log_file = temp_dir.path().join(format!("audit-{}.log", today));
        assert!(log_file.exists());

        // Verify failure is logged
        let content = std::fs::read_to_string(&log_file).unwrap();
        assert!(content.contains("User already exists"));
    }

    #[tokio::test]
    async fn test_audit_log_with_ip_address() {
        let temp_dir = TempDir::new().unwrap();
        let config = AuditConfig {
            enabled: true,
            log_dir: temp_dir.path().to_path_buf(),
            retention_days: 30,
            compress_logs: false,
        };

        let logger = AuditLogger::new(config).unwrap();

        // Test logging with IP address
        logger
            .log_authentication_failed(
                Some("testuser".to_string()),
                "Invalid password".to_string(),
                Some("192.168.1.100".to_string()),
            )
            .await
            .unwrap();

        // Verify log file was created
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let log_file = temp_dir.path().join(format!("audit-{}.log", today));
        assert!(log_file.exists());

        // Verify IP address is logged
        let content = std::fs::read_to_string(&log_file).unwrap();
        assert!(content.contains("192.168.1.100"));
    }
}
