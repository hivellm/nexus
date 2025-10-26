# Page Cache Specification

This document defines the page cache implementation for Nexus, responsible for managing memory pages for record stores.

## Overview

The page cache sits between the record stores and physical storage, providing:
- **Memory management**: Load pages on demand, evict when memory is full
- **Performance**: Keep hot pages in memory (DRAM >> SSD >> HDD)
- **Consistency**: Pin/unpin semantics for transaction safety
- **Durability**: Track dirty pages, flush on checkpoint

## Architecture

```
┌─────────────────────────────────────────┐
│          Application Layer              │
│     (Executor, Indexes, Storage)        │
└─────────────┬───────────────────────────┘
              │ get_page(id)
              │ pin_page(id)
              │ unpin_page(id)
┌─────────────▼───────────────────────────┐
│          Page Cache                      │
│  ┌──────────┐ ┌──────────┐ ┌─────────┐ │
│  │ Hash Map │ │ Eviction │ │  Flush  │ │
│  │page_id→  │ │  Policy  │ │  Queue  │ │
│  │  Page    │ │          │ │         │ │
│  └──────────┘ └──────────┘ └─────────┘ │
└─────────────┬───────────────────────────┘
              │ read_page(offset)
              │ write_page(offset, data)
┌─────────────▼───────────────────────────┐
│        Memory-Mapped Files               │
│          (memmap2)                       │
└─────────────┬───────────────────────────┘
              │
┌─────────────▼───────────────────────────┐
│           File System                    │
│   (nodes.store, rels.store, ...)        │
└──────────────────────────────────────────┘
```

## Page Structure

### Page Header

```
Page (8192 bytes total):
┌────────────────┬──────────────────────────┐
│  Header (16B)  │     Body (8176B)         │
└────────────────┴──────────────────────────┘

Header:
┌──────────┬──────────┬──────────┬──────────┐
│ page_id  │ checksum │  flags   │ reserved │
│(8 bytes) │(4 bytes) │(2 bytes) │(2 bytes) │
└──────────┴──────────┴──────────┴──────────┘

page_id:  Logical page number (u64)
checksum: xxHash3 of body (u32)
flags:    Page state bits (u16)
reserved: Future use (u16)
```

### Page Flags

```rust
const PAGE_DIRTY: u16    = 1 << 0;  // Modified, needs flush
const PAGE_PINNED: u16   = 1 << 1;  // Cannot evict (in use)
const PAGE_VALID: u16    = 1 << 2;  // Checksum validated
const PAGE_EVICTING: u16 = 1 << 3;  // Being evicted
const PAGE_LOADING: u16  = 1 << 4;  // Being loaded from disk
```

## Eviction Policies

### MVP: Clock (Second-Chance)

Simple and effective circular buffer algorithm:

```
Data Structures:
- pages: Vec<Page>              // Circular buffer
- clock_hand: usize             // Current position
- reference_bits: Vec<bool>     // Recently accessed?

Algorithm:
1. On page access: set reference_bit[page] = true
2. On eviction needed:
   loop:
     if reference_bit[clock_hand]:
       reference_bit[clock_hand] = false  // second chance
       clock_hand = (clock_hand + 1) % capacity
     else if not pinned(pages[clock_hand]):
       evict(pages[clock_hand])
       return clock_hand
     else:
       clock_hand = (clock_hand + 1) % capacity
```

**Characteristics**:
- Time complexity: O(1) amortized
- Space overhead: 1 bit per page
- Hit rate: Good for sequential scans
- Simple implementation

### V1: 2Q (Two-Queue)

Separate queues for hot and cold pages:

```
Data Structures:
- fifo_in: Queue<PageId>        // Recently accessed (cold)
- fifo_out: Queue<PageId>       // Evicted ghosts
- lru_hot: LRU<PageId>          // Frequently accessed (hot)

Algorithm:
1. On first access:
   - Add to fifo_in (tail)
2. On second access (in fifo_out):
   - Promote to lru_hot
3. On eviction:
   - Evict from fifo_in (head) OR lru_hot (LRU victim)
   - Add evicted page_id to fifo_out (ghost entry)

Tuning:
- fifo_in size: 25% of total cache
- lru_hot size: 75% of total cache
- fifo_out size: 50% of total cache (ghosts only)
```

