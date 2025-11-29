## MODIFIED Requirements

### Requirement: Node Creation with Index Updates
The system SHALL update the label_index automatically when creating nodes to ensure MATCH queries can find them by label.

#### Scenario: CREATE followed by MATCH
- **WHEN** a node is created with `CREATE (p:Person {name: "Alice"})`
- **AND** a MATCH query is executed with `MATCH (p:Person) RETURN p`
- **THEN** the query returns the created node

### Requirement: Shared Storage Access
The Executor SHALL use the same storage instance as the Engine for all query operations.

#### Scenario: Engine-Executor synchronization
- **WHEN** a node is created via Engine's create_node method
- **AND** a MATCH query is executed via Executor
- **THEN** the Executor returns the node created by the Engine

### Requirement: REST API Node Creation
The /data/nodes endpoint SHALL use the shared Engine instance for persistent node creation.

#### Scenario: POST /data/nodes creates persistent nodes
- **WHEN** a POST request is made to /data/nodes with labels and properties
- **AND** a GET request is made to /stats
- **THEN** the node_count reflects the newly created node

