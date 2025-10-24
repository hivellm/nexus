# Transaction Manager Specification (Delta)

## ADDED Requirements

### Requirement: Read Transaction Creation
The system SHALL support creating read-only transactions with snapshot isolation.

#### Scenario: Begin read transaction
- **WHEN** begin_read() is called
- **THEN** create a snapshot pinned to current epoch
- **AND** transaction sees consistent view of data

#### Scenario: Snapshot isolation
- **WHEN** a read transaction is active
- **AND** a write transaction commits new data
- **THEN** the read transaction does NOT see the new data
- **AND** maintains its snapshot view

### Requirement: Write Transaction Creation
The system SHALL support creating write transactions with single-writer locking.

#### Scenario: Begin write transaction
- **WHEN** begin_write() is called
- **THEN** acquire write lock (blocking other writers)
- **AND** create transaction with new epoch

#### Scenario: Single writer enforcement
- **WHEN** a write transaction is active
- **AND** another thread calls begin_write()
- **THEN** second thread blocks until first transaction completes

### Requirement: Transaction Commit
The system SHALL commit transactions durably via WAL.

#### Scenario: Commit write transaction
- **WHEN** commit() is called on a write transaction
- **THEN** append COMMIT entry to WAL
- **AND** fsync WAL to disk
- **AND** increment global epoch
- **AND** release write lock

#### Scenario: Commit failure
- **WHEN** WAL fsync fails during commit
- **THEN** transaction remains uncommitted
- **AND** changes are not visible to other transactions

### Requirement: Transaction Abort
The system SHALL support aborting transactions and rolling back changes.

#### Scenario: Abort write transaction
- **WHEN** abort() is called on a write transaction
- **THEN** append ABORT entry to WAL
- **AND** mark transaction versions as deleted
- **AND** release write lock

#### Scenario: Automatic abort on error
- **WHEN** an error occurs during transaction execution
- **THEN** automatically abort transaction
- **AND** rollback all changes

### Requirement: Epoch-Based MVCC
The system SHALL implement multi-version concurrency control using epochs.

#### Scenario: Version visibility
- **WHEN** a transaction reads a record
- **THEN** record is visible if created_epoch ≤ tx_epoch < deleted_epoch
- **AND** only one version is visible per transaction

#### Scenario: Concurrent readers
- **WHEN** multiple read transactions are active
- **THEN** each sees its own snapshot (no blocking)
- **AND** writers don't block readers

### Requirement: Garbage Collection
The system SHALL garbage collect old record versions.

#### Scenario: GC old versions
- **WHEN** garbage collector runs
- **THEN** find minimum active snapshot epoch
- **AND** remove versions deleted before min_epoch
- **AND** reclaim storage space

#### Scenario: GC frequency
- **WHEN** GC is configured with 60-second interval
- **THEN** run GC every 60 seconds
- **AND** log number of versions removed

### Requirement: WAL Integration
The system SHALL integrate with WAL for durability.

#### Scenario: Log transaction operations
- **WHEN** transaction creates a node
- **THEN** append CREATE_NODE entry to WAL
- **AND** include epoch, tx_id, and node data

#### Scenario: Recover transactions
- **WHEN** system recovers from crash
- **THEN** replay committed transactions from WAL
- **AND** abort uncommitted transactions

### Requirement: Transaction Timeout
The system SHALL enforce transaction timeouts to prevent resource leaks.

#### Scenario: Read transaction timeout
- **WHEN** a read transaction is active for >5 minutes (configurable)
- **THEN** automatically abort transaction
- **AND** unpin snapshot epoch

#### Scenario: Write transaction timeout
- **WHEN** a write transaction holds lock for >30 seconds
- **THEN** log warning (but don't abort - may be legitimate long operation)

