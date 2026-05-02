using System.Net.Http.Headers;
using System.Net.Http.Json;
using System.Text;
using System.Text.Json;
using Nexus.SDK.Transports;

namespace Nexus.SDK;

/// <summary>
/// Client for interacting with Nexus graph database.
///
/// Defaults to the native binary RPC transport on
/// <c>nexus://127.0.0.1:15475</c>. Callers can opt down to HTTP with
/// <see cref="NexusClientConfig.Transport"/> set to
/// <see cref="TransportMode.Http"/> or by passing an <c>http://</c>
/// URL as <see cref="NexusClientConfig.BaseUrl"/>.
/// </summary>
public class NexusClient : IDisposable, IAsyncDisposable
{
    private readonly HttpClient _httpClient;
    private readonly string _baseUrl;
    private readonly string? _apiKey;
    private string? _token;
    private bool _disposed;

    private readonly ITransport _transport;
    private readonly Endpoint _endpoint;
    private readonly TransportMode _mode;

    /// <summary>
    /// Creates a new Nexus client with the specified configuration.
    /// </summary>
    /// <param name="config">Client configuration.</param>
    public NexusClient(NexusClientConfig config)
    {
        var built = TransportFactory.Build(new TransportBuildOptions
        {
            BaseUrl = config.BaseUrl,
            Transport = config.Transport,
            RpcPort = config.RpcPort,
            Resp3Port = config.Resp3Port,
            Timeout = config.Timeout,
        }, new Credentials
        {
            ApiKey = config.ApiKey,
            Username = config.Username,
            Password = config.Password,
        });
        _transport = built.Transport;
        _endpoint = built.Endpoint;
        _mode = built.Mode;

        _baseUrl = _endpoint.AsHttpUrl().TrimEnd('/');
        _apiKey = config.ApiKey;

        _httpClient = new HttpClient
        {
            BaseAddress = new Uri(_baseUrl),
            Timeout = config.Timeout
        };

        _httpClient.DefaultRequestHeaders.Accept.Add(
            new MediaTypeWithQualityHeaderValue("application/json"));
    }

    /// <summary>Active transport mode after precedence chain resolution.</summary>
    public TransportMode TransportMode => _mode;

    /// <summary>Human-readable endpoint + transport label.</summary>
    public string EndpointDescription() => _transport.Describe();

