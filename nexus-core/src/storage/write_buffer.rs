//! Write Buffer for Batched Write Operations
//!
//! This module provides a write buffer that accumulates write operations
//! and flushes them in batches to improve write performance.
//!
//! Features:
//! - **Batching**: Accumulates writes up to a configurable batch size
//! - **Auto-flush**: Automatically flushes when batch size threshold is reached
//! - **Timeout-based flush**: Flushes after a configurable timeout
//! - **Manual flush**: API for manual flush when needed
//!
//! ## Performance Improvements
//!
//! - **Reduced I/O**: Batches multiple writes into single flush
//! - **Lower latency**: Reduces number of sync operations
//! - **Better throughput**: Maximizes write bandwidth utilization

use crate::Result;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Write operation type
#[derive(Debug, Clone)]
pub enum WriteOperation {
    /// Create node operation
    CreateNode {
        node_id: u64,
        label_bits: u64,
        properties: serde_json::Value,
    },
    /// Create relationship operation
    CreateRelationship {
        rel_id: u64,
        source_id: u64,
        target_id: u64,
        type_id: u32,
        properties: serde_json::Value,
    },
    /// Update node properties
    UpdateNode {
        node_id: u64,
        properties: serde_json::Value,
    },
    /// Update relationship properties
    UpdateRelationship {
        rel_id: u64,
        properties: serde_json::Value,
    },
    /// Delete node operation
    DeleteNode { node_id: u64 },
    /// Delete relationship operation
    DeleteRelationship { rel_id: u64 },
}

/// Write buffer configuration
#[derive(Debug, Clone)]
pub struct WriteBufferConfig {
    /// Maximum batch size (number of operations)
    pub max_batch_size: usize,
    /// Maximum batch age before auto-flush
    pub max_batch_age: Duration,
    /// Whether to enable auto-flush
    pub auto_flush_enabled: bool,
}

impl Default for WriteBufferConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 100,                      // Batch up to 100 operations
            max_batch_age: Duration::from_millis(10), // Flush after 10ms
            auto_flush_enabled: true,
        }
    }
}

/// Internal state of the write buffer
struct WriteBufferState {
    /// Pending write operations
    operations: VecDeque<WriteOperation>,
    /// Timestamp of first operation in current batch
    batch_start_time: Option<Instant>,
    /// Total operations buffered (including flushed)
    total_operations: u64,
    /// Total batches flushed
    total_batches_flushed: u64,
}

impl WriteBufferState {
    fn new() -> Self {
        Self {
            operations: VecDeque::new(),
            batch_start_time: None,
            total_operations: 0,
            total_batches_flushed: 0,
        }
    }

    fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }

    fn len(&self) -> usize {
        self.operations.len()
    }

    fn should_flush(&self, config: &WriteBufferConfig) -> bool {
        if !config.auto_flush_enabled {
            return false;
        }

        // Flush if batch size threshold reached
        if self.operations.len() >= config.max_batch_size {
            return true;
        }

        // Flush if timeout reached
        if let Some(start_time) = self.batch_start_time {
            if start_time.elapsed() >= config.max_batch_age {
                return true;
            }
        }

        false
    }

    fn take_operations(&mut self) -> Vec<WriteOperation> {
        let operations: Vec<WriteOperation> = self.operations.drain(..).collect();
        self.batch_start_time = None;
        if !operations.is_empty() {
            self.total_batches_flushed += 1;
        }
        operations
    }
}

/// Statistics for the write buffer
#[derive(Debug, Clone, Default)]
pub struct WriteBufferStats {
    /// Total operations buffered
    pub total_operations: u64,
    /// Total batches flushed
    pub total_batches_flushed: u64,
    /// Total operations flushed
    pub total_operations_flushed: u64,
    /// Current pending operations count
    pub current_pending: usize,
    /// Average batch size
    pub avg_batch_size: f64,
}

/// Write buffer for batching write operations
pub struct WriteBuffer {
    /// Internal state (protected by mutex)
    state: Arc<Mutex<WriteBufferState>>,
    /// Configuration
    config: WriteBufferConfig,
    /// Statistics
    stats: Arc<Mutex<WriteBufferStats>>,
}

impl WriteBuffer {
    /// Create a new write buffer with default configuration
    pub fn new() -> Self {
        Self::with_config(WriteBufferConfig::default())
    }

    /// Create a new write buffer with custom configuration
    pub fn with_config(config: WriteBufferConfig) -> Self {
        Self {
            state: Arc::new(Mutex::new(WriteBufferState::new())),
            config,
            stats: Arc::new(Mutex::new(WriteBufferStats::default())),
        }
    }

