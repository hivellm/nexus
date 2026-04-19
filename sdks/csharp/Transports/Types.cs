namespace Nexus.SDK.Transports;

/// <summary>
/// Transport selector. Values match the URL-scheme tokens and the
/// <c>NEXUS_SDK_TRANSPORT</c> env-var strings.
/// </summary>
public enum TransportMode
{
    /// <summary>Native binary RPC — length-prefixed MessagePack on port 15475. Default.</summary>
    NexusRpc,
    /// <summary>RESP3 on port 15476. Reserved; not yet shipped in the SDK.</summary>
    Resp3,
    /// <summary>HTTP/JSON on port 15474.</summary>
    Http,
    /// <summary>HTTPS/JSON on port 443.</summary>
    Https,
}

/// <summary>
/// Helpers for <see cref="TransportMode"/>.
/// </summary>
public static class TransportModeParser
{
    /// <summary>
    /// Parse the <c>NEXUS_SDK_TRANSPORT</c> env-var token.
    /// Accepts canonical values plus the <c>rpc</c> / <c>nexusrpc</c>
    /// aliases for ergonomics. Returns <c>null</c> for empty / <c>auto</c> /
    /// unknown tokens.
    /// </summary>
    public static TransportMode? Parse(string? raw)
    {
        if (string.IsNullOrWhiteSpace(raw)) return null;
        var v = raw.Trim().ToLowerInvariant();
        return v switch
        {
            "nexus" or "rpc" or "nexusrpc" => TransportMode.NexusRpc,
            "resp3" => TransportMode.Resp3,
            "http" => TransportMode.Http,
            "https" => TransportMode.Https,
            "auto" or "" => null,
            _ => null,
        };
    }

    /// <summary>Canonical string for a mode.</summary>
    public static string ToWireString(this TransportMode mode) => mode switch
    {
        TransportMode.NexusRpc => "nexus",
        TransportMode.Resp3 => "resp3",
        TransportMode.Http => "http",
        TransportMode.Https => "https",
        _ => throw new ArgumentOutOfRangeException(nameof(mode)),
    };
}

/// <summary>
/// Discriminator for <see cref="NexusValue"/>.
/// </summary>
public enum NexusValueKind
{
    Null,
    Bool,
    Int,
    Float,
    Bytes,
    Str,
    Array,
    Map,
}

/// <summary>
/// Dynamically-typed value carried by RPC requests and responses.
/// Mirrors <c>nexus_protocol::rpc::types::NexusValue</c>.
/// </summary>
public readonly struct NexusValue
{
    public NexusValueKind Kind { get; }
    public object? Value { get; }

    public NexusValue(NexusValueKind kind, object? value)
    {
        Kind = kind;
        Value = value;
    }

    public static NexusValue Null() => new(NexusValueKind.Null, null);
    public static NexusValue Bool(bool v) => new(NexusValueKind.Bool, v);
    public static NexusValue Int(long v) => new(NexusValueKind.Int, v);
    public static NexusValue Float(double v) => new(NexusValueKind.Float, v);
    public static NexusValue Bytes(byte[] v) => new(NexusValueKind.Bytes, v);
    public static NexusValue Str(string v) => new(NexusValueKind.Str, v);
    public static NexusValue Array(List<NexusValue> v) => new(NexusValueKind.Array, v);
    public static NexusValue Map(List<(NexusValue Key, NexusValue Value)> pairs) =>
        new(NexusValueKind.Map, pairs);

    public string? AsString() =>
        Kind == NexusValueKind.Str ? (string?)Value : null;

    public long? AsInt() =>
        Kind == NexusValueKind.Int ? (long?)Value : null;
}

/// <summary>
/// Credentials carried by a transport. Both paths may be set; APIKey wins.
/// </summary>
public class Credentials
{
    public string? ApiKey { get; set; }
    public string? Username { get; set; }
    public string? Password { get; set; }

    public bool HasAny() =>
        !string.IsNullOrEmpty(ApiKey) ||
        (!string.IsNullOrEmpty(Username) && !string.IsNullOrEmpty(Password));
}

/// <summary>A single request against the active transport.</summary>
public class TransportRequest
{
    public string Command { get; set; } = "";
    public List<NexusValue> Args { get; set; } = new();
}

/// <summary>A single response from the active transport.</summary>
public class TransportResponse
{
    public NexusValue Value { get; set; }
}

/// <summary>Generic transport interface.</summary>
public interface ITransport : IAsyncDisposable
{
    Task<TransportResponse> ExecuteAsync(TransportRequest request, CancellationToken cancellationToken = default);
    string Describe();
    bool IsRpc();
}

/// <summary>
/// Structured HTTP error surfaced by <see cref="HttpTransport"/> on
/// non-2xx responses. Callers can type-check with <c>catch (HttpRpcException)</c>
/// to recover the status code without parsing error strings.
/// </summary>
public class HttpRpcException : Exception
{
    public int StatusCode { get; }
    public string Body { get; }

    public HttpRpcException(int statusCode, string body)
        : base($"HTTP {statusCode}: {body}")
    {
        StatusCode = statusCode;
        Body = body;
    }
}
