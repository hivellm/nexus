using System.Net.Http.Headers;
using System.Net.Http.Json;
using System.Text;
using System.Text.Json;

namespace Nexus.SDK;

/// <summary>
/// Client for interacting with Nexus graph database.
/// </summary>
public class NexusClient : IDisposable
{
    private readonly HttpClient _httpClient;
    private readonly string _baseUrl;
    private readonly string? _apiKey;
    private string? _token;
    private bool _disposed;

    /// <summary>
    /// Creates a new Nexus client with the specified configuration.
    /// </summary>
    /// <param name="config">Client configuration.</param>
    public NexusClient(NexusClientConfig config)
    {
        _baseUrl = config.BaseUrl.TrimEnd('/');
        _apiKey = config.ApiKey;

        _httpClient = new HttpClient
        {
            BaseAddress = new Uri(_baseUrl),
            Timeout = config.Timeout
        };

        _httpClient.DefaultRequestHeaders.Accept.Add(
            new MediaTypeWithQualityHeaderValue("application/json"));
    }

    /// <summary>
    /// Sets the bearer token for authentication.
    /// </summary>
    /// <param name="token">Bearer token.</param>
    public void SetToken(string token)
    {
        _token = token;
    }

    #region Health Check

    /// <summary>
    /// Checks if the server is reachable.
    /// </summary>
    /// <param name="cancellationToken">Cancellation token.</param>
    public async Task PingAsync(CancellationToken cancellationToken = default)
    {
        await DoRequestAsync(HttpMethod.Get, "/health", null, cancellationToken);
    }

    #endregion

    #region Cypher Queries

    /// <summary>
    /// Executes a Cypher query and returns the results.
    /// </summary>
    /// <param name="query">Cypher query string.</param>
    /// <param name="parameters">Query parameters (optional).</param>
    /// <param name="cancellationToken">Cancellation token.</param>
    /// <returns>Query result.</returns>
    public async Task<QueryResult> ExecuteCypherAsync(
        string query,
        Dictionary<string, object?>? parameters = null,
        CancellationToken cancellationToken = default)
    {
        var requestBody = new Dictionary<string, object?>
        {
            ["query"] = query
        };

        if (parameters != null)
        {
            requestBody["parameters"] = parameters;
        }

        var response = await DoRequestAsync(
            HttpMethod.Post, "/cypher", requestBody, cancellationToken);

        return await response.Content.ReadFromJsonAsync<QueryResult>(
            cancellationToken: cancellationToken)
            ?? throw new NexusException("Failed to deserialize query result");
    }

    #endregion

    #region Node Operations

    /// <summary>
    /// Creates a new node with the given labels and properties.
    /// </summary>
    /// <param name="labels">Node labels.</param>
    /// <param name="properties">Node properties.</param>
    /// <param name="cancellationToken">Cancellation token.</param>
    /// <returns>Created node.</returns>
    public async Task<Node> CreateNodeAsync(
        List<string> labels,
        Dictionary<string, object?> properties,
        CancellationToken cancellationToken = default)
    {
        var requestBody = new
        {
            labels,
            properties
        };

        var response = await DoRequestAsync(
            HttpMethod.Post, "/nodes", requestBody, cancellationToken);

        return await response.Content.ReadFromJsonAsync<Node>(
            cancellationToken: cancellationToken)
            ?? throw new NexusException("Failed to deserialize node");
    }

    /// <summary>
    /// Retrieves a node by its ID.
    /// </summary>
    /// <param name="id">Node ID.</param>
    /// <param name="cancellationToken">Cancellation token.</param>
    /// <returns>Node.</returns>
    public async Task<Node> GetNodeAsync(
        string id,
        CancellationToken cancellationToken = default)
    {
        var response = await DoRequestAsync(
            HttpMethod.Get, $"/nodes/{Uri.EscapeDataString(id)}", null, cancellationToken);

        return await response.Content.ReadFromJsonAsync<Node>(
            cancellationToken: cancellationToken)
            ?? throw new NexusException("Failed to deserialize node");
    }

