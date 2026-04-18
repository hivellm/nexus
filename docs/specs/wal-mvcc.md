# WAL & MVCC Specification

This document defines the Write-Ahead Log (WAL) and Multi-Version Concurrency Control (MVCC) implementation for Nexus.

## Overview

### Write-Ahead Log (WAL)

Ensures durability via the "write-ahead" principle:
1. **Before** modifying data pages, write changes to WAL
2. Flush WAL to disk (fsync)
3. **Then** modify data pages in memory (cached)
4. Periodically checkpoint (flush pages + truncate WAL)

### MVCC (Multi-Version Concurrency Control)

Provides snapshot isolation without locking readers:
- **Readers** see consistent snapshot (pinned epoch)
- **Writers** create new versions (append-only)
- **No read-write conflicts** (readers never block writers)
- **Garbage collection** removes old versions

## WAL Format

### File Structure

```
data/
├── wal.log              # Active WAL file
├── wal.log.1            # Archived WAL (after checkpoint)
├── wal.log.2
└── checkpoints/
    ├── epoch_1000.ckpt  # Checkpoint metadata
    └── epoch_2000.ckpt
```

### WAL Entry Format

```
WAL Entry:
┌──────────┬──────────┬──────────┬──────────┬──────────┬──────────┐
│  epoch   │  tx_id   │  type    │  length  │ payload  │  crc32   │
│(8 bytes) │(8 bytes) │(1 byte)  │(4 bytes) │(variable)│(4 bytes) │
└──────────┴──────────┴──────────┴──────────┴──────────┴──────────┘

epoch:   Transaction epoch (u64)
tx_id:   Transaction ID (u64)
type:    Entry type (u8, see below)
length:  Payload length in bytes (u32)
payload: Entry-specific data
crc32:   CRC32 of entire entry (for integrity)
```

### Entry Types

```rust
enum WalEntryType {
    BeginTx = 0x01,         // Transaction start
    CommitTx = 0x02,        // Transaction commit
    AbortTx = 0x03,         // Transaction abort
    CreateNode = 0x10,      // Node creation
    DeleteNode = 0x11,      // Node deletion
    CreateRel = 0x20,       // Relationship creation
    DeleteRel = 0x21,       // Relationship deletion
    SetProperty = 0x30,     // Property set/update
    DeleteProperty = 0x31,  // Property deletion
    AddLabel = 0x40,        // Add label to node
    RemoveLabel = 0x41,     // Remove label from node
    Checkpoint = 0xFF,      // Checkpoint marker
}
```

### Entry Payloads

#### BEGIN_TX
```
Payload (16 bytes):
┌──────────┬──────────┐
│timestamp │ reserved │
│(8 bytes) │(8 bytes) │
└──────────┴──────────┘
```

#### COMMIT_TX / ABORT_TX
```
Payload (0 bytes):
(no additional data)
```

#### CREATE_NODE
```
Payload:
┌──────────┬──────────┬──────────┬──────────┐
│ node_id  │label_bits│ num_props│properties│
│(8 bytes) │(8 bytes) │(4 bytes) │(variable)│
└──────────┴──────────┴──────────┴──────────┘

properties: Repeated (key_id: u32, type: u8, value: bytes)
```

#### CREATE_REL
```
Payload:
┌──────────┬──────────┬──────────┬──────────┬──────────┬──────────┐
│  rel_id  │  src_id  │  dst_id  │ type_id  │ num_props│properties│
│(8 bytes) │(8 bytes) │(8 bytes) │(4 bytes) │(4 bytes) │(variable)│
└──────────┴──────────┴──────────┴──────────┴──────────┴──────────┘
```

#### SET_PROPERTY
```
Payload:
┌──────────┬──────────┬──────────┬──────────┬──────────┐
│entity_id │entity_typ│  key_id  │val_type  │  value   │
│(8 bytes) │(1 byte)  │(4 bytes) │(1 byte)  │(variable)│
└──────────┴──────────┴──────────┴──────────┴──────────┘

entity_type: 0=node, 1=relationship
```

