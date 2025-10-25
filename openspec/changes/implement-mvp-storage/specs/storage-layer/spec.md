# Storage Layer Specification (Delta)

## ADDED Requirements

### Requirement: Node Record Storage
The system SHALL store node records in a fixed-size format (32 bytes) with direct offset access.

#### Scenario: Create and read node
- **WHEN** a node is created with labels and properties
- **THEN** the node record is written to nodes.store at offset `node_id * 32`
- **AND** the node can be read back with O(1) complexity

#### Scenario: Node with multiple labels
- **WHEN** a node has multiple labels (e.g., Person, Employee)
- **THEN** labels are stored as a bitmap in label_bits field (64-bit)
- **AND** each bit position represents presence of a label ID

### Requirement: Relationship Record Storage
The system SHALL store relationship records in fixed-size format (48 bytes) with doubly-linked adjacency lists.

#### Scenario: Create relationship
- **WHEN** a relationship is created between two nodes
- **THEN** the relationship record is written to rels.store
- **AND** linked list pointers are updated (next_src_ptr, next_dst_ptr)

#### Scenario: Traverse outgoing relationships
- **WHEN** traversing outgoing relationships from a node
- **THEN** follow the linked list via next_src_ptr
- **AND** retrieve all relationships in O(degree) time

#### Scenario: Traverse incoming relationships
- **WHEN** traversing incoming relationships to a node
- **THEN** follow the linked list via next_dst_ptr
- **AND** retrieve all relationships in O(degree) time

### Requirement: Property Storage
The system SHALL store properties in variable-size records with overflow chains.

#### Scenario: Set property
- **WHEN** a property is set on a node or relationship
- **THEN** create a PropertyRecord with key_id, type, value
- **AND** link to existing property chain via next_ptr

#### Scenario: Read property
- **WHEN** reading a property by key
- **THEN** traverse property chain to find matching key_id
- **AND** return value in O(properties_per_entity) time

#### Scenario: Large string values
- **WHEN** a property value is a large string (>64 bytes)
- **THEN** store reference in strings.store
- **AND** property record contains offset pointer (8 bytes)

### Requirement: Memory-Mapped File Access
The system SHALL use memory-mapped files for efficient record access.

#### Scenario: Read node from mmap
- **WHEN** reading a node record
- **THEN** access directly via memory map (no read() syscall)
- **AND** achieve sub-microsecond access latency

#### Scenario: File growth
- **WHEN** storage file reaches capacity
- **THEN** grow file by 2x (1MB → 2MB → 4MB → ...)
- **AND** remap memory-mapped region

### Requirement: Corruption Detection
The system SHALL detect data corruption via checksums.

#### Scenario: Validate string data
- **WHEN** reading from strings.store
- **THEN** verify CRC32 checksum matches data
- **AND** return error if mismatch detected

#### Scenario: Validate page data
- **WHEN** loading a page from disk
- **THEN** verify xxHash3 checksum in page header
- **AND** return error if corruption detected