    /// <summary>
    /// Updates a node's properties.
    /// </summary>
    /// <param name="id">Node ID.</param>
    /// <param name="properties">New properties.</param>
    /// <param name="cancellationToken">Cancellation token.</param>
    /// <returns>Updated node.</returns>
    public async Task<Node> UpdateNodeAsync(
        string id,
        Dictionary<string, object?> properties,
        CancellationToken cancellationToken = default)
    {
        var requestBody = new { properties };

        var response = await DoRequestAsync(
            HttpMethod.Put, $"/nodes/{Uri.EscapeDataString(id)}", requestBody, cancellationToken);

        return await response.Content.ReadFromJsonAsync<Node>(
            cancellationToken: cancellationToken)
            ?? throw new NexusException("Failed to deserialize node");
    }

    /// <summary>
    /// Deletes a node by its ID.
    /// </summary>
    /// <param name="id">Node ID.</param>
    /// <param name="cancellationToken">Cancellation token.</param>
    public async Task DeleteNodeAsync(
        string id,
        CancellationToken cancellationToken = default)
    {
        await DoRequestAsync(
            HttpMethod.Delete, $"/nodes/{Uri.EscapeDataString(id)}", null, cancellationToken);
    }

    /// <summary>
    /// Batch creates multiple nodes.
    /// </summary>
    /// <param name="nodes">Nodes to create.</param>
    /// <param name="cancellationToken">Cancellation token.</param>
    /// <returns>Created nodes.</returns>
    public async Task<List<Node>> BatchCreateNodesAsync(
        List<NodeInput> nodes,
        CancellationToken cancellationToken = default)
    {
        var requestBody = new BatchNodesRequest { Nodes = nodes };

        var response = await DoRequestAsync(
            HttpMethod.Post, "/batch/nodes", requestBody, cancellationToken);

        return await response.Content.ReadFromJsonAsync<List<Node>>(
            cancellationToken: cancellationToken)
            ?? throw new NexusException("Failed to deserialize nodes");
    }

    #endregion

    #region Relationship Operations

    /// <summary>
    /// Creates a new relationship between two nodes.
    /// </summary>
    /// <param name="startNode">Start node ID.</param>
    /// <param name="endNode">End node ID.</param>
    /// <param name="type">Relationship type.</param>
    /// <param name="properties">Relationship properties.</param>
    /// <param name="cancellationToken">Cancellation token.</param>
    /// <returns>Created relationship.</returns>
    public async Task<Relationship> CreateRelationshipAsync(
        string startNode,
        string endNode,
        string type,
        Dictionary<string, object?> properties,
        CancellationToken cancellationToken = default)
    {
        var requestBody = new
        {
            start_node = startNode,
            end_node = endNode,
            type,
            properties
        };

        var response = await DoRequestAsync(
            HttpMethod.Post, "/relationships", requestBody, cancellationToken);

        return await response.Content.ReadFromJsonAsync<Relationship>(
            cancellationToken: cancellationToken)
            ?? throw new NexusException("Failed to deserialize relationship");
    }

    /// <summary>
    /// Retrieves a relationship by its ID.
    /// </summary>
    /// <param name="id">Relationship ID.</param>
    /// <param name="cancellationToken">Cancellation token.</param>
    /// <returns>Relationship.</returns>
    public async Task<Relationship> GetRelationshipAsync(
        string id,
        CancellationToken cancellationToken = default)
    {
        var response = await DoRequestAsync(
            HttpMethod.Get, $"/relationships/{Uri.EscapeDataString(id)}", null, cancellationToken);

        return await response.Content.ReadFromJsonAsync<Relationship>(
            cancellationToken: cancellationToken)
            ?? throw new NexusException("Failed to deserialize relationship");
    }

    /// <summary>
    /// Deletes a relationship by its ID.
    /// </summary>
    /// <param name="id">Relationship ID.</param>
    /// <param name="cancellationToken">Cancellation token.</param>
    public async Task DeleteRelationshipAsync(
        string id,
        CancellationToken cancellationToken = default)
    {
        await DoRequestAsync(
            HttpMethod.Delete, $"/relationships/{Uri.EscapeDataString(id)}", null, cancellationToken);
    }

    /// <summary>
    /// Batch creates multiple relationships.
    /// </summary>
    /// <param name="relationships">Relationships to create.</param>
    /// <param name="cancellationToken">Cancellation token.</param>
    /// <returns>Created relationships.</returns>
    public async Task<List<Relationship>> BatchCreateRelationshipsAsync(
        List<RelationshipInput> relationships,
        CancellationToken cancellationToken = default)
    {
        var requestBody = new BatchRelationshipsRequest { Relationships = relationships };

        var response = await DoRequestAsync(
            HttpMethod.Post, "/batch/relationships", requestBody, cancellationToken);

        return await response.Content.ReadFromJsonAsync<List<Relationship>>(
            cancellationToken: cancellationToken)
            ?? throw new NexusException("Failed to deserialize relationships");
    }

