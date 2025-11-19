//! Row-level locking for fine-grained concurrency control
//!
//! This module provides row-level locks for nodes and relationships,
//! allowing concurrent writes to different resources while maintaining
//! data consistency.

use crate::{Error, Result};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Resource type for locking
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceType {
    /// Node resource
    Node,
    /// Relationship resource
    Relationship,
}

/// Resource identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResourceId {
    /// Resource type
    pub resource_type: ResourceType,
    /// Resource ID
    pub id: u64,
}

impl ResourceId {
    /// Create a new node resource ID
    pub fn node(id: u64) -> Self {
        Self {
            resource_type: ResourceType::Node,
            id,
        }
    }

    /// Create a new relationship resource ID
    pub fn relationship(id: u64) -> Self {
        Self {
            resource_type: ResourceType::Relationship,
            id,
        }
    }
}

/// Lock holder information
#[derive(Debug, Clone)]
struct LockHolder {
    /// Transaction ID holding the lock
    tx_id: u64,
    /// Lock type (read or write)
    is_write: bool,
    /// When the lock was acquired
    acquired_at: Instant,
}

/// Row-level lock manager
#[derive(Debug, Clone)]
pub struct RowLockManager {
    /// Map of resource to lock holders
    locks: Arc<RwLock<HashMap<ResourceId, Vec<LockHolder>>>>,
    /// Lock escalation threshold (number of row locks before escalating to table lock)
    escalation_threshold: usize,
    /// Default timeout for lock acquisition
    default_timeout: Duration,
}

impl RowLockManager {
    /// Create a new row lock manager
    pub fn new(escalation_threshold: usize, timeout: Duration) -> Self {
        Self {
            locks: Arc::new(RwLock::new(HashMap::new())),
            escalation_threshold,
            default_timeout: timeout,
        }
    }

    /// Create with default settings
    pub fn default() -> Self {
        Self::new(100, Duration::from_secs(5))
    }

    /// Acquire a read lock on a resource
    pub fn acquire_read(&self, tx_id: u64, resource: ResourceId) -> Result<RowLockGuard> {
        self.acquire_read_with_timeout(tx_id, resource, self.default_timeout)
    }

    /// Acquire a read lock with timeout
    pub fn acquire_read_with_timeout(
        &self,
        tx_id: u64,
        resource: ResourceId,
        timeout: Duration,
    ) -> Result<RowLockGuard> {
        let start = Instant::now();

        while start.elapsed() < timeout {
            let mut locks = self.locks.write();

            // Check if we can acquire the lock
            if let Some(holders) = locks.get(&resource) {
                // Check if there's a write lock held by another transaction
                let has_conflicting_write = holders.iter().any(|h| h.tx_id != tx_id && h.is_write);

                if has_conflicting_write {
                    // Wait a bit and retry
                    drop(locks);
                    std::thread::sleep(Duration::from_millis(1));
                    continue;
                }
            }

            // Acquire the lock
            let holder = LockHolder {
                tx_id,
                is_write: false,
                acquired_at: Instant::now(),
            };

            locks.entry(resource).or_default().push(holder);

            return Ok(RowLockGuard {
                manager: self.clone(),
                tx_id,
                resource,
            });
        }

        Err(Error::LockTimeout(format!(
            "Failed to acquire read lock on {:?} within timeout",
            resource
        )))
    }

    /// Acquire a write lock on a resource
    pub fn acquire_write(&self, tx_id: u64, resource: ResourceId) -> Result<RowLockGuard> {
        self.acquire_write_with_timeout(tx_id, resource, self.default_timeout)
    }

    /// Acquire a write lock with timeout
    pub fn acquire_write_with_timeout(
        &self,
        tx_id: u64,
        resource: ResourceId,
        timeout: Duration,
    ) -> Result<RowLockGuard> {
        let start = Instant::now();

        while start.elapsed() < timeout {
            let mut locks = self.locks.write();

            // Check if we can acquire the lock
            if let Some(holders) = locks.get(&resource) {
                // Check if there are any other holders
                let has_other_holders = holders.iter().any(|h| h.tx_id != tx_id);

                if has_other_holders {
                    // Wait a bit and retry
                    drop(locks);
                    std::thread::sleep(Duration::from_millis(1));
                    continue;
                }
            }

            // Acquire the lock
            let holder = LockHolder {
                tx_id,
                is_write: true,
                acquired_at: Instant::now(),
            };

            locks.entry(resource).or_default().push(holder);

            return Ok(RowLockGuard {
                manager: self.clone(),
                tx_id,
                resource,
            });
        }

        Err(Error::LockTimeout(format!(
            "Failed to acquire write lock on {:?} within timeout",
            resource
        )))
    }

