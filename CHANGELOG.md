# Changelog

All notable changes to Nexus will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added - Graph Algorithms (Phase 13)
- **Pathfinding Algorithms**:
  - Bellman-Ford algorithm for shortest paths with negative cycle detection
  - Dijkstra and A* already implemented
- **Centrality Algorithms**:
  - PageRank algorithm with configurable damping factor and convergence tolerance
  - Betweenness Centrality calculation
  - Closeness Centrality calculation
  - Degree Centrality calculation
- **Community Detection**:
  - Louvain algorithm for modularity-based community detection
  - Label Propagation algorithm
  - Strongly Connected Components using Kosaraju's algorithm
  - Weakly Connected Components (already implemented)
- **Similarity Algorithms**:
  - Jaccard Similarity between nodes
  - Cosine Similarity (graph-based) between nodes
  - Node Similarity calculation for all pairs
- All algorithms include comprehensive unit tests (15 tests passing)
- **Procedure Wrappers**: Complete procedure registry structure created for all algorithms
  - Pathfinding: `gds.shortestPath.dijkstra`, `gds.shortestPath.astar`, `gds.shortestPath.bellmanFord`
  - Centrality: `gds.centrality.pagerank`, `gds.centrality.betweenness`, `gds.centrality.closeness`, `gds.centrality.degree`
  - Community: `gds.community.louvain`, `gds.community.labelPropagation`, `gds.community.stronglyConnectedComponents`, `gds.community.weaklyConnectedComponents`
  - Similarity: `gds.similarity.jaccard`, `gds.similarity.cosine`
  - Procedures ready for integration once CALL procedure support is fully implemented
- **Engine Integration**: Added `Graph::from_engine()` helper function to convert Engine storage data to algorithm Graph structure, enabling direct use of algorithms on database data

### Added - Performance Monitoring (Phase 11)
- **Query Statistics**: Comprehensive query execution tracking
  - Query execution time tracking with min/max/average
  - Query pattern statistics (normalized queries)
  - Success/failure rate tracking
  - Total queries and execution time aggregation
- **Slow Query Logging**: Configurable slow query detection and logging
  - Configurable threshold (default: 100ms)
  - Detailed query records with timestamps, errors, and row counts
  - Circular buffer with configurable max entries
- **Query Plan Cache**: LRU-based query plan caching
  - Plan caching with AST and operators
  - LRU eviction policy
  - Memory-based eviction
  - Cache invalidation on schema changes
  - Cache statistics and management
- **DBMS Procedures**: System management procedures
  - `dbms.showCurrentUser()` - Show current authenticated user
  - `dbms.listConfig()` - List database configuration
  - `dbms.listConnections()` - List active connections (structure ready)
  - `dbms.killQuery()` - Kill running queries (structure ready)
  - `dbms.clearQueryCaches()` - Clear query plan cache
- All features include comprehensive unit tests (8 tests passing)
- **API Endpoints**: REST endpoints for performance monitoring
  - `GET /performance/statistics` - Get query execution statistics
  - `GET /performance/slow-queries` - Get slow query log
  - `GET /performance/plan-cache` - Get plan cache statistics
  - `POST /performance/plan-cache/clear` - Clear plan cache
- **Automatic Query Tracking**: Integrated query statistics tracking in Cypher endpoint
  - All queries executed via `/cypher` endpoint are automatically tracked
  - Success/failure, execution time, and row counts are recorded
  - Slow queries are automatically logged based on configurable threshold
- **Test Coverage**: Comprehensive test suite added (34 tests total)
  - Query statistics: 10 tests covering edge cases, normalization, min/max tracking, pattern statistics, clear operations, eviction
  - Plan cache: 9 tests covering LRU eviction, pattern invalidation, statistics, access counting, time-based eviction
  - DBMS procedures: 8 tests covering all procedures, user management, anonymous users, config listing
  - API endpoints: 8 tests covering all endpoints with data validation and error handling

## [0.11.0] - 2025-11-12

### Added
- **Data Import/Export - Phase 9**: Complete implementation of bulk import and export functionality
  - **Bulk Import API**: Enhanced `/ingest` endpoint with transaction batching and progress reporting
    - Transaction batching: Process data in batches with automatic transaction management
    - Configurable batch size (default: 1000 items per batch)
    - Progress reporting: `batches_processed` and `progress_percent` in response
    - Support for `use_batching` flag to enable/disable batching
    - JSON batch format support for nodes and relationships with properties
    - Automatic transaction commit/rollback on errors
  - **Export API**: New `/export` endpoint for data export
    - JSON export format: Structured JSON output with query results
    - CSV export format: Comma-separated values with proper escaping
    - Streaming export support for large datasets
    - Customizable Cypher queries for selective data export
    - Proper HTTP headers: Content-Type and Content-Disposition
  - **LOAD CSV**: Complete CSV import functionality (already implemented)
    - `LOAD CSV FROM 'file:///path/to/file.csv' AS row` syntax
    - `WITH HEADERS` support for CSV files with header rows
    - `FIELDTERMINATOR` support for custom field separators
    - Integration with FOREACH and UNWIND clauses for processing CSV rows
- **V1 Authentication & Security - Phase 1 & Phase 2**: Complete implementation of authentication and security features
  - **Phase 1**: Root user and authentication foundation
  - **Root User Configuration**: Configurable root user with environment variables and TOML file support
    - Support for `NEXUS_ROOT_USERNAME`, `NEXUS_ROOT_PASSWORD`, `NEXUS_ROOT_ENABLED` environment variables
    - Configuration file support: `config/auth.toml` with TOML parsing
    - Default root credentials: `root/root` (configurable)
    - `NEXUS_DISABLE_ROOT_AFTER_SETUP` flag for automatic root user disabling
    - Docker secrets support via `NEXUS_ROOT_PASSWORD_FILE`
  - **Root User Management**: Secure root user handling
    - Root user creation on startup
    - Root user disable functionality
    - Prevention of root user deletion (only disable allowed)
    - Root user validation (cannot be modified by non-root users)
    - Auto-disable root user after first admin user creation
  - **User CRUD Operations**: Complete user management via Cypher and REST API
    - `CREATE USER username [SET PASSWORD 'password']` Cypher command with password hashing
    - `DROP USER username` Cypher command
    - `SHOW USERS` and `SHOW USER username` Cypher commands
    - REST endpoints: `POST /auth/users`, `DELETE /auth/users/{username}`, `GET /auth/users`, `GET /auth/users/{username}`
  - **Permission Management**: Fine-grained permission system
    - `GRANT permission TO username` and `REVOKE permission FROM username` Cypher commands
    - Fine-grained permissions: QUEUE, CHATROOM, REST
    - REST endpoints: `POST /auth/users/{username}/permissions`, `DELETE /auth/users/{username}/permissions/{permission}`, `GET /auth/users/{username}/permissions`
  - **API Key Management**: Complete API key lifecycle with LMDB persistence
    - API key generation (32-char random, prefixed with `nx_`)
    - Argon2 hashing for API keys
    - LMDB persistence for API keys (replacing in-memory storage)
    - Key metadata: name, permissions, expiry, created_at, user_id, revocation
    - `CREATE API KEY`, `REVOKE API KEY`, `SHOW API KEYS`, `DELETE API KEY` Cypher commands
    - REST endpoints: `POST /auth/keys`, `GET /auth/keys`, `GET /auth/keys/{key_id}`, `DELETE /auth/keys/{key_id}`, `POST /auth/keys/{key_id}/revoke`
    - Expiration date support with automatic expiration check
    - `EXPIRES IN 'duration'` parsing (e.g., "7d", "24h", "30m")
    - Cleanup job for expired keys (runs hourly)
    - Key revocation with reason/comment support
  - **REST Endpoint Protection**: Authentication middleware for all REST routes
    - Bearer token extraction (`Authorization: Bearer nx_...`)
    - API key header extraction (`X-API-Key: nx_...`)
    - 401 Unauthorized responses for missing/invalid credentials
    - 403 Forbidden responses for insufficient permissions
    - Protection for all `/cypher`, `/data/*`, `/schema/*`, `/knn_traverse`, `/ingest`, `/clustering/*`, `/stats` endpoints
    - Optional authentication for `/health` endpoint (configurable)
  - **Phase 2**: API Key Management & REST Protection
    - **Rate Limiting Integration**: Complete rate limiting system with API key authentication
      - Rate limiter integrated with API key authentication middleware
      - 429 Too Many Requests responses when rate limit is exceeded
      - Rate limit headers: `X-RateLimit-Limit`, `X-RateLimit-Remaining`, `X-RateLimit-Reset`
      - Per-API-key rate limiting with configurable windows
      - Automatic cleanup of expired rate limit entries
      - Comprehensive unit tests for rate limiting functionality
  - **Phase 3**: Audit Logging
    - **Audit Log Structure**: Complete audit logging system with JSON format
      - Structured JSON log entries with timestamp, operation, user context, and result
      - Support for multiple operation types: user management, permissions, API keys, authentication, write operations
      - Metadata field for additional context
      - IP address tracking for security events
    - **Security Event Logging**: Comprehensive logging of security-sensitive operations
      - User creation and deletion events
      - Permission grants and revocations
      - API key creation, revocation, and deletion
      - Authentication failures and successes
      - All write operations (CREATE, SET, DELETE) in Cypher queries
    - **Log Management**: Advanced log file management
      - Daily log rotation (creates new log file each day)
      - Automatic log compression (gzip) for old log files
      - Configurable retention period (default: 30 days, 0 = keep forever)
      - Asynchronous file I/O to prevent blocking the main runtime
      - Configurable log directory and enable/disable flag
    - **Integration**: Complete integration with authentication system
      - Audit logging integrated into authentication middleware
      - Audit logging integrated into all REST API endpoints
      - Audit logging integrated into Cypher query executor
      - Actor information extraction from authentication context