    /// Add a write operation to the buffer
    ///
    /// Returns `true` if the buffer should be flushed (threshold reached)
    pub fn add_operation(&self, operation: WriteOperation) -> Result<bool> {
        let mut state = self.state.lock();
        let mut stats = self.stats.lock();

        // Set batch start time if this is the first operation
        if state.batch_start_time.is_none() && !state.operations.is_empty() {
            state.batch_start_time = Some(Instant::now());
        }

        state.operations.push_back(operation);
        state.total_operations += 1;
        stats.total_operations += 1;
        stats.current_pending = state.operations.len();

        // Check if we should flush
        let should_flush = state.should_flush(&self.config);
        if should_flush {
            // Update stats before flushing
            stats.current_pending = 0;
        }

        Ok(should_flush)
    }

    /// Take all pending operations for flushing
    ///
    /// This clears the buffer and returns all accumulated operations
    pub fn take_operations(&self) -> Vec<WriteOperation> {
        let mut state = self.state.lock();
        let mut stats = self.stats.lock();

        let operations = state.take_operations();
        stats.total_operations_flushed += operations.len() as u64;
        stats.current_pending = state.operations.len();

        // Update average batch size
        if stats.total_batches_flushed > 0 {
            stats.avg_batch_size =
                stats.total_operations_flushed as f64 / stats.total_batches_flushed as f64;
        }

        operations
    }

    /// Check if the buffer should be flushed (without taking operations)
    pub fn should_flush(&self) -> bool {
        let state = self.state.lock();
        state.should_flush(&self.config)
    }

    /// Get current pending operations count
    pub fn pending_count(&self) -> usize {
        let state = self.state.lock();
        state.len()
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        let state = self.state.lock();
        state.is_empty()
    }

    /// Get statistics
    pub fn stats(&self) -> WriteBufferStats {
        let state = self.state.lock();
        let stats = self.stats.lock();
        WriteBufferStats {
            total_operations: state.total_operations,
            total_batches_flushed: state.total_batches_flushed,
            total_operations_flushed: stats.total_operations_flushed,
            current_pending: state.operations.len(),
            avg_batch_size: stats.avg_batch_size,
        }
    }

    /// Update configuration
    pub fn update_config(&mut self, config: WriteBufferConfig) {
        self.config = config;
    }

    /// Get current configuration
    pub fn config(&self) -> &WriteBufferConfig {
        &self.config
    }
}

impl Default for WriteBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_write_buffer_empty() {
        let buffer = WriteBuffer::new();
        assert!(buffer.is_empty());
        assert_eq!(buffer.pending_count(), 0);
    }

    #[test]
    fn test_write_buffer_add_operation() {
        let buffer = WriteBuffer::new();
        let operation = WriteOperation::CreateNode {
            node_id: 1,
            label_bits: 0,
            properties: json!({}),
        };

        buffer.add_operation(operation).unwrap();
        assert!(!buffer.is_empty());
        assert_eq!(buffer.pending_count(), 1);
    }

    #[test]
    fn test_write_buffer_batch_threshold() {
        let mut config = WriteBufferConfig::default();
        config.max_batch_size = 5;
        let buffer = WriteBuffer::with_config(config);

        // Add operations up to threshold
        for i in 0..5 {
            let operation = WriteOperation::CreateNode {
                node_id: i,
                label_bits: 0,
                properties: json!({}),
            };
            let should_flush = buffer.add_operation(operation).unwrap();
            if i == 4 {
                assert!(should_flush); // Should flush at threshold
            } else {
                assert!(!should_flush);
            }
        }
    }

    #[test]
    fn test_write_buffer_take_operations() {
        let buffer = WriteBuffer::new();

        // Add some operations
        for i in 0..3 {
            let operation = WriteOperation::CreateNode {
                node_id: i,
                label_bits: 0,
                properties: json!({}),
            };
            buffer.add_operation(operation).unwrap();
        }

        assert_eq!(buffer.pending_count(), 3);

        // Take operations
        let operations = buffer.take_operations();
        assert_eq!(operations.len(), 3);
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_write_buffer_stats() {
        let buffer = WriteBuffer::new();

        // Add operations
        for i in 0..10 {
            let operation = WriteOperation::CreateNode {
                node_id: i,
                label_bits: 0,
                properties: json!({}),
            };
            buffer.add_operation(operation).unwrap();
        }

        let stats = buffer.stats();
        assert_eq!(stats.total_operations, 10);
        assert_eq!(stats.current_pending, 10);

        // Flush
        buffer.take_operations();
        let stats = buffer.stats();
        assert_eq!(stats.total_operations_flushed, 10);
        assert_eq!(stats.current_pending, 0);
        assert_eq!(stats.total_batches_flushed, 1);
    }
}
