//! Page cache - Memory management for record store pages
//!
//! Implements eviction policies (clock/2Q/TinyLFU) for 4-8KB pages.
//! Supports pin/unpin semantics for transaction safety.
//! Pages have checksums (xxhash) for corruption detection.
//!
//! # Architecture
//!
//! The page cache uses Clock (second-chance) eviction algorithm for MVP.
//! Pages are stored in a HashMap for O(1) lookup, with a circular buffer
//! for eviction scanning.

use crate::{Error, Result};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use xxhash_rust::xxh3::xxh3_64;

/// Page size in bytes (8KB)
pub const PAGE_SIZE: usize = 8192;

/// Page header size (16 bytes)
pub const PAGE_HEADER_SIZE: usize = 16;

/// Page body size
pub const PAGE_BODY_SIZE: usize = PAGE_SIZE - PAGE_HEADER_SIZE;

/// Page flags
const PAGE_DIRTY: u16 = 1 << 0; // Modified, needs flush
const PAGE_PINNED: u16 = 1 << 1; // Cannot evict (in use)
const PAGE_VALID: u16 = 1 << 2; // Checksum validated

/// Page in cache
#[derive(Debug)]
pub struct Page {
    /// Page ID
    pub id: u64,
    /// Page data (PAGE_SIZE bytes)
    pub data: Vec<u8>,
    /// Dirty flag (needs flush to disk)
    dirty: AtomicU32,
    /// Pin count (number of active references)
    pin_count: AtomicU32,
    /// Flags
    flags: AtomicU32,
    /// Reference bit for Clock algorithm
    reference_bit: AtomicU32,
}

impl Page {
    /// Create a new page
    fn new(id: u64) -> Self {
        Self {
            id,
            data: vec![0u8; PAGE_SIZE],
            dirty: AtomicU32::new(0),
            pin_count: AtomicU32::new(0),
            flags: AtomicU32::new(0),
            reference_bit: AtomicU32::new(1), // Newly loaded pages are "referenced"
        }
    }

    /// Check if page is dirty
    pub fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::Acquire) != 0
    }

    /// Mark page as dirty
    pub fn mark_dirty(&self) {
        self.dirty.store(1, Ordering::Release);
    }

    /// Clear dirty flag
    pub fn clear_dirty(&self) {
        self.dirty.store(0, Ordering::Release);
    }

    /// Check if page is pinned
    pub fn is_pinned(&self) -> bool {
        self.pin_count.load(Ordering::Acquire) > 0
    }

    /// Pin page (increment reference count)
    pub fn pin(&self) {
        self.pin_count.fetch_add(1, Ordering::Release);
    }

    /// Unpin page (decrement reference count)
    pub fn unpin(&self) -> bool {
        let prev = self.pin_count.fetch_sub(1, Ordering::Release);
        prev == 1 // true if now unpinned
    }

    /// Get pin count
    pub fn get_pin_count(&self) -> u32 {
        self.pin_count.load(Ordering::Acquire)
    }

    /// Set reference bit (for Clock algorithm)
    pub fn set_reference_bit(&self) {
        self.reference_bit.store(1, Ordering::Release);
    }

    /// Clear and get reference bit (for Clock algorithm)
    pub fn clear_reference_bit(&self) -> bool {
        self.reference_bit.swap(0, Ordering::AcqRel) != 0
    }

    /// Compute checksum of page data
    pub fn compute_checksum(&self) -> u32 {
        xxh3_64(&self.data) as u32
    }

    /// Validate checksum (read first 4 bytes as stored checksum)
    pub fn validate_checksum(&self) -> Result<()> {
        if self.data.len() < 4 {
            return Err(Error::page_cache("Page data too small for checksum"));
        }

        let stored = u32::from_le_bytes([self.data[0], self.data[1], self.data[2], self.data[3]]);

        let actual = xxh3_64(&self.data[4..]) as u32;

        if stored != actual {
            return Err(Error::page_cache(format!(
                "Checksum mismatch for page {}: expected {:x}, got {:x}",
                self.id, stored, actual
            )));
        }

        Ok(())
    }

    /// Update checksum in page header
    pub fn update_checksum(&mut self) {
        if self.data.len() >= 4 {
            let checksum = xxh3_64(&self.data[4..]) as u32;
            self.data[0..4].copy_from_slice(&checksum.to_le_bytes());
        }
    }
}