### Testing
- **Comprehensive Unit Tests**: Complete test coverage for Phase 1 features
  - Unit tests for user CRUD operations (create, read, update, delete, list)
  - Unit tests for permission management (grant, revoke, check permissions)
  - Unit tests for API key revocation logic
  - Unit tests for root user validation and management
  - Unit tests for configuration file loading
  - All tests passing with high coverage

### Improved
- **Code Quality**: All code passes clippy with `-D warnings` (no warnings allowed)
- **Storage**: LMDB persistence for API keys provides durable storage
- **Security**: Root user auto-disable enhances security after initial setup
- **Configuration**: Flexible configuration via environment variables and TOML files

## [0.10.4] - 2025-11-12

### Fixed
- **Code Quality**: Fixed all clippy warnings
  - Replaced `args.len() >= 1` with `!args.is_empty()` for better readability
  - Fixed `contains_key` + `insert` pattern to use `Entry` API
  - Fixed `as_ref().map()` pattern to use direct `clone()`
  - Added `#[allow(clippy::too_many_arguments)]` for functions that legitimately need many parameters
  - All code now passes `cargo clippy --workspace -- -D warnings`

### Improved
- **Test Coverage**: Comprehensive test suite verification
  - 974 tests passing (100% success rate)
    - nexus-core: 761 tests passing
    - nexus-protocol: 21 tests passing
    - nexus-server: 192 tests passing
  - All critical functionality verified and working

### Testing
- **Full Test Suite**: Complete verification of all implemented features
  - Variable-length paths: All quantifier types tested
  - Shortest path functions: BFS and DFS algorithms verified
  - Database management: All CRUD operations tested
  - User management: RBAC operations validated
  - Advanced Cypher features: CASE, FOREACH, EXISTS, comprehensions all working

## [0.10.3] - 2025-11-12

### Added
- **Variable-Length Paths**: Complete implementation of variable-length path quantifiers
  - **Fixed-length paths**: `-[*5]->` - Match exactly 5 relationships
  - **Range paths**: `-[*1..3]->` - Match 1 to 3 relationships
  - **Unbounded paths**: `-[*]->` - Match zero or more relationships
  - **One or more**: `-[+]->` - Match one or more relationships
  - **Zero or one**: `-[?]->` - Match zero or one relationship
  - **BFS traversal**: Efficient breadth-first search for path finding
  - **Cycle detection**: Prevents revisiting nodes in current path
  - **Path length constraints**: Enforces min/max length limits
  - **Bidirectional paths**: Supports undirected relationship patterns

- **Shortest Path Functions**: Implementation of shortest path algorithms
  - **shortestPath()**: Find shortest path between two nodes using BFS
    - Syntax: `shortestPath([(a)-[*]->(b)])` using PatternComprehension
    - Returns single shortest path as JSON (nodes and relationships arrays)
    - Supports relationship type filtering and direction constraints
  - **allShortestPaths()**: Find all paths of shortest length
    - Uses BFS to find shortest distance, then DFS to find all paths of that length
    - Returns array of paths, each with nodes and relationships
    - Efficiently handles multiple paths of same length
  - **Path struct**: Custom representation of paths with nodes and relationships
  - **Path serialization**: Converts paths to JSON format for API responses

### Improved
- **Parser Enhancement**: Enhanced relationship pattern parsing
  - Correctly distinguishes between property maps `{key: value}` and quantifiers `{n}` or `{n..m}`
  - Improved `peek_char_at` logic for accurate pattern detection
  - Supports range quantifiers with `..` separator
- **Query Planner**: Updated to generate VariableLengthPath operator
  - Automatically detects quantifiers in relationship patterns
  - Generates appropriate operator (Expand vs VariableLengthPath) based on pattern
  - Cost estimation for variable-length path operations

### Testing
- **Comprehensive Test Suite**: Added tests for variable-length paths and shortest path functions
  - Unit tests for parser quantifier parsing (all types covered)
  - Unit tests for planner VariableLengthPath generation
  - S2S tests for variable-length paths (fixed-length, range, unbounded, bidirectional)
  - S2S tests for shortestPath() and allShortestPaths() functions
  - All tests passing (100% success rate)

### Technical Details
- **BFS Algorithm**: Breadth-first search for efficient shortest path finding
- **DFS Algorithm**: Depth-first search for finding all paths of given length
- **Cycle Detection**: Prevents infinite loops by tracking visited nodes in current path
- **Path Representation**: Custom `Path` struct with nodes and relationships vectors
- **JSON Serialization**: Paths converted to JSON with `nodes` and `relationships` arrays

## [0.10.2] - 2025-11-11

### Added
- **Advanced Cypher Features**: Complete implementation of advanced Cypher query language features
  - **CASE Expressions**: Conditional logic in queries
    - Simple CASE: `CASE WHEN condition THEN result ELSE default END`
    - Generic CASE: `CASE input WHEN value THEN result ELSE default END`
    - Nested CASE expressions supported
    - Works in WHERE, ORDER BY, and RETURN clauses
    - Full type support (strings, numbers, booleans, NULL)
  - **FOREACH Clause**: Iterative operations on lists
    - `FOREACH (item IN list | SET n.property = item)`
    - Supports SET operations within iteration
    - Supports DELETE and DETACH DELETE within iteration
    - Works with variable lists and literal lists
  - **EXISTS Subqueries**: Pattern existence checking
    - `EXISTS { (n)-[:REL]->(m) }` - Checks if pattern exists
    - `EXISTS { (n)-[:REL]->(m) WHERE n.age > 30 }` - Pattern with WHERE clause
    - Works in WHERE clauses for filtering
    - Supports multi-hop patterns and relationship filtering
  - **Map Projections**: Dynamic map construction
    - `n {.name, .age}` - Property selection
    - `n {.name AS fullName, computed: n.age * 2}` - Virtual keys and aliases
    - Nested map projections supported
    - Computed properties with expressions
  - **List Comprehensions**: Functional list transformations
    - `[x IN list WHERE x > 10 | x * 2]` - Filter and transform
    - `[x IN list | x.property]` - Simple transformation
    - `[x IN list WHERE condition]` - Filter only
    - Nested comprehensions supported
  - **Pattern Comprehensions**: Pattern-based collection
    - `[(n)-[:REL]->(m) WHERE n.age > 30 | m.name]` - Collect with filtering
    - `[(n)-[:REL]->(m) | m {.name, .age}]` - Map projection transformation
    - Multi-hop patterns supported
    - Relationship property filtering

### Fixed
- **EXISTS WHERE Clause Parsing**: Fixed parsing of EXISTS patterns with WHERE clauses
  - Parser now correctly delimits pattern boundaries
  - Added `parse_pattern_until_where_or_brace()` helper function
  - Resolved syntax errors in complex EXISTS queries
- **Pattern Comprehension Transformations**: Fixed evaluation of complex transformations
  - Map projections now work correctly in transform expressions
  - Fixed context handling for pattern variables
  - Improved evaluation of nested expressions

### Improved
- **Parser Architecture**: Enhanced pattern parsing for advanced features
  - Better boundary detection for EXISTS and Pattern Comprehensions
  - Improved WHERE clause integration in pattern expressions
  - More robust error messages for parsing failures
- **Expression Evaluation**: Enhanced executor for advanced expressions
  - Better context management for comprehensions
  - Improved variable scoping in FOREACH iterations
  - Enhanced pattern matching in EXISTS subqueries

### Testing
- **Comprehensive Test Suite**: 75 integration tests for advanced features
  - 28 complex scenario tests covering edge cases
  - 36 extended tests with real-world patterns
  - 11 business scenario tests
  - 100% test pass rate
  - All features validated end-to-end via HTTP API
- **Test Coverage**: Maintained 95%+ coverage requirement
  - All advanced feature code paths covered
  - Edge cases validated (NULL values, empty lists, nested expressions)
  - Real-world business scenarios tested

## [0.10.1] - 2025-11-11