    #endregion

    #region Schema Management

    /// <summary>
    /// Retrieves all node labels in the database.
    /// </summary>
    /// <param name="cancellationToken">Cancellation token.</param>
    /// <returns>List of labels.</returns>
    public async Task<List<string>> ListLabelsAsync(
        CancellationToken cancellationToken = default)
    {
        var response = await DoRequestAsync(
            HttpMethod.Get, "/schema/labels", null, cancellationToken);

        var result = await response.Content.ReadFromJsonAsync<Dictionary<string, List<string>>>(
            cancellationToken: cancellationToken);

        return result?["labels"] ?? new List<string>();
    }

    /// <summary>
    /// Retrieves all relationship types in the database.
    /// </summary>
    /// <param name="cancellationToken">Cancellation token.</param>
    /// <returns>List of relationship types.</returns>
    public async Task<List<string>> ListRelationshipTypesAsync(
        CancellationToken cancellationToken = default)
    {
        var response = await DoRequestAsync(
            HttpMethod.Get, "/schema/relationship-types", null, cancellationToken);

        var result = await response.Content.ReadFromJsonAsync<Dictionary<string, List<string>>>(
            cancellationToken: cancellationToken);

        return result?["types"] ?? new List<string>();
    }

    /// <summary>
    /// Creates a new index on node properties.
    /// </summary>
    /// <param name="name">Index name.</param>
    /// <param name="label">Node label.</param>
    /// <param name="properties">Properties to index.</param>
    /// <param name="cancellationToken">Cancellation token.</param>
    public async Task CreateIndexAsync(
        string name,
        string label,
        List<string> properties,
        CancellationToken cancellationToken = default)
    {
        var requestBody = new
        {
            name,
            label,
            properties
        };

        await DoRequestAsync(
            HttpMethod.Post, "/schema/indexes", requestBody, cancellationToken);
    }

    /// <summary>
    /// Retrieves all indexes in the database.
    /// </summary>
    /// <param name="cancellationToken">Cancellation token.</param>
    /// <returns>List of indexes.</returns>
    public async Task<List<Index>> ListIndexesAsync(
        CancellationToken cancellationToken = default)
    {
        var response = await DoRequestAsync(
            HttpMethod.Get, "/schema/indexes", null, cancellationToken);

        var result = await response.Content.ReadFromJsonAsync<Dictionary<string, List<Index>>>(
            cancellationToken: cancellationToken);

        return result?["indexes"] ?? new List<Index>();
    }

    /// <summary>
    /// Deletes an index by name.
    /// </summary>
    /// <param name="name">Index name.</param>
    /// <param name="cancellationToken">Cancellation token.</param>
    public async Task DeleteIndexAsync(
        string name,
        CancellationToken cancellationToken = default)
    {
        await DoRequestAsync(
            HttpMethod.Delete, $"/schema/indexes/{Uri.EscapeDataString(name)}", null, cancellationToken);
    }

    #endregion

    #region Transactions

    /// <summary>
    /// Begins a new transaction.
    /// </summary>
    /// <param name="cancellationToken">Cancellation token.</param>
    /// <returns>Transaction object.</returns>
    public async Task<NexusTransaction> BeginTransactionAsync(
        CancellationToken cancellationToken = default)
    {
        var response = await DoRequestAsync(
            HttpMethod.Post, "/transaction/begin", null, cancellationToken);

        var result = await response.Content.ReadFromJsonAsync<Dictionary<string, string>>(
            cancellationToken: cancellationToken);

        var transactionId = result?["transaction_id"]
            ?? throw new NexusTransactionException("Failed to get transaction ID");

        return new NexusTransaction(this, transactionId);
    }

