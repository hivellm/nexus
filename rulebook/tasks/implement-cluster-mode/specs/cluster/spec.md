# Cluster Mode Specification

## ADDED Requirements

### Requirement: HiveHub API Integration
The system SHALL integrate with HiveHub API using the official HiveHub Internal SDK (Rust) to fetch user quotas, validate user status, and report usage metrics. The SDK SHALL provide built-in caching to minimize API calls and graceful degradation when the API is unavailable.

#### Scenario: Fetching User Quota
Given the system needs quota information for a user
When a query or operation is executed for that user
Then the system MUST use HiveHub SDK's `nexus().get_user_database()` method to fetch quota data
And the SDK SHALL handle API communication and caching internally
And the quota data SHALL be cached for a configurable TTL period by the SDK
And subsequent requests within TTL SHALL use cached data from the SDK

#### Scenario: HiveHub API Unavailable
Given the HiveHub API is temporarily unavailable
When the system attempts to fetch quota or user data
Then the system SHALL use cached quota data if available
And the system SHALL allow operations to proceed with cached data
And the system SHALL log API failures for monitoring

#### Scenario: User Metadata Validation
Given the system receives a request with an API key
When the API key is validated
Then the system MUST use HiveHub SDK's `get_user_info()` method to fetch user metadata
And the SDK SHALL handle API communication with proper authentication
And the system SHALL validate that the user is active and allowed using SDK response
And the system SHALL reject requests for inactive or suspended users

### Requirement: Data Segmentation by User Namespace
The system SHALL segment all data by user namespace, ensuring complete isolation between users where each user can only access data within their own namespace, and all queries MUST be automatically scoped to the requesting user's namespace.

#### Scenario: Namespace Isolation for Nodes
Given two users have nodes with the same labels and properties
When user A queries for nodes
Then user A SHALL only see nodes in their namespace
And user A SHALL NOT see any nodes from user B's namespace
And the system SHALL prevent cross-namespace access

#### Scenario: Namespace Isolation for Relationships
Given two users have relationships between nodes
When user A queries for relationships
Then user A SHALL only see relationships in their namespace
And user A SHALL NOT traverse relationships to nodes in other namespaces
And the system SHALL enforce namespace boundaries in path queries

#### Scenario: Creating Data in Namespace
Given a user creates a node or relationship
When the CREATE operation is executed
Then the system MUST automatically assign the user's namespace
And the data SHALL be stored with namespace prefix
And the data SHALL be isolated from other users' data

#### Scenario: Query Scoping to Namespace
Given a user executes a Cypher query
When the query is planned and executed
Then the system MUST automatically inject namespace filters
And the query SHALL only return data from the user's namespace
And the query SHALL fail if attempting to access other namespaces

### Requirement: Enhanced API Key System with Function Permissions
The system SHALL support API keys with function-level permissions where each API key can have a list of allowed MCP functions, and the system MUST filter available functions based on the API key's permissions, preventing access to administrative functions unless explicitly granted.

#### Scenario: API Key with Limited Functions
Given an API key is created with specific function permissions
When a client uses that API key to access MCP
Then the system SHALL only expose allowed functions
And the system SHALL hide functions not in the permission list
And the system SHALL reject requests to unauthorized functions

#### Scenario: Administrative Function Protection
Given an API key without administrative permissions
When the client attempts to call an administrative MCP function
Then the system SHALL reject the request with permission denied error
And the system SHALL NOT execute the administrative function
And the system SHALL log the unauthorized access attempt

#### Scenario: Full Access API Key
Given an API key with full permissions
When the client uses that API key
Then the system SHALL expose all available MCP functions
And the system SHALL allow access to administrative functions
And the system SHALL track all operations for audit

#### Scenario: Function Permission Validation
Given a request includes an API key
When the request targets a specific MCP function
Then the system MUST validate the API key has permission for that function
And the system SHALL allow execution if permission exists
And the system SHALL reject execution if permission is missing

