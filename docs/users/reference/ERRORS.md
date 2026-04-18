---
title: Error Reference
module: reference
id: error-reference
order: 4
description: Error codes and messages
tags: [errors, reference, troubleshooting]
---

# Error Reference

Complete reference for error codes and messages in Nexus.

## Error Format

All errors follow this format:

```json
{
  "error": {
    "type": "ErrorType",
    "message": "Error description",
    "status_code": 400
  }
}
```

## Error Types

### SyntaxError

**Status Code**: 400

**Description**: Invalid Cypher syntax

**Example:**
```json
{
  "error": {
    "type": "SyntaxError",
    "message": "Invalid Cypher syntax at line 1, column 10",
    "status_code": 400
  }
}
```

**Common Causes:**
- Missing parentheses
- Invalid clause order
- Unclosed strings

### AuthenticationError

**Status Code**: 401

**Description**: Authentication failed

**Example:**
```json
{
  "error": {
    "type": "AuthenticationError",
    "message": "Invalid API key",
    "status_code": 401
  }
}
```

**Common Causes:**
- Missing API key
- Invalid API key
- Expired JWT token

### PermissionError

**Status Code**: 403

**Description**: Insufficient permissions

**Example:**
```json
{
  "error": {
    "type": "PermissionError",
    "message": "WRITE permission required",
    "status_code": 403
  }
}
```

**Common Causes:**
- Read-only user trying to write
- Insufficient permissions for operation

### NotFoundError

**Status Code**: 404

**Description**: Resource not found

**Example:**
```json
{
  "error": {
    "type": "NotFoundError",
    "message": "Database 'mydb' not found",
    "status_code": 404
  }
}
```

**Common Causes:**
- Database doesn't exist
- Node/relationship not found
- Index not found

### ValidationError

**Status Code**: 400

**Description**: Invalid input

**Example:**
```json
{
  "error": {
    "type": "ValidationError",
    "message": "Vector dimension mismatch: expected 128, got 64",
    "status_code": 400
  }
}
```

**Common Causes:**
- Invalid parameter format
- Type mismatch
- Constraint violation

### TimeoutError

**Status Code**: 408

**Description**: Query timeout

**Example:**
```json
{
  "error": {
    "type": "TimeoutError",
    "message": "Query exceeded timeout of 5000ms",
    "status_code": 408
  }
}
```

**Common Causes:**
- Query takes too long
- Complex patterns
- Large result sets

### InternalError

**Status Code**: 500

**Description**: Internal server error

**Example:**
```json
{
  "error": {
    "type": "InternalError",
    "message": "Internal server error",
    "status_code": 500
  }
}
```

**Common Causes:**
- Database corruption
- Memory issues
- System errors

## HTTP Status Codes

- **200**: Success
- **400**: Bad Request (syntax, validation errors)
- **401**: Unauthorized (authentication errors)
- **403**: Forbidden (permission errors)
- **404**: Not Found
- **408**: Request Timeout
- **429**: Too Many Requests (rate limiting)
- **500**: Internal Server Error
- **503**: Service Unavailable

## Related Topics

- [Troubleshooting](../operations/TROUBLESHOOTING.md) - Common problems
- [API Reference](../api/API_REFERENCE.md) - API documentation
- [Cypher Guide](../cypher/CYPHER.md) - Query language