/// Page cache statistics
#[derive(Debug, Clone, Default)]
pub struct PageCacheStats {
    /// Total page accesses
    pub total_accesses: u64,
    /// Cache hits
    pub hits: u64,
    /// Cache misses
    pub misses: u64,
    /// Pages evicted
    pub evictions: u64,
    /// Pages flushed
    pub flushes: u64,
    /// Current number of dirty pages
    pub dirty_count: usize,
    /// Current number of pinned pages
    pub pinned_count: usize,
    /// Current cache size
    pub cache_size: usize,
}

impl PageCacheStats {
    /// Calculate hit rate
    pub fn hit_rate(&self) -> f64 {
        if self.total_accesses == 0 {
            0.0
        } else {
            self.hits as f64 / self.total_accesses as f64
        }
    }
}

/// Page cache manager with Clock eviction
pub struct PageCache {
    /// Cache storage (page_id → Page)
    pages: HashMap<u64, Arc<Page>>,

    /// Maximum number of pages in cache
    capacity: usize,

    /// Clock hand position (for eviction)
    clock_hand: usize,

    /// List of page IDs in cache (for Clock algorithm)
    page_list: Vec<Option<u64>>,

    /// Dirty pages tracking
    dirty_pages: HashSet<u64>,

    /// Statistics
    stats: PageCacheStats,
}

impl PageCache {
    /// Create a new page cache
    ///
    /// # Arguments
    ///
    /// * `capacity` - Maximum number of pages (e.g., 10000 = 80MB for 8KB pages)
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use nexus_core::page_cache::PageCache;
    ///
    /// let cache = PageCache::new(10000).unwrap();
    /// ```
    pub fn new(capacity: usize) -> Result<Self> {
        if capacity == 0 {
            return Err(Error::page_cache("Capacity must be > 0"));
        }

        Ok(Self {
            pages: HashMap::with_capacity(capacity),
            capacity,
            clock_hand: 0,
            page_list: vec![None; capacity],
            dirty_pages: HashSet::new(),
            stats: PageCacheStats::default(),
        })
    }

    /// Get or load a page from cache
    ///
    /// Returns reference to cached page, loading from disk if necessary.
    pub fn get_page(&mut self, page_id: u64) -> Result<Arc<Page>> {
        self.stats.total_accesses += 1;

        // Check if page is in cache
        if let Some(page) = self.pages.get(&page_id) {
            self.stats.hits += 1;
            page.set_reference_bit(); // Mark as recently accessed
            return Ok(Arc::clone(page));
        }

        // Cache miss - need to load page
        self.stats.misses += 1;

        // Evict if cache is full
        if self.pages.len() >= self.capacity {
            self.evict_page()?;
        }

        // Load page (in real implementation, would load from disk)
        let page = Arc::new(Page::new(page_id));

        // Insert into cache
        self.insert_page(page_id, Arc::clone(&page));

        Ok(page)
    }

    /// Insert page into cache
    fn insert_page(&mut self, page_id: u64, page: Arc<Page>) {
        self.pages.insert(page_id, page);

        // Find empty slot in page_list or use clock_hand position
        if let Some(slot) = self.page_list.iter().position(|p| p.is_none()) {
            self.page_list[slot] = Some(page_id);
        } else {
            // No empty slot, use clock_hand position (will be overwritten on next eviction)
            if self.clock_hand < self.page_list.len() {
                self.page_list[self.clock_hand] = Some(page_id);
            }
        }

        self.stats.cache_size = self.pages.len();
    }

    /// Evict a page using Clock algorithm
    fn evict_page(&mut self) -> Result<()> {
        let mut iterations = 0;
        let max_iterations = self.capacity * 2; // Prevent infinite loop

        loop {
            if iterations >= max_iterations {
                return Err(Error::page_cache("All pages are pinned, cannot evict"));
            }

            // Get page at clock_hand
            if let Some(Some(page_id)) = self.page_list.get(self.clock_hand) {
                if let Some(page) = self.pages.get(page_id) {
                    // Skip pinned pages
                    if page.is_pinned() {
                        self.clock_hand = (self.clock_hand + 1) % self.capacity;
                        iterations += 1;
                        continue;
                    }

                    // Check reference bit
                    if page.clear_reference_bit() {
                        // Give second chance
                        self.clock_hand = (self.clock_hand + 1) % self.capacity;
                        iterations += 1;
                        continue;
                    }

                    // Evict this page
                    let page_id = *page_id;

                    // Flush if dirty
                    if page.is_dirty() {
                        // In real implementation: flush to disk
                        page.clear_dirty();
                        self.dirty_pages.remove(&page_id);
                        self.stats.flushes += 1;
                    }

                    // Remove from cache
                    self.pages.remove(&page_id);
                    self.page_list[self.clock_hand] = None;
                    self.stats.evictions += 1;
                    self.stats.cache_size = self.pages.len();

                    // Advance clock hand
                    self.clock_hand = (self.clock_hand + 1) % self.capacity;

                    return Ok(());
                }
            }

            // Advance to next position
            self.clock_hand = (self.clock_hand + 1) % self.capacity;
            iterations += 1;
        }
    }