**Characteristics**:
- Better than LRU for scan-resistant workloads
- Adapts to access patterns (cold vs hot)
- More complex than Clock
- Good for mixed workloads

### Future: TinyLFU (Frequency + Recency)

Combines frequency estimation (Count-Min Sketch) with recency (LRU):

```
Data Structures:
- window_lru: LRU<PageId>       // Recent pages (1% of cache)
- probation_lru: LRU<PageId>    // On trial (20%)
- protected_lru: LRU<PageId>    // Frequently accessed (79%)
- frequency: CountMinSketch     // Frequency estimator (4KB)

Algorithm:
1. New page → window_lru
2. Evict from window_lru → probation_lru
3. Access in probation_lru:
   - If frequency > threshold → protected_lru
4. Evict from probation_lru or protected_lru:
   - Compare frequencies, evict lower frequency

Frequency Aging:
- Halve all frequencies periodically (avoid staleness)
```

**Characteristics**:
- Best hit rate for Zipfian distributions
- Resistant to one-time scans
- More complex (requires Count-Min Sketch)
- 4KB overhead for frequency table

## Pin/Unpin Semantics

### Pinning Rules

```rust
// Pin a page (transaction or operator in progress)
cache.pin_page(page_id)?;
// Page cannot be evicted while pinned
// Multiple pins allowed (reference counting)

// Unpin when done
cache.unpin_page(page_id)?;
// Page can be evicted when pin_count reaches 0
```

### Reference Counting

```rust
struct Page {
    id: u64,
    data: Vec<u8>,
    pin_count: AtomicU32,  // Atomic for thread safety
    dirty: AtomicBool,
    flags: AtomicU16,
}

impl Page {
    fn pin(&self) {
        self.pin_count.fetch_add(1, Ordering::Release);
    }
    
    fn unpin(&self) -> bool {
        let prev = self.pin_count.fetch_sub(1, Ordering::Release);
        prev == 1  // true if now unpinned
    }
    
    fn is_pinned(&self) -> bool {
        self.pin_count.load(Ordering::Acquire) > 0
    }
}
```

### RAII Guard

```rust
// Automatic unpin on drop
struct PageGuard<'a> {
    cache: &'a PageCache,
    page_id: u64,
}

impl Drop for PageGuard<'_> {
    fn drop(&mut self) {
        self.cache.unpin_page(self.page_id).ok();
    }
}

// Usage:
let guard = cache.get_and_pin_page(42)?;
// Use page...
// Auto-unpinned when guard goes out of scope
```

## Dirty Page Management

### Tracking

```rust
struct DirtyPageTracker {
    dirty_pages: HashSet<u64>,  // page_ids
    dirty_queue: VecDeque<u64>, // FIFO order
}

impl DirtyPageTracker {
    fn mark_dirty(&mut self, page_id: u64) {
        if self.dirty_pages.insert(page_id) {
            self.dirty_queue.push_back(page_id);
        }
    }
    
    fn get_dirty_pages(&self) -> &HashSet<u64> {
        &self.dirty_pages
    }
}
```

### Flushing

```rust
// Flush on checkpoint
fn flush_dirty_pages(&mut self) -> Result<()> {
    for page_id in self.dirty_tracker.dirty_pages.drain() {
        let page = self.get_page(page_id)?;
        page.flush_to_disk()?;
        page.clear_dirty();
    }
    self.dirty_tracker.dirty_queue.clear();
    Ok(())
}

// Flush single page (eviction)
fn flush_page(&mut self, page_id: u64) -> Result<()> {
    let page = self.get_page(page_id)?;
    if page.is_dirty() {
        page.flush_to_disk()?;
        page.clear_dirty();
        self.dirty_tracker.remove(page_id);
    }
    Ok(())
}
```

## Checksum Validation

### xxHash3

Fast, high-quality non-cryptographic hash:

```rust
use xxhash_rust::xxh3::xxh3_64;

fn validate_page(page: &Page) -> Result<()> {
    let expected = page.header.checksum;
    let actual = xxh3_64(&page.data);
    
    if expected != actual as u32 {
        return Err(Error::page_cache(
            format!("Checksum mismatch for page {}: expected {:x}, got {:x}",
                    page.id, expected, actual)
        ));
    }
    
    Ok(())
}

fn update_checksum(page: &mut Page) {
    let checksum = xxh3_64(&page.data);
    page.header.checksum = checksum as u32;
}
```

