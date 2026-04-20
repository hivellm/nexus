# `db.index.fulltext.*` Procedure Spec

## ADDED Requirements

### Requirement: `createNodeIndex` / `createRelationshipIndex`

The system SHALL expose
`db.index.fulltext.createNodeIndex(name: STRING, labels: LIST<STRING>,
properties: LIST<STRING>, config: MAP = {})` and a symmetrical
relationship-scoped variant. The `config` MAP accepts keys
`analyzer`, `refresh_ms`, `top_k`.

#### Scenario: Create node index with default config
Given an empty database
When `CALL db.index.fulltext.createNodeIndex("movies", ["Movie"], ["title"])` is executed
Then the procedure SHALL return one row with `name = "movies"` and `state = "ONLINE"`
And the analyzer SHALL default to `"standard"`

#### Scenario: Unknown analyzer rejected
Given an empty database
When `CALL db.index.fulltext.createNodeIndex("x", ["X"], ["p"], {analyzer: "martian"})` is executed
Then the server SHALL respond with HTTP 400
And the error code SHALL be `ERR_INVALID_ARG_VALUE`

### Requirement: `queryNodes` Returns (node, score) Ranked by BM25

`db.index.fulltext.queryNodes(name: STRING, query: STRING)` SHALL
return streaming rows `(node: NODE, score: FLOAT)` ordered by `score`
descending. Ties SHALL be broken by node id ascending. Default
`top_k` is 100.

#### Scenario: Basic term query
Given an index `"movies"` with two documents titled `"Star Wars"` and
  `"Star Trek"`
When `CALL db.index.fulltext.queryNodes("movies", "wars")` is executed
Then exactly one row SHALL be returned
And that row's node SHALL be the `"Star Wars"` node

#### Scenario: Phrase query
Given documents `"A New Hope"`, `"Hope and Glory"`, `"A New Dawn"`
When `CALL db.index.fulltext.queryNodes("movies", "\"A New\"")` is executed
Then `"A New Hope"` and `"A New Dawn"` SHALL be returned
And `"Hope and Glory"` SHALL NOT be returned

#### Scenario: Fuzzy query
Given a document titled `"Star Wars"`
When `CALL db.index.fulltext.queryNodes("movies", "wrs~2")` is executed
Then the `"Star Wars"` node SHALL be returned

### Requirement: Query Syntax Errors Report a Parse Code

Malformed queries SHALL cause the procedure to fail with
`ERR_FTS_PARSE` and a message pointing at the problematic position.

#### Scenario: Unbalanced quote
Given any valid index
When `CALL db.index.fulltext.queryNodes("movies", "\"unterminated")` is executed
Then the error code SHALL be `ERR_FTS_PARSE`

### Requirement: Unknown Index Raises Error

Calling any query procedure with an index name that does not exist
SHALL raise `ERR_FTS_INDEX_NOT_FOUND(name)`.

#### Scenario: Missing index
Given no index named `"ghost"` exists
When `CALL db.index.fulltext.queryNodes("ghost", "anything")` is executed
Then the error code SHALL be `ERR_FTS_INDEX_NOT_FOUND`

### Requirement: `awaitEventuallyConsistentIndexRefresh`

The procedure
`db.index.fulltext.awaitEventuallyConsistentIndexRefresh()` SHALL
block until every currently-configured FTS index has completed one
refresh cycle after the call enters.

#### Scenario: Write then await then read
Given an empty index `"movies"`
When a transaction inserts a `Movie` node and commits
And the session calls `awaitEventuallyConsistentIndexRefresh()`
Then a subsequent `queryNodes("movies", <title>)` SHALL return the node