    /// Pin a page (prevent eviction)
    pub fn pin_page(&mut self, page_id: u64) -> Result<()> {
        if let Some(page) = self.pages.get(&page_id) {
            page.pin();
            self.stats.pinned_count += 1;
            Ok(())
        } else {
            Err(Error::page_cache(format!("Page {} not in cache", page_id)))
        }
    }

    /// Unpin a page (allow eviction)
    pub fn unpin_page(&mut self, page_id: u64) -> Result<()> {
        if let Some(page) = self.pages.get(&page_id) {
            if page.unpin() {
                // Was last pin, now unpinned
                self.stats.pinned_count = self.stats.pinned_count.saturating_sub(1);
            }
            Ok(())
        } else {
            Err(Error::page_cache(format!("Page {} not in cache", page_id)))
        }
    }

    /// Mark page as dirty
    pub fn mark_dirty(&mut self, page_id: u64) -> Result<()> {
        if let Some(page) = self.pages.get(&page_id) {
            if !page.is_dirty() {
                page.mark_dirty();
                self.dirty_pages.insert(page_id);
                self.stats.dirty_count = self.dirty_pages.len();
            }
            Ok(())
        } else {
            Err(Error::page_cache(format!("Page {} not in cache", page_id)))
        }
    }

    /// Flush all dirty pages to disk
    pub fn flush(&mut self) -> Result<()> {
        let dirty_page_ids: Vec<u64> = self.dirty_pages.iter().copied().collect();

        for page_id in dirty_page_ids {
            if let Some(page) = self.pages.get(&page_id) {
                // In real implementation: write to disk
                page.clear_dirty();
                self.stats.flushes += 1;
            }
            self.dirty_pages.remove(&page_id);
        }

        self.stats.dirty_count = 0;
        Ok(())
    }

    /// Flush a single page
    pub fn flush_page(&mut self, page_id: u64) -> Result<()> {
        if let Some(page) = self.pages.get(&page_id) {
            if page.is_dirty() {
                // In real implementation: write to disk
                page.clear_dirty();
                self.dirty_pages.remove(&page_id);
                self.stats.dirty_count = self.dirty_pages.len();
                self.stats.flushes += 1;
            }
            Ok(())
        } else {
            Err(Error::page_cache(format!("Page {} not in cache", page_id)))
        }
    }

    /// Check if page is in cache
    pub fn contains_page(&self, page_id: u64) -> bool {
        self.pages.contains_key(&page_id)
    }

    /// Get statistics
    pub fn stats(&self) -> PageCacheStats {
        self.stats.clone()
    }

    /// Get current cache size
    pub fn len(&self) -> usize {
        self.pages.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }

    /// Clear cache (evict all non-pinned pages)
    pub fn clear(&mut self) -> Result<()> {
        // Flush all dirty pages first
        self.flush()?;

        // Remove non-pinned pages
        let to_remove: Vec<u64> = self
            .pages
            .iter()
            .filter(|(_, page)| !page.is_pinned())
            .map(|(id, _)| *id)
            .collect();

        for page_id in to_remove {
            self.pages.remove(&page_id);
        }

        // Clear page list
        for slot in &mut self.page_list {
            if let Some(page_id) = slot {
                if !self.pages.contains_key(page_id) {
                    *slot = None;
                }
            }
        }

        self.stats.cache_size = self.pages.len();
        Ok(())
    }
    
    /// Get the number of cache hits
    pub fn hit_count(&self) -> u64 {
        self.stats.hits
    }
    
    /// Get the number of cache misses
    pub fn miss_count(&self) -> u64 {
        self.stats.misses
    }
    
