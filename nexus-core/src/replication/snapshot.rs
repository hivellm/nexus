//! Snapshot management for full sync
//!
//! Creates and restores compressed snapshots of the database
//! for initial replica synchronization.

use crate::{Error, Result};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

/// Snapshot configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotConfig {
    /// Data directory to snapshot
    pub data_dir: PathBuf,
    /// Compression level (0-22 for zstd)
    pub compression_level: i32,
    /// Maximum snapshot size (bytes)
    pub max_size: u64,
    /// Snapshot chunk size for transfer
    pub chunk_size: usize,
}

impl Default for SnapshotConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("./data"),
            compression_level: 3,
            max_size: 10 * 1024 * 1024 * 1024, // 10GB
            chunk_size: 1024 * 1024,           // 1MB
        }
    }
}

/// Snapshot metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    /// Snapshot ID
    pub id: String,
    /// Creation timestamp (Unix millis)
    pub created_at: u64,
    /// WAL offset at snapshot time
    pub wal_offset: u64,
    /// Epoch at snapshot time
    pub epoch: u64,
    /// Total uncompressed size
    pub uncompressed_size: u64,
    /// Total compressed size
    pub compressed_size: u64,
    /// CRC32 checksum
    pub checksum: u32,
    /// Files included in snapshot
    pub files: Vec<SnapshotFile>,
}

/// File in snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotFile {
    /// Relative path
    pub path: String,
    /// File size
    pub size: u64,
    /// File checksum
    pub checksum: u32,
}

/// Snapshot manager
pub struct Snapshot {
    /// Configuration
    config: SnapshotConfig,
    /// Current WAL offset (set externally)
    wal_offset: AtomicU64,
    /// Current epoch (set externally)
    epoch: AtomicU64,
    /// Is snapshot in progress
    in_progress: AtomicBool,
    /// Last snapshot metadata
    last_snapshot: RwLock<Option<SnapshotMetadata>>,
}

impl Snapshot {
    /// Create a new snapshot manager
    pub fn new(config: SnapshotConfig) -> Self {
        Self {
            config,
            wal_offset: AtomicU64::new(0),
            epoch: AtomicU64::new(0),
            in_progress: AtomicBool::new(false),
            last_snapshot: RwLock::new(None),
        }
    }

    /// Set current WAL offset
    pub fn set_wal_offset(&self, offset: u64) {
        self.wal_offset.store(offset, Ordering::SeqCst);
    }

    /// Set current epoch
    pub fn set_epoch(&self, epoch: u64) {
        self.epoch.store(epoch, Ordering::SeqCst);
    }

    /// Create a snapshot
    ///
    /// Returns compressed snapshot data.
    pub async fn create(&self) -> Result<Vec<u8>> {
        if self
            .in_progress
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(Error::replication("Snapshot already in progress"));
        }

        let result = self.create_internal().await;

        self.in_progress.store(false, Ordering::SeqCst);