#### CHECKPOINT
```
Payload:
┌──────────┬──────────┬──────────┐
│  epoch   │timestamp │ reserved │
│(8 bytes) │(8 bytes) │(8 bytes) │
└──────────┴──────────┴──────────┘
```

## WAL Operations

### Append Entry

```rust
impl Wal {
    fn append(&mut self, entry: WalEntry) -> Result<u64> {
        // Serialize entry
        let mut buf = Vec::new();
        buf.extend_from_slice(&entry.epoch.to_le_bytes());
        buf.extend_from_slice(&entry.tx_id.to_le_bytes());
        buf.push(entry.entry_type as u8);
        let payload_len = entry.payload.len() as u32;
        buf.extend_from_slice(&payload_len.to_le_bytes());
        buf.extend_from_slice(&entry.payload);
        
        // Compute CRC32
        let crc = crc32c(&buf);
        buf.extend_from_slice(&crc.to_le_bytes());
        
        // Append to file
        let offset = self.file.seek(SeekFrom::End(0))?;
        self.file.write_all(&buf)?;
        
        Ok(offset)
    }
    
    fn flush(&mut self) -> Result<()> {
        self.file.sync_all()?;  // fsync
        Ok(())
    }
}
```

### Recovery

```rust
impl Wal {
    fn recover(&mut self) -> Result<Vec<WalEntry>> {
        let mut entries = Vec::new();
        let mut offset = 0;
        
        loop {
            self.file.seek(SeekFrom::Start(offset))?;
            
            // Read entry header (25 bytes minimum)
            let mut header = [0u8; 25];
            match self.file.read_exact(&mut header) {
                Ok(_) => {},
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.into()),
            }
            
            // Parse header
            let epoch = u64::from_le_bytes(header[0..8].try_into().unwrap());
            let tx_id = u64::from_le_bytes(header[8..16].try_into().unwrap());
            let entry_type = header[16];
            let payload_len = u32::from_le_bytes(header[17..21].try_into().unwrap());
            
            // Read payload + CRC
            let mut payload = vec![0u8; payload_len as usize];
            self.file.read_exact(&mut payload)?;
            let mut crc_buf = [0u8; 4];
            self.file.read_exact(&mut crc_buf)?;
            let stored_crc = u32::from_le_bytes(crc_buf);
            
            // Validate CRC
            let mut buf = Vec::new();
            buf.extend_from_slice(&header[0..21]);
            buf.extend_from_slice(&payload);
            let computed_crc = crc32c(&buf);
            
            if stored_crc != computed_crc {
                return Err(Error::wal(format!(
                    "CRC mismatch at offset {}: expected {:x}, got {:x}",
                    offset, stored_crc, computed_crc
                )));
            }
            
            // Add valid entry
            entries.push(WalEntry {
                epoch,
                tx_id,
                entry_type: WalEntryType::from_u8(entry_type)?,
                payload,
            });
            
            offset += (25 + payload_len + 4) as u64;
        }
        
        Ok(entries)
    }
}
```

### Checkpoint

```rust
impl Wal {
    fn checkpoint(&mut self, epoch: u64) -> Result<()> {
        // 1. Write checkpoint marker
        let entry = WalEntry {
            epoch,
            tx_id: 0,
            entry_type: WalEntryType::Checkpoint,
            payload: checkpoint_payload(epoch),
        };
        self.append(entry)?;
        self.flush()?;
        
        // 2. Flush all dirty pages
        self.page_cache.flush_dirty_pages()?;
        
        // 3. Archive old WAL
        self.archive_wal(epoch)?;
        
        // 4. Truncate active WAL
        self.file.set_len(0)?;
        self.file.seek(SeekFrom::Start(0))?;
        
        Ok(())
    }
    
    fn archive_wal(&self, epoch: u64) -> Result<()> {
        let archive_path = format!("data/wal.log.{}", epoch);
        std::fs::copy("data/wal.log", &archive_path)?;
        Ok(())
    }
}
```

