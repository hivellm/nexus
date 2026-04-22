using System.Net.Http.Headers;
using System.Text;
using System.Text.Json;

namespace Nexus.SDK.Transports;

/// <summary>
/// HTTP fallback transport — wraps <see cref="HttpClient"/> behind the
/// same <see cref="ITransport"/> interface the RPC path uses.
/// </summary>
public class HttpTransport : ITransport
{
    private readonly Endpoint _endpoint;
    private readonly Credentials _credentials;
    private readonly HttpClient _http;
    private readonly string _baseUrl;

    public HttpTransport(Endpoint endpoint, Credentials credentials, TimeSpan? timeout = null)
    {
        _endpoint = endpoint;
        _credentials = credentials;
        _baseUrl = endpoint.AsHttpUrl();
        _http = new HttpClient
        {
            BaseAddress = new Uri(_baseUrl),
            Timeout = timeout ?? TimeSpan.FromSeconds(30),
        };
        _http.DefaultRequestHeaders.Accept.Add(new MediaTypeWithQualityHeaderValue("application/json"));
        ApplyAuth(_http);
    }

    public string Describe() =>
        $"{_endpoint} ({(_endpoint.Scheme == "https" ? "HTTPS" : "HTTP")})";

    public bool IsRpc() => false;

    public ValueTask DisposeAsync()
    {
        _http.Dispose();
        return ValueTask.CompletedTask;
    }

    public async Task<TransportResponse> ExecuteAsync(
        TransportRequest request,
        CancellationToken cancellationToken = default)
    {
        var value = await DispatchAsync(request.Command, request.Args, cancellationToken).ConfigureAwait(false);
        return new TransportResponse { Value = value };
    }

    private void ApplyAuth(HttpClient client)
    {
        if (!string.IsNullOrEmpty(_credentials.ApiKey))
        {
            client.DefaultRequestHeaders.Add("X-API-Key", _credentials.ApiKey);
        }
        else if (!string.IsNullOrEmpty(_credentials.Username) &&
                 !string.IsNullOrEmpty(_credentials.Password))
        {
            var token = Convert.ToBase64String(
                Encoding.UTF8.GetBytes($"{_credentials.Username}:{_credentials.Password}"));
            client.DefaultRequestHeaders.Authorization = new AuthenticationHeaderValue("Basic", token);
        }
    }

    private async Task<NexusValue> DispatchAsync(
        string cmd, List<NexusValue> args, CancellationToken cancellationToken)
    {
        switch (cmd)
        {
            case "CYPHER":
                {
                    var query = args[0].AsString()
                        ?? throw new ArgumentException("CYPHER arg 0 must be a string");
                    var body = new Dictionary<string, object?> { ["query"] = query };
                    if (args.Count > 1)
                        body["parameters"] = CommandMap.NexusToJson(args[1]);
                    return await PostJsonAsync("/cypher", body, cancellationToken).ConfigureAwait(false);
                }
            case "PING":
            case "HEALTH":
                return await GetJsonAsync("/health", cancellationToken).ConfigureAwait(false);
            case "STATS":
                return await GetJsonAsync("/stats", cancellationToken).ConfigureAwait(false);
            case "DB_LIST":
                return await GetJsonAsync("/databases", cancellationToken).ConfigureAwait(false);
            case "DB_CREATE":
                {
                    var name = args[0].AsString()
                        ?? throw new ArgumentException("DB_CREATE arg 0 must be a string");
                    return await PostJsonAsync("/databases",
                        new Dictionary<string, object?> { ["name"] = name }, cancellationToken).ConfigureAwait(false);
                }
            case "DB_DROP":
                {
                    var name = args[0].AsString()
                        ?? throw new ArgumentException("DB_DROP arg 0 must be a string");
                    return await SendAsync(HttpMethod.Delete, $"/databases/{Uri.EscapeDataString(name)}",
                        null, cancellationToken).ConfigureAwait(false);
                }
            case "DB_USE":
                {
                    var name = args[0].AsString()
                        ?? throw new ArgumentException("DB_USE arg 0 must be a string");
                    return await SendAsync(HttpMethod.Put, "/session/database",
                        JsonSerializer.Serialize(new { name }), cancellationToken).ConfigureAwait(false);
                }
            case "DB_CURRENT":
                return await GetJsonAsync("/session/database", cancellationToken).ConfigureAwait(false);
            case "LABELS":
                return await GetJsonAsync("/schema/labels", cancellationToken).ConfigureAwait(false);
            case "REL_TYPES":
                return await GetJsonAsync("/schema/relationship-types", cancellationToken).ConfigureAwait(false);
        }
        throw new ArgumentException(
            $"HTTP fallback does not know how to route '{cmd}' — add an entry to sdks/csharp/Transports/HttpTransport.cs");
    }

