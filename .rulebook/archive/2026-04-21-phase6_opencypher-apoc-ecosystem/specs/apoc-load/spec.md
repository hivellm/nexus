# `apoc.load.*` Procedure Spec

## ADDED Requirements

### Requirement: `apoc.load.json(url)`

The procedure SHALL fetch JSON from `url` and yield one row per
top-level element. When the root is a JSON object, the procedure
SHALL yield a single row whose `value` column holds the object.

#### Scenario: JSON array
Given an HTTP endpoint that returns `[{"a": 1}, {"a": 2}]`
When `CALL apoc.load.json("http://host/data.json") YIELD value` is executed
Then two rows SHALL be returned
And row 1's `value` SHALL equal `{a: 1}`

#### Scenario: HTTP disabled
Given `apoc.http.enabled = false`
When `CALL apoc.load.json("http://host/anything")` is executed
Then the call SHALL fail with `ERR_HTTP_DISABLED`

### Requirement: `apoc.load.jsonParams(url, headers, payload)`

The procedure SHALL issue an HTTP request with the specified
`headers` map and JSON `payload`. The method is `POST` when `payload`
is non-null, `GET` otherwise.

#### Scenario: POST with payload
Given an HTTP echo endpoint
When `CALL apoc.load.jsonParams("http://host/echo", {X-Key: "abc"}, {q: "v"})`
  is executed
Then the request SHALL be a POST with header `X-Key: abc`
And the request body SHALL be `{"q": "v"}`

### Requirement: Allow-List Enforcement

When `apoc.http.allow` is non-empty, the procedure SHALL reject any
URL whose host+scheme does not match one of the allow-list regexes
with `ERR_HTTP_DISALLOWED`.

#### Scenario: Disallowed host
Given `apoc.http.allow = ["^https://api\\.trusted\\.com/.*"]`
When `CALL apoc.load.json("http://malicious.example/data")` is executed
Then the call SHALL fail with `ERR_HTTP_DISALLOWED`

### Requirement: `apoc.load.csv(url, config)`

The procedure SHALL parse CSV rows, yielding one row per line with
columns `lineNo:INTEGER, list:LIST<STRING>, map:MAP` (map available
when headers are present).

#### Scenario: CSV with headers
Given a CSV body `name,age\nAlice,30\nBob,25`
When `CALL apoc.load.csv(url, {header: true})` is executed
Then two rows SHALL be emitted
And row 1's `map` SHALL equal `{name: "Alice", age: "30"}`

### Requirement: File Loading Disabled by Default

When `apoc.import.file.enabled = false` (default), `apoc.load.*`
SHALL reject `file://` URLs with `ERR_IMPORT_DISABLED`.

#### Scenario: File URL rejected
Given the default configuration
When `CALL apoc.load.json("file:///tmp/data.json")` is executed
Then the call SHALL fail with `ERR_IMPORT_DISABLED`

### Requirement: Request Timeout

HTTP requests SHALL time out after `apoc.http.timeout_ms` (default
5000). Timeouts SHALL raise `ERR_HTTP_TIMEOUT`.

#### Scenario: Slow endpoint
Given `apoc.http.timeout_ms = 1000`
And an HTTP endpoint that responds in 3 seconds
When `CALL apoc.load.json(slow_url)` is executed
Then the call SHALL fail within 1.5 seconds
And the error code SHALL be `ERR_HTTP_TIMEOUT`
