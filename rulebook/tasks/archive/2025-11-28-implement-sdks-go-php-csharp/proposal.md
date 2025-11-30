# Proposal: Go, PHP, and C# SDKs for Nexus

## Why

Go, PHP, and C# are widely-used programming languages in enterprise and web development. Creating official SDKs for these languages will significantly expand Nexus adoption across different technology stacks and enterprise environments. Go is essential for cloud-native and microservices applications, PHP powers a large portion of web applications, and C# is critical for .NET ecosystem and enterprise applications. These SDKs will enable developers in these ecosystems to easily integrate Nexus into their applications, opening new markets and use cases.

## Purpose

Create official Software Development Kits (SDKs) for Nexus graph database in Go, PHP, and C# programming languages to enable developers to easily integrate Nexus into their applications regardless of their technology stack. This will provide type-safe interfaces, better IDE support, comprehensive documentation, and consistent API usage patterns for these important language ecosystems.

## Context

Currently, Nexus provides REST APIs that can be consumed by any HTTP client, but developers must manually construct requests and handle responses. By providing official SDKs in Go, PHP, and C#, we can:

- Reduce integration time and complexity
- Provide type-safe interfaces
- Offer better IDE support and autocomplete
- Include comprehensive documentation and examples
- Ensure consistent API usage patterns
- Handle connection pooling, retries, and error handling automatically

## Scope

This proposal covers SDKs for the following languages:

1. **Go** - For cloud-native and microservices applications
2. **PHP** - For web development and content management systems
3. **C#** - For .NET ecosystem and enterprise applications

## Requirements

### Core SDK Features

Each SDK MUST provide:

1. **Client Initialization**
   - Connection configuration (host, port, authentication)
   - Connection pooling and management
   - Timeout configuration

2. **Cypher Query Execution**
   - Execute queries with parameters
   - Handle result sets with proper type mapping
   - Transaction support (begin, commit, rollback)

3. **Data Operations**
   - Create, read, update, delete nodes
   - Create, read, update, delete relationships
   - Batch operations

4. **Schema Management**
   - Create/list labels
   - Create/list relationship types
   - Index management

5. **Query Builder** (where applicable)
   - Fluent API for constructing Cypher queries
   - Type-safe query building

6. **Error Handling**
   - Proper exception/error types
   - Retry logic for transient failures
   - Connection error handling

7. **Authentication**
   - API key support
   - User/password authentication
   - Token management

8. **Performance Features**
   - Query statistics
   - Slow query analysis
   - Plan cache management

9. **Graph Algorithms**
   - Pathfinding algorithms
   - Centrality algorithms
   - Community detection

10. **Documentation**
    - API reference documentation
    - Getting started guides
    - Code examples
    - Best practices

### Language-Specific Requirements

#### Go SDK
- Support Go 1.19+
- Use `net/http` standard library
- Context support for cancellation
- Proper error wrapping
- Include `go.mod` and `go.sum`
- Follow Go conventions and idioms

#### PHP SDK
- Support PHP 8.1+
- Use PSR-18 HTTP client interface
- Support Composer package management
- Follow PSR standards (PSR-1, PSR-4, PSR-12)
- Support both sync and async operations (ReactPHP)
- Type hints and PHPDoc comments

#### C# SDK
- Support .NET 6.0+
- Use `HttpClient` for HTTP requests
- Async/await support
- XML documentation comments
- Target both .NET Framework and .NET Core
- Follow C# coding conventions

## Implementation Strategy

### Phase 1: Go SDK
- Start with Go as it's simpler and has good HTTP support
- Establish patterns for error handling and context
- Create comprehensive test suite

### Phase 2: C# SDK
- Leverage .NET's strong typing and async support
- Focus on enterprise use cases
- Ensure .NET Framework compatibility

### Phase 3: PHP SDK
- Focus on web development use cases
- Support both sync and async operations
- Ensure Composer compatibility

## Success Criteria

- Each SDK has ≥90% test coverage
- Each SDK has comprehensive documentation
- Each SDK includes at least 5 example projects
- Each SDK supports all core Nexus features

## Dependencies

- Stable REST API (already available)
- OpenAPI specification (already available)
- Authentication system (already implemented)
- Performance monitoring endpoints (already implemented)

## Future Enhancements

- GraphQL support
- Reactive/streaming APIs
- Code generation from OpenAPI spec
- SDK-specific optimizations (connection pooling, caching)
- Language-specific query builders
- ORM-like abstractions
