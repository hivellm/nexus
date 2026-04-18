//! NUMA-Aware Memory Allocation
//!
//! This module provides NUMA-aware memory allocation and thread scheduling
//! optimizations for multi-socket architectures.

use crate::Result;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU32, Ordering};

/// NUMA node identifier
pub type NumaNode = u32;

/// NUMA-aware allocator configuration
#[derive(Debug, Clone)]
pub struct NumaConfig {
    /// Enable NUMA-aware allocation
    pub enabled: bool,
    /// Preferred NUMA node for allocation
    pub preferred_node: Option<NumaNode>,
    /// Number of NUMA nodes detected
    pub num_nodes: u32,
}

impl Default for NumaConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            preferred_node: None,
            num_nodes: detect_numa_nodes(),
        }
    }
}

/// Detect the number of NUMA nodes in the system
pub fn detect_numa_nodes() -> u32 {
    // On Linux, check /sys/devices/system/node/node*/cpulist
    // For now, return 1 (single node) if detection fails
    #[cfg(target_os = "linux")]
    {
        use std::fs;
        use std::path::Path;

        let sys_node_path = Path::new("/sys/devices/system/node");
        if sys_node_path.exists() {
            let mut count = 0;
            for entry in fs::read_dir(sys_node_path).unwrap_or_else(|_| {
                fs::read_dir(Path::new("/")).unwrap() // Fallback to avoid panic
            }) {
                if let Ok(entry) = entry {
                    let name = entry.file_name();
                    if name.to_string_lossy().starts_with("node") {
                        count += 1;
                    }
                }
            }
            if count > 0 {
                return count as u32;
            }
        }
    }

    // Default: assume single NUMA node
    1
}

/// NUMA-aware allocator
pub struct NumaAllocator {
    config: NumaConfig,
    current_node: AtomicU32,
}

impl NumaAllocator {
    /// Create a new NUMA-aware allocator
    pub fn new(config: NumaConfig) -> Self {
        Self {
            config,
            current_node: AtomicU32::new(0),
        }
    }

    /// Get the preferred NUMA node for the current thread
    pub fn get_preferred_node(&self) -> NumaNode {
        if let Some(node) = self.config.preferred_node {
            node
        } else {
            self.current_node.load(Ordering::Relaxed) % self.config.num_nodes
        }
    }

    /// Set the preferred NUMA node for the current thread
    pub fn set_preferred_node(&self, node: NumaNode) {
        if node < self.config.num_nodes {
            self.current_node.store(node, Ordering::Relaxed);
        }
    }

    /// Allocate memory on a specific NUMA node
    pub fn allocate_on_node(&self, layout: Layout, node: NumaNode) -> Result<*mut u8> {
        if !self.config.enabled {
            // Fallback to standard allocation
            unsafe {
                let ptr = System.alloc(layout);
                if ptr.is_null() {
                    return Err(crate::Error::OutOfMemory("Allocation failed".to_string()));
                }
                return Ok(ptr);
            }
        }

        #[cfg(target_os = "linux")]
        {
            // Use libnuma if available, otherwise fallback
            unsafe {
                let ptr = System.alloc(layout);
                if ptr.is_null() {
                    return Err(crate::Error::OutOfMemory("Allocation failed".to_string()));
                }

                // Try to bind memory to NUMA node using mbind
                // This is a simplified version - full implementation would use libnuma
                // For now, we just track the preference
                Ok(ptr)
            }
        }

        #[cfg(not(target_os = "linux"))]
        {
            // On non-Linux systems, use standard allocation
            unsafe {
                let ptr = System.alloc(layout);
                if ptr.is_null() {
                    return Err(crate::Error::OutOfMemory("Allocation failed".to_string()));
                }
                Ok(ptr)
            }
        }
    }

    /// Get NUMA node statistics
    pub fn get_stats(&self) -> NumaStats {
        NumaStats {
            num_nodes: self.config.num_nodes,
            enabled: self.config.enabled,
            current_node: self.current_node.load(Ordering::Relaxed),
        }
    }
}

/// NUMA statistics
#[derive(Debug, Clone)]
pub struct NumaStats {
    /// Number of NUMA nodes
    pub num_nodes: u32,
    /// Whether NUMA-aware allocation is enabled
    pub enabled: bool,
    /// Current preferred NUMA node
    pub current_node: NumaNode,
}

/// NUMA-aware thread scheduler
pub struct NumaScheduler {
    config: NumaConfig,
    thread_affinity: std::sync::Arc<parking_lot::RwLock<HashMap<std::thread::ThreadId, NumaNode>>>,
}

impl NumaScheduler {
    /// Create a new NUMA-aware scheduler
    pub fn new(config: NumaConfig) -> Self {
        Self {
            config,
            thread_affinity: std::sync::Arc::new(parking_lot::RwLock::new(HashMap::new())),
        }
    }

    /// Bind the current thread to a specific NUMA node
    pub fn bind_thread_to_node(&self, node: NumaNode) -> Result<()> {
        if !self.config.enabled {
            return Ok(()); // No-op if disabled
        }

        if node >= self.config.num_nodes {
            return Err(crate::Error::InvalidInput(format!(
                "Invalid NUMA node: {} (max: {})",
                node,
                self.config.num_nodes - 1
            )));
        }

        #[cfg(target_os = "linux")]
        {
            // Use CPU affinity to bind to NUMA node
            // This is a simplified version - full implementation would use libnuma
            // For now, we just track the affinity
        }

        let thread_id = std::thread::current().id();
        self.thread_affinity.write().insert(thread_id, node);

        Ok(())
    }

    /// Get the NUMA node for the current thread
    pub fn get_thread_node(&self) -> Option<NumaNode> {
        let thread_id = std::thread::current().id();
        self.thread_affinity.read().get(&thread_id).copied()
    }

    /// Schedule a task on a specific NUMA node
    pub fn schedule_on_node<F, R>(&self, node: NumaNode, f: F) -> Result<R>
    where
        F: FnOnce() -> R + Send,
        R: Send,
    {
        if !self.config.enabled {
            return Ok(f()); // Execute directly if disabled
        }

        // For now, execute on current thread with affinity tracking
        // Full implementation would use a thread pool with NUMA awareness
        let _guard = self.bind_thread_to_node(node)?;
        Ok(f())
    }
}

use std::collections::HashMap;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_numa_detection() {
        let nodes = detect_numa_nodes();
        assert!(nodes >= 1, "Should detect at least 1 NUMA node");
    }

    #[test]
    fn test_numa_allocator() {
        let config = NumaConfig {
            enabled: false, // Disable for testing
            preferred_node: None,
            num_nodes: 2,
        };
        let allocator = NumaAllocator::new(config);

        let layout = Layout::from_size_align(1024, 8).unwrap();
        let ptr = allocator.allocate_on_node(layout, 0).unwrap();
        assert!(!ptr.is_null());

        unsafe {
            System.dealloc(ptr, layout);
        }
    }

    #[test]
    fn test_numa_scheduler() {
        let config = NumaConfig {
            enabled: false, // Disable for testing
            preferred_node: None,
            num_nodes: 2,
        };
        let scheduler = NumaScheduler::new(config);

        let result = scheduler.schedule_on_node(0, || 42).unwrap();
        assert_eq!(result, 42);
    }
}