### Added
- **Cypher Write Operations**: Complete implementation of write operations for data modification
  - **MERGE Clause**: Full match-or-create semantics with property matching
    - `MERGE (n:Person {email: 'alice@example.com'})` - Creates node if not exists, matches if exists
    - `ON CREATE SET` - Sets properties/labels only when creating new node
    - `ON MATCH SET` - Sets properties/labels only when matching existing node
    - Variable context tracking for created/matched nodes
  - **SET Clause**: Update node properties and labels
    - `SET n.age = 31, n.city = 'NYC'` - Updates multiple properties
    - `SET n:Employee` - Adds labels to nodes
    - Supports property assignment and label addition in single clause
  - **REMOVE Clause**: Remove properties and labels from nodes
    - `REMOVE n.age` - Removes specific property
    - `REMOVE n:Employee` - Removes specific label
    - Supports multiple removals in single clause
  - **DELETE Clause**: Delete nodes with relationship validation
    - `DELETE n` - Deletes node (fails if relationships exist)
    - `DETACH DELETE n` - Deletes node and all its relationships automatically
    - Prevents orphaned relationships by validating before deletion
  - **NodeWriteState**: Internal state management for atomic node updates
    - Tracks properties and labels during write operations
    - Ensures consistent updates across multiple clauses
  - **LabelIndex Enhancement**: Added `set_node_labels()` method
    - Efficiently updates all labels for a node
    - Removes node from old labels and adds to new labels atomically
    - Maintains index consistency during label changes

### Fixed
- **MERGE Parser Bug**: Fixed parsing of `ON CREATE SET` and `ON MATCH SET` clauses
  - Parser now correctly consumes `SET` keyword before parsing set operations
  - Resolved "expected property assignment or label" error in MERGE queries
- **Clippy Warnings**: Fixed all code quality warnings
  - Removed unnecessary casts (`u64` to `u64`)
  - Simplified collapsible match expressions
  - Replaced `contains_key` + `insert` with `Entry` API for better performance
  - Moved `NodeWriteState` struct before test module to fix items-after-test-module warning

### Improved
- **OpenAPI Documentation**: Updated API specification with write operations
  - Added comprehensive examples for MERGE, SET, REMOVE, DELETE operations
  - Documented all supported write operation patterns
  - Updated version to 0.5.0 to reflect new capabilities
  - Generated `openapi.json` from updated YAML specification
- **Error Handling**: Enhanced error messages for write operations
  - Clear error when attempting DELETE on node with relationships
  - Suggests using DETACH DELETE when appropriate
  - Better validation of node state during updates

### Testing
- **Integration Tests**: Comprehensive test suite for write operations
  - 390 tests passing (100% pass rate)
  - Tests cover MERGE create/match scenarios
  - Tests validate SET property and label updates
  - Tests verify REMOVE property and label removal
  - Tests ensure DELETE validation and DETACH DELETE behavior
  - All tests executed via HTTP API for end-to-end validation
- **Test Coverage**: Maintained 95%+ coverage requirement
  - All write operation code paths covered
  - Edge cases validated (empty properties, multiple labels, etc.)

### Changed
- **Parser Structure**: Enhanced parser to handle complex MERGE clauses
  - Explicit SET keyword parsing in ON CREATE/ON MATCH blocks
  - Better clause boundary detection for write operations
- **Engine Architecture**: Added `execute_write_query()` helper method
  - Centralized handling of write operation clauses
  - Improved variable context management across clauses
  - Better separation of read and write query execution paths

## [0.10.0] - 2025-11-01

### Added
- **Multiple Database Support**: Full multi-database isolation and management
  - **DatabaseManager**: Manage multiple isolated graph databases in single instance
  - **Database API**:
    - `POST /management/databases` - Create new database
    - `DELETE /management/databases/:name` - Drop database (protects default)
    - `GET /management/databases` - List all databases with statistics
    - `GET /management/databases/:name` - Get specific database info
  - **Cypher Integration**: Optional `database` field in CypherRequest for database selection
  - **Isolation**: Each database has separate storage, catalog, indexes, and transaction log
  - **Default Database**: "neo4j" created automatically, cannot be dropped
  - **Tests**: 12 tests for DatabaseManager (100% passing)

- **Property Keys API**: Introspection and statistics for property keys
  - **Property Keys Endpoint**:
    - `GET /management/property-keys` - List all property keys
    - `GET /management/property-keys/stats` - Get usage statistics
  - **Catalog Enhancement**: Added `list_all_keys()` method to enumerate all registered keys
  - **Admin UI**: Property keys dashboard (views/property_keys.hbs)
    - Table view with node/relationship usage counts
    - Data type tracking
    - Auto-refresh functionality
  - **Tests**: 2 tests for property keys API (100% passing)

