# HiveHub.Cloud Integration Specification (Nexus)

## ADDED Requirements

### Requirement: Hub Authentication
The system SHALL authenticate all user requests through HiveHub.Cloud access keys.

#### Scenario: Valid Hub access key
Given a request includes valid Hub-issued access key
When the request is processed
Then the system SHALL validate key with Hub and extract user_id

#### Scenario: Invalid access key
Given a request includes invalid access key
When the request is processed
Then the system SHALL return 401 Unauthorized

### Requirement: Database-Per-User Isolation
The system SHALL create exclusive database per user with complete isolation.

#### Scenario: First user access
Given a user accesses Nexus for the first time
When the request is processed
Then the system SHALL create database user_{user_id}_nexus

#### Scenario: Route to user database
Given a user makes Cypher query
When the query is processed
Then the system SHALL route to user's exclusive database

#### Scenario: Prevent cross-database access
Given a user attempts to access another user's database
When the request is processed
Then the system SHALL deny with 403 Forbidden

### Requirement: Credit Management
The system SHALL manage credits for LLM operations via Hub API.

#### Scenario: LLM classification with credits
Given a user requests LLM classification
When the operation is initiated
Then the system SHALL check credits with Hub and consume credits

#### Scenario: Insufficient credits
Given a user has insufficient credits
When they request credit-consuming operation
Then the system SHALL return 402 Payment Required

### Requirement: Quota Enforcement
The system SHALL enforce quotas for nodes/relationships via Hub API.

#### Scenario: Create node within quota
Given a user is within node limit
When they create a node
Then the system SHALL validate with Hub and create node

#### Scenario: Exceed quota
Given a user has reached node limit
When they attempt to create node
Then the system SHALL return 429 Too Many Requests

### Requirement: Usage Reporting
The system SHALL report usage metrics to Hub for billing.

#### Scenario: Report node operations
Given nodes are created/deleted
When the operation completes
Then the system SHALL report node count and storage to Hub

#### Scenario: Report credit usage
Given credit-consuming operation completes
When the operation finishes
Then the system SHALL report credits consumed to Hub

#### Scenario: Periodic sync
Given the system is running
When usage interval elapses
Then the system SHALL sync all usage metrics to Hub

### Requirement: MCP Integration
The system SHALL integrate with Hub's MCP gateway for Cypher queries.

#### Scenario: MCP Cypher query
Given an MCP request includes user key
When Cypher query is executed
Then the system SHALL route to user's database only

### Requirement: Cluster Mode
The system SHALL support distributed operation with user database routing.

#### Scenario: Cross-node request
Given a request routes to different node
When processed with user context
Then the system SHALL route to correct database shard

