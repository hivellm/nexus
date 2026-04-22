# Byte Array Type Spec

## ADDED Requirements

### Requirement: `BYTES` Value Type

The system SHALL add a new scalar value type `BYTES` storing a
variable-length byte sequence up to 64 MiB per property.

#### Scenario: Round-trip a hash
Given a 32-byte SHA-256 digest `h`
When the client sets property `n.digest = $h` and reads it back
Then the returned value SHALL equal `h` byte-for-byte

#### Scenario: Size limit enforced
Given an attempt to store 128 MiB in a single property
When the write is submitted
Then the server SHALL respond with HTTP 400
And the error code SHALL be `ERR_BYTES_TOO_LARGE`

### Requirement: Wire Format

Responses SHALL encode BYTES as `{"_bytes": "<base64>"}`. Request
parameters SHALL accept the same shape, and also accept a plain
base64 STRING when the parameter's declared type is BYTES.

#### Scenario: Response shape
Given a node `(n {digest: <0x00, 0x01, 0xFF>})`
When `MATCH (n) RETURN n.digest` is executed
Then the JSON response row 0 column 0 SHALL equal `{"_bytes": "AAH/"}`

#### Scenario: Parameter round-trip
Given the parameter `{"digest": {"_bytes": "AAH/"}}`
When `CREATE (n {digest: $digest}) RETURN n.digest AS d` is executed
Then the returned `d` SHALL base64-encode to `"AAH/"`

### Requirement: `bytes.*` Scalar Functions

The system SHALL expose:

- `bytes(str: STRING) -> BYTES` (UTF-8 encode)
- `bytes.fromBase64(str: STRING) -> BYTES`
- `bytes.toBase64(b: BYTES) -> STRING`
- `bytes.toHex(b: BYTES) -> STRING`
- `bytes.length(b: BYTES) -> INTEGER`
- `bytes.slice(b: BYTES, start: INTEGER, len: INTEGER) -> BYTES`

#### Scenario: UTF-8 encode
Given the query `RETURN bytes.toHex(bytes("abc"))`
When the query is executed
Then the result SHALL equal `"616263"`

#### Scenario: Slice
Given a BYTES value `0x00 0x01 0x02 0x03 0x04`
When `RETURN bytes.toHex(bytes.slice($b, 1, 3))` is executed
Then the result SHALL equal `"010203"`

### Requirement: NULL Propagation

All `bytes.*` functions SHALL return NULL on NULL input.

#### Scenario: NULL in NULL out
Given the query `RETURN bytes.toHex(null)`
When the query is executed
Then the result SHALL be `null`
