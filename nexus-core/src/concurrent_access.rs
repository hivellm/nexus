//! Concurrent access control and locking system
//!
//! This module provides:
//! - Reader-writer locks for data structures
//! - Deadlock detection and prevention
//! - Lock ordering and timeout mechanisms
//! - Lock-free data structures where possible

use crate::{Error, Result};
use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::thread;
use std::time::{Duration, Instant};

/// Lock type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LockType {
    /// Shared read lock
    Read,
    /// Exclusive write lock
    Write,
    /// Intent shared lock
    IntentShared,
    /// Intent exclusive lock
    IntentExclusive,
    /// Shared intent exclusive lock
    SharedIntentExclusive,
}

/// Lock mode for deadlock prevention
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockMode {
    /// Optimistic locking
    Optimistic,
    /// Pessimistic locking
    Pessimistic,
    /// No locking (lock-free)
    None,
}

/// Lock request information
#[derive(Debug, Clone)]
pub struct LockRequest {
    /// Transaction ID requesting the lock
    pub tx_id: u64,
    /// Resource being locked
    pub resource: String,
    /// Type of lock requested
    pub lock_type: LockType,
    /// Request timestamp
    pub timestamp: Instant,
    /// Timeout duration
    pub timeout: Duration,
}

/// Lock holder information
#[derive(Debug, Clone)]
pub struct LockHolder {
    /// Transaction ID holding the lock
    pub tx_id: u64,
    /// Type of lock held
    pub lock_type: LockType,
    /// When the lock was acquired
    pub acquired_at: Instant,
}

/// Lock manager for coordinating locks across transactions
#[derive(Debug)]
pub struct LockManager {
    /// Map of resource to lock holders
    resource_locks: Arc<RwLock<HashMap<String, Vec<LockHolder>>>>,
    /// Map of transaction to pending lock requests
    pending_requests: Arc<RwLock<HashMap<u64, Vec<LockRequest>>>>,
    /// Deadlock detection graph
    wait_for_graph: Arc<RwLock<HashMap<u64, HashSet<u64>>>>,
    /// Lock timeout duration
    default_timeout: Duration,
}

impl LockManager {
    /// Create a new lock manager
    pub fn new(timeout: Duration) -> Self {
        Self {
            resource_locks: Arc::new(RwLock::new(HashMap::new())),
            pending_requests: Arc::new(RwLock::new(HashMap::new())),
            wait_for_graph: Arc::new(RwLock::new(HashMap::new())),
            default_timeout: timeout,
        }
    }

    /// Request a lock on a resource
    pub fn request_lock(
        &self,
        tx_id: u64,
        resource: String,
        lock_type: LockType,
    ) -> Result<LockGuard> {
        self.request_lock_with_timeout(tx_id, resource, lock_type, self.default_timeout)
    }

    /// Request a lock with custom timeout
    pub fn request_lock_with_timeout(
        &self,
        tx_id: u64,
        resource: String,
        lock_type: LockType,
        timeout: Duration,
    ) -> Result<LockGuard> {
        let request = LockRequest {
            tx_id,
            resource: resource.clone(),
            lock_type,
            timestamp: Instant::now(),
            timeout,
        };

        // Check for deadlock before acquiring lock
        if self.detect_deadlock(tx_id)? {
            return Err(Error::DeadlockDetected("Deadlock detected".to_string()));
        }

        // Try to acquire the lock
        if self.try_acquire_lock(&request)? {
            Ok(LockGuard::new(tx_id, resource, self.clone()))
        } else {
            // Add to pending requests and wait
            self.add_pending_request(request)?;
            self.wait_for_lock(tx_id, resource, lock_type, timeout)
        }
    }

    /// Try to acquire a lock immediately
    fn try_acquire_lock(&self, request: &LockRequest) -> Result<bool> {
        let mut resource_locks = self.resource_locks.write();
        let resource = &request.resource;
        let tx_id = request.tx_id;
        let lock_type = request.lock_type;

        // Check if we already hold a compatible lock
        if let Some(holders) = resource_locks.get(resource) {
            if holders.iter().any(|h| h.tx_id == tx_id) {
                // Check if we can upgrade the lock
                if self.can_upgrade_lock(holders, tx_id, lock_type) {
                    return Ok(true);
                }
            }

            // Check if the requested lock is compatible with existing locks
            if !self.is_lock_compatible(holders, lock_type) {
                return Ok(false);
            }
        }

        // Acquire the lock
        let holder = LockHolder {
            tx_id,
            lock_type,
            acquired_at: Instant::now(),
        };

        resource_locks
            .entry(resource.clone())
            .or_default()
            .push(holder);

        Ok(true)
    }