    /// <summary>Release the persistent RPC socket (if any) and the HTTP client.</summary>
    public async ValueTask DisposeAsync()
    {
        if (_disposed) return;
        _disposed = true;
        await _transport.DisposeAsync().ConfigureAwait(false);
        _httpClient.Dispose();
        GC.SuppressFinalize(this);
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
    /// Executes a Cypher query via the active transport and returns
    /// the results. Works on both RPC and HTTP transports.
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
        var args = new List<NexusValue> { NexusValue.Str(query) };
        if (parameters != null)
            args.Add(CommandMap.JsonToNexus(parameters));

        TransportResponse resp;
        try
        {
            resp = await _transport.ExecuteAsync(
                new TransportRequest { Command = "CYPHER", Args = args },
                cancellationToken).ConfigureAwait(false);
        }
        catch (HttpRpcException e)
        {
            throw new NexusException($"CYPHER failed: HTTP {e.StatusCode}: {e.Body}");
        }

        var json = CommandMap.NexusToJson(resp.Value);
        if (json is not IDictionary<string, object?> obj)
            throw new NexusException($"CYPHER: expected object response, got {json?.GetType().Name}");

        var result = new QueryResult();
        if (obj.TryGetValue("columns", out var colsRaw) && colsRaw is IEnumerable<object?> cols)
        {
            var list = new List<string>();
            foreach (var c in cols) list.Add(c?.ToString() ?? "");
            result.Columns = list;
        }
        if (obj.TryGetValue("rows", out var rowsRaw) && rowsRaw is IEnumerable<object?> rows)
        {
            var list = new List<List<object?>>();
            foreach (var r in rows)
            {
                if (r is IEnumerable<object?> rr)
                {
                    var row = new List<object?>();
                    foreach (var cell in rr) row.Add(cell);
                    list.Add(row);
                }
            }
            result.Rows = list;
        }
        return result;
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
    /// Creates a new node with a caller-supplied external id.
    /// </summary>
    /// <param name="labels">Node labels.</param>
    /// <param name="properties">Node properties.</param>
    /// <param name="externalId">
    /// Prefixed string form: <c>sha256:&lt;hex&gt;</c>, <c>blake3:&lt;hex&gt;</c>,
    /// <c>sha512:&lt;hex&gt;</c>, <c>uuid:&lt;canonical&gt;</c>, <c>str:&lt;utf8&gt;</c>,
    /// <c>bytes:&lt;hex&gt;</c>.
    /// </param>
    /// <param name="conflictPolicy">
    /// <c>"error"</c> (default), <c>"match"</c>, or <c>"replace"</c>.
    /// Pass <see langword="null"/> to use the server default.
    /// </param>
    /// <param name="cancellationToken">Cancellation token.</param>
    /// <returns>Response containing the new node id.</returns>
    public async Task<CreateNodeResponse> CreateNodeWithExternalIdAsync(
        List<string> labels,
        Dictionary<string, object?> properties,
        string externalId,
        string? conflictPolicy = null,
        CancellationToken cancellationToken = default)
    {
        var requestBody = new CreateNodeRequest
        {
            Labels = labels,
            Properties = properties,
            ExternalId = externalId,
            ConflictPolicy = conflictPolicy,
        };

        var response = await DoRequestAsync(
            HttpMethod.Post, "/data/nodes", requestBody, cancellationToken);

        return await response.Content.ReadFromJsonAsync<CreateNodeResponse>(
            cancellationToken: cancellationToken)
            ?? throw new NexusException("Failed to deserialize create-node response");
    }

    /// <summary>
    /// Resolves a node by its external id.
    /// </summary>
    /// <param name="externalId">
    /// Prefixed string form matching what was supplied at creation time.
    /// </param>
    /// <param name="cancellationToken">Cancellation token.</param>
    /// <returns>
    /// Response whose <see cref="GetNodeByExternalIdResponse.Node"/> is
    /// <see langword="null"/> when no matching node exists.
    /// </returns>
    public async Task<GetNodeByExternalIdResponse> GetNodeByExternalIdAsync(
        string externalId,
        CancellationToken cancellationToken = default)
    {
        var path = $"/data/nodes/by-external-id?external_id={Uri.EscapeDataString(externalId)}";
        var response = await DoRequestAsync(HttpMethod.Get, path, null, cancellationToken);

        return await response.Content.ReadFromJsonAsync<GetNodeByExternalIdResponse>(
            cancellationToken: cancellationToken)
            ?? throw new NexusException("Failed to deserialize get-node-by-external-id response");
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
    /// <returns>
    /// List of <see cref="LabelInfo"/> entries (name + catalog id).
    /// Wire shape changed in nexus-server 1.15+ — see issue #2.
    /// </returns>
    public async Task<List<LabelInfo>> ListLabelsAsync(
        CancellationToken cancellationToken = default)
    {
        var response = await DoRequestAsync(
            HttpMethod.Get, "/schema/labels", null, cancellationToken);

        var result = await response.Content.ReadFromJsonAsync<Dictionary<string, List<LabelInfo>>>(
            cancellationToken: cancellationToken);

        return result?["labels"] ?? new List<LabelInfo>();
    }

    /// <summary>
    /// Retrieves all relationship types in the database.
    /// </summary>
    /// <param name="cancellationToken">Cancellation token.</param>
    /// <returns>
    /// List of <see cref="RelTypeInfo"/> entries. Server route is
    /// <c>/schema/rel_types</c> (this SDK previously used the
    /// non-existent <c>/schema/relationship-types</c>).
    /// </returns>
    public async Task<List<RelTypeInfo>> ListRelationshipTypesAsync(
        CancellationToken cancellationToken = default)
    {
        var response = await DoRequestAsync(
            HttpMethod.Get, "/schema/rel_types", null, cancellationToken);

        var result = await response.Content.ReadFromJsonAsync<Dictionary<string, List<RelTypeInfo>>>(
            cancellationToken: cancellationToken);

        return result?["types"] ?? new List<RelTypeInfo>();
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
