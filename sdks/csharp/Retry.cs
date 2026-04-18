using System.Net;

namespace Nexus.SDK;

/// <summary>
/// Configuration for retry behavior.
/// </summary>
public class RetryConfig
{
    /// <summary>
    /// Maximum number of retry attempts (default: 3).
    /// </summary>
    public int MaxRetries { get; set; } = 3;

    /// <summary>
    /// Initial backoff duration (default: 100ms).
    /// </summary>
    public TimeSpan InitialBackoff { get; set; } = TimeSpan.FromMilliseconds(100);

    /// <summary>
    /// Maximum backoff duration (default: 10s).
    /// </summary>
    public TimeSpan MaxBackoff { get; set; } = TimeSpan.FromSeconds(10);

    /// <summary>
    /// Multiplier for exponential backoff (default: 2.0).
    /// </summary>
    public double BackoffMultiplier { get; set; } = 2.0;

    /// <summary>
    /// Whether to add jitter to backoff (default: true).
    /// </summary>
    public bool Jitter { get; set; } = true;

    /// <summary>
    /// HTTP status codes that should trigger a retry.
    /// </summary>
    public HashSet<HttpStatusCode> RetryableStatusCodes { get; set; } = new()
    {
        HttpStatusCode.RequestTimeout,        // 408
        HttpStatusCode.TooManyRequests,       // 429
        HttpStatusCode.InternalServerError,   // 500
        HttpStatusCode.BadGateway,            // 502
        HttpStatusCode.ServiceUnavailable,    // 503
        HttpStatusCode.GatewayTimeout         // 504
    };

    /// <summary>
    /// Creates a default retry configuration.
    /// </summary>
    public static RetryConfig Default => new();
}

/// <summary>
/// Extension methods for adding retry support to NexusClient.
/// </summary>
public static class RetryExtensions
{
    private static readonly Random Random = new();

    /// <summary>
    /// Creates a retryable client wrapper.
    /// </summary>
    public static RetryableNexusClient WithRetry(this NexusClient client, RetryConfig? config = null)
    {
        return new RetryableNexusClient(client, config ?? RetryConfig.Default);
    }

    internal static bool IsRetryableException(this RetryConfig config, Exception ex)
    {
        if (ex is NexusApiException apiEx)
        {
            return config.RetryableStatusCodes.Contains((HttpStatusCode)apiEx.StatusCode);
        }

        // Network errors and timeouts are retryable
        return ex is HttpRequestException or TaskCanceledException or OperationCanceledException;
    }

    internal static TimeSpan CalculateBackoff(this RetryConfig config, int attempt)
    {
        var backoff = config.InitialBackoff.TotalMilliseconds * Math.Pow(config.BackoffMultiplier, attempt);

        if (config.Jitter)
        {
            // Add ±25% jitter
            var jitterRange = backoff * 0.25;
            backoff = backoff - jitterRange + (Random.NextDouble() * jitterRange * 2);
        }

        var duration = TimeSpan.FromMilliseconds(backoff);
        return duration > config.MaxBackoff ? config.MaxBackoff : duration;
    }
}

/// <summary>
/// A wrapper around NexusClient that adds automatic retry functionality.
/// </summary>
public class RetryableNexusClient : IDisposable
{
    private readonly NexusClient _client;
    private readonly RetryConfig _retryConfig;
    private bool _disposed;

    /// <summary>
    /// Creates a new retryable client.
    /// </summary>
    public RetryableNexusClient(NexusClient client, RetryConfig? config = null)
    {
        _client = client ?? throw new ArgumentNullException(nameof(client));
        _retryConfig = config ?? RetryConfig.Default;
    }

    /// <summary>
    /// Creates a new retryable client with configuration.
    /// </summary>
    public RetryableNexusClient(NexusClientConfig clientConfig, RetryConfig? retryConfig = null)
    {
        _client = new NexusClient(clientConfig);
        _retryConfig = retryConfig ?? RetryConfig.Default;
    }

    /// <summary>
    /// Executes an operation with automatic retry.
    /// </summary>
    private async Task<T> ExecuteWithRetryAsync<T>(
        Func<CancellationToken, Task<T>> operation,
        CancellationToken cancellationToken = default)
    {
        Exception? lastException = null;

        for (var attempt = 0; attempt <= _retryConfig.MaxRetries; attempt++)
        {
            cancellationToken.ThrowIfCancellationRequested();

            try
            {
                return await operation(cancellationToken);
            }
            catch (Exception ex) when (_retryConfig.IsRetryableException(ex))
            {
                lastException = ex;

                if (attempt < _retryConfig.MaxRetries)
                {
                    var backoff = _retryConfig.CalculateBackoff(attempt);
                    await Task.Delay(backoff, cancellationToken);
                }
            }
        }

        throw lastException ?? new InvalidOperationException("Retry failed without exception");
    }