## Concurrency

### Locking Strategy (MVP)

```rust
use parking_lot::RwLock;
use std::collections::HashMap;

struct PageCache {
    pages: RwLock<HashMap<u64, Arc<Page>>>,  // Global lock
    capacity: usize,
    eviction_policy: EvictionPolicy,
}

// Read page (shared)
fn get_page(&self, page_id: u64) -> Result<Arc<Page>> {
    let pages = self.pages.read();
    if let Some(page) = pages.get(&page_id) {
        return Ok(Arc::clone(page));
    }
    drop(pages);  // Release read lock
    
    // Load page (write lock)
    let mut pages = self.pages.write();
    // Double-check (another thread may have loaded)
    if let Some(page) = pages.get(&page_id) {
        return Ok(Arc::clone(page));
    }
    
    // Actually load page
    let page = self.load_page(page_id)?;
    let page_arc = Arc::new(page);
    pages.insert(page_id, Arc::clone(&page_arc));
    Ok(page_arc)
}
```

### V1: Fine-Grained Locking

```rust
struct PageCache {
    // Shard by page_id % NUM_SHARDS
    shards: Vec<RwLock<HashMap<u64, Arc<Page>>>>,
    num_shards: usize,
}

impl PageCache {
    fn get_shard(&self, page_id: u64) -> &RwLock<HashMap<u64, Arc<Page>>> {
        &self.shards[(page_id % self.num_shards as u64) as usize]
    }
    
    fn get_page(&self, page_id: u64) -> Result<Arc<Page>> {
        let shard = self.get_shard(page_id);
        let pages = shard.read();
        // ... (same logic as before)
    }
}
```

## Memory-Mapped I/O

### memmap2 Integration

```rust
use memmap2::{Mmap, MmapMut};
use std::fs::OpenOptions;

struct MmappedStore {
    file: File,
    mmap: Mmap,
}

impl MmappedStore {
    fn open(path: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path)?;
        
        // Ensure file is large enough
        let size = file.metadata()?.len();
        if size == 0 {
            file.set_len(1024 * 1024)?;  // 1MB initial
        }
        
        // Memory-map the file
        let mmap = unsafe { Mmap::map(&file)? };
        
        Ok(Self { file, mmap })
    }
    
    fn read_page(&self, page_id: u64, page_size: usize) -> Result<&[u8]> {
        let offset = page_id * page_size as u64;
        let end = offset + page_size as u64;
        
        if end > self.mmap.len() as u64 {
            return Err(Error::page_cache("Page out of bounds"));
        }
        
        Ok(&self.mmap[offset as usize..end as usize])
    }
}
```

### Write-Through vs Write-Back

**MVP: Write-Back (dirty pages)**
```rust
// Writes go to page cache only
fn write_page(&mut self, page_id: u64, data: &[u8]) -> Result<()> {
    let page = self.get_page_mut(page_id)?;
    page.data.copy_from_slice(data);
    page.mark_dirty();  // Flush later (checkpoint or eviction)
    Ok(())
}
```

**Alternative: Write-Through (synchronous)**
```rust
// Writes go to cache AND disk immediately
fn write_page(&mut self, page_id: u64, data: &[u8]) -> Result<()> {
    let page = self.get_page_mut(page_id)?;
    page.data.copy_from_slice(data);
    page.flush_to_disk()?;  // Immediate fsync
    Ok(())
}
```

MVP uses write-back for performance; durability via WAL.

## Configuration

### Tuning Parameters

```rust
struct PageCacheConfig {
    /// Maximum number of pages in cache
    capacity: usize,  // Default: 10_000 pages = 80MB for 8KB pages
    
    /// Page size in bytes
    page_size: usize,  // Default: 8192 (8KB)
    
    /// Eviction policy
    eviction_policy: EvictionPolicy,  // Default: Clock
    
    /// Flush dirty pages interval
    flush_interval_secs: u64,  // Default: 60 seconds
    
    /// Max dirty pages before forced flush
    max_dirty_pages: usize,  // Default: 1000
}

enum EvictionPolicy {
    Clock,
    TwoQueue,
    TinyLFU,
}
```

### Size Calculations

