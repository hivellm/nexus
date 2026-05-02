# Storage spec: external node identity

## ADDED Requirements

### Requirement: External-id catalog index
The system SHALL maintain an `external_ids` LMDB sub-database mapping caller-supplied external identifiers to internal `u64` node ids, and a reverse `internal_ids` sub-database mapping internal ids back to external identifiers.

#### Scenario: Insert with new external id
Given an empty `external_ids` index
When a node is created with `external_id = sha256:abc…`
Then the node's internal id is allocated through `allocate_node_id`
And the entry `(sha256:abc…, internal_id)` is committed to `external_ids`
And the entry `(internal_id, sha256:abc…)` is committed to `internal_ids`

#### Scenario: Reopen reloads both maps
Given a database with N nodes carrying external ids
When the storage engine is closed and reopened
Then `external_ids` and `internal_ids` are both fully accessible
And every forward entry has a matching reverse entry

### Requirement: External id encoding
The system SHALL encode `ExternalId` as a 1-byte discriminator followed by a variant payload.

#### Scenario: Hash variant
Given an external id `sha256:<32 raw bytes>`
When the value is encoded for the catalog
Then the encoded bytes start with discriminator `0x01` (Hash) followed by `0x02` (Sha256) followed by the 32 raw bytes

#### Scenario: String variant length cap
Given a caller submits an external id of variant `String` with length 257 bytes
When the value is validated
Then the operation MUST fail with `ExternalIdTooLong { kind: String, max: 256, actual: 257 }`

### Requirement: ConflictPolicy on create
The system SHALL accept a `ConflictPolicy` (`Error`, `Match`, `Replace`) on every create-with-external-id call and apply it deterministically.

#### Scenario: Error policy on duplicate
Given a node with `external_id = uuid:…` already exists
When a second create is attempted with the same external id and `policy = Error`
Then the operation MUST fail with `ExternalIdConflict { existing_internal_id, attempted_external_id }`
And no new record is written

#### Scenario: Match policy on duplicate
Given a node with `external_id = uuid:…` already exists with internal id 42
When a second create is attempted with the same external id and `policy = Match`
Then the operation MUST return internal id 42
And no new record is written
And property writes from the second call are discarded

#### Scenario: Replace policy on duplicate
Given a node with `external_id = uuid:…` already exists with internal id 42 and properties `{name: "old"}`
When a second create is attempted with the same external id, `properties = {name: "new"}`, and `policy = Replace`
Then the operation MUST return internal id 42
And the property store reflects `{name: "new"}`
And label bits are unchanged unless explicitly provided

### Requirement: Delete cleans up both maps
The system SHALL remove forward and reverse external-id entries atomically when the underlying node is deleted.

#### Scenario: Delete removes both directions
Given a node with internal id 42 and `external_id = blake3:…`
When the node is deleted
Then `external_ids[blake3:…]` is absent
And `internal_ids[42]` is absent
And the operation is atomic with respect to the WAL
