# `dbms.*` Procedure Spec

## ADDED Requirements

### Requirement: `dbms.components()` Identifies Server

The system SHALL expose `dbms.components()` with columns:
`name:STRING, versions:LIST<STRING>, edition:STRING`.
The kernel component SHALL always be present in the result.

#### Scenario: Nexus identifies itself
When `CALL dbms.components()` is executed
Then one row SHALL have `name = "Nexus Kernel"`
And `versions` SHALL be a non-empty list containing the running server version
And `edition` SHALL be `"community"` or `"enterprise"`

### Requirement: `dbms.procedures()` Lists All Procedures

The procedure SHALL return one row per registered procedure with
columns: `name:STRING, signature:STRING, description:STRING,
mode:STRING, worksOnSystem:BOOLEAN`.

#### Scenario: Self-description includes itself
When `CALL dbms.procedures()` is executed
Then the result SHALL include a row where `name = "dbms.procedures"`
And that row's `mode` SHALL be `"DBMS"`

#### Scenario: Output includes system, GDS, APOC namespaces when registered
Given the server has system, GDS, and APOC procedure registries enabled
When `CALL dbms.procedures()` is executed
Then the result SHALL include procedures prefixed `db.`, `dbms.`, `gds.`, `apoc.`

### Requirement: `dbms.functions()` Lists All Functions

Columns: `name:STRING, signature:STRING, description:STRING,
aggregating:BOOLEAN`. Aggregation functions SHALL be reported with
`aggregating = true`.

#### Scenario: `count` is aggregating
When `CALL dbms.functions()` is executed
Then the row with `name = "count"` SHALL have `aggregating = true`

### Requirement: `dbms.listConfig(search)` Requires Admin

`dbms.listConfig(search: STRING)` SHALL return only configuration keys
matching the search substring. The procedure SHALL be callable only by
users with the `Admin` role.

#### Scenario: Unauthorised caller rejected
Given a user with only `Reader` role
When they call `CALL dbms.listConfig("")`
Then the server SHALL respond with HTTP 403
And the error code SHALL be `ERR_PERMISSION_DENIED`

#### Scenario: Substring filter
Given the server config contains keys
  `server.default_listen_address`, `server.default_advertised_address`,
  `browser.retain_connection_credentials`
When an admin calls `CALL dbms.listConfig("listen")`
Then the result SHALL contain exactly one row for
  `server.default_listen_address`

### Requirement: `dbms.info()` Reports Uptime

The procedure SHALL return a single row with columns: `id:STRING,
name:STRING, creationDate:DATETIME`.

#### Scenario: Fresh server
When `CALL dbms.info()` is executed within 5 seconds of server startup
Then `creationDate` SHALL be within 5 seconds of `datetime()`

### Requirement: `dbms.showCurrentUser()` Reflects Session

The procedure SHALL return the authenticated user's identity with
columns: `username:STRING, roles:LIST<STRING>, flags:LIST<STRING>`.

#### Scenario: Authenticated session
Given a session authenticated as user `"alice"` with roles `["Editor"]`
When `CALL dbms.showCurrentUser()` is executed
Then the returned row SHALL have `username = "alice"` and
  `roles = ["Editor"]`