    internal async Task<QueryResult> ExecuteInTransactionAsync(
        string transactionId,
        string query,
        Dictionary<string, object?>? parameters,
        CancellationToken cancellationToken)
    {
        var requestBody = new Dictionary<string, object?>
        {
            ["query"] = query,
            ["transaction_id"] = transactionId
        };

        if (parameters != null)
        {
            requestBody["parameters"] = parameters;
        }

        var response = await DoRequestAsync(
            HttpMethod.Post, "/transaction/execute", requestBody, cancellationToken);

        return await response.Content.ReadFromJsonAsync<QueryResult>(
            cancellationToken: cancellationToken)
            ?? throw new NexusException("Failed to deserialize query result");
    }

    internal async Task CommitTransactionAsync(
        string transactionId,
        CancellationToken cancellationToken)
    {
        var requestBody = new { transaction_id = transactionId };

        await DoRequestAsync(
            HttpMethod.Post, "/transaction/commit", requestBody, cancellationToken);
    }

    internal async Task RollbackTransactionAsync(
        string transactionId,
        CancellationToken cancellationToken)
    {
        var requestBody = new { transaction_id = transactionId };

        await DoRequestAsync(
            HttpMethod.Post, "/transaction/rollback", requestBody, cancellationToken);
    }

    #endregion

    #region HTTP Helpers

    private async Task<HttpResponseMessage> DoRequestAsync(
        HttpMethod method,
        string path,
        object? body,
        CancellationToken cancellationToken)
    {
        using var request = new HttpRequestMessage(method, path);

        // Add authentication
        if (!string.IsNullOrEmpty(_apiKey))
        {
            request.Headers.Add("X-API-Key", _apiKey);
        }
        else if (!string.IsNullOrEmpty(_token))
        {
            request.Headers.Authorization = new AuthenticationHeaderValue("Bearer", _token);
        }

        // Add body
        if (body != null)
        {
            var json = JsonSerializer.Serialize(body);
            request.Content = new StringContent(json, Encoding.UTF8, "application/json");
        }

        var response = await _httpClient.SendAsync(request, cancellationToken);

        if (!response.IsSuccessStatusCode)
        {
            var errorBody = await response.Content.ReadAsStringAsync(cancellationToken);
            throw new NexusApiException((int)response.StatusCode, errorBody);
        }

        return response;
    }

    #endregion

    #region IDisposable

    protected virtual void Dispose(bool disposing)
    {
        if (!_disposed)
        {
            if (disposing)
            {
                _httpClient.Dispose();
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

/// <summary>
/// Represents a database transaction.
/// </summary>
public class NexusTransaction : IDisposable
{
    private readonly NexusClient _client;
    private readonly string _transactionId;
    private bool _disposed;
    private bool _completed;

    internal NexusTransaction(NexusClient client, string transactionId)
    {
        _client = client;
        _transactionId = transactionId;
    }

    /// <summary>
    /// Executes a Cypher query within the transaction.
    /// </summary>
    /// <param name="query">Cypher query string.</param>
    /// <param name="parameters">Query parameters (optional).</param>
    /// <param name="cancellationToken">Cancellation token.</param>
    /// <returns>Query result.</returns>
    public async Task<QueryResult> ExecuteCypherAsync(
        string query,
        Dictionary<string, object?>? parameters = null,
        CancellationToken cancellationToken = default)
    {
        if (_completed)
        {
            throw new NexusTransactionException("Transaction has already been completed");
        }

        return await _client.ExecuteInTransactionAsync(
            _transactionId, query, parameters, cancellationToken);
    }

    /// <summary>
    /// Commits the transaction.
    /// </summary>
    /// <param name="cancellationToken">Cancellation token.</param>
    public async Task CommitAsync(CancellationToken cancellationToken = default)
    {
        if (_completed)
        {
            throw new NexusTransactionException("Transaction has already been completed");
        }

        await _client.CommitTransactionAsync(_transactionId, cancellationToken);
        _completed = true;
    }

    /// <summary>
    /// Rolls back the transaction.
    /// </summary>
    /// <param name="cancellationToken">Cancellation token.</param>
    public async Task RollbackAsync(CancellationToken cancellationToken = default)
    {
        if (_completed)
        {
            throw new NexusTransactionException("Transaction has already been completed");
        }

        await _client.RollbackTransactionAsync(_transactionId, cancellationToken);
        _completed = true;
    }

    public void Dispose()
    {
        if (!_disposed && !_completed)
        {
            // Auto-rollback if not committed
            try
            {
                RollbackAsync().Wait();
            }
            catch
            {
                // Ignore errors during auto-rollback
            }
        }
        _disposed = true;
    }
}