    /// <summary>
    /// Executes an operation with automatic retry (void return).
    /// </summary>
    private async Task ExecuteWithRetryAsync(
        Func<CancellationToken, Task> operation,
        CancellationToken cancellationToken = default)
    {
        await ExecuteWithRetryAsync(async ct =>
        {
            await operation(ct);
            return true;
        }, cancellationToken);
    }

    #region Health Check

    /// <summary>
    /// Checks if the server is reachable with automatic retry.
    /// </summary>
    public async Task PingAsync(CancellationToken cancellationToken = default)
    {
        await ExecuteWithRetryAsync(
            ct => _client.PingAsync(ct),
            cancellationToken);
    }

    #endregion

    #region Cypher Queries

    /// <summary>
    /// Executes a Cypher query with automatic retry.
    /// </summary>
    public async Task<QueryResult> ExecuteCypherAsync(
        string query,
        Dictionary<string, object?>? parameters = null,
        CancellationToken cancellationToken = default)
    {
        return await ExecuteWithRetryAsync(
            ct => _client.ExecuteCypherAsync(query, parameters, ct),
            cancellationToken);
    }

    #endregion

    #region Node Operations

    /// <summary>
    /// Creates a new node with automatic retry.
    /// </summary>
    public async Task<Node> CreateNodeAsync(
        List<string> labels,
        Dictionary<string, object?> properties,
        CancellationToken cancellationToken = default)
    {
        return await ExecuteWithRetryAsync(
            ct => _client.CreateNodeAsync(labels, properties, ct),
            cancellationToken);
    }

    /// <summary>
    /// Retrieves a node by ID with automatic retry.
    /// </summary>
    public async Task<Node> GetNodeAsync(
        string id,
        CancellationToken cancellationToken = default)
    {
        return await ExecuteWithRetryAsync(
            ct => _client.GetNodeAsync(id, ct),
            cancellationToken);
    }

    /// <summary>
    /// Updates a node with automatic retry.
    /// </summary>
    public async Task<Node> UpdateNodeAsync(
        string id,
        Dictionary<string, object?> properties,
        CancellationToken cancellationToken = default)
    {
        return await ExecuteWithRetryAsync(
            ct => _client.UpdateNodeAsync(id, properties, ct),
            cancellationToken);
    }

    /// <summary>
    /// Deletes a node with automatic retry.
    /// </summary>
    public async Task DeleteNodeAsync(
        string id,
        CancellationToken cancellationToken = default)
    {
        await ExecuteWithRetryAsync(
            ct => _client.DeleteNodeAsync(id, ct),
            cancellationToken);
    }

    #endregion

    #region Schema Management

    /// <summary>
    /// Retrieves all node labels with automatic retry.
    /// </summary>
    public async Task<List<string>> ListLabelsAsync(
        CancellationToken cancellationToken = default)
    {
        return await ExecuteWithRetryAsync(
            ct => _client.ListLabelsAsync(ct),
            cancellationToken);
    }

    /// <summary>
    /// Retrieves all relationship types with automatic retry.
    /// </summary>
    public async Task<List<string>> ListRelationshipTypesAsync(
        CancellationToken cancellationToken = default)
    {
        return await ExecuteWithRetryAsync(
            ct => _client.ListRelationshipTypesAsync(ct),
            cancellationToken);
    }

    /// <summary>
    /// Retrieves all indexes with automatic retry.
    /// </summary>
    public async Task<List<Index>> ListIndexesAsync(
        CancellationToken cancellationToken = default)
    {
        return await ExecuteWithRetryAsync(
            ct => _client.ListIndexesAsync(ct),
            cancellationToken);
    }

    #endregion

    #region IDisposable

    protected virtual void Dispose(bool disposing)
    {
        if (!_disposed)
        {
            if (disposing)
            {
                _client.Dispose();
            }
            _disposed = true;
        }
    }

    public void Dispose()
    {
        Dispose(true);
        GC.SuppressFinalize(this);
    }

    #endregion
}
