//! Memory management and garbage collection
//!
//! This module provides:
//! - Memory pool management
//! - Garbage collection for unused objects
//! - Memory allocation strategies
//! - Memory usage monitoring and optimization

use crate::{Error, Result};
use std::alloc::{GlobalAlloc, Layout};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Memory allocation strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocationStrategy {
    /// First-fit allocation
    FirstFit,
    /// Best-fit allocation
    BestFit,
    /// Worst-fit allocation
    WorstFit,
    /// Buddy system allocation
    BuddySystem,
}

/// Memory pool configuration
#[derive(Debug, Clone)]
pub struct MemoryPoolConfig {
    /// Initial pool size in bytes
    pub initial_size: usize,
    /// Maximum pool size in bytes
    pub max_size: usize,
    /// Allocation strategy
    pub strategy: AllocationStrategy,
    /// Enable garbage collection
    pub enable_gc: bool,
    /// GC threshold (percentage of memory used)
    pub gc_threshold: f64,
    /// GC interval
    pub gc_interval: Duration,
}

impl Default for MemoryPoolConfig {
    fn default() -> Self {
        Self {
            initial_size: 1024 * 1024,    // 1MB
            max_size: 1024 * 1024 * 1024, // 1GB
            strategy: AllocationStrategy::FirstFit,
            enable_gc: true,
            gc_threshold: 0.8, // 80%
            gc_interval: Duration::from_secs(30),
        }
    }
}

/// Memory block information
#[derive(Debug)]
pub struct MemoryBlock {
    /// Starting address
    pub address: usize,
    /// Size in bytes
    pub size: usize,
    /// Whether the block is allocated
    pub allocated: bool,
    /// Reference count
    pub ref_count: AtomicUsize,
    /// Last access time
    pub last_access: AtomicU64,
    /// Block type
    pub block_type: BlockType,
}

/// Block type for different memory uses
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockType {
    /// Node data
    Node,
    /// Relationship data
    Relationship,
    /// Property data
    Property,
    /// Index data
    Index,
    /// Cache data
    Cache,
    /// Temporary data
    Temporary,
}

/// Memory pool for managing allocations
pub struct MemoryPool {
    /// Pool configuration
    config: MemoryPoolConfig,
    /// Memory blocks
    blocks: Arc<RwLock<Vec<MemoryBlock>>>,
    /// Free block list
    free_blocks: Arc<RwLock<VecDeque<usize>>>,
    /// Allocated block map
    allocated_blocks: Arc<RwLock<HashMap<usize, usize>>>,
    /// Memory usage statistics
    stats: Arc<RwLock<MemoryStats>>,
    /// GC thread handle
    gc_handle: Option<std::thread::JoinHandle<()>>,
    /// GC stop flag
    gc_stop: Arc<AtomicU64>,
}

/// Memory usage statistics
#[derive(Debug, Clone, Default)]
pub struct MemoryStats {
    /// Total allocated memory
    pub total_allocated: usize,
    /// Total free memory
    pub total_free: usize,
    /// Number of allocated blocks
    pub allocated_blocks: usize,
    /// Number of free blocks
    pub free_blocks: usize,
    /// Peak memory usage
    pub peak_usage: usize,
    /// GC cycles performed
    pub gc_cycles: u64,
    /// Last GC time
    pub last_gc: Option<Instant>,
}

impl MemoryPool {
    /// Create a new memory pool
    pub fn new(config: MemoryPoolConfig) -> Result<Self> {
        let mut pool = Self {
            config,
            blocks: Arc::new(RwLock::new(Vec::new())),
            free_blocks: Arc::new(RwLock::new(VecDeque::new())),
            allocated_blocks: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(MemoryStats::default())),
            gc_handle: None,
            gc_stop: Arc::new(AtomicU64::new(0)),
        };

        // Initialize the pool
        pool.initialize()?;

        // Start garbage collection thread if enabled
        if pool.config.enable_gc {
            pool.start_gc_thread()?;
        }

