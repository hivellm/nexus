# Cypher List Type Converters Spec

## ADDED Requirements

### Requirement: List Type Coercion Functions

The system SHALL expose four list type-converter functions:
`toIntegerList(list)`, `toFloatList(list)`, `toStringList(list)`,
`toBooleanList(list)`. Each MUST return a LIST of the target type with
the same length as the input; elements that fail coercion MUST become
NULL rather than fail the query.

#### Scenario: Convert numeric strings to integers
Given the query `RETURN toIntegerList(["1", "2", "three", null])`
When the query is executed
Then the result SHALL be `[1, 2, null, null]`

#### Scenario: Convert heterogeneous list to floats
Given the query `RETURN toFloatList([1, "2.5", true, null])`
When the query is executed
Then the result SHALL be `[1.0, 2.5, null, null]`

#### Scenario: Convert anything to strings
Given the query `RETURN toStringList([1, 2.5, true, null])`
When the query is executed
Then the result SHALL be `["1", "2.5", "true", null]`

#### Scenario: Boolean coercion recognises canonical tokens
Given the query `RETURN toBooleanList([true, "false", 1, 0, "TRUE", "x"])`
When the query is executed
Then the result SHALL be `[true, false, true, false, true, null]`

### Requirement: NULL Input Returns NULL

When the list argument is NULL, converters SHALL return NULL (not an
empty list).

#### Scenario: NULL list
Given the query `RETURN toIntegerList(null)`
When the query is executed
Then the result SHALL be `null`

### Requirement: Non-List Input Raises Type Error

When the argument is neither a list nor NULL, converters MUST raise
`ERR_INVALID_ARG_TYPE(expected="LIST", got="<type>")`.

#### Scenario: Scalar argument rejected
Given the query `RETURN toIntegerList(42)`
When the query is executed
Then the server SHALL respond with HTTP 400
And the error code SHALL be `ERR_INVALID_ARG_TYPE`