## MVCC Implementation

### Epoch-Based Snapshots

```rust
struct EpochManager {
    current_epoch: AtomicU64,
    active_snapshots: RwLock<HashSet<u64>>,  // Readers pinned here
}

impl EpochManager {
    fn current(&self) -> u64 {
        self.current_epoch.load(Ordering::Acquire)
    }
    
    fn increment(&self) -> u64 {
        self.current_epoch.fetch_add(1, Ordering::AcqRel) + 1
    }
    
    fn pin_snapshot(&self) -> Snapshot {
        let epoch = self.current();
        self.active_snapshots.write().insert(epoch);
        Snapshot { epoch, manager: self }
    }
}

struct Snapshot<'a> {
    epoch: u64,
    manager: &'a EpochManager,
}

impl Drop for Snapshot<'_> {
    fn drop(&mut self) {
        self.manager.active_snapshots.write().remove(&self.epoch);
    }
}
```

### Version Visibility

```rust
struct RecordVersion {
    created_epoch: u64,
    deleted_epoch: Option<u64>,  // None if still alive
    data: RecordData,
}

impl RecordVersion {
    fn is_visible(&self, snapshot_epoch: u64) -> bool {
        // Visible if created before snapshot and not deleted yet
        self.created_epoch <= snapshot_epoch &&
        self.deleted_epoch.map_or(true, |d| d > snapshot_epoch)
    }
}

// Usage in executor:
fn read_node(&self, node_id: u64, snapshot: &Snapshot) -> Result<Option<NodeRecord>> {
    let versions = self.storage.get_node_versions(node_id)?;
    
    // Find visible version
    for version in versions {
        if version.is_visible(snapshot.epoch) {
            return Ok(Some(version.data));
        }
    }
    
    Ok(None)  // Node doesn't exist in this snapshot
}
```

### Append-Only Updates

```rust
fn update_node_property(
    &mut self,
    tx: &mut Transaction,
    node_id: u64,
    key_id: u32,
    value: Value,
) -> Result<()> {
    // 1. Log to WAL
    let wal_entry = WalEntry::set_property(tx.epoch, tx.id, node_id, key_id, value);
    self.wal.append(wal_entry)?;
    
    // 2. Create new version (append-only)
    let mut new_version = self.storage.get_latest_version(node_id)?.clone();
    new_version.created_epoch = tx.epoch;
    new_version.deleted_epoch = None;
    new_version.set_property(key_id, value);
    
    // 3. Mark old version as deleted
    self.storage.mark_deleted(node_id, tx.epoch)?;
    
    // 4. Append new version
    self.storage.append_version(node_id, new_version)?;
    
    Ok(())
}
```

### Garbage Collection

```rust
struct GarbageCollector {
    epoch_manager: Arc<EpochManager>,
}

impl GarbageCollector {
    fn collect(&self) -> Result<usize> {
        // Find minimum active snapshot epoch
        let min_epoch = self.epoch_manager
            .active_snapshots
            .read()
            .iter()
            .min()
            .copied()
            .unwrap_or(self.epoch_manager.current());
        
        // Remove versions deleted before min_epoch
        let mut removed = 0;
        for (node_id, versions) in self.storage.all_versions() {
            versions.retain(|v| {
                if let Some(deleted) = v.deleted_epoch {
                    if deleted < min_epoch {
                        removed += 1;
                        return false;  // Can be safely removed
                    }
                }
                true  // Keep
            });
        }
        
        Ok(removed)
    }
}
```

## Transaction Model

### Transaction Struct

```rust
#[derive(Debug, Clone)]
struct Transaction {
    id: u64,
    epoch: u64,
    state: TxState,
    started_at: Instant,
    wal_offset: u64,  // First WAL entry offset
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TxState {
    Active,
    Committed,
    Aborted,
}
```

### Transaction Lifecycle