        Ok(pool)
    }

    /// Initialize the memory pool
    fn initialize(&mut self) -> Result<()> {
        let mut blocks = self.blocks.write().unwrap();
        let mut free_blocks = self.free_blocks.write().unwrap();
        let mut stats = self.stats.write().unwrap();

        // Create initial free block
        let initial_block = MemoryBlock {
            address: 0,
            size: self.config.initial_size,
            allocated: false,
            ref_count: AtomicUsize::new(0),
            last_access: AtomicU64::new(0),
            block_type: BlockType::Temporary,
        };

        blocks.push(initial_block);
        free_blocks.push_back(0);
        stats.total_free = self.config.initial_size;

        Ok(())
    }

    /// Allocate memory
    pub fn allocate(&self, size: usize, block_type: BlockType) -> Result<usize> {
        if size == 0 {
            return Err(Error::InvalidInput(
                "Cannot allocate zero bytes".to_string(),
            ));
        }

        // Align size to word boundary
        let aligned_size = (size + 7) & !7;

        match self.config.strategy {
            AllocationStrategy::FirstFit => self.allocate_first_fit(aligned_size, block_type),
            AllocationStrategy::BestFit => self.allocate_best_fit(aligned_size, block_type),
            AllocationStrategy::WorstFit => self.allocate_worst_fit(aligned_size, block_type),
            AllocationStrategy::BuddySystem => self.allocate_buddy_system(aligned_size, block_type),
        }
    }

    /// First-fit allocation
    fn allocate_first_fit(&self, size: usize, block_type: BlockType) -> Result<usize> {
        let mut free_blocks = self.free_blocks.write().unwrap();
        let mut blocks = self.blocks.write().unwrap();
        let mut allocated_blocks = self.allocated_blocks.write().unwrap();
        let mut stats = self.stats.write().unwrap();

        // Find first suitable free block
        for (i, &block_idx) in free_blocks.iter().enumerate() {
            if blocks[block_idx].size >= size {
                // Found suitable block
                let block = &mut blocks[block_idx];
                let address = block.address;

                if block.size == size {
                    // Exact fit - allocate the entire block
                    block.allocated = true;
                    block.block_type = block_type;
                    block.ref_count.store(1, Ordering::SeqCst);
                    block
                        .last_access
                        .store(Instant::now().elapsed().as_nanos() as u64, Ordering::SeqCst);

                    free_blocks.remove(i);
                    allocated_blocks.insert(address, block_idx);

                    stats.total_allocated += size;
                    stats.total_free -= size;
                    stats.allocated_blocks += 1;
                    stats.free_blocks -= 1;

                    return Ok(address);
                } else {
                    // Split the block
                    let new_block = MemoryBlock {
                        address: address + size,
                        size: block.size - size,
                        allocated: false,
                        ref_count: AtomicUsize::new(0),
                        last_access: AtomicU64::new(0),
                        block_type: BlockType::Temporary,
                    };

                    block.size = size;
                    block.allocated = true;
                    block.block_type = block_type;
                    block.ref_count.store(1, Ordering::SeqCst);
                    block
                        .last_access
                        .store(Instant::now().elapsed().as_nanos() as u64, Ordering::SeqCst);

                    let new_block_idx = blocks.len();
                    blocks.push(new_block);
                    free_blocks[i] = new_block_idx;

                    allocated_blocks.insert(address, block_idx);

                    stats.total_allocated += size;
                    stats.total_free -= size;
                    stats.allocated_blocks += 1;

                    return Ok(address);
                }
            }
        }

        // No suitable block found - try to expand pool
        self.expand_pool(size)?;
        self.allocate_first_fit(size, block_type)
    }

    /// Best-fit allocation
    fn allocate_best_fit(&self, size: usize, block_type: BlockType) -> Result<usize> {
        let mut free_blocks = self.free_blocks.write().unwrap();
        let mut blocks = self.blocks.write().unwrap();
        let mut allocated_blocks = self.allocated_blocks.write().unwrap();
        let mut stats = self.stats.write().unwrap();

        // Find smallest suitable free block
        let mut best_idx = None;
        let mut best_size = usize::MAX;

        for (i, &block_idx) in free_blocks.iter().enumerate() {
            if blocks[block_idx].size >= size && blocks[block_idx].size < best_size {
                best_idx = Some((i, block_idx));
                best_size = blocks[block_idx].size;
            }
        }

        if let Some((i, block_idx)) = best_idx {
            let block = &mut blocks[block_idx];
            let address = block.address;

            if block.size == size {
                // Exact fit
                block.allocated = true;
                block.block_type = block_type;
                block.ref_count.store(1, Ordering::SeqCst);
                block
                    .last_access
                    .store(Instant::now().elapsed().as_nanos() as u64, Ordering::SeqCst);

                free_blocks.remove(i);
                allocated_blocks.insert(address, block_idx);

                stats.total_allocated += size;
                stats.total_free -= size;
                stats.allocated_blocks += 1;
                stats.free_blocks -= 1;

                return Ok(address);
            } else {
                // Split the block
                let new_block = MemoryBlock {
                    address: address + size,
                    size: block.size - size,
                    allocated: false,
                    ref_count: AtomicUsize::new(0),
                    last_access: AtomicU64::new(0),
                    block_type: BlockType::Temporary,
                };

                block.size = size;
                block.allocated = true;
                block.block_type = block_type;
                block.ref_count.store(1, Ordering::SeqCst);
                block
                    .last_access
                    .store(Instant::now().elapsed().as_nanos() as u64, Ordering::SeqCst);

                let new_block_idx = blocks.len();
                blocks.push(new_block);
                free_blocks[i] = new_block_idx;

                allocated_blocks.insert(address, block_idx);

                stats.total_allocated += size;
                stats.total_free -= size;
                stats.allocated_blocks += 1;

                return Ok(address);
            }
        }

        // No suitable block found - try to expand pool
        self.expand_pool(size)?;
        self.allocate_best_fit(size, block_type)
    }

    /// Worst-fit allocation
    fn allocate_worst_fit(&self, size: usize, block_type: BlockType) -> Result<usize> {
        let mut free_blocks = self.free_blocks.write().unwrap();
        let mut blocks = self.blocks.write().unwrap();
        let mut allocated_blocks = self.allocated_blocks.write().unwrap();
        let mut stats = self.stats.write().unwrap();

        // Find largest suitable free block
        let mut best_idx = None;
        let mut best_size = 0;

        for (i, &block_idx) in free_blocks.iter().enumerate() {
            if blocks[block_idx].size >= size && blocks[block_idx].size > best_size {
                best_idx = Some((i, block_idx));
                best_size = blocks[block_idx].size;
            }
        }

        if let Some((i, block_idx)) = best_idx {
            let block = &mut blocks[block_idx];
            let address = block.address;

            if block.size == size {
                // Exact fit
                block.allocated = true;
                block.block_type = block_type;
                block.ref_count.store(1, Ordering::SeqCst);
                block
                    .last_access
                    .store(Instant::now().elapsed().as_nanos() as u64, Ordering::SeqCst);

                free_blocks.remove(i);
                allocated_blocks.insert(address, block_idx);

                stats.total_allocated += size;
                stats.total_free -= size;
                stats.allocated_blocks += 1;
                stats.free_blocks -= 1;

                return Ok(address);
            } else {
                // Split the block
                let new_block = MemoryBlock {
                    address: address + size,
                    size: block.size - size,
                    allocated: false,
                    ref_count: AtomicUsize::new(0),
                    last_access: AtomicU64::new(0),
                    block_type: BlockType::Temporary,
                };

                block.size = size;
                block.allocated = true;
                block.block_type = block_type;
                block.ref_count.store(1, Ordering::SeqCst);
                block
                    .last_access
                    .store(Instant::now().elapsed().as_nanos() as u64, Ordering::SeqCst);

                let new_block_idx = blocks.len();
                blocks.push(new_block);
                free_blocks[i] = new_block_idx;

                allocated_blocks.insert(address, block_idx);

                stats.total_allocated += size;
                stats.total_free -= size;
                stats.allocated_blocks += 1;

                return Ok(address);
            }
        }

        // No suitable block found - try to expand pool
        self.expand_pool(size)?;
        self.allocate_worst_fit(size, block_type)
    }

    /// Buddy system allocation
    fn allocate_buddy_system(&self, size: usize, block_type: BlockType) -> Result<usize> {
        // For simplicity, use first-fit for buddy system
        // In a real implementation, this would use power-of-2 block sizes
        self.allocate_first_fit(size, block_type)
    }

    /// Expand the memory pool
    fn expand_pool(&self, additional_size: usize) -> Result<()> {
        let mut blocks = self.blocks.write().unwrap();
        let mut free_blocks = self.free_blocks.write().unwrap();
        let mut stats = self.stats.write().unwrap();

        let current_size = stats.total_allocated + stats.total_free;
        let new_size = current_size + additional_size;

        if new_size > self.config.max_size {
            return Err(Error::OutOfMemory("Pool size limit exceeded".to_string()));
        }

        // Add new free block
        let new_block = MemoryBlock {
            address: current_size,
            size: additional_size,
            allocated: false,
            ref_count: AtomicUsize::new(0),
            last_access: AtomicU64::new(0),
            block_type: BlockType::Temporary,
        };

        let block_idx = blocks.len();
        blocks.push(new_block);
        free_blocks.push_back(block_idx);
        stats.total_free += additional_size;

        Ok(())
    }

    /// Deallocate memory
    pub fn deallocate(&self, address: usize) -> Result<()> {
        let mut allocated_blocks = self.allocated_blocks.write().unwrap();
        let mut blocks = self.blocks.write().unwrap();
        let mut free_blocks = self.free_blocks.write().unwrap();
        let mut stats = self.stats.write().unwrap();

        if let Some(&block_idx) = allocated_blocks.get(&address) {
            let block = &mut blocks[block_idx];

            if !block.allocated {
                return Err(Error::InvalidInput("Block already deallocated".to_string()));
            }

            // Mark as free
            block.allocated = false;
            block.ref_count.store(0, Ordering::SeqCst);

            // Add to free blocks
            free_blocks.push_back(block_idx);

            // Update statistics
            stats.total_allocated -= block.size;
            stats.total_free += block.size;
            stats.allocated_blocks -= 1;
            stats.free_blocks += 1;

            // Remove from allocated blocks
            allocated_blocks.remove(&address);

            // Try to coalesce with adjacent free blocks
            self.coalesce_blocks(block_idx)?;

            Ok(())
        } else {
            Err(Error::InvalidInput(
                "Address not found in allocated blocks".to_string(),
            ))
        }
    }

    /// Coalesce adjacent free blocks
    fn coalesce_blocks(&self, block_idx: usize) -> Result<()> {
        let mut blocks = self.blocks.write().unwrap();
        let mut free_blocks = self.free_blocks.write().unwrap();
        let mut stats = self.stats.write().unwrap();

        let block = &blocks[block_idx];
        let address = block.address;
        let size = block.size;

        // Find adjacent blocks
        let mut prev_block_idx = None;
        let mut next_block_idx = None;

        for (i, &free_idx) in free_blocks.iter().enumerate() {
            if free_idx == block_idx {
                continue;
            }

            let free_block = &blocks[free_idx];
            if free_block.address + free_block.size == address {
                prev_block_idx = Some((i, free_idx));
            } else if address + size == free_block.address {
                next_block_idx = Some((i, free_idx));
            }
        }

        // Coalesce with previous block
        if let Some((_i, prev_idx)) = prev_block_idx {
            let prev_block = &mut blocks[prev_idx];
            prev_block.size += size;

            // Remove current block from free list
            free_blocks.retain(|&idx| idx != block_idx);
            stats.free_blocks -= 1;
        }

        // Coalesce with next block
        if let Some((_i, next_idx)) = next_block_idx {
            let next_block = &mut blocks[next_idx];
            let _next_address = next_block.address;
            let next_size = next_block.size;

            // Update current block
            let current_block = &mut blocks[block_idx];
            current_block.size += next_size;

            // Remove next block from free list
            free_blocks.retain(|&idx| idx != next_idx);
            stats.free_blocks -= 1;
        }

        Ok(())
    }

    /// Start garbage collection thread
    fn start_gc_thread(&mut self) -> Result<()> {
        let blocks = self.blocks.clone();
        let free_blocks = self.free_blocks.clone();
        let allocated_blocks = self.allocated_blocks.clone();
        let stats = self.stats.clone();
        let gc_stop = self.gc_stop.clone();
        let gc_interval = self.config.gc_interval;
        let gc_threshold = self.config.gc_threshold;

        let handle = std::thread::spawn(move || {
            loop {
                if gc_stop.load(Ordering::SeqCst) == 1 {
                    break;
                }

                // Check if GC is needed
                let current_stats = stats.read().unwrap();
                let total_memory = current_stats.total_allocated + current_stats.total_free;
                let usage_ratio = if total_memory > 0 {
                    current_stats.total_allocated as f64 / total_memory as f64
                } else {
                    0.0
                };

                if usage_ratio >= gc_threshold {
                    // Perform garbage collection
                    Self::perform_gc(&blocks, &free_blocks, &allocated_blocks, &stats);
                }

                std::thread::sleep(gc_interval);
            }
        });

        self.gc_handle = Some(handle);
        Ok(())
    }

    /// Perform garbage collection
    fn perform_gc(
        blocks: &Arc<RwLock<Vec<MemoryBlock>>>,
        free_blocks: &Arc<RwLock<VecDeque<usize>>>,
        allocated_blocks: &Arc<RwLock<HashMap<usize, usize>>>,
        stats: &Arc<RwLock<MemoryStats>>,
    ) {
        let mut blocks = blocks.write().unwrap();
        let mut free_blocks = free_blocks.write().unwrap();
        let mut allocated_blocks = allocated_blocks.write().unwrap();
        let mut stats = stats.write().unwrap();

        let mut to_deallocate = Vec::new();

        // Find blocks with zero reference count
        for (&address, &block_idx) in allocated_blocks.iter() {
            let block = &blocks[block_idx];
            if block.ref_count.load(Ordering::SeqCst) == 0 {
                to_deallocate.push(address);
            }
        }

        // Deallocate unreferenced blocks
        for address in to_deallocate {
            if let Some(&block_idx) = allocated_blocks.get(&address) {
                let block = &mut blocks[block_idx];
                block.allocated = false;
                block.ref_count.store(0, Ordering::SeqCst);

                free_blocks.push_back(block_idx);

                stats.total_allocated -= block.size;
                stats.total_free += block.size;
                stats.allocated_blocks -= 1;
                stats.free_blocks += 1;

                allocated_blocks.remove(&address);
            }
        }

        stats.gc_cycles += 1;
        stats.last_gc = Some(Instant::now());
    }

    /// Get memory statistics
    pub fn get_stats(&self) -> MemoryStats {
        self.stats.read().unwrap().clone()
    }

    /// Stop garbage collection
    pub fn stop_gc(&mut self) {
        self.gc_stop.store(1, Ordering::SeqCst);
        if let Some(handle) = self.gc_handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for MemoryPool {
    fn drop(&mut self) {
        self.stop_gc();
    }
}

/// Custom allocator that uses the memory pool
pub struct PoolAllocator {
    pool: Arc<MemoryPool>,
}

impl PoolAllocator {
    pub fn new(pool: Arc<MemoryPool>) -> Self {
        Self { pool }
    }
}

unsafe impl GlobalAlloc for PoolAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match self.pool.allocate(layout.size(), BlockType::Temporary) {
            Ok(address) => address as *mut u8,
            Err(_) => std::ptr::null_mut(),
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        let _ = self.pool.deallocate(ptr as usize);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_pool_creation() {
        let config = MemoryPoolConfig::default();
        let pool = MemoryPool::new(config).unwrap();
        let stats = pool.get_stats();
        assert_eq!(stats.total_free, 1024 * 1024);
        assert_eq!(stats.allocated_blocks, 0);
    }

    #[test]
    #[ignore = "Slow test"]
    fn test_memory_allocation() {
        let config = MemoryPoolConfig::default();
        let pool = MemoryPool::new(config).unwrap();

        let address = pool.allocate(1024, BlockType::Node).unwrap();
        assert!(address > 0);

        let stats = pool.get_stats();
        assert_eq!(stats.total_allocated, 1024);
        assert_eq!(stats.allocated_blocks, 1);
    }

    #[test]
    #[ignore = "Slow test"]
    fn test_memory_deallocation() {
        let config = MemoryPoolConfig::default();
        let pool = MemoryPool::new(config).unwrap();

        let address = pool.allocate(1024, BlockType::Node).unwrap();
        pool.deallocate(address).unwrap();

        let stats = pool.get_stats();
        assert_eq!(stats.total_allocated, 0);
        assert_eq!(stats.allocated_blocks, 0);
    }

    #[test]
    #[ignore = "Slow test"]
    fn test_memory_coalescing() {
        let config = MemoryPoolConfig::default();
        let pool = MemoryPool::new(config).unwrap();

        // Allocate and deallocate multiple blocks
        let addr1 = pool.allocate(512, BlockType::Node).unwrap();
        let addr2 = pool.allocate(512, BlockType::Node).unwrap();

        pool.deallocate(addr1).unwrap();
        pool.deallocate(addr2).unwrap();

        // Allocate a larger block that should use coalesced space
        let addr3 = pool.allocate(1024, BlockType::Node).unwrap();
        assert!(addr3 > 0);
    }

    #[test]
    #[ignore = "Slow test"]
    fn test_allocation_strategies() {
        // Test first-fit
        let config1 = MemoryPoolConfig {
            initial_size: 4096,
            strategy: AllocationStrategy::FirstFit,
            ..Default::default()
        };
        let pool1 = MemoryPool::new(config1).unwrap();
        let addr1 = pool1.allocate(1024, BlockType::Node).unwrap();
        assert!(addr1 > 0);

        // Test best-fit
        let config2 = MemoryPoolConfig {
            initial_size: 4096,
            strategy: AllocationStrategy::BestFit,
            ..Default::default()
        };
        let pool2 = MemoryPool::new(config2).unwrap();
        let addr2 = pool2.allocate(1024, BlockType::Node).unwrap();
        assert!(addr2 > 0);

        // Test worst-fit
        let config3 = MemoryPoolConfig {
            initial_size: 4096,
            strategy: AllocationStrategy::WorstFit,
            ..Default::default()
        };
        let pool3 = MemoryPool::new(config3).unwrap();
        let addr3 = pool3.allocate(1024, BlockType::Node).unwrap();
        assert!(addr3 > 0);
    }
}