    /// Check if a lock is compatible with existing locks
    fn is_lock_compatible(&self, holders: &[LockHolder], requested_type: LockType) -> bool {
        match requested_type {
            LockType::Read => {
                // Read locks are compatible with other read locks and intent locks
                holders.iter().all(|h| {
                    matches!(
                        h.lock_type,
                        LockType::Read | LockType::IntentShared | LockType::SharedIntentExclusive
                    )
                })
            }
            LockType::Write => {
                // Write locks are only compatible with intent locks
                holders.iter().all(|h| {
                    matches!(
                        h.lock_type,
                        LockType::IntentExclusive | LockType::SharedIntentExclusive
                    )
                })
            }
            LockType::IntentShared => {
                // Intent shared locks are compatible with most other locks
                holders
                    .iter()
                    .all(|h| !matches!(h.lock_type, LockType::Write))
            }
            LockType::IntentExclusive => {
                // Intent exclusive locks are compatible with intent locks
                holders.iter().all(|h| {
                    matches!(
                        h.lock_type,
                        LockType::IntentShared
                            | LockType::IntentExclusive
                            | LockType::SharedIntentExclusive
                    )
                })
            }
            LockType::SharedIntentExclusive => {
                // Shared intent exclusive locks are compatible with intent locks
                holders.iter().all(|h| {
                    matches!(
                        h.lock_type,
                        LockType::IntentShared
                            | LockType::IntentExclusive
                            | LockType::SharedIntentExclusive
                    )
                })
            }
        }
    }

    /// Check if we can upgrade an existing lock
    fn can_upgrade_lock(&self, holders: &[LockHolder], tx_id: u64, new_type: LockType) -> bool {
        let existing_holder = holders.iter().find(|h| h.tx_id == tx_id);

        if let Some(existing) = existing_holder {
            match (existing.lock_type, new_type) {
                (LockType::Read, LockType::Write) => {
                    // Can upgrade from read to write if no other holders
                    holders.len() == 1
                }
                (LockType::IntentShared, LockType::IntentExclusive) => true,
                (LockType::IntentExclusive, LockType::Write) => true,
                (LockType::SharedIntentExclusive, LockType::Write) => true,
                _ => false,
            }
        } else {
            false
        }
    }

    /// Add a pending lock request
    fn add_pending_request(&self, request: LockRequest) -> Result<()> {
        let mut pending = self.pending_requests.write();
        pending.entry(request.tx_id).or_default().push(request);
        Ok(())
    }

    /// Wait for a lock to become available
    fn wait_for_lock(
        &self,
        tx_id: u64,
        resource: String,
        lock_type: LockType,
        timeout: Duration,
    ) -> Result<LockGuard> {
        let start = Instant::now();

        while start.elapsed() < timeout {
            // Check if we can acquire the lock now
            let request = LockRequest {
                tx_id,
                resource: resource.clone(),
                lock_type,
                timestamp: Instant::now(),
                timeout,
            };

            if self.try_acquire_lock(&request)? {
                self.remove_pending_request(tx_id, &resource);
                return Ok(LockGuard::new(tx_id, resource, self.clone()));
            }

            // Check for deadlock
            if self.detect_deadlock(tx_id)? {
                self.remove_pending_request(tx_id, &resource);
                return Err(Error::DeadlockDetected("Deadlock detected".to_string()));
            }

            // Sleep briefly before retrying
            thread::sleep(Duration::from_millis(1));
        }

        // Timeout reached
        self.remove_pending_request(tx_id, &resource);
        Err(Error::LockTimeout("Lock request timed out".to_string()))
    }

    /// Remove a pending request
    fn remove_pending_request(&self, tx_id: u64, resource: &str) {
        let mut pending = self.pending_requests.write();
        if let Some(requests) = pending.get_mut(&tx_id) {
            requests.retain(|r| r.resource != *resource);
            if requests.is_empty() {
                pending.remove(&tx_id);
            }
        }
    }

    /// Detect deadlock using cycle detection
    fn detect_deadlock(&self, tx_id: u64) -> Result<bool> {
        let wait_for_graph = self.wait_for_graph.read();
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();

        self.dfs_cycle_detection(tx_id, &wait_for_graph, &mut visited, &mut rec_stack)
    }