```rust
impl TransactionManager {
    fn begin(&mut self) -> Result<Transaction> {
        let epoch = self.epoch_manager.current();
        let tx_id = self.next_tx_id.fetch_add(1, Ordering::Relaxed);
        
        // Log BEGIN
        let entry = WalEntry {
            epoch,
            tx_id,
            entry_type: WalEntryType::BeginTx,
            payload: Vec::new(),
        };
        let wal_offset = self.wal.append(entry)?;
        
        Ok(Transaction {
            id: tx_id,
            epoch,
            state: TxState::Active,
            started_at: Instant::now(),
            wal_offset,
        })
    }
    
    fn commit(&mut self, tx: &mut Transaction) -> Result<()> {
        if tx.state != TxState::Active {
            return Err(Error::transaction("Transaction not active"));
        }
        
        // Log COMMIT
        let entry = WalEntry {
            epoch: tx.epoch,
            tx_id: tx.id,
            entry_type: WalEntryType::CommitTx,
            payload: Vec::new(),
        };
        self.wal.append(entry)?;
        self.wal.flush()?;  // fsync before returning
        
        // Increment epoch
        self.epoch_manager.increment();
        
        tx.state = TxState::Committed;
        Ok(())
    }
    
    fn abort(&mut self, tx: &mut Transaction) -> Result<()> {
        if tx.state != TxState::Active {
            return Err(Error::transaction("Transaction not active"));
        }
        
        // Log ABORT
        let entry = WalEntry {
            epoch: tx.epoch,
            tx_id: tx.id,
            entry_type: WalEntryType::AbortTx,
            payload: Vec::new(),
        };
        self.wal.append(entry)?;
        
        // Rollback changes (mark versions as deleted)
        self.rollback_tx(tx)?;
        
        tx.state = TxState::Aborted;
        Ok(())
    }
}
```

## Concurrency Control

### Single-Writer MVP

```rust
struct WriteLock {
    lock: Mutex<()>,  // Only one writer at a time
}

impl TransactionManager {
    fn begin_write(&mut self) -> Result<(Transaction, WriteLockGuard)> {
        let guard = self.write_lock.lock.lock();
        let tx = self.begin()?;
        Ok((tx, WriteLockGuard { guard, tx }))
    }
}

struct WriteLockGuard<'a> {
    guard: MutexGuard<'a, ()>,
    tx: Transaction,
}

// RAII: auto-unlock on drop
```

### V1: Group Commit

```rust
struct GroupCommit {
    pending: Mutex<Vec<Transaction>>,
    commit_interval: Duration,
}

impl GroupCommit {
    fn enqueue(&self, tx: Transaction) -> Result<()> {
        self.pending.lock().push(tx);
        Ok(())
    }
    
    async fn flush_loop(&self) {
        let mut interval = tokio::time::interval(self.commit_interval);
        loop {
            interval.tick().await;
            self.flush().await.ok();
        }
    }
    
    async fn flush(&self) -> Result<()> {
        let mut pending = self.pending.lock();
        if pending.is_empty() {
            return Ok(());
        }
        
        // Write all COMMIT entries
        for tx in pending.iter() {
            let entry = WalEntry::commit_tx(tx.epoch, tx.id);
            self.wal.append(entry)?;
        }
        
        // Single fsync for all
        self.wal.flush()?;
        
        // Clear pending
        pending.clear();
        
        Ok(())
    }
}
```

## Crash Recovery

### Recovery Process

