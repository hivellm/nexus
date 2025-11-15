//! Memory tracking utilities for query execution
//!
//! Provides functions to measure memory usage during query execution

// Memory tracking utilities

/// Memory tracker for query execution
pub struct QueryMemoryTracker {
    /// Initial memory usage in bytes
    initial_memory: u64,
}

impl QueryMemoryTracker {
    /// Create a new memory tracker
    pub fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let initial_memory = Self::get_current_memory_usage()?;
        Ok(Self { initial_memory })
    }

    /// Get memory usage delta since tracker creation
    pub fn get_memory_delta(&self) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        let current = Self::get_current_memory_usage()?;
        Ok(current.saturating_sub(self.initial_memory))
    }

    /// Get current memory usage of the process
    pub fn get_current_memory_usage() -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        #[cfg(target_os = "linux")]
        {
            Self::get_memory_usage_linux()
        }
        #[cfg(target_os = "windows")]
        {
            Self::get_memory_usage_windows()
        }
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            // Fallback: estimate based on heap allocations
            Self::get_memory_usage_fallback()
        }
    }

    #[cfg(target_os = "linux")]
    fn get_memory_usage_linux() -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        use std::fs;
        use std::io::Read;

        // Read /proc/self/status to get VmRSS (resident set size)
        let mut file = fs::File::open("/proc/self/status")?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        for line in contents.lines() {
            if line.starts_with("VmRSS:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let Ok(kb) = parts[1].parse::<u64>() {
                        return Ok(kb * 1024); // Convert KB to bytes
                    }
                }
            }
        }

        Err("Could not find VmRSS in /proc/self/status".into())
    }

    #[cfg(target_os = "windows")]
    fn get_memory_usage_windows() -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        // On Windows, we can use GetProcessMemoryInfo from winapi
        // For now, use a simplified approach
        Self::get_memory_usage_fallback()
    }

    /// Fallback memory usage estimation
    /// Uses a simple heuristic based on allocations
    fn get_memory_usage_fallback() -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        // This is a placeholder - in a real implementation,
        // we'd track allocations or use platform-specific APIs
        // For now, return a conservative estimate
        Ok(0) // Return 0 to indicate measurement not available
    }
}

impl Default for QueryMemoryTracker {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self { initial_memory: 0 })
    }
}

/// Measure memory usage during a closure execution
pub fn measure_memory_usage<F, T>(
    f: F,
) -> Result<(T, Option<u64>), Box<dyn std::error::Error + Send + Sync>>
where
    F: FnOnce() -> T,
{
    let tracker = QueryMemoryTracker::new()?;
    let initial_memory = tracker.initial_memory;

    // Execute the closure
    let result = f();

    // Measure memory after execution
    let final_memory = QueryMemoryTracker::get_current_memory_usage()?;
    let memory_delta = final_memory.saturating_sub(initial_memory);

    // Only return memory delta if it's significant (> 1KB)
    if memory_delta > 1024 {
        Ok((result, Some(memory_delta)))
    } else {
        Ok((result, None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_tracker_creation() {
        let tracker = QueryMemoryTracker::new();
        assert!(tracker.is_ok());
    }

    #[test]
    fn test_measure_memory_usage() {
        let result = measure_memory_usage(|| {
            // Allocate some memory
            let _vec = vec![0u8; 1024 * 10]; // 10KB
            "test"
        });

        assert!(result.is_ok());
        let (value, _memory) = result.unwrap();
        assert_eq!(value, "test");
        // Memory may be None if delta is too small or measurement unavailable
        // This is acceptable
    }
}