```
Example Configuration:
- capacity: 10,000 pages
- page_size: 8KB
Total memory: 10,000 × 8KB = 80MB

For 100,000 pages:
Total memory: 100,000 × 8KB = 800MB

Recommended:
- Development: 10K pages (80MB)
- Production: 100K-1M pages (0.8-8GB)
- High-end: 10M pages (80GB)
```

## Performance Characteristics

### Hit Rate

```
Target hit rates (workload dependent):
- Sequential scan: 50-70% (one-time access)
- Random reads:    90-95% (with sufficient cache)
- Mixed workload:  80-90%
```

### Latency

```
Cache hit:  ~100 ns (DRAM access)
Cache miss: ~1 ms (SSD read) to ~10 ms (HDD read)

Speedup: 10,000x (SSD) to 100,000x (HDD)
```

### Throughput

```
Sequential read (cache hit):  ~10 GB/sec (DRAM bandwidth)
Sequential read (cache miss): ~500 MB/sec (SSD) to ~100 MB/sec (HDD)

Random read (cache hit):  ~100K pages/sec
Random read (cache miss): ~10K pages/sec (SSD) to ~100 pages/sec (HDD)
```

## Monitoring & Metrics

### Key Metrics

```rust
struct PageCacheStats {
    /// Total page accesses
    total_accesses: AtomicU64,
    
    /// Cache hits
    hits: AtomicU64,
    
    /// Cache misses
    misses: AtomicU64,
    
    /// Pages evicted
    evictions: AtomicU64,
    
    /// Pages flushed
    flushes: AtomicU64,
    
    /// Current number of dirty pages
    dirty_count: AtomicUsize,
    
    /// Current number of pinned pages
    pinned_count: AtomicUsize,
}

impl PageCacheStats {
    fn hit_rate(&self) -> f64 {
        let hits = self.hits.load(Ordering::Relaxed) as f64;
        let total = self.total_accesses.load(Ordering::Relaxed) as f64;
        if total == 0.0 { 0.0 } else { hits / total }
    }
}
```

### Prometheus Metrics

```
nexus_page_cache_accesses_total{result="hit|miss"}
nexus_page_cache_evictions_total
nexus_page_cache_flushes_total
nexus_page_cache_dirty_pages
nexus_page_cache_pinned_pages
nexus_page_cache_size_bytes
nexus_page_cache_hit_rate
```

## Testing

### Unit Tests

```rust
#[test]
fn test_page_pin_unpin() {
    let cache = PageCache::new(PageCacheConfig::default()).unwrap();
    let page_id = 42;
    
    // Load page
    cache.get_page(page_id).unwrap();
    
    // Pin it
    cache.pin_page(page_id).unwrap();
    assert!(cache.is_pinned(page_id));
    
    // Cannot evict while pinned
    assert!(cache.try_evict_page(page_id).is_err());
    
    // Unpin
    cache.unpin_page(page_id).unwrap();
    assert!(!cache.is_pinned(page_id));
    
    // Can evict now
    cache.try_evict_page(page_id).unwrap();
}
```

### Integration Tests

```rust
#[test]
fn test_eviction_policy_clock() {
    let config = PageCacheConfig {
        capacity: 100,
        eviction_policy: EvictionPolicy::Clock,
        ..Default::default()
    };
    let mut cache = PageCache::new(config).unwrap();
    
    // Fill cache
    for i in 0..100 {
        cache.get_page(i).unwrap();
    }
    
    // Access pages 0-49 (set reference bits)
    for i in 0..50 {
        cache.get_page(i).unwrap();
    }
    
    // Load page 100 (should evict page from 50-99)
    cache.get_page(100).unwrap();
    
    // Pages 0-49 should still be cached (second chance)
    for i in 0..50 {
        assert!(cache.contains_page(i));
    }
}
```

## Future Enhancements

### V1

- Adaptive eviction (switch policy based on workload)
- Pre-fetching (read-ahead for sequential scans)
- Compression (LZ4 for cold pages)
- NUMA-aware allocation

### V2

- Distributed page cache (remote pages via RDMA)
- Tiered caching (DRAM → SSD → HDD)
- ML-based eviction (predict future accesses)

## References

- LIRS: https://www.usenix.org/legacy/events/fast02/jiang.html
- 2Q: http://www.vldb.org/conf/1994/P439.PDF
- TinyLFU: https://arxiv.org/abs/1512.00727
- PostgreSQL Buffer Cache: https://www.postgresql.org/docs/current/kernel-resources.html