```rust
impl Engine {
    fn recover(&mut self) -> Result<()> {
        // 1. Load last checkpoint
        let checkpoint = self.load_latest_checkpoint()?;
        
        // 2. Replay WAL from checkpoint epoch
        let wal_entries = self.wal.recover()?;
        
        // 3. Rebuild in-memory state
        let mut active_txs: HashMap<u64, Transaction> = HashMap::new();
        
        for entry in wal_entries {
            if entry.epoch < checkpoint.epoch {
                continue;  // Before checkpoint, skip
            }
            
            match entry.entry_type {
                WalEntryType::BeginTx => {
                    active_txs.insert(entry.tx_id, Transaction::from_wal(&entry));
                }
                WalEntryType::CommitTx => {
                    if let Some(tx) = active_txs.remove(&entry.tx_id) {
                        self.apply_committed_tx(&tx)?;
                    }
                }
                WalEntryType::AbortTx => {
                    active_txs.remove(&entry.tx_id);
                }
                _ => {
                    // Apply operation to transaction
                    if let Some(tx) = active_txs.get_mut(&entry.tx_id) {
                        tx.add_operation(entry);
                    }
                }
            }
        }
        
        // 4. Abort active (uncommitted) transactions
        for tx in active_txs.values() {
            self.rollback_tx(tx)?;
        }
        
        Ok(())
    }
}
```

## Performance Characteristics

### WAL Throughput

```
Sequential append: ~50K entries/sec (SSD)
With fsync:        ~1K commits/sec (single)
Group commit:      ~10K commits/sec (batched, 10ms interval)
```

### MVCC Overhead

```
Storage overhead: 
- 16 bytes per version (created_epoch + deleted_epoch)
- Typical: 1-2 versions per record (low update rate)

GC frequency:
- Run every 60 seconds
- Remove versions older than min active snapshot
```

### Transaction Latency

```
Begin:  < 1 μs (increment counter)
Commit: ~1 ms (WAL fsync)
Abort:  ~100 μs (no fsync needed)
```

## Configuration

```rust
struct WalConfig {
    /// WAL file path
    wal_path: PathBuf,
    
    /// Checkpoint interval (seconds)
    checkpoint_interval_secs: u64,  // Default: 300 (5 minutes)
    
    /// Max WAL size before forced checkpoint
    max_wal_size_mb: usize,  // Default: 1024 (1GB)
    
    /// GC interval (seconds)
    gc_interval_secs: u64,  // Default: 60
}

struct MvccConfig {
    /// Enable MVCC (vs single-version)
    enabled: bool,  // Default: true
    
    /// Max versions per record
    max_versions: usize,  // Default: 100
    
    /// Version retention time (seconds)
    version_retention_secs: u64,  // Default: 3600 (1 hour)
}
```

## Testing

### Unit Tests

```rust
#[test]
fn test_wal_recovery() {
    let mut wal = Wal::new("test_wal.log").unwrap();
    
    // Append entries
    wal.append(WalEntry::begin_tx(1, 100)).unwrap();
    wal.append(WalEntry::create_node(1, 100, 1, vec![])).unwrap();
    wal.append(WalEntry::commit_tx(1, 100)).unwrap();
    wal.flush().unwrap();
    
    // Simulate crash
    drop(wal);
    
    // Recover
    let mut wal = Wal::new("test_wal.log").unwrap();
    let entries = wal.recover().unwrap();
    assert_eq!(entries.len(), 3);
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_mvcc_snapshot_isolation() {
    let engine = Engine::new().unwrap();
    
    // Writer: Create node
    let mut tx_w = engine.begin_write().unwrap();
    engine.create_node(&mut tx_w, vec![0], props()).unwrap();
    
    // Reader: Start before commit (shouldn't see node)
    let snapshot = engine.snapshot();
    
    // Writer: Commit
    engine.commit(&mut tx_w).unwrap();
    
    // Reader: Still doesn't see node (snapshot isolation)
    let node = engine.get_node(1, &snapshot).unwrap();
    assert!(node.is_none());
    
    // New reader: Sees node
    let snapshot2 = engine.snapshot();
    let node = engine.get_node(1, &snapshot2).unwrap();
    assert!(node.is_some());
}
```

## References

- PostgreSQL WAL: https://www.postgresql.org/docs/current/wal-intro.html
- MVCC in Postgres: https://www.postgresql.org/docs/current/mvcc.html
- SQLite WAL Mode: https://www.sqlite.org/wal.html
- CockroachDB MVCC: https://www.cockroachlabs.com/blog/living-without-atomic-clocks/

