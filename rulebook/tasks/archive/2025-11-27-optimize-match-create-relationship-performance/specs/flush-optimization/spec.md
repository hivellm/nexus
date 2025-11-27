# Flush Optimization Specification

## Overview

The synchronous `flush()` operation is the primary bottleneck in MATCH+CREATE relationship performance, costing ~10-20ms per call. This spec defines how to optimize flush behavior while maintaining data durability.

## Current Implementation

```rust
// nexus-core/src/storage/mod.rs:371-375
pub fn flush(&mut self) -> Result<()> {
    self.flush_sync()
}

fn flush_sync(&mut self) -> Result<()> {
    self.nodes_mmap.flush()?;      // ~3-5ms
    self.rels_mmap.flush()?;       // ~3-5ms
    self.property_store.flush()?;  // ~2-3ms
    self.adjacency_store.flush()?; // ~2-3ms
    Ok(())
}
```

## Problem

The `execute_create_with_context()` calls `flush()` after creating each relationship:

```rust
// nexus-core/src/executor/mod.rs:5507
self.store_mut().flush()?;
```

For a query that creates 1 relationship, this adds ~15ms of latency.

## Proposed Solution

### 1. FlushMode Enum

```rust
/// Controls when data is flushed to disk
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FlushMode {
    /// Synchronous flush - waits for OS to confirm write
    /// Use for: Critical data, end of transactions
    Sync,

    /// Asynchronous flush - triggers flush without waiting
    /// Use for: Individual operations within a query
    Async,

    /// No flush - rely on OS page cache
    /// Use for: Batched operations where explicit flush follows
    None,
}
```

### 2. Modified Store Methods

```rust
impl Store {
    /// Flush with specified mode
    pub fn flush_with_mode(&mut self, mode: FlushMode) -> Result<()> {
        match mode {
            FlushMode::Sync => self.flush_sync(),
            FlushMode::Async => self.flush_async_internal(),
            FlushMode::None => Ok(()),
        }
    }

    /// Async flush - triggers write without waiting
    fn flush_async_internal(&mut self) -> Result<()> {
        // Use mmap::flush_async() if available
        // Or just mark pages dirty and let OS handle
        #[cfg(unix)]
        {
            self.nodes_mmap.flush_async()?;
            self.rels_mmap.flush_async()?;
        }
        #[cfg(windows)]
        {
            // Windows FlushViewOfFile is always sync
            // Just skip flush and rely on periodic sync
        }
        Ok(())
    }
}
```

### 3. Query Execution Changes

```rust
// In execute_create_with_context()
fn execute_create_with_context(&self, ...) -> Result<()> {
    // ... create relationships ...

    // Don't flush here - let query executor handle it
    // self.store_mut().flush()?;  // REMOVED

    Ok(())
}

// In execute() - after all operators complete
pub fn execute(&self, query: &Query) -> Result<ResultSet> {
    // ... execute operators ...

    // Flush once at end of query if any writes occurred
    if is_write_query {
        self.store_mut().flush_with_mode(FlushMode::Sync)?;
    }

    Ok(result)
}
```

### 4. Configuration Option

```rust
/// Storage configuration
pub struct StorageConfig {
    /// Default flush mode for write operations
    pub default_flush_mode: FlushMode,

    /// Periodic sync interval (for async mode)
    pub sync_interval_ms: u64,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            default_flush_mode: FlushMode::Sync,
            sync_interval_ms: 1000, // 1 second
        }
    }
}
```

## Expected Performance Impact

| Scenario | Before | After | Improvement |
|----------|--------|-------|-------------|
| Single MATCH+CREATE rel | 34ms | 19ms | -15ms |
| 10 relationships (same query) | 340ms | 25ms | -315ms |
| 100 relationships (batch) | 3400ms | 50ms | -3350ms |

## Durability Guarantees

### Sync Mode (Default)
- Data is guaranteed on disk after each write
- Crash-safe at query boundary
- Use for production with strict durability

### Async Mode
- Data may be lost on crash (up to sync_interval)
- Better performance for bulk operations
- Use for bulk imports, testing

### None Mode
- No durability guarantee
- Maximum performance
- Use only within batched operations

## Testing Requirements

1. **Durability Test**: Write data, crash, verify recovery
2. **Performance Test**: Benchmark before/after
3. **Concurrent Test**: Multiple writers with async flush
4. **Stress Test**: High-volume writes with periodic sync

## Migration Path

1. Add `FlushMode` and new methods (backward compatible)
2. Update `execute_create_with_context()` to skip flush
3. Add flush at end of `execute()`
4. Test extensively
5. Make async default (optional)
