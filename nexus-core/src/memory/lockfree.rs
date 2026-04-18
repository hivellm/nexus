//! Lock-Free Data Structures
//!
//! This module provides lock-free alternatives to RwLock-based structures
//! for improved concurrent performance.

use crate::Result;
use std::sync::Arc;
use std::sync::atomic::{AtomicPtr, AtomicU64, Ordering};

/// Lock-free counter using atomic operations
#[derive(Debug)]
pub struct LockFreeCounter {
    value: AtomicU64,
}

impl LockFreeCounter {
    /// Create a new lock-free counter
    pub fn new(initial: u64) -> Self {
        Self {
            value: AtomicU64::new(initial),
        }
    }

    /// Increment the counter and return the new value
    pub fn increment(&self) -> u64 {
        self.value.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Decrement the counter and return the new value
    pub fn decrement(&self) -> u64 {
        self.value.fetch_sub(1, Ordering::Relaxed) - 1
    }

    /// Get the current value
    pub fn get(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }

    /// Add a value and return the previous value
    pub fn add(&self, delta: u64) -> u64 {
        self.value.fetch_add(delta, Ordering::Relaxed)
    }

    /// Compare and swap operation
    pub fn compare_and_swap(&self, expected: u64, new: u64) -> Result<u64> {
        match self
            .value
            .compare_exchange(expected, new, Ordering::SeqCst, Ordering::SeqCst)
        {
            Ok(old) => Ok(old),
            Err(current) => Err(crate::Error::Internal(format!(
                "Compare and swap failed: expected {}, got {}",
                expected, current
            ))),
        }
    }
}

impl Default for LockFreeCounter {
    fn default() -> Self {
        Self::new(0)
    }
}

/// Lock-free stack using atomic pointers
pub struct LockFreeStack<T> {
    head: AtomicPtr<Node<T>>,
}

struct Node<T> {
    value: T,
    next: *mut Node<T>,
}

impl<T> LockFreeStack<T> {
    /// Create a new lock-free stack
    pub fn new() -> Self {
        Self {
            head: AtomicPtr::new(std::ptr::null_mut()),
        }
    }

    /// Push a value onto the stack
    pub fn push(&self, value: T) {
        let node = Box::into_raw(Box::new(Node {
            value,
            next: std::ptr::null_mut(),
        }));

        loop {
            let head = self.head.load(Ordering::Acquire);
            unsafe {
                (*node).next = head;
            }

            if self
                .head
                .compare_exchange(head, node, Ordering::Release, Ordering::Relaxed)
                .is_ok()
            {
                break;
            }
        }
    }

    /// Pop a value from the stack
    pub fn pop(&self) -> Option<T> {
        loop {
            let head = self.head.load(Ordering::Acquire);
            if head.is_null() {
                return None;
            }

            let next = unsafe { (*head).next };

            if self
                .head
                .compare_exchange(head, next, Ordering::Release, Ordering::Relaxed)
                .is_ok()
            {
                let node = unsafe { Box::from_raw(head) };
                return Some(node.value);
            }
        }
    }

    /// Check if the stack is empty
    pub fn is_empty(&self) -> bool {
        self.head.load(Ordering::Acquire).is_null()
    }
}

impl<T> Default for LockFreeStack<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Lock-free hash map using atomic operations
/// This is a simplified version - full implementation would use more sophisticated techniques
pub struct LockFreeHashMap<K, V> {
    // For a true lock-free hash map, we'd need a more complex structure
    // This is a placeholder that uses atomic operations where possible
    buckets: Vec<Arc<parking_lot::RwLock<Vec<(K, V)>>>>,
    size: AtomicU64,
    capacity: usize,
}

impl<K, V> LockFreeHashMap<K, V>
where
    K: Eq + std::hash::Hash + Clone,
    V: Clone,
{
    /// Create a new lock-free hash map
    pub fn new(capacity: usize) -> Self {
        let mut buckets = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            buckets.push(Arc::new(parking_lot::RwLock::new(Vec::new())));
        }

        Self {
            buckets,
            size: AtomicU64::new(0),
            capacity,
        }
    }

    /// Get a value by key
    pub fn get(&self, key: &K) -> Option<V> {
        let hash = self.hash(key);
        let bucket_idx = hash % self.capacity;
        let bucket = &self.buckets[bucket_idx];

        let bucket_guard = bucket.read();
        bucket_guard
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
    }

    /// Insert a key-value pair
    pub fn insert(&self, key: K, value: V) -> Option<V> {
        let hash = self.hash(&key);
        let bucket_idx = hash % self.capacity;
        let bucket = &self.buckets[bucket_idx];

        let mut bucket_guard = bucket.write();
        if let Some((_, v)) = bucket_guard.iter_mut().find(|(k, _)| k == &key) {
            let old_value = std::mem::replace(v, value);
            return Some(old_value);
        }

        bucket_guard.push((key, value));
        self.size.fetch_add(1, Ordering::Relaxed);
        None
    }

    /// Remove a key-value pair
    pub fn remove(&self, key: &K) -> Option<V> {
        let hash = self.hash(key);
        let bucket_idx = hash % self.capacity;
        let bucket = &self.buckets[bucket_idx];

        let mut bucket_guard = bucket.write();
        if let Some(pos) = bucket_guard.iter().position(|(k, _)| k == key) {
            let (_, value) = bucket_guard.remove(pos);
            self.size.fetch_sub(1, Ordering::Relaxed);
            return Some(value);
        }

        None
    }

    /// Get the size of the map
    pub fn len(&self) -> usize {
        self.size.load(Ordering::Relaxed) as usize
    }

    /// Check if the map is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Hash function
    fn hash(&self, key: &K) -> usize {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hasher);
        hasher.finish() as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lock_free_counter() {
        let counter = LockFreeCounter::new(0);
        assert_eq!(counter.get(), 0);

        assert_eq!(counter.increment(), 1);
        assert_eq!(counter.get(), 1);

        assert_eq!(counter.add(5), 1);
        assert_eq!(counter.get(), 6);

        assert_eq!(counter.decrement(), 5);
        assert_eq!(counter.get(), 5);
    }

    #[test]
    fn test_lock_free_stack() {
        let stack = LockFreeStack::new();
        assert!(stack.is_empty());

        stack.push(1);
        stack.push(2);
        stack.push(3);

        assert!(!stack.is_empty());

        assert_eq!(stack.pop(), Some(3));
        assert_eq!(stack.pop(), Some(2));
        assert_eq!(stack.pop(), Some(1));
        assert_eq!(stack.pop(), None);
        assert!(stack.is_empty());
    }

    #[test]
    fn test_lock_free_hash_map() {
        let map = LockFreeHashMap::new(16);

        assert!(map.is_empty());

        map.insert("key1".to_string(), "value1".to_string());
        assert_eq!(map.len(), 1);

        assert_eq!(map.get(&"key1".to_string()), Some("value1".to_string()));

        let old = map.insert("key1".to_string(), "value2".to_string());
        assert_eq!(old, Some("value1".to_string()));
        assert_eq!(map.get(&"key1".to_string()), Some("value2".to_string()));

        let removed = map.remove(&"key1".to_string());
        assert_eq!(removed, Some("value2".to_string()));
        assert!(map.is_empty());
    }
}