- **Built-in Functions**: Implemented 55 essential Cypher functions for data manipulation
  - **String Functions** (7):
    - `toLower()` / `toUpper()`: Case conversion
    - `substring(string, start, [length])`: Extract substrings
    - `trim()` / `ltrim()` / `rtrim()`: Whitespace removal
    - `replace(string, search, replace)`: String replacement
    - `split(string, delimiter)`: Split string into array
  - **Math Functions** (6):
    - `abs()`, `ceil()`, `floor()`, `round()`: Rounding and absolute value
    - `sqrt()`: Square root
    - `pow(base, exponent)`: Exponentiation
  - **Type Conversion** (4):
    - `toInteger()`, `toFloat()`, `toString()`, `toBoolean()`: Type coercion
  - **Temporal Functions** (5):
    - `date()` / `date(string)`: Current or parsed date in ISO format (YYYY-MM-DD)
    - `datetime()` / `datetime(string)`: Current or parsed datetime in RFC3339 format
    - `time()` / `time(string)`: Current or parsed time in HH:MM:SS format
    - `timestamp()` / `timestamp(string)`: Unix timestamp in milliseconds
    - `duration(map)`: Create duration object from components
  - **List Functions** (6):
    - `size()`: Get array/string length
    - `head()`, `tail()`, `last()`: Array element access
    - `range(start, end, [step])`: Generate numeric sequences
    - `reverse()`: Reverse array order
  - **Aggregation Functions** (5):
    - `collect()` / `collect(DISTINCT ...)`: Collect values into array with optional deduplication
    - `percentileDisc(column, percentile)`: Discrete percentile (nearest value)
    - `percentileCont(column, percentile)`: Continuous percentile (linear interpolation)
    - `stDev(column)`: Sample standard deviation (Bessel's correction)
    - `stDevP(column)`: Population standard deviation
  - **Path Functions** (3):
    - `nodes(path)`: Extract nodes from path or array
    - `relationships(path)`: Extract relationships from path or array
    - `length(path)`: Get path length (number of relationships)

### Improved
- **Query Support**: RETURN can now evaluate literal expressions without MATCH/UNWIND
  - `RETURN 1+1 AS result` now works correctly
  - `RETURN toLower('HELLO') AS lower` evaluates functions in isolation
  - Fixed planner to handle queries without graph patterns
- **Executor Enhancements**: Enhanced `execute_project` to handle various query contexts
  - Detects and handles literal-only queries correctly
  - Preserves existing behavior for MATCH, UNWIND, and UNION queries
  - Proper NULL handling in all mathematical functions

### Testing
- Added comprehensive test suite for built-in functions (61 tests, 100% passing)
- Added database management tests (12 tests, 100% passing)
- Added property keys API tests (2 tests, 100% passing)
- All existing tests maintained (1200+ tests passing)
- **Total**: 1275+ tests passing
- Verified RETURN literal queries work correctly
- Validated NULL handling across all functions
- Tested `collect()` with GROUP BY, DISTINCT, and multiple aggregations
- Added temporal function tests (date parsing, current values, format validation)
- Tested aggregation functions (percentiles, standard deviation)
- Added path function tests (nodes, relationships, length extraction)

### Fixed
- **RETURN without MATCH**: Fixed literal expression evaluation in standalone RETURN
- **Range Function**: Fixed step parameter handling for negative steps
- **NULL Handling**: All math functions now return NULL for NULL inputs

## [0.9.10] - 2025-10-31

### Fixed
- **100% Neo4j Compatibility**: Final four critical issues resolved
  - **IS NULL / IS NOT NULL**: Implemented NULL checking syntax in WHERE clauses
    - Added `IsNull` expression variant to parser AST
    - Implemented NULL evaluation logic in executor
    - Added support for `WHERE property IS NULL` and `WHERE property IS NOT NULL`
  - **WHERE AND Precedence**: Fixed operator precedence for boolean expressions
    - Refactored parser with proper precedence hierarchy (OR → AND → Comparison → Additive → Multiplicative → Unary)
    - Fixed `WHERE n.age >= 25 AND n.age <= 35` parsing and evaluation
  - **Bidirectional Relationships**: Fixed relationship filtering to match Neo4j behavior
    - `MATCH ()-[r:KNOWS]-() WHERE r.since >= 2015` now returns 6 relationships (not 3)
    - Emits each bidirectional relationship twice (once per direction)
  - **Multi-Hop Patterns**: Fixed intermediate node handling in graph traversal
    - `MATCH (p:Person)-[:KNOWS]->()-[:KNOWS]->(end)` now returns correct 2-hop paths
    - Planner generates temporary variables (`__tmp_N`) for unnamed intermediate nodes
    - Fixed `prev_node_var` tracking for proper chain expansion

### Improved
- **Neo4j Compatibility**: Achieved **100% compatibility** (35/35 extended validation tests passing)
  - All core Cypher operations fully functional
  - WHERE clause logic matches Neo4j behavior exactly
  - Graph traversal patterns working correctly
  - All critical bugs resolved

### Testing
- Extended cross-compatibility validation: **35/35 tests passing (100%)**
- Verified against extended-compatibility-suite.ps1 test suite
- All core functionality validated against Neo4j behavior
- Total: 1279+ tests across all suites (100% pass rate)

### Changed
- Cleaned up temporary test files and debug scripts
- Updated OpenSpec documentation to reflect 100% completion
- Removed all test files from version control (added to .gitignore)

## [0.9.9] - 2025-10-31

### Fixed
- **DELETE Parser Bug (CRITICAL)**: Fixed DETACH DELETE not being recognized as clause boundary
  - `MATCH (n) DETACH DELETE n` was incorrectly parsed as single Match clause
  - Added `DETACH` to `is_clause_boundary()` in parser
  - Moved DETACH DELETE detection to after keyword parsing
  - Parser now correctly creates separate Delete clause with `detach: true`
  - All DELETE operations now work correctly

### Improved
- **Neo4j Compatibility**: **88.57% compatibility** (31/35 extended validation tests passing)
  - DELETE operations now fully functional
  - CREATE operations creating exact node counts (no duplication)
  - Inline property filters working correctly
  - 4 remaining critical bugs identified for 100% compatibility

### Testing
- Extended cross-compatibility validation: **31/35 tests passing (88.57%)**
- Tested across multiple executions for consistency
- All core functionality validated against Neo4j behavior
- Total: 1279+ tests across all suites

### Changed
- Removed temporary debug logging after successful validation
- Cleaned up test scripts and validation tools

## [0.9.7] - 2025-10-31

### Added
- **Multiple Label Support**: Full support for MATCH queries with multiple labels (label intersection)
  - Queries like `MATCH (n:Person:Employee)` now work correctly
  - Planner generates NodeByLabel scan + Filter operators for additional labels
  - Filter implements variable:Label pattern checking via label_bits bitmap
  - Multiple labels are combined using bitmap intersection for efficient filtering
  - Added comprehensive test suite in `tests/neo4j_compatibility_test.rs`

- **UNION Query Support**: Fully implemented UNION operator in planner and executor
  - Planner splits queries at UNION clause, plans left/right recursively
  - Operator::Union now holds Vec<Operator> pipelines for each side
  - Executor runs both pipelines sequentially and combines results
  - Proper column handling (uses left context columns)
  - `UNION` removes duplicate rows between result sets
  - `UNION ALL` preserves all rows including duplicates
  - Column alignment and type consistency validated across queries

- **id() Function**: Neo4j-compatible ID function
  - Returns _nexus_id from nodes and relationships
  - Used in queries like `MATCH (n) RETURN id(n)`
  - Enables ID-based operations and testing

- **Bidirectional Relationships**: Enhanced relationship traversal
  - Undirected relationship patterns work correctly (e.g., `MATCH (a)-[r]-(b)`)
  - Efficiently scans relationships in both directions
  - Maintains proper node identification for source and target

- **Relationship Property Access**: Full support for relationship properties
  - Read relationship properties in WHERE clauses
  - Filter by relationship properties
  - Return full relationship objects with properties

- **keys() Function**: Implemented property introspection function
  - Returns sorted array of property names for nodes and relationships
  - Filters out internal fields (e.g., `_nexus_id`)
  - Enables property mapping validation in import scripts
  - Example: `MATCH (n:Person) RETURN keys(n)` returns `["age", "city", "name"]`

- **CREATE Clause**: Full implementation of CREATE operations in Cypher
  - CREATE now properly persists nodes and relationships
  - Supports multiple labels: `CREATE (n:Person:Employee {name: "Alice"})`
  - Supports properties on nodes and relationships
  - Intercepts CREATE in Engine.execute_cypher() for proper transaction handling
  - Automatic executor refresh after CREATE to ensure data visibility
  - All 736 core tests continue passing

- **Enhanced Import Logging**: Detailed statistics and progress tracking
  - Timestamp logging for all import operations
  - Entity creation statistics by type (nodes and relationships)
  - Progress tracking with percentage complete
  - JSON log export to import-nexus.log
  - VERBOSE mode for detailed debugging (set VERBOSE=true)
  - Throughput and duration metrics

### Fixed
- **Engine Test Suite**: Fixed critical bug in `Engine::new()` causing 11 tests to fail
  - `Engine::new()` now properly keeps temporary directory alive for Engine lifetime
  - Added `_temp_dir: Option<TempDir>` field to Engine struct to store directory guard
  - All 11 previously failing tests now pass (test_update_node, test_delete_node, test_clear_all_data, and 8 others)
  - No impact on production code (production uses `Engine::with_data_dir()` with persistent storage)
  - Root cause: TempDir guard was dropped immediately after Engine::new() returned, causing "No such file or directory" errors

- **Import Validation Script**: Created PowerShell validation script
  - Validates node type counts and distributions
  - Verifies relationship type counts
  - Compares property mappings between Nexus and Neo4j (optional)
  - Location: `nexus/scripts/validate-import.ps1`

### Fixed
- Fixed PowerShell validation script variable interpolation issue
  - Removed incorrect backticks that were preventing proper Cypher query generation
  - Queries now correctly substitute node and relationship types

### Validated
- **Node Type Import**: Verified all expected node types created successfully
  - Document: 3,852 nodes
  - Module: 301 nodes
  - Class: 696 nodes
  - Function: 1,146 nodes
  - Type: 19 nodes

- **Relationship Type Import**: Verified all relationship types created successfully
  - All 8 expected types (MENTIONS, IMPORTS, HAS, CONTAINS, EXTENDS, IMPLEMENTS, CALLS, REFERENCES): 3,639 each

### Testing
- **Neo4j Compatibility**: 29/33 tests passing (88% pass rate, 95% feature complete)
  - Expanded from 7 to 33 tests (371% increase in coverage)
  - ✅ test_multiple_labels_match
  - ✅ test_multiple_labels_filtering
  - ✅ test_union_queries
  - ✅ test_relationship_property_access
  - ✅ test_relationship_property_return
  - ✅ test_bidirectional_relationship_queries
  - ⏸️ test_complex_multiple_labels_query (known bug: result duplication)
  - Plus 26 additional coverage tests for edge cases
- All 736 core tests passing (100% pass rate)
- Total: 1088+ tests across all suites
- Test setup uses Engine API directly to bypass executor RecordStore cloning limitation
- Made refresh_executor() public for state synchronization after API operations

### Known Issues
- **Multi-label + Relationship Duplication**: MATCH queries combining multiple labels with relationship traversal may return duplicate results
  - Example: `MATCH (n:Person:Employee)-[r:WORKS_AT]->(c)` returns 2 identical rows instead of 1
  - Only affects this specific pattern combination
  - Other multi-label queries work correctly
  - Single ignored test out of 7 total compatibility tests

## [0.9.6] - 2025-10-30

### Fixed
- **DISTINCT Clause Support**: Fixed `DISTINCT` operator not being applied correctly in queries
  - Planner now correctly generates `Distinct` operator when `RETURN DISTINCT` is used
  - `execute_distinct` now properly deduplicates rows based on column values
  - Queries like `MATCH (n) RETURN DISTINCT labels(n)` now return unique values correctly

- **labels() Function**: Fixed `labels()` function not returning node labels correctly
  - Function now reads node record and extracts labels from bitmap using catalog
  - Returns array of label names matching Neo4j's behavior

- **type() Function**: Fixed `type()` function not returning relationship type correctly
  - Function now reads relationship record and extracts type_id
  - Looks up type name from catalog to return correct relationship type string
  - Added `_nexus_id` to relationship objects for internal ID extraction

- **Queries Without Explicit Nodes**: Fixed queries like `MATCH ()-[r]->() RETURN count(r)` returning 0 results
  - `execute_expand` now scans all relationships directly when source_var is empty
  - Supports queries without explicit node labels in MATCH patterns

- **Scan All Nodes**: Fixed `label_id: 0` handling to scan all nodes when no label specified
  - `execute_node_by_label` now correctly handles `label_id: 0` as "scan all" operator
  - Builds bitmap of all nodes from storage for label-less queries

### Added
- Support for `DISTINCT` modifier in `RETURN` and `WITH` clauses
- Enhanced relationship object serialization with `_nexus_id` for internal use
- Support for relationship-only queries without explicit node patterns

### Testing
- Comprehensive test suite now passing 15/20 tests (75% pass rate)
- All critical functionality tests passing (counts, relationships, labels, types, DISTINCT)

## [0.9.5] - 2025-10-30

### Fixed
- **Property Persistence**: Fixed critical issue where properties were not being persisted to disk or loaded after server restart
  - Enhanced `PropertyStore::flush()` to call `sync_all()` for OS-level file synchronization
  - Added `refresh_executor()` method to update executor after `create_node()` and `create_relationship()` operations
  - Fixed executor using separate `PropertyStore` instance that didn't see newly written properties
  - Properties now correctly persist and load after server restart

## [0.9.4] - 2025-10-30

### Fixed
- **Node Property Loading** ✅ **CRITICAL FIX**
  - Fixed `PropertyStore::load_properties` to correctly load properties from persistent storage
  - Fixed `PropertyStore::rebuild_index` to properly reconstruct property index on server restart
  - Properties are now correctly persisted and retrieved for all nodes and relationships
  - Queries like `MATCH (d:Document) RETURN d` now return full properties (title, domain, file_hash, etc.)
  - Resolved issue where properties were saved but not loaded during MATCH queries

- **Query Planner Target Node Handling** ✅
  - Fixed planner to skip `NodeByLabel` operator for target nodes without labels in relationship patterns
  - Prevents double-scanning and incorrect filtering when expanding relationships
  - Queries like `MATCH (d:Document)-[:MENTIONS]->(e)` now work correctly (target `e` populated only by Expand)
  - Improved test pass rate from 11/20 to 13/20 in comprehensive Neo4j comparison suite

### Changed
- **Result Set Row Management** ✅
  - `execute_expand` now uses `result_set.rows` instead of variables when available
  - Maintains row context from previous operators for better data flow

### Testing
- Comprehensive test suite comparing Nexus vs Neo4j results (20 test queries)
- 13/20 tests passing (queries with labels work perfectly)
- Remaining failures relate to general expansion patterns and property ordering

## [0.9.3] - 2025-10-30

### Fixed
- **Relationship Expansion Parity with Neo4j** ✅
  - `execute_expand` now respects the target variable's label-filtered bindings, preventing unrelated nodes from surfacing in MATCH clauses
  - `extract_entity_id` resolves `_nexus_id`, `_element_id`, and string-based IDs so relationship traversals hydrate the intended entities
  - `read_node_as_value` now returns properties in flat format matching Neo4j output (with `_nexus_id` for internal use)
  - `read_relationship_as_value` simplified to return only properties (no metadata fields)
  - Fixed duplicate data import issue (removed double import causing 2x node/rel counts)

### Changed
- **Query Result Format Alignment** ✅
  - Nodes and relationships now return properties as flat objects matching Neo4j format
  - Only `_nexus_id` included for internal ID tracking (not exposed in user-facing results)
  - Property ordering may differ from Neo4j (does not affect functionality)

### Planning
- **Future Enhancements** 📋
  - Added task for Multiple Database Support (isolated data directories per database)
  - Added task for Property Keys API (expose Catalog's property key mappings through REST)

### Tooling
- **Import Helper Compatibility** ✅
  - Added `// @ts-nocheck` to `scripts/import-classify-to-neo4j.ts` and `scripts/import-classify-to-nexus.ts`
  - Import scripts work correctly with single database instance

### Testing
- `cargo test -p nexus-core` (736 tests passed)
- `npx tsx scripts/compare-nexus-neo4j.ts` (all queries match Neo4j results)

## [0.9.2] - 2025-10-27

### Added
- **Cypher Write Operations Parser Support** ✅
  - Added SET clause parsing for property updates and label additions
  - Added DELETE clause parsing (including DETACH DELETE support)
  - Added REMOVE clause parsing for property and label removal
  - All write operation parsers now complete and functional

- **MERGE Clause Complete Implementation** ✅
  - Implemented full match-or-create semantics with property matching
  - MERGE searches existing nodes by label and properties
  - Creates new node only if no matching node found
  - Added variable context tracking for created nodes
  - Added comprehensive MERGE tests

- **Variable Context Infrastructure** ✅
  - Added `variable_context` HashMap to store node_id bindings between clauses
  - CREATE and MERGE operations now store node_id in variable context
  - SET/DELETE/REMOVE clause handlers added with detection
  - Foundation ready for implementing multi-clause queries (e.g., MATCH + SET)

- **Cypher Write Operations Execution** ✅
  - SET clause execution: Updates node properties and adds labels using Engine::update_node()
  - DELETE clause execution: Deletes nodes using Engine::delete_node()
  - REMOVE clause execution: Removes properties and labels from nodes
  - All clauses use variable_context for node lookups
  - Properties loaded, modified, and saved atomically

### Published
- **Progress**: Cypher Write Operations now 87% complete (20/23 tasks) ✅
- **Parsers**: 100% complete (CREATE, MERGE, SET, DELETE, REMOVE) ✅
- **Execution**: 100% complete (all write operations working) ✅
- **Tests**: 21 comprehensive tests passing ✅
- **Remaining**: DETACH DELETE fully, ON CREATE/ON MATCH support

## [0.9.1] - 2025-10-27

### Fixed
- **Data Source Unification** ✅
  - Fixed MATCH queries returning empty results by ensuring label_index is updated when creating nodes
  - Engine::create_node now automatically updates label_index after node creation
  - Fixed Engine and Executor data synchronization issue
  - MATCH queries via Engine now correctly find nodes by label
  - /data/nodes endpoint refactored to use shared Engine instance

### Added
- **Engine-Executor Integration** (2025-10-27) ✅
  - MATCH queries now use engine.execute_cypher() to access shared storage
  - /data/nodes endpoint now uses ENGINE.get() shared instance
  - Added init_engine() function to data.rs module
  - CREATE and MATCH operations share the same catalog, storage, and label_index

### Testing
- **Full Integration Testing** (2025-10-27) ✅
  - Tested CREATE via Cypher: Nodes persist correctly ✅
  - Tested MATCH via Cypher: Returns nodes correctly ✅
  - Tested /data/nodes: Creates nodes successfully ✅
  - Tested multiple nodes creation: All counted in stats ✅
  - Tested ORDER BY in MATCH: Query executed successfully ✅

**Test Results** (2025-10-27):
```
✅ CREATE (p:Person {name: "Alice", age: 30}) → Success
✅ MATCH (p:Person) RETURN p → Returns 2 nodes
✅ POST /data/nodes → Creates node with node_id returned
✅ GET /stats → Accurately reflects all created nodes
✅ MATCH with ORDER BY → Works correctly
✅ All 1041 tests passing
```

## [0.9.0] - 2025-10-26

### Fixed

- **Critical Persistence Bugs** ✅
  - Fixed CREATE queries not persisting nodes to storage
  - Fixed stats endpoint always showing node_count: 0
  - Fixed create_node MCP tool failing to extract node_id
  - Fixed graph_correlation_analyze requiring complete graph structures

- **Cypher Parser Improvements** ✅
  - Added missing `skip_whitespace()` calls in parser
  - Fixed `is_clause_boundary()` to recognize CREATE and MERGE keywords
  - Fixed property map parsing with proper whitespace handling
  - Added MergeClause to parser AST

- **Engine Integration** ✅
  - Integrated Engine into REST /cypher endpoint for CREATE operations
  - Added ENGINE static to stats module for real-time statistics
  - Implemented direct node creation via Engine.create_node()
  - Stats now query Engine.stats() for accurate node/label counts

### Changed

- **License Simplification** ✅
  - Changed from dual-license (MIT OR Apache-2.0) to MIT only
  - Removed Apache-2.0 license text from LICENSE file
  - Updated Cargo.toml workspace license field

- **Architecture Improvements** ✅
  - CREATE queries now use Engine instead of Executor for persistence
  - Stats endpoint consults Engine as primary source of truth
  - MCP tools use Engine directly for all write operations
  - Fallback to old catalog stats if Engine unavailable

### Added

- **Graph Normalization** ✅
  - Added automatic graph structure normalization in graph_correlation_analyze
  - Default values for missing fields: name, created_at, updated_at, metadata
  - Accepts partial graph structures without complete metadata
  - Normalizes nodes and edges with missing optional fields

### Testing

- **Verification** ✅
  - Tested CREATE via REST: Nodes persist correctly
  - Tested create_node via MCP: Returns node_id successfully
  - Tested stats endpoint: Shows accurate node_count
  - Tested graph_correlation_analyze: Accepts partial graphs

**Test Results**:
```
✅ CREATE (n:TestNode {value: 999}) → Success
✅ CREATE (p:Person {age: 30, name: 'Alice'}) → Success  
✅ GET /stats → {"node_count": 2, "label_count": 2} ✅
✅ create_node MCP → {"node_id": 2, "status": "created"} ✅
✅ graph_correlation_analyze → Accepts partial graphs ✅
```

## [0.8.0] - 2025-10-26

### Fixed

- **Critical Bug Fixes** ✅
  - Fixed infinite recursion in RecordStore persistence logic
  - Corrected `REL_RECORD_SIZE` from 40 to 52 bytes (actual struct size)
  - Fixed packed field unaligned reference errors in integration tests
  - Fixed concurrent transaction test to use `Mutex<RecordStore>` for thread safety
  - Fixed flaky `test_create_rel_type_with_initialized_catalog` due to OnceLock race condition
  - Fixed flaky `test_init_graphs_success` due to OnceLock global state
  - Fixed `test_knn_index_search_knn_default` to handle HNSW behavior with small indexes
  - Fixed compilation errors in clustering methods (catalog API updates)
  - Removed `.truncate(true)` flag that was deleting data on RecordStore reopen
  - Implemented ID tracking via record scanning to restore `next_node_id` and `next_rel_id`

- **Test Suite Improvements** ✅
  - All **858 tests** now passing (100% success rate)
  - Test count correction: 670 lib + 158 server + 15 integration + 10 HTTP + 5 doctests
  - Fixed `Executor::default()` to create RecordStore with temporary directory
  - Fixed `GraphNode` test missing `size` and `color` fields
  - Improved test robustness for concurrent execution

### Added

- **OpenSpec Documentation** ✅
  - Created comprehensive `OPENSPEC_SUMMARY.md` showing MVP at 89.8% complete
  - Added `STATUS.md` to graph-correlation-analysis with phase breakdown
  - Archived 4 MVP changes to `archive/2025-10-25-*` (198 tasks complete)
  - Documented bonus modules: ~10,000 lines beyond MVP scope
  - Updated all tasks.md with implementation status

- **Modular OpenSpec Structure for Complete Neo4j Cypher Implementation** ✅
  - Created master tracker `implement-cypher-complete-clauses/` (MASTER PLAN)
  - Split massive 554-task proposal into **14 manageable phases**
  - Each phase has dedicated `proposal.md` + `tasks.md` files
  - Added comprehensive `openspec/changes/README.md` for navigation
  - **Phase 1**: Write Operations (MERGE, SET, DELETE, REMOVE) - Ready to start
  - **Phase 2**: Query Composition (WITH, OPTIONAL MATCH, UNWIND, UNION)
  - **Phase 3**: Advanced Features (FOREACH, EXISTS, CASE, comprehensions)
  - **Phase 4**: String Operations (STARTS WITH, ENDS WITH, CONTAINS, regex)
  - **Phase 5**: Path Operations (variable-length, shortest path, all paths)
  - **Phase 6**: Built-in Functions (50+ functions: string, math, list, aggregation)
  - **Phase 7**: Schema & Admin (indexes, constraints, transactions)
  - **Phase 8**: Data Import/Export (LOAD CSV, bulk operations)
  - **Phase 9**: Query Analysis (EXPLAIN, PROFILE, hints)
  - **Phase 10**: Advanced DB Features (USE DATABASE, subqueries, named paths)
  - **Phase 11**: Performance Monitoring (metrics, slow queries, statistics)
  - **Phase 12**: UDF & Procedures (user-defined functions, plugin system)
  - **Phase 13**: Graph Algorithms (pathfinding, centrality, community detection)
  - **Phase 14**: Geospatial Support (Point, Distance, spatial indexes)
  - Total timeline: **32-46 weeks** for full Neo4j Cypher compatibility
  - Clear dependencies and implementation order defined

- **Discovered Modules** ✅
  - Authentication system (5 files, 82 items, Argon2 + RBAC)
  - Performance optimization suite (8 files, ~3,000 lines)
  - Clustering algorithms (1,670 lines, 6 algorithms)
  - Bulk loader (1,081 lines, parallel processing)
  - B-tree property index (588 lines)
  - Graph validation (951 lines)
  - Security/rate limiting (592 lines)

### Changed

- **Project Organization** ✅
  - Moved completed MVP phases to archive (storage, indexes, executor, API)
  - Separated MVP tasks from V1/V2 future features
  - Reorganized OpenSpec structure for better progress tracking
  - Updated progress metrics: MVP at 89.8% (283/315 tasks)

### Statistics

- **Code**: 40,758 lines (nexus-core: 33,648 + nexus-server: 7,110)
- **Tests**: 858 tests (100% passing)
- **Coverage**: 70.39% overall, 95%+ in core modules
- **Files**: 50 Rust files across 19 modules
- **MVP Progress**: 89.8% complete (only 12% visible in watcher due to V1/V2 tasks)

## [0.7.0] - 2025-10-25

### Added

- **Rate Limiting System** ✅
  - Configurable rate limiting in auth middleware
  - Window-based rate limiting with sliding window support
  - Per-client rate limit tracking with automatic cleanup
  - Rate limit configuration with customizable thresholds
  - Comprehensive rate limiting tests and validation

- **Async Monitoring System** ✅
  - Proper async monitoring with Send trait compliance
  - System resource monitoring with background tasks
  - Memory usage monitoring with continuous tracking
  - Performance metrics collection with Arc<RwLock<T>> for thread safety
  - Configurable monitoring intervals and thresholds
  - Graceful shutdown of monitoring tasks

- **Property Chain Traversal** ✅
  - Full property chain traversal system in graph.rs
  - PropertyStore for managing property storage and retrieval
  - PropertyRecord structure for property chain management
  - Serialization and deserialization of property chains
  - Property pointer management and traversal
  - Comprehensive property chain tests

- **Bulk Data Loading** ✅
  - Complete loader module implementation
  - Support for JSON, CSV, and in-memory data sources
  - Progress reporting and statistics tracking
  - Error handling and validation
  - Batch processing capabilities
  - Data transformation and mapping

- **Security Features** ✅
  - Comprehensive security module implementation
  - Rate limiting with configurable windows
  - IP blocking and whitelist management
  - SQL injection protection
  - XSS protection and request validation
  - Security statistics and monitoring
  - Async security operations

### Changed

- **Error Handling** ✅
  - Updated error types to use `Box<dyn std::error::Error + Send + Sync>`
  - Consistent error handling across async boundaries
  - Improved error propagation in spawned tasks
  - Better error messages and debugging information

- **Async Trait Compatibility** ✅
  - Fixed async trait compatibility issues
  - Introduced CollectionQueryEnum wrapper for trait objects
  - Resolved dyn trait object limitations with async functions
  - Maintained type safety while enabling async operations

### Fixed

- **Test Coverage** ✅
  - Fixed failing test_clear_cache with proper node creation
  - Updated async test patterns for proper synchronization
  - Added comprehensive test coverage for new features
  - All 628 tests now passing

- **Code Quality** ✅
  - Fixed all clippy warnings and linting issues
  - Applied consistent code formatting
  - Resolved unused variable warnings
  - Added proper documentation and comments

## [0.6.0] - 2025-10-25

### Added

- **Node Clustering and Grouping** (Task 2.3) ✅
  - Comprehensive clustering algorithms implementation
  - K-means clustering with k-means++ initialization
  - Hierarchical clustering with multiple linkage types
  - Label-based and property-based grouping
  - Community detection using connected components
  - DBSCAN density-based clustering
  - Multiple distance metrics (Euclidean, Manhattan, Cosine, Jaccard, Hamming)
  - Feature extraction strategies (label-based, property-based, structural, combined)
  - Quality metrics calculation (silhouette score, WCSS, BCSS, Calinski-Harabasz, Davies-Bouldin)

- **Clustering API Endpoints**
  - GET /clustering/algorithms - List available algorithms and parameters
  - POST /clustering/cluster - Perform clustering with configurable parameters
  - POST /clustering/group-by-label - Group nodes by their labels
  - POST /clustering/group-by-property - Group nodes by specific properties
  - Comprehensive request/response models with JSON serialization
  - Error handling and validation for all clustering operations

- **Core Engine Implementation**
  - Implemented `Engine::new()` method with full component initialization
  - Added storage, catalog, page cache, WAL, and transaction manager integration
  - Added `Engine::new_default()` convenience method

- **Protocol Client Implementations**
  - Implemented REST client with POST, GET, and streaming methods
  - Implemented MCP client with JSON-RPC 2.0 support
  - Implemented UMICP client for universal model communication
  - Added proper error handling and HTTP status code checking

- **Performance Optimizations**
  - Added query cache with configurable TTL and capacity
  - Implemented exponential backoff retry mechanism
  - Added jitter to prevent thundering herd problems
  - Added cache statistics and management

- **Error Handling and Recovery**
  - Added comprehensive retry mechanisms for transient failures
  - Implemented retryable error detection for I/O and database errors
  - Added retry statistics and context tracking
  - Added specialized retry functions for storage, network, and database operations

- **Monitoring and Logging**
  - Added comprehensive health check endpoint with component status
  - Implemented detailed metrics endpoint with system information
  - Added uptime tracking and human-readable duration formatting
  - Added component-specific health checks with timeout handling

### Changed

- **API Endpoints**
  - Updated health check endpoint to use new health module
  - Added `/metrics` endpoint for detailed system metrics
  - Enhanced error responses with more detailed information

- **Dependencies**
  - Added `reqwest` and `futures-util` for HTTP client functionality
  - Added `rand` for retry mechanism jitter
  - Added `chrono` for timestamp formatting

### Technical Details

- **Query Cache**: 1000 entry capacity with 5-minute TTL
- **Retry Configuration**: 3 attempts with exponential backoff (100ms initial, 2x multiplier)
- **Health Check Timeouts**: Database (5s), Storage (3s), Indexes (2s), WAL (1s), Page Cache (500ms)
- **Error Recovery**: Automatic retry for transient I/O and database errors

## [0.5.0] - 2025-10-25

### Fixed

- **Test Suite Fixes**
  - Fixed `Executor::default()` to create RecordStore with temporary directory
  - Fixed `GraphNode` test missing `size` and `color` fields
  - Fixed packed field unaligned reference errors by copying values locally
  - Fixed concurrent transaction test to use `Mutex<RecordStore>` for thread-safe access
  - Corrected `REL_RECORD_SIZE` constant from 40 to 52 bytes
  - Fixed RecordStore persistence by removing `.truncate(true)` flag that was deleting data
  - Implemented ID tracking via record scanning to properly restore `next_node_id` and `next_rel_id` on reopening
  - All 309 tests now passing (195 lib + 15 integration + 84 server + 10 HTTP + 5 doctests)

### Added

- **Complete MVP Integration & Testing** (Phase 1.6) ✅
  - Comprehensive end-to-end testing framework
  - Performance benchmarking suite
  - Complete documentation ecosystem

- **Sample Datasets** (`examples/datasets/`)
  - Social network dataset with users, posts, comments, and relationships
  - Knowledge graph dataset with entities, concepts, and semantic relationships
  - Dataset loader utility for easy data ingestion

- **Cypher Test Suite** (`examples/cypher_tests/`)
  - Comprehensive test suite with 7 categories of tests
  - Basic queries, aggregation, relationships, knowledge graph queries
  - KNN vector queries, performance tests, error handling
  - Test runner with performance benchmarking capabilities

- **KNN + Traversal Hybrid Queries**
  - Vector similarity search combined with graph traversal
  - Hybrid queries for recommendation systems
  - Semantic similarity with relationship analysis

- **Crash Recovery Testing** (`examples/crash_recovery_tests/`)
  - WAL recovery during write transactions
  - Catalog recovery after corruption
  - Index recovery after crash scenarios
  - Partial transaction recovery testing
  - Concurrent transaction recovery testing
  - Performance testing for recovery scenarios

- **Performance Benchmarks** (`examples/benchmarks/`)
  - Point reads benchmarking (100K+ ops/sec target)
  - KNN queries benchmarking (10K+ ops/sec target)
  - Pattern traversal benchmarking (1K-10K ops/sec target)
  - Bulk ingest benchmarking (100K+ nodes/sec target)
  - Memory usage monitoring and optimization

- **Comprehensive Documentation**
  - **User Guide** (`docs/USER_GUIDE.md`): Complete usage guide with examples
  - **API Reference** (`docs/api/openapi.yml`): OpenAPI 3.0.3 specification
  - **Deployment Guide** (`docs/DEPLOYMENT_GUIDE.md`): Production deployment instructions
  - **Performance Tuning Guide** (`docs/PERFORMANCE_TUNING_GUIDE.md`): Optimization strategies

### Changed

- **MVP Phase Completion**: All MVP phases (1.1-1.6) now complete
- **Documentation Structure**: Organized documentation in `/docs` directory
- **Test Coverage**: Maintained 79.13% test coverage with comprehensive integration tests

### Technical Details

- **Dataset Format**: JSON-based datasets with nodes, relationships, and metadata
- **Test Framework**: Rust-based testing with async support and performance metrics
- **Recovery Testing**: Comprehensive crash recovery scenarios with WAL and transaction management
- **Benchmarking**: Multi-threaded performance testing with detailed metrics
- **Documentation**: Markdown-based documentation with code examples and best practices

## [0.4.0] - 2025-10-25

### Added

- **Complete MVP HTTP API** (Phase 1.5) ✅
  - REST endpoints with comprehensive test coverage (79.13%)
  - Server-Sent Events (SSE) streaming support
  - End-to-end integration tests (282 tests passing)

- **REST API Endpoints** (`nexus-server/src/api/`)
  - POST /cypher: Execute Cypher queries with parameter support
  - POST /knn_traverse: KNN-seeded graph traversal
  - POST /ingest: Bulk data ingestion with throughput metrics
  - GET /health: Health check with version information
  - GET /stats: Database statistics (nodes, relationships, indexes)
  - POST /schema/labels: Create and manage node labels
  - GET /schema/labels: List all node labels
  - POST /schema/rel_types: Create relationship types
  - GET /schema/rel_types: List relationship types
  - POST /data/nodes: Create nodes with properties
  - POST /data/relationships: Create relationships
  - PUT /data/nodes: Update node properties
  - DELETE /data/nodes: Delete nodes

- **Streaming Support** (`nexus-server/src/api/streaming.rs`)
  - Server-Sent Events (SSE) for large result sets
  - GET /sse/cypher: Stream Cypher query results
  - GET /sse/stats: Stream database statistics updates
  - GET /sse/heartbeat: Stream heartbeat events
  - Chunked transfer encoding with backpressure handling
  - Configurable streaming timeouts

- **Comprehensive Testing**
  - Unit tests for all API endpoints (84 tests)
  - Integration tests for end-to-end validation (10 tests)
  - Test coverage: 79.13% lines, 77.92% regions
  - All 282 tests passing (173 core + 15 core integration + 84 server + 10 server integration)
  - Performance tests for concurrent requests and large payloads

- **MCP Integration** (`nexus-server/src/api/streaming.rs`)
  - NexusMcpService for MCP protocol support
  - Tool registration and execution
  - Resource management and health monitoring
  - Request context handling

### Dependencies Added

- `async-stream 0.3` - Async stream generation for SSE
- `futures 0.3` - Future utilities for streaming
- `tower 0.5` - Service abstraction layer
- `tower-http 0.6` - HTTP middleware for Axum

### Performance

- **API throughput**: >1000 requests/sec for health checks
- **Concurrent handling**: 10+ concurrent requests tested
- **Large payload support**: 10KB+ payloads handled efficiently
- **Streaming**: Real-time data streaming with SSE

### Testing

- **282 tests total**: 173 core + 15 core integration + 84 server + 10 server integration
- **79.13% coverage**: Exceeds minimum requirements for MVP
- **Zero warnings**: Clippy passes with -D warnings
- **All tests passing**: 100% pass rate

### Quality

- Rust edition 2024 with nightly 1.85+
- All code formatted with `cargo +nightly fmt`
- Zero clippy warnings
- Comprehensive error handling
- Detailed API documentation

## [Unreleased]

## [0.9.8] - 2025-10-31

### Added
- **COUNT(DISTINCT ...)**: Full support for distinct counting in aggregations
  - Parser detects `DISTINCT` keyword in `count()` function calls
  - HashSet-based deduplication for unique value counting
  - Works with variables and property access: `COUNT(DISTINCT n.age)`
- **UNION Deduplication Fix**: Proper deduplication for UNION queries
  - `distinct` field added to `Operator::Union` enum
  - HashSet-based row deduplication via JSON serialization
  - Automatic detection of `UnionType::Distinct` vs `UnionType::All`
- **PropertyAccess in Aggregations**: Enhanced aggregation support
  - Planner correctly extracts property expressions from aggregation functions
  - Required columns tracking for property access patterns
  - Support for `avg(n.property)`, `min(n.property)`, `max(n.property)`, `sum(n.property)`

### Fixed
- UNION operator now deduplicates results correctly (was returning all rows like UNION ALL)
- Aggregation functions now correctly handle `avg(n.property)` syntax
- Required columns tracking now skips `__DISTINCT__` marker in aggregation args
- PropertyAccess expressions now correctly registered in required columns

### Improved
- Neo4j cross-compatibility increased to **88.24%** (15/17 validation tests passing)
- UNION deduplication matches Neo4j behavior exactly
- Better property access handling in query planner

### Testing
- Cross-compatibility validation: **15/17 tests passing (88.24%)**
- COUNT(DISTINCT) test: ✅ 4 unique values
- UNION deduplication test: ✅ 5 rows (was 50)
- All aggregation tests passing ✅
- Total: 1542+ tests across all suites

## [0.2.0] - 2025-10-25

### Added

- **Complete MVP Storage Layer** (Phase 1.1-1.2) ✅
  - LMDB catalog with bidirectional mappings (98.64% coverage)
  - Memory-mapped record stores (96.96% coverage)
  - Page cache with Clock eviction (96.15% coverage)
  - Write-Ahead Log with CRC32 (96.71% coverage)
  - MVCC transaction manager (99.02% coverage)

- **Catalog Module** (`nexus-core/src/catalog/`)
  - LMDB integration via heed (10GB max size, 8 databases)
  - Bidirectional mappings: label_name ↔ label_id, type_name ↔ type_id, key_name ↔ key_id
  - Metadata storage (version, epoch, page_size)
  - Statistics tracking (node counts per label, relationship counts per type)
  - Thread-safe with RwLock for concurrent reads
  - 21 unit tests covering all functionality

- **Record Stores** (`nexus-core/src/storage/`)
  - NodeRecord (32 bytes fixed-size): label_bits, first_rel_ptr, prop_ptr, flags
  - RelationshipRecord (48 bytes fixed-size): src, dst, type, next_src, next_dst, prop_ptr
  - Memory-mapped files with automatic growth (1MB → 2x exponential)
  - Doubly-linked lists for O(1) relationship traversal
  - Label bitmap operations (supports 64 labels per node)
  - 18 unit tests including file growth and linked list traversal

- **Page Cache** (`nexus-core/src/page_cache/`)
  - Clock (second-chance) eviction algorithm
  - Pin/unpin semantics with atomic reference counting
  - Dirty page tracking with HashSet
  - xxHash3 checksums for corruption detection
  - Statistics (hits, misses, evictions, hit rate)
  - 21 unit tests covering eviction, pinning, checksums, concurrency

- **Write-Ahead Log** (`nexus-core/src/wal/`)
  - 10 entry types (BeginTx, CommitTx, CreateNode, CreateRel, SetProperty, etc)
  - Binary format: [type:1][length:4][payload:N][crc32:4]
  - CRC32 validation for data integrity
  - Append-only log with fsync for durability
  - Checkpoint mechanism with statistics tracking
  - Crash recovery with entry replay
  - 16 unit tests including corruption detection and large payloads

- **Transaction Manager** (`nexus-core/src/transaction/`)
  - Epoch-based MVCC for snapshot isolation
  - Single-writer model (queue-based, prevents deadlocks)
  - Read transactions pin current epoch
  - Write transactions increment epoch on commit
  - Visibility rules: created_epoch <= tx_epoch < deleted_epoch
  - 20 unit tests covering all transaction lifecycle

- **Integration Tests** (`nexus-core/tests/integration.rs`)
  - 15 end-to-end tests covering multi-module interactions
  - Performance benchmarks (100K+ reads/sec, 10K+ writes/sec)
  - Crash recovery validation
  - MVCC snapshot isolation verification
  - Concurrent access validation (5 readers + 3 writers)

### Dependencies Added

- `heed 0.20` - LMDB wrapper for catalog
- `memmap2 0.9` - Memory-mapped files for record stores
- `xxhash-rust 0.8` - Fast checksums for page cache
- `crc32fast 1.4` - CRC32 for WAL integrity
- `parking_lot 0.12` - Efficient locking primitives
- `tempfile 3.15` - Temporary directories for tests

### Performance

- **Node reads**: >100,000 ops/sec (O(1) direct offset access)
- **Node writes**: >10,000 ops/sec (append-only with auto-growth)
- **Page cache**: Clock eviction prevents memory exhaustion
- **WAL**: Append-only for predictable write performance

### Testing

- **133 tests total**: 118 unit tests + 15 integration tests
- **96.06% coverage**: All implemented modules exceed 95%+ requirement
- **Zero warnings**: Clippy passes with -D warnings
- **All tests passing**: 100% pass rate

### Quality

- Rust edition 2024 with nightly 1.85+
- All code formatted with `cargo +nightly fmt`
- Zero clippy warnings
- Comprehensive documentation with examples
- Doctests for all public APIs

## [0.1.0] - 2024-10-24

### Added

- **Project Initialization**
  - Cargo workspace setup (edition 2024, nightly)
  - Module structure (nexus-core, nexus-server, nexus-protocol)
  - Comprehensive architecture documentation

- **Documentation**
  - [ARCHITECTURE.md](docs/ARCHITECTURE.md) - Complete system design
  - [ROADMAP.md](docs/ROADMAP.md) - Implementation phases and timeline
  - [DAG.md](docs/DAG.md) - Component dependency graph
  - [storage-format.md](docs/specs/storage-format.md) - Record store layouts
  - [cypher-subset.md](docs/specs/cypher-subset.md) - Supported Cypher syntax
  - [page-cache.md](docs/specs/page-cache.md) - Memory management design
  - [wal-mvcc.md](docs/specs/wal-mvcc.md) - Transaction model
  - [knn-integration.md](docs/specs/knn-integration.md) - Vector search integration
  - [api-protocols.md](docs/specs/api-protocols.md) - REST, MCP, UMICP specs
  - README.md - Project overview and quick start
  - CHANGELOG.md - This file

- **Core Module Scaffolding** (nexus-core)
  - `error` - Error types and Result aliases
  - `catalog` - Label/Type/Key ID mappings (LMDB)
  - `storage` - Record stores (nodes, rels, props, strings)
  - `page_cache` - Page management with eviction policies
  - `wal` - Write-ahead log for durability
  - `index` - Indexing subsystems (label bitmap, B-tree, full-text, KNN)
  - `executor` - Cypher query executor (parser, planner, operators)
  - `transaction` - MVCC and locking

- **Server Scaffolding** (nexus-server)
  - Axum HTTP server setup
  - REST API endpoints (stubs):
    - `GET /health` - Health check
    - `POST /cypher` - Execute Cypher queries
    - `POST /knn_traverse` - KNN-seeded traversal
    - `POST /ingest` - Bulk data ingestion
  - Configuration management

- **Protocol Layer** (nexus-protocol)
  - REST client for external integrations
  - MCP client stub
  - UMICP client stub

- **Build Infrastructure**
  - `.gitignore` for Rust projects
  - `rust-toolchain.toml` (nightly, edition 2024)
  - Workspace dependencies in `Cargo.toml`
  - LICENSE (MIT OR Apache-2.0)

### Dependencies

- **Storage**: memmap2, heed (LMDB), parking_lot, roaring
- **Indexes**: tantivy, hnsw_rs
- **Async**: tokio, axum, tower, hyper
- **Serialization**: serde, serde_json, bincode, bytes, bytemuck
- **Error**: thiserror, anyhow
- **Observability**: tracing, tracing-subscriber
- **Utilities**: uuid, chrono

### Testing

- Test structure defined in `/tests` directory
- 95% coverage requirement documented
- Integration test framework prepared

## [0.0.0] - 2024-10-23

### Initial Concept

- Project planning and architecture design
- Technology stack selection (Rust, LMDB, HNSW, Axum)
- Neo4j-inspired storage model research

---

## Versioning Strategy

- **MAJOR** (x.0.0): Breaking API changes, storage format changes
- **MINOR** (0.x.0): New features, backwards compatible
- **PATCH** (0.0.x): Bug fixes, performance improvements

---

## Upcoming Releases

### [0.2.0] - MVP Core (Planned: Q4 2024)

#### Storage Layer

- [x] Catalog implementation (LMDB)
- [ ] Record stores (nodes, rels, props, strings)
- [ ] Page cache with clock eviction
- [ ] WAL with checkpoint/recovery
- [ ] MVCC transaction manager

#### Indexes

- [ ] Label bitmap index (RoaringBitmap)
- [ ] KNN vector index (HNSW)
- [ ] Index statistics for query planner

#### Query Execution

- [ ] Cypher parser (basic patterns)
- [ ] Query planner (heuristic cost-based)
- [ ] Physical operators:
  - [ ] NodeByLabel
  - [ ] Filter
  - [ ] Expand
  - [ ] Project
  - [ ] OrderBy + Limit
  - [ ] Aggregate (COUNT, SUM, AVG, MIN, MAX)

#### API

- [ ] Complete REST endpoints
- [ ] Error handling and validation
- [ ] Query timeout support
- [ ] Bulk ingestion

#### Testing

- [ ] Unit tests (95%+ coverage)
- [ ] Integration tests
- [ ] Performance benchmarks
- [ ] Crash recovery tests

### [0.3.0] - V1 Advanced Features (Planned: Q1 2025)

- [ ] Property B-tree indexes
- [ ] Full-text search (Tantivy)
- [ ] Constraints (UNIQUE, NOT NULL)
- [ ] Query optimization (cost model)
- [ ] Bulk loader (bypass WAL)
- [ ] Prometheus metrics
- [ ] OpenAPI specification

### [0.4.0] - V2 Distributed (Planned: Q2 2025)

- [ ] Sharding architecture
- [ ] Raft consensus (openraft)
- [ ] Read replicas
- [ ] Distributed query coordinator
- [ ] Cluster management

---

## Notes

### Breaking Changes Policy

- Breaking changes only in major version bumps
- Deprecation warnings 2 minor versions before removal
- Migration guides provided for all breaking changes

### Security Updates

- Security patches released as PATCH versions
- Security advisories published on GitHub
- CVE tracking for production releases

### Performance Targets

Maintained across versions:

- Point reads: 100K+ ops/sec
- KNN queries: 10K+ ops/sec
- Pattern traversal: 1K-10K ops/sec
- 95%+ test coverage
- Zero known critical bugs

---

## Links

- **Repository**: https://github.com/hivellm/nexus
- **Documentation**: https://docs.nexus-db.io
- **Releases**: https://github.com/hivellm/nexus/releases
- **Issues**: https://github.com/hivellm/nexus/issues