    /// Acquire multiple write locks atomically
    pub fn acquire_multiple_write(
        &self,
        tx_id: u64,
        resources: &[ResourceId],
    ) -> Result<Vec<RowLockGuard>> {
        // Lock escalation: if we need to lock too many resources,
        // it may be more efficient to use a table-level lock.
        // However, for now we still use row locks to maintain fine-grained control.
        // Future optimization: implement table-level lock when threshold is exceeded.

        if resources.len() >= self.escalation_threshold {
            // Log that we're acquiring many locks (potential optimization point)
            // For now, we still acquire row locks individually
        }

        let mut guards = Vec::new();

        // Try to acquire all locks
        for resource in resources {
            match self.acquire_write(tx_id, *resource) {
                Ok(guard) => {
                    guards.push(guard);
                }
                Err(e) => {
                    // Release all acquired locks
                    for guard in guards {
                        drop(guard);
                    }
                    return Err(e);
                }
            }
        }

        Ok(guards)
    }

    /// Release a lock
    fn release(&self, tx_id: u64, resource: ResourceId) {
        let mut locks = self.locks.write();

        if let Some(holders) = locks.get_mut(&resource) {
            holders.retain(|h| h.tx_id != tx_id);

            if holders.is_empty() {
                locks.remove(&resource);
            }
        }
    }

    /// Get lock statistics
    pub fn stats(&self) -> LockStats {
        let locks = self.locks.read();
        let total_locks = locks.len();
        let total_holders: usize = locks.values().map(|v| v.len()).sum();

        let read_locks = locks.values().flatten().filter(|h| !h.is_write).count();
        let write_locks = locks.values().flatten().filter(|h| h.is_write).count();

        LockStats {
            total_resources: total_locks,
            total_holders,
            read_locks,
            write_locks,
        }
    }
}

/// Lock guard that automatically releases the lock when dropped
pub struct RowLockGuard {
    manager: RowLockManager,
    tx_id: u64,
    resource: ResourceId,
}

impl Drop for RowLockGuard {
    fn drop(&mut self) {
        self.manager.release(self.tx_id, self.resource);
    }
}

/// Lock statistics
#[derive(Debug, Clone)]
pub struct LockStats {
    /// Total number of resources with locks
    pub total_resources: usize,
    /// Total number of lock holders
    pub total_holders: usize,
    /// Number of read locks
    pub read_locks: usize,
    /// Number of write locks
    pub write_locks: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_lock_compatibility() {
        let manager = RowLockManager::default();
        let resource = ResourceId::node(1);

        // First read lock should succeed
        let guard1 = manager.acquire_read(1, resource).unwrap();

        // Second read lock should also succeed
        let guard2 = manager.acquire_read(2, resource).unwrap();

        // Both locks should be held
        let stats = manager.stats();
        assert_eq!(stats.total_resources, 1);
        assert_eq!(stats.total_holders, 2);
        assert_eq!(stats.read_locks, 2);

        drop(guard1);
        drop(guard2);
    }

    #[test]
    fn test_write_lock_exclusivity() {
        let manager = RowLockManager::default();
        let resource = ResourceId::node(1);

        // Write lock should succeed
        let guard1 = manager.acquire_write(1, resource).unwrap();

        // Second write lock should fail (timeout)
        let result = manager.acquire_write_with_timeout(2, resource, Duration::from_millis(100));
        assert!(result.is_err());

        drop(guard1);
    }

    #[test]
    fn test_read_write_conflict() {
        let manager = RowLockManager::default();
        let resource = ResourceId::node(1);

        // Read lock should succeed
        let read_guard = manager.acquire_read(1, resource).unwrap();

        // Write lock should fail (timeout)
        let result = manager.acquire_write_with_timeout(2, resource, Duration::from_millis(100));
        assert!(result.is_err());

        drop(read_guard);

        // Now write lock should succeed
        let write_guard = manager.acquire_write(2, resource).unwrap();
        drop(write_guard);
    }

    #[test]
    fn test_multiple_resources() {
        let manager = RowLockManager::default();

        // Lock different resources concurrently
        let guard1 = manager.acquire_write(1, ResourceId::node(1)).unwrap();
        let guard2 = manager.acquire_write(2, ResourceId::node(2)).unwrap();
        let guard3 = manager
            .acquire_write(3, ResourceId::relationship(1))
            .unwrap();

        let stats = manager.stats();
        assert_eq!(stats.total_resources, 3);
        assert_eq!(stats.total_holders, 3);

        drop(guard1);
        drop(guard2);
        drop(guard3);
    }

    #[test]
    fn test_multiple_write_locks() {
        let manager = RowLockManager::default();
        let resources = vec![
            ResourceId::node(1),
            ResourceId::node(2),
            ResourceId::node(3),
        ];

        let guards = manager.acquire_multiple_write(1, &resources).unwrap();
        assert_eq!(guards.len(), 3);

        let stats = manager.stats();
        assert_eq!(stats.total_resources, 3);
        assert_eq!(stats.write_locks, 3);

        drop(guards);
    }
}