### Requirement: Mandatory Authentication in Cluster Mode
The system SHALL require authentication for all endpoints when cluster mode is enabled, including health checks, metrics, and all REST and MCP endpoints, and the system MUST reject unauthenticated requests with appropriate error responses.

#### Scenario: Authenticated Request in Cluster Mode
Given cluster mode is enabled
When a client sends a request with valid API key
Then the system SHALL authenticate the API key
And the system SHALL proceed with the request if authentication succeeds
And the system SHALL associate the request with the user's namespace

#### Scenario: Unauthenticated Request in Cluster Mode
Given cluster mode is enabled
When a client sends a request without authentication
Then the system SHALL reject the request with 401 Unauthorized
And the system SHALL NOT process the request
And the system SHALL NOT expose any data or information

#### Scenario: Health Check Authentication
Given cluster mode is enabled
When a client accesses the health check endpoint
Then the system SHALL require valid API key authentication
And the system SHALL NOT allow public access to health endpoints
And the system SHALL return health status only for authenticated requests

#### Scenario: MCP Endpoint Authentication
Given cluster mode is enabled
When a client accesses any MCP endpoint
Then the system SHALL require valid API key authentication
And the system SHALL validate API key before processing MCP request
And the system SHALL reject unauthenticated MCP requests

### Requirement: Storage Quota Enforcement
The system SHALL track storage usage per user namespace and enforce storage quotas by rejecting write operations when the user's quota is exceeded, while accurately tracking storage size including nodes, relationships, properties, and indexes.

#### Scenario: Storage Quota Check Before Write
Given a user attempts to create a node or relationship
When the CREATE operation is executed
Then the system MUST check current storage usage against quota
And the system SHALL allow the operation if quota is not exceeded
And the system SHALL reject the operation with quota exceeded error if limit is reached

#### Scenario: Storage Quota Tracking
Given data is stored in a user's namespace
When storage operations occur
Then the system SHALL track total storage size for that namespace
And the system SHALL include nodes, relationships, and properties in calculation
And the system SHALL update storage metrics in real-time

#### Scenario: Quota Exceeded Error
Given a user has reached their storage quota limit
When the user attempts a write operation
Then the system SHALL reject the operation immediately
And the system SHALL return a clear quota exceeded error message
And the system SHALL include current usage and limit in error response

#### Scenario: Quota Synchronization
Given storage usage is tracked locally
When periodic synchronization occurs
Then the system SHALL use HiveHub SDK's `nexus().update_usage()` method to report usage metrics
And the system SHALL use SDK's `nexus().get_user_database()` to update quota limits from HiveHub if changed
And the SDK SHALL handle synchronization failures gracefully with retry logic

### Requirement: Rate Limiting Based on Quotas
The system SHALL enforce rate limits per user based on their quota configuration, tracking requests per time period and rejecting requests that exceed the rate limit with appropriate HTTP 429 responses.

#### Scenario: Rate Limit Enforcement
Given a user has a configured rate limit in their quota
When the user makes requests
Then the system SHALL track request count per time window
And the system SHALL allow requests within the rate limit
And the system SHALL reject requests exceeding the limit with 429 status

#### Scenario: Rate Limit Headers
Given a user makes a request
When the request is processed
Then the system SHALL include X-RateLimit-Limit header
And the system SHALL include X-RateLimit-Remaining header
And the system SHALL include X-RateLimit-Reset header

#### Scenario: Rate Limit Quota Exceeded
Given a user exceeds their rate limit
When the user makes another request
Then the system SHALL return 429 Too Many Requests status
And the system SHALL include Retry-After header with wait time
And the system SHALL NOT process the request

### Requirement: User Context Propagation
The system SHALL extract user identification from API keys and propagate user context through all request processing, ensuring that all operations are associated with the correct user and namespace.

#### Scenario: User Context Extraction
Given a request includes a valid API key
When the request is processed
Then the system MUST extract user_id from the API key
And the system SHALL create user context for the request
And the system SHALL associate all operations with that user

