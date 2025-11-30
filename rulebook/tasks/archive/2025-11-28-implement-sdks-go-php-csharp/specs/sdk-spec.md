# SDK Specifications for Go, PHP, and C#

## Common API Interface

All SDKs MUST implement the following core operations with language-appropriate APIs:

### Client Initialization

#### Go
```go
package nexus

import (
    "context"
    "time"
)

type Client struct {
    baseURL    string
    apiKey     string
    httpClient *http.Client
}

func NewClient(baseURL string, options ...Option) (*Client, error) {
    // Implementation
}

type Option func(*Client)

func WithAPIKey(apiKey string) Option {
    return func(c *Client) {
        c.apiKey = apiKey
    }
}

func WithTimeout(timeout time.Duration) Option {
    return func(c *Client) {
        c.httpClient.Timeout = timeout
    }
}
```

#### C#
```csharp
using Nexus.Client;

public class NexusClient
{
    private readonly string _baseUrl;
    private readonly string _apiKey;
    private readonly HttpClient _httpClient;

    public NexusClient(string baseUrl, NexusClientOptions? options = null)
    {
        // Implementation
    }
}

public class NexusClientOptions
{
    public string? ApiKey { get; set; }
    public TimeSpan? Timeout { get; set; }
    public HttpClient? HttpClient { get; set; }
}
```

#### PHP
```php
<?php

namespace Nexus;

class Client
{
    private string $baseUrl;
    private ?string $apiKey;
    private HttpClientInterface $httpClient;

    public function __construct(string $baseUrl, array $options = [])
    {
        // Implementation
    }
}
```

### Cypher Query Execution

#### Go
```go
type CypherResult struct {
    Columns []string
    Rows    [][]interface{}
    ExecutionTime int64 // milliseconds
}

func (c *Client) ExecuteCypher(ctx context.Context, query string, params map[string]interface{}) (*CypherResult, error) {
    // Implementation
}
```

#### C#
```csharp
public class CypherResult
{
    public IList<string> Columns { get; set; }
    public IList<IList<object>> Rows { get; set; }
    public long ExecutionTime { get; set; } // milliseconds
}

public async Task<CypherResult> ExecuteCypherAsync(
    string query,
    IDictionary<string, object>? parameters = null,
    CancellationToken cancellationToken = default)
{
    // Implementation
}
```

#### PHP
```php
class CypherResult
{
    public array $columns;
    public array $rows;
    public int $executionTime; // milliseconds
}

public function executeCypher(
    string $query,
    array $parameters = []
): CypherResult
{
    // Implementation
}
```

### Node Operations

#### Go
```go
type Node struct {
    ID         uint64
    Labels     []string
    Properties map[string]interface{}
}

func (c *Client) CreateNode(ctx context.Context, labels []string, properties map[string]interface{}) (*Node, error) {
    // Implementation
}

func (c *Client) GetNode(ctx context.Context, id uint64) (*Node, error) {
    // Implementation
}

func (c *Client) UpdateNode(ctx context.Context, id uint64, properties map[string]interface{}) error {
    // Implementation
}

func (c *Client) DeleteNode(ctx context.Context, id uint64) error {
    // Implementation
}
```

#### C#
```csharp
public class Node
{
    public ulong Id { get; set; }
    public IList<string> Labels { get; set; }
    public IDictionary<string, object> Properties { get; set; }
}

public async Task<Node> CreateNodeAsync(
    IList<string> labels,
    IDictionary<string, object> properties,
    CancellationToken cancellationToken = default)
{
    // Implementation
}

public async Task<Node> GetNodeAsync(
    ulong id,
    CancellationToken cancellationToken = default)
{
    // Implementation
}

public async Task UpdateNodeAsync(
    ulong id,
    IDictionary<string, object> properties,
    CancellationToken cancellationToken = default)
{
    // Implementation
}

public async Task DeleteNodeAsync(
    ulong id,
    CancellationToken cancellationToken = default)
{
    // Implementation
}
```

#### PHP
```php
class Node
{
    public int $id;
    public array $labels;
    public array $properties;
}

public function createNode(array $labels, array $properties): Node
{
    // Implementation
}

public function getNode(int $id): Node
{
    // Implementation
}

public function updateNode(int $id, array $properties): void
{
    // Implementation
}

public function deleteNode(int $id): void
{
    // Implementation
}
```

### Relationship Operations

#### Go
```go
type Relationship struct {
    ID         uint64
    Type       string
    FromNode   uint64
    ToNode     uint64
    Properties map[string]interface{}
}

func (c *Client) CreateRelationship(ctx context.Context, fromNode, toNode uint64, relType string, properties map[string]interface{}) (*Relationship, error) {
    // Implementation
}
```