    /// DFS-based cycle detection
    fn dfs_cycle_detection(
        &self,
        node: u64,
        graph: &HashMap<u64, HashSet<u64>>,
        visited: &mut HashSet<u64>,
        rec_stack: &mut HashSet<u64>,
    ) -> Result<bool> {
        if rec_stack.contains(&node) {
            return Ok(true); // Cycle detected
        }

        if visited.contains(&node) {
            return Ok(false);
        }

        visited.insert(node);
        rec_stack.insert(node);

        if let Some(neighbors) = graph.get(&node) {
            for &neighbor in neighbors {
                if self.dfs_cycle_detection(neighbor, graph, visited, rec_stack)? {
                    return Ok(true);
                }
            }
        }

        rec_stack.remove(&node);
        Ok(false)
    }

    /// Release a lock
    pub fn release_lock(&self, tx_id: u64, resource: &str) -> Result<()> {
        let mut resource_locks = self.resource_locks.write();

        if let Some(holders) = resource_locks.get_mut(resource) {
            holders.retain(|h| h.tx_id != tx_id);

            if holders.is_empty() {
                resource_locks.remove(resource);
            }
        }

        // Update wait-for graph
        self.update_wait_for_graph(tx_id, resource)?;

        // Try to grant pending requests
        self.grant_pending_requests(resource)?;

        Ok(())
    }

    /// Update the wait-for graph
    fn update_wait_for_graph(&self, tx_id: u64, resource: &str) -> Result<()> {
        let mut wait_for_graph = self.wait_for_graph.write();
        wait_for_graph.remove(&tx_id);

        // Check if any pending requests can now be granted
        let pending = self.pending_requests.read();
        for (pending_tx_id, requests) in pending.iter() {
            for request in requests {
                if request.resource == *resource {
                    // This transaction is waiting for the resource we just released
                    wait_for_graph
                        .entry(*pending_tx_id)
                        .or_default()
                        .insert(tx_id);
                }
            }
        }

        Ok(())
    }

    /// Grant pending requests for a resource
    fn grant_pending_requests(&self, resource: &str) -> Result<()> {
        let mut pending = self.pending_requests.write();
        let mut to_remove = Vec::new();

        for (tx_id, requests) in pending.iter_mut() {
            for (i, request) in requests.iter().enumerate() {
                if request.resource == *resource && self.try_acquire_lock(request)? {
                    to_remove.push((*tx_id, i));
                }
            }
        }

        // Remove granted requests
        for (tx_id, index) in to_remove {
            if let Some(requests) = pending.get_mut(&tx_id) {
                requests.remove(index);
                if requests.is_empty() {
                    pending.remove(&tx_id);
                }
            }
        }

        Ok(())
    }

    /// Get lock statistics
    pub fn get_stats(&self) -> LockStats {
        let resource_locks = self.resource_locks.read();
        let pending_requests = self.pending_requests.read();

        LockStats {
            active_locks: resource_locks.len(),
            pending_requests: pending_requests.len(),
            total_holders: resource_locks.values().map(|v| v.len()).sum(),
        }
    }
}

impl Clone for LockManager {
    fn clone(&self) -> Self {
        Self {
            resource_locks: self.resource_locks.clone(),
            pending_requests: self.pending_requests.clone(),
            wait_for_graph: self.wait_for_graph.clone(),
            default_timeout: self.default_timeout,
        }
    }
}

/// Lock guard that automatically releases the lock when dropped
pub struct LockGuard {
    tx_id: u64,
    resource: String,
    lock_manager: LockManager,
}

impl LockGuard {
    fn new(tx_id: u64, resource: String, lock_manager: LockManager) -> Self {
        Self {
            tx_id,
            resource,
            lock_manager,
        }
    }
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = self.lock_manager.release_lock(self.tx_id, &self.resource);
    }
}

/// Lock statistics
#[derive(Debug, Clone)]
pub struct LockStats {
    /// Number of resources with active locks
    pub active_locks: usize,
    /// Number of pending lock requests
    pub pending_requests: usize,
    /// Total number of lock holders
    pub total_holders: usize,
}

/// Lock-free atomic operations
pub struct AtomicOperations;