#### Scenario: User Context in Query Execution
Given a query is executed with user context
When the query is planned and executed
Then the system SHALL use user context to determine namespace
And the system SHALL scope all operations to that namespace
And the system SHALL prevent access to other users' data

#### Scenario: User Context in Error Responses
Given an error occurs during request processing
When the error is returned to the client
Then the system SHALL NOT expose user_id or namespace in error messages
And the system SHALL log user context internally for audit
And the system SHALL provide generic error messages to clients

### Requirement: Cluster Mode Configuration
The system SHALL support configuration to enable or disable cluster mode, and when cluster mode is enabled, the system MUST require HiveHub SDK configuration including base URL and service API key for SDK initialization and server-to-server authentication.

#### Scenario: Cluster Mode Enabled
Given cluster mode is enabled in configuration
When the system starts
Then the system SHALL require HiveHub SDK configuration (base_url and service_api_key)
And the system SHALL initialize HiveHub SDK client with provided configuration
And the system SHALL validate HiveHub SDK connectivity on startup
And the system SHALL enforce mandatory authentication for all endpoints

#### Scenario: Cluster Mode Disabled
Given cluster mode is disabled in configuration
When the system starts
Then the system SHALL operate in standalone mode
And the system SHALL allow optional authentication as configured
And the system SHALL NOT require HiveHub API configuration

#### Scenario: Configuration Validation
Given cluster mode configuration is provided
When the system validates configuration
Then the system SHALL require HiveHub base_url if cluster mode enabled
And the system SHALL require HiveHub service API key if cluster mode enabled
And the system SHALL validate that the `hivehub-cloud-internal-sdk` crate is available
And the system SHALL fail startup if required configuration or SDK dependency is missing

### Requirement: Data Isolation Verification
The system SHALL provide mechanisms to verify complete data isolation between users, ensuring that no data leakage can occur between namespaces under any circumstances, including edge cases and malicious queries.

#### Scenario: Cross-Namespace Query Prevention
Given a user attempts to construct a query targeting another namespace
When the query is executed
Then the system SHALL detect namespace boundary violations
And the system SHALL reject the query with access denied error
And the system SHALL log the violation attempt

#### Scenario: Namespace Boundary Enforcement
Given data exists in multiple user namespaces
When any operation is performed
Then the system SHALL enforce strict namespace boundaries
And the system SHALL prevent any form of cross-namespace data access
And the system SHALL verify isolation in all storage layers

#### Scenario: Isolation Testing
Given the system is in cluster mode with multiple users
When isolation tests are performed
Then the system SHALL pass all isolation verification tests
And the system SHALL demonstrate zero data leakage between users
And the system SHALL maintain isolation under load

## MODIFIED Requirements

### Requirement: API Key Structure
The system SHALL extend API key structure to include user_id and allowed_functions fields, maintaining backward compatibility with existing API keys in standalone mode.

#### Scenario: Enhanced API Key Creation
Given an API key is created in cluster mode
When the API key is created
Then the system SHALL include user_id in the API key structure
And the system SHALL include allowed_functions list in the API key
And the system SHALL store enhanced API key in persistent storage

#### Scenario: Backward Compatibility
Given an existing API key without user_id or function permissions
When the API key is used in standalone mode
Then the system SHALL accept the API key as valid
And the system SHALL grant full permissions if not specified
And the system SHALL maintain compatibility with existing keys

### Requirement: Authentication Middleware
The system SHALL modify authentication middleware to require authentication in cluster mode for all endpoints, while maintaining optional authentication in standalone mode.

#### Scenario: Cluster Mode Authentication
Given cluster mode is enabled
When authentication middleware processes a request
Then the system SHALL require valid API key for all endpoints
And the system SHALL reject unauthenticated requests
And the system SHALL extract user context from API key

#### Scenario: Standalone Mode Authentication
Given cluster mode is disabled
When authentication middleware processes a request
Then the system SHALL use configured authentication settings
And the system SHALL allow public endpoints if configured
And the system SHALL maintain existing authentication behavior