    private async Task<NexusValue> GetJsonAsync(string path, CancellationToken cancellationToken)
    {
        using var resp = await _http.GetAsync(path, cancellationToken).ConfigureAwait(false);
        return await ReadJsonAsync(resp, cancellationToken).ConfigureAwait(false);
    }

    private async Task<NexusValue> PostJsonAsync(
        string path, object body, CancellationToken cancellationToken)
    {
        var json = JsonSerializer.Serialize(body);
        var content = new StringContent(json, Encoding.UTF8, "application/json");
        using var resp = await _http.PostAsync(path, content, cancellationToken).ConfigureAwait(false);
        return await ReadJsonAsync(resp, cancellationToken).ConfigureAwait(false);
    }

    private async Task<NexusValue> SendAsync(
        HttpMethod method, string path, string? body, CancellationToken cancellationToken)
    {
        using var msg = new HttpRequestMessage(method, path);
        if (body != null)
            msg.Content = new StringContent(body, Encoding.UTF8, "application/json");
        using var resp = await _http.SendAsync(msg, cancellationToken).ConfigureAwait(false);
        return await ReadJsonAsync(resp, cancellationToken).ConfigureAwait(false);
    }

    private static async Task<NexusValue> ReadJsonAsync(
        HttpResponseMessage resp, CancellationToken cancellationToken)
    {
        var text = await resp.Content.ReadAsStringAsync(cancellationToken).ConfigureAwait(false);
        if (!resp.IsSuccessStatusCode)
            throw new HttpRpcException((int)resp.StatusCode, text);
        if (string.IsNullOrEmpty(text)) return NexusValue.Null();
        try
        {
            using var doc = JsonDocument.Parse(text);
            return JsonElementToNexus(doc.RootElement);
        }
        catch
        {
            return NexusValue.Str(text);
        }
    }

    private static NexusValue JsonElementToNexus(JsonElement el)
    {
        switch (el.ValueKind)
        {
            case JsonValueKind.Null:
            case JsonValueKind.Undefined:
                return NexusValue.Null();
            case JsonValueKind.True: return NexusValue.Bool(true);
            case JsonValueKind.False: return NexusValue.Bool(false);
            case JsonValueKind.String: return NexusValue.Str(el.GetString() ?? "");
            case JsonValueKind.Number:
                if (el.TryGetInt64(out var i)) return NexusValue.Int(i);
                if (el.TryGetDouble(out var d)) return NexusValue.Float(d);
                return NexusValue.Null();
            case JsonValueKind.Array:
                {
                    var arr = new List<NexusValue>();
                    foreach (var item in el.EnumerateArray())
                        arr.Add(JsonElementToNexus(item));
                    return NexusValue.Array(arr);
                }
            case JsonValueKind.Object:
                {
                    var pairs = new List<(NexusValue, NexusValue)>();
                    foreach (var p in el.EnumerateObject())
                        pairs.Add((NexusValue.Str(p.Name), JsonElementToNexus(p.Value)));
                    return NexusValue.Map(pairs);
                }
        }
        return NexusValue.Null();
    }
}