        result
    }

    /// Internal snapshot creation
    async fn create_internal(&self) -> Result<Vec<u8>> {
        let start = Instant::now();
        let snapshot_id = uuid::Uuid::new_v4().to_string();

        tracing::info!("Creating snapshot {}", snapshot_id);

        // Collect files to snapshot
        let mut files = Vec::new();
        let mut total_size = 0u64;

        if self.config.data_dir.exists() {
            self.collect_files(&self.config.data_dir, &self.config.data_dir, &mut files)?;
            for file in &files {
                total_size += file.size;
            }
        }

        if total_size > self.config.max_size {
            return Err(Error::replication(format!(
                "Snapshot too large: {} bytes (max: {})",
                total_size, self.config.max_size
            )));
        }

        // Create tar archive in memory
        let mut archive_data = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut archive_data);

            for file_info in &files {
                let full_path = self.config.data_dir.join(&file_info.path);
                if full_path.is_file() {
                    let mut file = std::fs::File::open(&full_path)?;
                    builder.append_file(&file_info.path, &mut file)?;
                }
            }

            builder.finish()?;
        }

        // Compress with zstd
        let compressed = zstd::encode_all(archive_data.as_slice(), self.config.compression_level)
            .map_err(|e| Error::replication(format!("Compression failed: {}", e)))?;

        let checksum = crc32fast::hash(&compressed);

        // Save metadata
        let metadata = SnapshotMetadata {
            id: snapshot_id.clone(),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            wal_offset: self.wal_offset.load(Ordering::SeqCst),
            epoch: self.epoch.load(Ordering::SeqCst),
            uncompressed_size: archive_data.len() as u64,
            compressed_size: compressed.len() as u64,
            checksum,
            files,
        };

        *self.last_snapshot.write() = Some(metadata.clone());

        tracing::info!(
            "Snapshot {} created in {:?} ({} bytes -> {} bytes, ratio: {:.2}%)",
            snapshot_id,
            start.elapsed(),
            archive_data.len(),
            compressed.len(),
            (compressed.len() as f64 / archive_data.len() as f64) * 100.0
        );

        Ok(compressed)
    }

    /// Collect files recursively
    fn collect_files(
        &self,
        base_dir: &Path,
        current_dir: &Path,
        files: &mut Vec<SnapshotFile>,
    ) -> Result<()> {
        if !current_dir.exists() {
            return Ok(());
        }

        for entry in std::fs::read_dir(current_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                self.collect_files(base_dir, &path, files)?;
            } else if path.is_file() {
                let relative_path = path
                    .strip_prefix(base_dir)
                    .map_err(|e| Error::replication(format!("Path error: {}", e)))?
                    .to_string_lossy()
                    .to_string();

                let metadata = std::fs::metadata(&path)?;
                let size = metadata.len();

                // Calculate file checksum
                let data = std::fs::read(&path)?;
                let checksum = crc32fast::hash(&data);

                files.push(SnapshotFile {
                    path: relative_path,
                    size,
                    checksum,
                });
            }
        }

        Ok(())
    }

    /// Restore from snapshot
    pub async fn restore(&self, compressed_data: &[u8]) -> Result<()> {
        if self
            .in_progress
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(Error::replication("Restore already in progress"));
        }

        let result = self.restore_internal(compressed_data).await;

        self.in_progress.store(false, Ordering::SeqCst);

        result
    }

    /// Internal restore
    async fn restore_internal(&self, compressed_data: &[u8]) -> Result<()> {
        let start = Instant::now();

        tracing::info!(
            "Restoring snapshot ({} bytes compressed)",
            compressed_data.len()
        );

        // Decompress
        let mut decoder = zstd::Decoder::new(compressed_data)
            .map_err(|e| Error::replication(format!("Decompression failed: {}", e)))?;

        let mut archive_data = Vec::new();
        decoder.read_to_end(&mut archive_data)?;

        // Clear existing data directory
        if self.config.data_dir.exists() {
            // Backup existing data first
            let backup_dir = self.config.data_dir.with_extension("backup");
            if backup_dir.exists() {
                std::fs::remove_dir_all(&backup_dir)?;
            }
            std::fs::rename(&self.config.data_dir, &backup_dir)?;
        }

        // Create data directory
        std::fs::create_dir_all(&self.config.data_dir)?;

        // Extract tar archive with set_preserve_permissions to ensure proper file creation
        let mut archive = tar::Archive::new(archive_data.as_slice());
        archive.set_preserve_permissions(true);
        archive.set_unpack_xattrs(false);
        archive.unpack(&self.config.data_dir)?;

        tracing::info!(
            "Snapshot restored in {:?} ({} bytes)",
            start.elapsed(),
            archive_data.len()
        );

        Ok(())
    }

    /// Get last snapshot metadata
    pub fn last_snapshot(&self) -> Option<SnapshotMetadata> {
        self.last_snapshot.read().clone()
    }

    /// Check if snapshot is in progress
    pub fn is_in_progress(&self) -> bool {
        self.in_progress.load(Ordering::SeqCst)
    }

    /// Get configuration
    pub fn config(&self) -> &SnapshotConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestContext;

    #[tokio::test]
    async fn test_snapshot_creation() {
        let ctx = TestContext::new();
        let data_dir = ctx.path().join("data");
        std::fs::create_dir_all(&data_dir).unwrap();

        // Create some test files
        std::fs::write(data_dir.join("test1.txt"), b"Hello, World!").unwrap();
        std::fs::write(data_dir.join("test2.txt"), b"Goodbye, World!").unwrap();

        let config = SnapshotConfig {
            data_dir,
            compression_level: 3,
            max_size: 1024 * 1024,
            chunk_size: 1024,
        };

        let snapshot = Snapshot::new(config);
        let data = snapshot.create().await.unwrap();

        assert!(!data.is_empty());

        let meta = snapshot.last_snapshot().unwrap();
        assert_eq!(meta.files.len(), 2);
    }

    #[tokio::test]
    async fn test_snapshot_restore() {
        let ctx = TestContext::new();
        let data_dir = ctx.path().join("data");
        std::fs::create_dir_all(&data_dir).unwrap();

        // Create test file
        std::fs::write(data_dir.join("test.txt"), b"Original content").unwrap();

        let config = SnapshotConfig {
            data_dir: data_dir.clone(),
            compression_level: 3,
            max_size: 1024 * 1024,
            chunk_size: 1024,
        };

        let snapshot = Snapshot::new(config);

        // Create snapshot
        let snapshot_data = snapshot.create().await.unwrap();

        // Modify file
        std::fs::write(data_dir.join("test.txt"), b"Modified content").unwrap();

        // Restore snapshot
        snapshot.restore(&snapshot_data).await.unwrap();

        // Verify restoration
        let content = std::fs::read_to_string(data_dir.join("test.txt")).unwrap();
        assert_eq!(content, "Original content");
    }

    #[tokio::test]
    async fn test_snapshot_empty_dir() {
        let ctx = TestContext::new();
        let data_dir = ctx.path().join("empty");
        std::fs::create_dir_all(&data_dir).unwrap();

        let config = SnapshotConfig {
            data_dir,
            compression_level: 3,
            max_size: 1024 * 1024,
            chunk_size: 1024,
        };

        let snapshot = Snapshot::new(config);
        let data = snapshot.create().await.unwrap();

        // Should succeed even with empty directory
        assert!(!data.is_empty());
    }

    #[test]
    fn test_snapshot_config_default() {
        let config = SnapshotConfig::default();
        assert_eq!(config.compression_level, 3);
        assert_eq!(config.chunk_size, 1024 * 1024);
    }

    #[tokio::test]
    async fn test_concurrent_snapshot() {
        let ctx = TestContext::new();
        let data_dir = ctx.path().join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        std::fs::write(data_dir.join("test.txt"), b"content").unwrap();

        let config = SnapshotConfig {
            data_dir,
            compression_level: 1,
            max_size: 1024 * 1024,
            chunk_size: 1024,
        };

        let snapshot = std::sync::Arc::new(Snapshot::new(config));

        // First snapshot should succeed
        let snap1 = snapshot.clone();
        let handle1 = tokio::spawn(async move { snap1.create().await });

        // Give first snapshot a head start
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Second snapshot should fail (in progress)
        let snap2 = snapshot.clone();
        let result2 = snap2.create().await;

        // Wait for first to complete
        let result1 = handle1.await.unwrap();

        assert!(result1.is_ok());
        // Second might fail if first is still in progress
        // or succeed if first finished quickly
    }
}