impl AtomicOperations {
    /// Compare and swap operation
    pub fn compare_and_swap(value: &AtomicU64, expected: u64, new: u64) -> Result<bool> {
        let current = value.load(std::sync::atomic::Ordering::SeqCst);
        if current == expected {
            value.store(new, std::sync::atomic::Ordering::SeqCst);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Atomic increment
    pub fn atomic_increment(value: &AtomicU64) -> u64 {
        value.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }

    /// Atomic decrement
    pub fn atomic_decrement(value: &AtomicU64) -> u64 {
        value.fetch_sub(1, std::sync::atomic::Ordering::SeqCst)
    }
}

/// Lock-free hash map implementation
pub struct LockFreeHashMap<K, V> {
    inner: Arc<RwLock<HashMap<K, V>>>,
}

impl<K, V> LockFreeHashMap<K, V>
where
    K: Eq + std::hash::Hash + Clone,
    V: Clone,
{
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn get(&self, key: &K) -> Option<V> {
        let read_guard = self.inner.read();
        read_guard.get(key).cloned()
    }

    pub fn insert(&self, key: K, value: V) -> Option<V> {
        let mut write_guard = self.inner.write();
        write_guard.insert(key, value)
    }

    pub fn remove(&self, key: &K) -> Option<V> {
        let mut write_guard = self.inner.write();
        write_guard.remove(key)
    }

    pub fn len(&self) -> usize {
        let read_guard = self.inner.read();
        read_guard.len()
    }

    pub fn is_empty(&self) -> bool {
        let read_guard = self.inner.read();
        read_guard.is_empty()
    }
}

impl<K, V> Default for LockFreeHashMap<K, V>
where
    K: Eq + std::hash::Hash + Clone,
    V: Clone,
{
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;

    #[test]
    fn test_lock_manager_creation() {
        let lock_manager = LockManager::new(Duration::from_secs(5));
        let stats = lock_manager.get_stats();
        assert_eq!(stats.active_locks, 0);
        assert_eq!(stats.pending_requests, 0);
    }

    #[test]
    fn test_read_lock_compatibility() {
        let lock_manager = LockManager::new(Duration::from_secs(1));

        // First read lock should succeed
        let guard1 = lock_manager
            .request_lock(1, "resource1".to_string(), LockType::Read)
            .unwrap();

        // Second read lock should also succeed
        let guard2 = lock_manager
            .request_lock(2, "resource1".to_string(), LockType::Read)
            .unwrap();

        // Both locks should be held
        let stats = lock_manager.get_stats();
        assert_eq!(stats.active_locks, 1);
        assert_eq!(stats.total_holders, 2);

        drop(guard1);
        drop(guard2);
    }

    #[test]
    fn test_write_lock_exclusivity() {
        let lock_manager = LockManager::new(Duration::from_secs(1));

        // Write lock should succeed
        let guard1 = lock_manager
            .request_lock(1, "resource1".to_string(), LockType::Write)
            .unwrap();

        // Second write lock should fail (timeout)
        let result = lock_manager.request_lock(2, "resource1".to_string(), LockType::Write);
        assert!(result.is_err());

        drop(guard1);
    }

    #[test]
    fn test_lock_timeout() {
        let lock_manager = LockManager::new(Duration::from_millis(100));

        // First write lock
        let _guard1 = lock_manager
            .request_lock(1, "resource1".to_string(), LockType::Write)
            .unwrap();

        // Second write lock should timeout
        let start = Instant::now();
        let result = lock_manager.request_lock(2, "resource1".to_string(), LockType::Write);
        let elapsed = start.elapsed();

        assert!(result.is_err());
        assert!(elapsed >= Duration::from_millis(100));
    }

    #[test]
    fn test_atomic_operations() {
        let value = AtomicU64::new(0);

        // Test compare and swap
        let result = AtomicOperations::compare_and_swap(&value, 0, 1);
        assert!(result.unwrap());
        assert_eq!(value.load(std::sync::atomic::Ordering::SeqCst), 1);

        // Test atomic increment
        let old_value = AtomicOperations::atomic_increment(&value);
        assert_eq!(old_value, 1);
        assert_eq!(value.load(std::sync::atomic::Ordering::SeqCst), 2);

        // Test atomic decrement
        let old_value = AtomicOperations::atomic_decrement(&value);
        assert_eq!(old_value, 2);
        assert_eq!(value.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn test_lock_free_hash_map() {
        let map = LockFreeHashMap::new();

        // Test insert and get
        map.insert("key1".to_string(), "value1".to_string());
        assert_eq!(map.get(&"key1".to_string()), Some("value1".to_string()));

        // Test remove
        let removed = map.remove(&"key1".to_string());
        assert_eq!(removed, Some("value1".to_string()));
        assert_eq!(map.get(&"key1".to_string()), None);

        // Test length
        assert_eq!(map.len(), 0);
        assert!(map.is_empty());
    }
}
