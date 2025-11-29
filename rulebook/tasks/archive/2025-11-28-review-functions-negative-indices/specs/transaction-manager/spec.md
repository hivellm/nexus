# Transaction Manager Specification - Rollback Fixes

## Purpose

This specification defines the requirements for fixing transaction rollback behavior to ensure proper cleanup of nodes and relationships from indexes and storage. The current implementation does not remove entities from indexes during rollback, leading to inconsistent state and index corruption.

## MODIFIED Requirements

### Requirement: Node Removal on Rollback

The system SHALL remove all nodes created within a rolled-back transaction from both indexes and storage, ensuring complete cleanup and index consistency.

#### Scenario: Rollback Node Creation

Given a transaction that creates a node with label Person
And the node is added to label index
When the transaction is rolled back
Then the node SHALL be removed from the label index
And the node SHALL be removed from storage
And subsequent queries SHALL NOT find the rolled-back node
And index consistency SHALL be maintained

#### Scenario: Rollback Node with Multiple Labels

Given a transaction that creates a node with labels Person and Employee
And the node is added to both label indexes
When the transaction is rolled back
Then the node SHALL be removed from Person label index
And the node SHALL be removed from Employee label index
And the node SHALL be removed from storage
And all indexes SHALL remain consistent

#### Scenario: Rollback Node with Properties

Given a transaction that creates a node with properties {name: "Alice", age: 30}
And the node is added to property indexes if applicable
When the transaction is rolled back
Then the node SHALL be removed from all property indexes
And the node SHALL be removed from storage
And property index consistency SHALL be maintained

### Requirement: Relationship Removal on Rollback

The system SHALL remove all relationships created within a rolled-back transaction from both indexes and storage.

#### Scenario: Rollback Relationship Creation

Given a transaction that creates a relationship of type KNOWS
And the relationship is added to relationship indexes
When the transaction is rolled back
Then the relationship SHALL be removed from relationship indexes
And the relationship SHALL be removed from storage
And subsequent queries SHALL NOT find the rolled-back relationship
And relationship index consistency SHALL be maintained

#### Scenario: Rollback Relationship with Properties

Given a transaction that creates a relationship with properties {since: 2020}
And the relationship is added to property indexes
When the transaction is rolled back
Then the relationship SHALL be removed from all property indexes
And the relationship SHALL be removed from storage
And referential integrity SHALL be maintained

### Requirement: Index Consistency After Rollback

The system SHALL maintain index consistency after rollback operations, ensuring indexes accurately reflect committed data only.

#### Scenario: Label Index Consistency After Rollback

Given a label index containing nodes
And a transaction creates and then rolls back a node with that label
When the index is queried
Then the rolled-back node SHALL NOT appear in index results
And index count SHALL match actual committed node count
And index structure SHALL remain valid

#### Scenario: Property Index Consistency After Rollback

Given a property index on property "name"
And a transaction creates and then rolls back a node with name="Test"
When the index is queried for name="Test"
Then the rolled-back node SHALL NOT appear in results
And index consistency SHALL be maintained
And no orphaned index entries SHALL exist

### Requirement: Storage Cleanup on Rollback

The system SHALL remove all entities from storage during rollback, preventing storage leaks and data inconsistency.

#### Scenario: Storage Node Removal

Given a transaction that creates a node
And the node is stored in node storage
When the transaction is rolled back
Then the node SHALL be removed from node storage
And storage space SHALL be reclaimed
And no storage leaks SHALL occur

#### Scenario: Storage Relationship Removal

Given a transaction that creates a relationship
And the relationship is stored in relationship storage
When the transaction is rolled back
Then the relationship SHALL be removed from relationship storage
And storage space SHALL be reclaimed
And adjacency lists SHALL be updated correctly

## ADDED Requirements

### Requirement: Rollback Transaction Tracking

The system SHALL track all entities created within a transaction to enable complete rollback cleanup.

#### Scenario: Track Created Nodes

Given a transaction that creates multiple nodes
When nodes are created
Then each node SHALL be added to transaction creation list
And rollback SHALL iterate through creation list
And all tracked nodes SHALL be removed during rollback

#### Scenario: Track Created Relationships

Given a transaction that creates multiple relationships
When relationships are created
Then each relationship SHALL be added to transaction creation list
And rollback SHALL iterate through creation list
And all tracked relationships SHALL be removed during rollback

### Requirement: Comprehensive Rollback Testing

The system SHALL have comprehensive test coverage for all rollback scenarios.

#### Scenario: Test Rollback with Multiple Nodes

Given a transaction creating 10 nodes with various labels
When the transaction is rolled back
Then all 10 nodes SHALL be removed from indexes
And all 10 nodes SHALL be removed from storage
And all tests SHALL pass

#### Scenario: Test Rollback with Relationships

Given a transaction creating nodes and relationships
When the transaction is rolled back
Then all relationships SHALL be removed from indexes
And all relationships SHALL be removed from storage
And referential integrity SHALL be maintained
And all tests SHALL pass

#### Scenario: Enable Previously Ignored Rollback Tests

Given rollback tests marked with #[ignore]
When rollback functionality is fixed
Then #[ignore] attributes SHALL be removed
And all rollback tests SHALL pass
And test coverage SHALL be at least 95%

## Implementation Notes

### Rollback Cleanup Order

1. Remove from property indexes (if applicable)
2. Remove from relationship indexes (if applicable)
3. Remove from label indexes
4. Remove from storage
5. Update adjacency lists

### Transaction State Tracking

Maintain per-transaction lists:
- `created_nodes: Vec<NodeId>`
- `created_relationships: Vec<RelationshipId>`
- `modified_nodes: Vec<NodeId>`
- `modified_relationships: Vec<RelationshipId>`

### Index Removal Operations

For each index type:
- Label index: Remove node ID from bitmap
- Property index: Remove node/relationship ID from index structure
- Relationship index: Remove relationship ID from index

### Error Handling

If rollback cleanup fails:
- Log error with transaction ID
- Continue cleanup for remaining entities
- Mark transaction as failed
- Report error to user