#### C#
```csharp
public class Relationship
{
    public ulong Id { get; set; }
    public string Type { get; set; }
    public ulong FromNode { get; set; }
    public ulong ToNode { get; set; }
    public IDictionary<string, object> Properties { get; set; }
}

public async Task<Relationship> CreateRelationshipAsync(
    ulong fromNode,
    ulong toNode,
    string type,
    IDictionary<string, object>? properties = null,
    CancellationToken cancellationToken = default)
{
    // Implementation
}
```

#### PHP
```php
class Relationship
{
    public int $id;
    public string $type;
    public int $fromNode;
    public int $toNode;
    public array $properties;
}

public function createRelationship(
    int $fromNode,
    int $toNode,
    string $type,
    array $properties = []
): Relationship
{
    // Implementation
}
```

### Authentication

#### Go
```go
type AuthConfig struct {
    APIKey  string
    Username string
    Password string
}

func WithAuth(config AuthConfig) Option {
    // Implementation
}
```

#### C#
```csharp
public class NexusClientOptions
{
    public string? ApiKey { get; set; }
    public string? Username { get; set; }
    public string? Password { get; set; }
}
```

#### PHP
```php
class ClientOptions
{
    public ?string $apiKey = null;
    public ?string $username = null;
    public ?string $password = null;
}
```

### Error Handling

#### Go
```go
type Error struct {
    Type    string
    Message string
    Code    int
}

func (e *Error) Error() string {
    return fmt.Sprintf("%s: %s", e.Type, e.Message)
}

var (
    ErrConnection    = &Error{Type: "ConnectionError", Message: "Failed to connect"}
    ErrAuthentication = &Error{Type: "AuthenticationError", Message: "Invalid credentials"}
    ErrQuery         = &Error{Type: "QueryError", Message: "Query execution failed"}
)
```

#### C#
```csharp
public class NexusException : Exception
{
    public string ErrorType { get; }
    public int? StatusCode { get; }

    public NexusException(string message, string errorType, int? statusCode = null)
        : base(message)
    {
        ErrorType = errorType;
        StatusCode = statusCode;
    }
}

public class NexusConnectionException : NexusException
{
    public NexusConnectionException(string message)
        : base(message, "ConnectionError")
    {
    }
}

public class NexusAuthenticationException : NexusException
{
    public NexusAuthenticationException(string message)
        : base(message, "AuthenticationError", 401)
    {
    }
}
```

#### PHP
```php
class NexusException extends Exception
{
    public string $errorType;
    public ?int $statusCode;

    public function __construct(
        string $message,
        string $errorType,
        ?int $statusCode = null
    ) {
        parent::__construct($message);
        $this->errorType = $errorType;
        $this->statusCode = $statusCode;
    }
}

class ConnectionException extends NexusException
{
    public function __construct(string $message)
    {
        parent::__construct($message, 'ConnectionError');
    }
}

class AuthenticationException extends NexusException
{
    public function __construct(string $message)
    {
        parent::__construct($message, 'AuthenticationError', 401);
    }
}
```

### Retry Logic

#### Go
```go
func (c *Client) executeWithRetry(ctx context.Context, req *http.Request, maxRetries int) (*http.Response, error) {
    var lastErr error
    for i := 0; i < maxRetries; i++ {
        resp, err := c.httpClient.Do(req.WithContext(ctx))
        if err == nil && resp.StatusCode < 500 {
            return resp, nil
        }
        lastErr = err
        time.Sleep(time.Duration(i+1) * 100 * time.Millisecond) // exponential backoff
    }
    return nil, lastErr
}
```

#### C#
```csharp
using Polly;

var retryPolicy = Policy
    .Handle<HttpRequestException>()
    .OrResult<HttpResponseMessage>(r => (int)r.StatusCode >= 500)
    .WaitAndRetryAsync(
        retryCount: 3,
        sleepDurationProvider: retryAttempt => TimeSpan.FromMilliseconds(100 * Math.Pow(2, retryAttempt)),
        onRetry: (outcome, timespan, retryCount, context) => {
            // Log retry
        });
```

#### PHP
```php
private function executeWithRetry(callable $request, int $maxRetries = 3): ResponseInterface
{
    $lastException = null;
    for ($i = 0; $i < $maxRetries; $i++) {
        try {
            $response = $request();
            if ($response->getStatusCode() < 500) {
                return $response;
            }
        } catch (RequestException $e) {
            $lastException = $e;
        }
        usleep(100000 * ($i + 1)); // exponential backoff
    }
    throw $lastException ?? new ConnectionException('Request failed');
}
```

## Testing Requirements

### Unit Tests

- Test all client methods
- Test error handling
- Test retry logic
- Test authentication
- ≥90% code coverage

### Integration Tests

- Test with real Nexus server
- Test all operations end-to-end
- Test error scenarios
- Test concurrent operations

### Language-Specific Tests

#### Go
- Test context cancellation
- Test goroutine safety
- Test error wrapping

#### C#
- Test async operations
- Test cancellation tokens
- Test exception handling

#### PHP
- Test PSR compliance
- Test Composer autoloading
- Test error handling

