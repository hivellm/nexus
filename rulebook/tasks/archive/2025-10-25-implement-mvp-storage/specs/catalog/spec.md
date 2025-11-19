# Catalog Specification (Delta)

## ADDED Requirements

### Requirement: Label ID Mapping
The system SHALL maintain bidirectional mappings between label names and label IDs using LMDB.

#### Scenario: Get or create label
- **WHEN** get_or_create_label("Person") is called
- **THEN** return existing label_id if label exists
- **OR** create new label_id and store bidirectional mapping

#### Scenario: Lookup label by ID
- **WHEN** looking up label_id 5
- **THEN** return the label name (e.g., "Person")
- **AND** complete lookup in O(1) time (LMDB B-tree)

#### Scenario: Concurrent label creation
- **WHEN** multiple threads call get_or_create_label("Person") simultaneously
- **THEN** only one label_id is created
- **AND** all threads receive the same label_id

### Requirement: Type ID Mapping
The system SHALL maintain bidirectional mappings between relationship type names and type IDs.

#### Scenario: Get or create type
- **WHEN** get_or_create_type("KNOWS") is called
- **THEN** return existing type_id if type exists
- **OR** create new type_id and store bidirectional mapping

#### Scenario: Type name uniqueness
- **WHEN** creating type "KNOWS" multiple times
- **THEN** always return the same type_id
- **AND** maintain single mapping in catalog

### Requirement: Property Key Mapping
The system SHALL maintain bidirectional mappings between property key names and key IDs.

#### Scenario: Get or create key
- **WHEN** get_or_create_key("age") is called
- **THEN** return existing key_id if key exists
- **OR** create new key_id and store bidirectional mapping

### Requirement: Statistics Storage
The system SHALL store database statistics in the catalog.

#### Scenario: Track node count per label
- **WHEN** a node with label "Person" is created
- **THEN** increment node_count statistic for label_id
- **AND** statistics are persisted to LMDB

#### Scenario: Track relationship count per type
- **WHEN** a relationship with type "KNOWS" is created
- **THEN** increment rel_count statistic for type_id

#### Scenario: Query statistics
- **WHEN** query executor needs cardinality estimates
- **THEN** read statistics from catalog
- **AND** use for cost-based query planning

### Requirement: Metadata Persistence
The system SHALL persist system metadata in the catalog.

#### Scenario: Store current epoch
- **WHEN** a transaction commits
- **THEN** update current epoch in metadata table
- **AND** persist to LMDB for crash recovery

#### Scenario: Store schema version
- **WHEN** database is initialized
- **THEN** store schema version (e.g., "0.1.0") in metadata
- **AND** verify version on startup (prevent incompatible format access)

### Requirement: Atomic Updates
The system SHALL ensure catalog updates are atomic via LMDB transactions.

#### Scenario: Atomic label creation
- **WHEN** creating bidirectional label mapping
- **THEN** both label_name→id and id→name are written in same LMDB transaction
- **AND** either both succeed or both rollback

#### Scenario: Crash during catalog update
- **WHEN** system crashes during catalog update
- **THEN** LMDB transaction rollback ensures consistency
- **AND** no partial mappings exist after recovery