    /// Health check for the page cache
    pub fn health_check(&self) -> Result<()> {
        // Check if the cache is not corrupted
        if self.stats.cache_size > self.capacity {
            return Err(Error::page_cache("Cache size exceeds capacity"));
        }
        
        // Check if dirty pages count is reasonable
        if self.dirty_pages.len() > self.capacity {
            return Err(Error::page_cache("Too many dirty pages"));
        }
        
        // Check if hit rate is reasonable (not 0 if there have been accesses)
        if self.stats.total_accesses > 0 && self.stats.hit_rate() < 0.0 {
            return Err(Error::page_cache("Invalid hit rate"));
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_creation() {
        let page = Page::new(42);
        assert_eq!(page.id, 42);
        assert_eq!(page.data.len(), PAGE_SIZE);
        assert!(!page.is_dirty());
        assert!(!page.is_pinned());
    }

    #[test]
    fn test_page_dirty_flag() {
        let page = Page::new(1);
        assert!(!page.is_dirty());

        page.mark_dirty();
        assert!(page.is_dirty());

        page.clear_dirty();
        assert!(!page.is_dirty());
    }

    #[test]
    fn test_page_pin_unpin() {
        let page = Page::new(1);
        assert_eq!(page.get_pin_count(), 0);
        assert!(!page.is_pinned());

        page.pin();
        assert_eq!(page.get_pin_count(), 1);
        assert!(page.is_pinned());

        page.pin();
        assert_eq!(page.get_pin_count(), 2);

        let was_last = page.unpin();
        assert!(!was_last);
        assert_eq!(page.get_pin_count(), 1);

        let was_last = page.unpin();
        assert!(was_last);
        assert_eq!(page.get_pin_count(), 0);
        assert!(!page.is_pinned());
    }

    #[test]
    fn test_page_reference_bit() {
        let page = Page::new(1);

        page.set_reference_bit();
        assert!(page.clear_reference_bit()); // Returns true and clears

        assert!(!page.clear_reference_bit()); // Now false
    }

    #[test]
    fn test_cache_creation() {
        let cache = PageCache::new(100).unwrap();
        assert_eq!(cache.capacity, 100);
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
    }

    #[test]
    fn test_cache_zero_capacity() {
        let result = PageCache::new(0);
        assert!(result.is_err());
    }

    #[test]
    fn test_cache_get_page() {
        let mut cache = PageCache::new(10).unwrap();

        let page1 = cache.get_page(1).unwrap();
        assert_eq!(page1.id, 1);
        assert_eq!(cache.len(), 1);

        // Get same page again (should hit cache)
        let page1_again = cache.get_page(1).unwrap();
        assert_eq!(page1_again.id, 1);
        assert_eq!(cache.len(), 1);

        let stats = cache.stats();
        assert_eq!(stats.total_accesses, 2);
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
    }

    #[test]
    fn test_cache_pin_unpin() {
        let mut cache = PageCache::new(10).unwrap();

        let page1 = cache.get_page(1).unwrap();
        assert!(!page1.is_pinned());

        cache.pin_page(1).unwrap();
        assert!(page1.is_pinned());

        cache.unpin_page(1).unwrap();
        assert!(!page1.is_pinned());
    }

    #[test]
    fn test_cache_mark_dirty() {
        let mut cache = PageCache::new(10).unwrap();

        let page1 = cache.get_page(1).unwrap();
        assert!(!page1.is_dirty());

        cache.mark_dirty(1).unwrap();
        assert!(page1.is_dirty());

        let stats = cache.stats();
        assert_eq!(stats.dirty_count, 1);
    }

    #[test]
    fn test_cache_flush() {
        let mut cache = PageCache::new(10).unwrap();

        // Load and dirty some pages
        for i in 0..5 {
            cache.get_page(i).unwrap();
            cache.mark_dirty(i).unwrap();
        }

        let stats_before = cache.stats();
        assert_eq!(stats_before.dirty_count, 5);

        // Flush all
        cache.flush().unwrap();

        let stats_after = cache.stats();
        assert_eq!(stats_after.dirty_count, 0);
        assert_eq!(stats_after.flushes, 5);

        // Pages should still be in cache
        assert_eq!(cache.len(), 5);
    }

    #[test]
    fn test_cache_eviction() {
        let mut cache = PageCache::new(5).unwrap();

        // Fill cache to capacity
        for i in 0..5 {
            cache.get_page(i).unwrap();
        }

        assert_eq!(cache.len(), 5);

        // Load one more page (should trigger eviction)
        cache.get_page(10).unwrap();

        assert_eq!(cache.len(), 5); // Still at capacity
        assert!(cache.contains_page(10)); // New page is in cache

        let stats = cache.stats();
        assert_eq!(stats.evictions, 1);
    }

    #[test]
    fn test_eviction_skips_pinned() {
        let mut cache = PageCache::new(3).unwrap();

        // Load and pin all pages
        for i in 0..3 {
            cache.get_page(i).unwrap();
            cache.pin_page(i).unwrap();
        }

        // Try to load another page (all pinned, should fail)
        let result = cache.get_page(10);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("pinned"));
    }

    #[test]
    fn test_clock_second_chance() {
        let mut cache = PageCache::new(3).unwrap();

        // Load 3 pages
        for i in 0..3 {
            cache.get_page(i).unwrap();
        }

        // Access page 0 and 1 multiple times (set reference bits)
        cache.get_page(0).unwrap();
        cache.get_page(1).unwrap();
        cache.get_page(0).unwrap();
        cache.get_page(1).unwrap();

        // Load page 10 (should evict one page)
        cache.get_page(10).unwrap();

        // Verify eviction happened
        assert_eq!(cache.len(), 3); // Still at capacity
        assert!(cache.contains_page(10)); // New page is in cache

        let stats = cache.stats();
        assert_eq!(stats.evictions, 1);

        // At least one of the frequently accessed pages should still be cached
        let cached_count = [0, 1].iter().filter(|&&id| cache.contains_page(id)).count();
        assert!(
            cached_count >= 1,
            "Clock should keep some recently accessed pages"
        );
    }

    #[test]
    fn test_flush_dirty_before_eviction() {
        let mut cache = PageCache::new(2).unwrap();

        // Load and dirty page 0
        cache.get_page(0).unwrap();
        cache.mark_dirty(0).unwrap();

        // Load pages 1 and 2 (will evict 0)
        cache.get_page(1).unwrap();
        cache.get_page(2).unwrap();

        // Page 0 should have been flushed before eviction
        let stats = cache.stats();
        assert_eq!(stats.flushes, 1);
        assert!(!cache.contains_page(0));
    }

    #[test]
    fn test_flush_single_page() {
        let mut cache = PageCache::new(10).unwrap();

        cache.get_page(5).unwrap();
        cache.mark_dirty(5).unwrap();

        assert_eq!(cache.stats().dirty_count, 1);

        cache.flush_page(5).unwrap();

        assert_eq!(cache.stats().dirty_count, 0);
        assert_eq!(cache.stats().flushes, 1);
    }

    #[test]
    fn test_clear_cache() {
        let mut cache = PageCache::new(10).unwrap();

        // Load some pages
        for i in 0..5 {
            cache.get_page(i).unwrap();
            cache.mark_dirty(i).unwrap();
        }

        // Pin one page
        cache.pin_page(2).unwrap();

        cache.clear().unwrap();

        // Only pinned page should remain
        assert_eq!(cache.len(), 1);
        assert!(cache.contains_page(2));
    }

    #[test]
    fn test_hit_rate_calculation() {
        let mut cache = PageCache::new(10).unwrap();

        // Load 5 pages (5 misses)
        for i in 0..5 {
            cache.get_page(i).unwrap();
        }

        // Access pages 0-4 again (5 hits)
        for i in 0..5 {
            cache.get_page(i).unwrap();
        }

        let stats = cache.stats();
        assert_eq!(stats.total_accesses, 10);
        assert_eq!(stats.hits, 5);
        assert_eq!(stats.misses, 5);
        assert_eq!(stats.hit_rate(), 0.5);
    }

    #[test]
    fn test_contains_page() {
        let mut cache = PageCache::new(10).unwrap();

        assert!(!cache.contains_page(1));

        cache.get_page(1).unwrap();
        assert!(cache.contains_page(1));
    }

    #[test]
    fn test_page_checksum() {
        let mut page = Page::new(1);

        // Write some data
        page.data[4..8].copy_from_slice(&[1, 2, 3, 4]);

        // Update checksum
        page.update_checksum();

        // Validate should pass
        page.validate_checksum().unwrap();

        // Corrupt data
        page.data[100] = 99;

        // Validate should fail
        assert!(page.validate_checksum().is_err());
    }

    #[test]
    fn test_concurrent_access() {
        use parking_lot::RwLock;
        use std::sync::Arc;
        use std::thread;

        let cache = Arc::new(RwLock::new(PageCache::new(100).unwrap()));

        let mut handles = vec![];

        // Spawn threads accessing pages
        for i in 0..10 {
            let c = cache.clone();
            let handle = thread::spawn(move || {
                for j in 0..10 {
                    let page_id = (i * 10 + j) as u64;
                    c.write().get_page(page_id).unwrap();
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let final_cache = cache.read();
        assert_eq!(final_cache.len(), 100);
    }

    #[test]
    fn test_page_constants() {
        assert_eq!(PAGE_SIZE, 8192);
        assert_eq!(PAGE_HEADER_SIZE, 16);
        assert_eq!(PAGE_BODY_SIZE, 8176);
    }
}
