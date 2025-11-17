# Async WAL Implementation Specification

## 🎯 **Overview**

Replace synchronous WAL operations with asynchronous background processing to eliminate I/O bottlenecks in write operations.

## 📋 **Requirements**

### Functional Requirements:
- [ ] WAL entries must be durable before transaction commit returns
- [ ] Background flushing must not block foreground operations
- [ ] System must maintain ACID properties
- [ ] Crash recovery must work correctly with async WAL

### Performance Requirements:
- [ ] CREATE operations <5ms average (currently 14-28ms)
- [ ] WAL flush latency <10ms for batches
- [ ] Background thread CPU usage <20%
- [ ] Memory usage for WAL queue <100MB

### Reliability Requirements:
- [ ] WAL entries never lost on system crash
- [ ] Background thread failures don't corrupt data
- [ ] Graceful degradation to sync mode if async fails
- [ ] Proper shutdown drains all pending WAL entries

## 🏗️ **Architecture**

### Components:

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   Foreground    │    │   WAL Queue       │    │  Background     │
│   Threads       │───▶│   (Async)        │───▶│   Flush Thread   │
│                 │    │                  │    │                 │
└─────────────────┘    └──────────────────┘    └─────────────────┘
         │                       │                       │
         └───────────────────────┼───────────────────────┘
                                 ▼
                    ┌─────────────────────┐
                    │   WAL File          │
                    │   (Durable)         │
                    └─────────────────────┘
```

### Data Structures:

```rust
struct AsyncWal {
    queue: Arc<Mutex<VecDeque<WalEntry>>>,
    flush_trigger: Arc<AtomicBool>,
    background_handle: Option<JoinHandle<()>>,
    config: WalConfig,
    stats: Arc<WalStats>,
}

struct WalConfig {
    max_queue_size: usize,           // 10,000 entries
    flush_interval_ms: u64,         // 10ms
    max_batch_size: usize,          // 1,000 entries
    sync_on_overflow: bool,         // true for safety
}

struct WalStats {
    queue_depth: AtomicUsize,
    flush_count: AtomicUsize,
    avg_flush_time: AtomicU64,
    background_thread_healthy: AtomicBool,
}
```

## 🔄 **Implementation Details**

### 1. Async WAL Queue Management

```rust
impl AsyncWal {
    pub fn append_async(&self, entry: WalEntry) -> Result<()> {
        // Add to queue
        {
            let mut queue = self.queue.lock();
            queue.push_back(entry);

            // Trigger flush if queue getting full
            if queue.len() >= self.config.max_batch_size {
                self.flush_trigger.store(true, Ordering::Release);
            }
        }

        // Wake background thread
        // (implementation depends on chosen sync primitive)

        Ok(())
    }

    pub fn flush_if_needed(&self) -> Result<()> {
        if self.flush_trigger.load(Ordering::Acquire) {
            self.flush_sync()?;
        }
        Ok(())
    }
}
```

### 2. Background Flush Thread

```rust
fn background_flush_loop(wal: Arc<AsyncWal>) {
    loop {
        // Wait for work or timeout
        std::thread::sleep(Duration::from_millis(
            wal.config.flush_interval_ms
        ));

        // Check if we have work to do
        if wal.queue.lock().is_empty() &&
           !wal.flush_trigger.load(Ordering::Acquire) {
            continue;
        }

        // Perform batch flush
        if let Err(e) = wal.flush_batch() {
            eprintln!("WAL background flush error: {:?}", e);
            // Continue running despite errors
        }
    }
}
```

### 3. Batch Flush Implementation

```rust
impl AsyncWal {
    fn flush_batch(&self) -> Result<()> {
        let start_time = Instant::now();

        // Extract batch from queue
        let batch = {
            let mut queue = self.queue.lock();
            let batch_size = queue.len().min(self.config.max_batch_size);
            queue.drain(0..batch_size).collect::<Vec<_>>()
        };

        if batch.is_empty() {
            return Ok(());
        }

        // Write batch to WAL file
        let mut file = self.open_wal_file()?;
        for entry in &batch {
            self.write_entry(&mut file, entry)?;
        }
        file.sync_all()?;  // fsync for durability

        // Update statistics
        let flush_time = start_time.elapsed().as_micros() as u64;
        self.stats.flush_count.fetch_add(1, Ordering::Relaxed);
        // Update rolling average...

        Ok(())
    }
}
```

### 4. Integration with Transaction Manager

```rust
impl TransactionManager {
    pub fn commit_async(&mut self, tx: &mut Transaction) -> Result<()> {
        // Validate transaction
        if tx.state != TxState::Active {
            return Err(Error::transaction("Transaction not active"));
        }

        // Generate WAL entries
        let wal_entries = self.generate_wal_entries(tx)?;

        // Submit to async WAL (non-blocking)
        for entry in wal_entries {
            self.wal.append_async(entry)?;
        }

        // Mark transaction committed
        tx.state = TxState::Committed;

        // Optionally trigger flush for critical transactions
        if tx.critical {
            self.wal.flush_if_needed()?;
        }

        Ok(())
    }
}
```

## 📊 **Performance Characteristics**

### Latency Targets:
- Foreground append: <1μs
- Background flush: <10ms for 1000 entries
- Queue depth: <100 entries under normal load

### Throughput Targets:
- 100,000 WAL entries/second (foreground)
- 10,000 entries/second (background flush)
- Sustained: 50,000 entries/second

### Memory Usage:
- Queue: <100MB (100k entries × 1KB avg)
- Background thread stack: 2MB
- WAL file buffer: 1MB

## 🧪 **Testing Strategy**

### Unit Tests:
- [ ] WAL entry serialization/deserialization
- [ ] Queue operations under concurrent load
- [ ] Background thread lifecycle management

### Integration Tests:
- [ ] Full transaction lifecycle with async WAL
- [ ] Crash recovery with pending WAL entries
- [ ] Concurrent transactions with WAL conflicts

### Performance Tests:
- [ ] WAL append throughput (target: 100k/s)
- [ ] Background flush latency (target: <10ms)
- [ ] Memory usage under sustained load

### Reliability Tests:
- [ ] System crash with pending WAL entries
- [ ] Background thread failure recovery
- [ ] Disk full conditions
- [ ] File system permissions issues

## 📈 **Monitoring & Observability**

### Metrics to Collect:
- WAL queue depth (gauge)
- Flush operations per second (counter)
- Average flush latency (histogram)
- Background thread health (boolean)
- WAL file size (gauge)
- Entries written per second (counter)

### Alerts:
- Queue depth > 10,000 entries
- Flush latency > 100ms
- Background thread unhealthy
- WAL file size > 1GB

## 🔄 **Migration Strategy**

### Phase 1: Dual Mode
- Keep sync WAL as default
- Add async WAL as optional feature
- Allow runtime switching between modes

### Phase 2: Gradual Rollout
- Enable async WAL for non-critical workloads
- Monitor performance and reliability
- Gradually increase adoption

### Phase 3: Full Migration
- Make async WAL the default
- Remove sync WAL code path
- Update documentation and configurations

## 🚨 **Safety Guarantees**

### Durability:
- WAL entries are durable before commit returns (via conditional flush)
- Background thread failures trigger sync fallback
- System crash recovery works correctly

### Consistency:
- WAL entries maintain proper ordering
- Transaction isolation properties preserved
- No dirty reads or lost updates

### Performance:
- Async operations never block foreground threads
- Memory usage bounded and monitored
- CPU usage for background threads controlled
