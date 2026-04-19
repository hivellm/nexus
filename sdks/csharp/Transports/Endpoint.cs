namespace Nexus.SDK.Transports;

/// <summary>
/// A parsed endpoint URL. Mirrors <c>nexus-cli/src/endpoint.rs</c> and
/// <c>sdks/rust/src/transport/endpoint.rs</c> — same URL grammar so
/// users can copy-paste endpoints between languages.
/// </summary>
public class Endpoint
{
    public const ushort RpcDefaultPort = 15475;
    public const ushort HttpDefaultPort = 15474;
    public const ushort HttpsDefaultPort = 443;
    public const ushort Resp3DefaultPort = 15476;

    public string Scheme { get; }
    public string Host { get; }
    public ushort Port { get; }

    public Endpoint(string scheme, string host, ushort port)
    {
        Scheme = scheme;
        Host = host;
        Port = port;
    }

    /// <summary><c>nexus://127.0.0.1:15475</c> — the SDK's default.</summary>
    public static Endpoint DefaultLocal() =>
        new("nexus", "127.0.0.1", RpcDefaultPort);

    public string Authority => $"{Host}:{Port}";

    public override string ToString() => $"{Scheme}://{Authority}";

    /// <summary>
    /// Render the endpoint as an HTTP URL. <c>nexus://</c> and
    /// <c>resp3://</c> schemes swap to the sibling HTTP port (15474).
    /// </summary>
    public string AsHttpUrl() => Scheme switch
    {
        "http" => $"http://{Authority}",
        "https" => $"https://{Authority}",
        _ => $"http://{Host}:{HttpDefaultPort}",
    };

    public bool IsRpc() => Scheme == "nexus";

    /// <summary>
    /// Parse any of the accepted URL forms. Rejects <c>nexus-rpc://</c>
    /// explicitly — the single canonical token is <c>nexus</c>.
    /// </summary>
    public static Endpoint Parse(string raw)
    {
        if (raw is null) throw new ArgumentNullException(nameof(raw));
        var trimmed = raw.Trim();
        if (trimmed.Length == 0)
            throw new ArgumentException("endpoint URL must not be empty", nameof(raw));

        var sepIdx = trimmed.IndexOf("://", StringComparison.Ordinal);
        if (sepIdx != -1)
        {
            var schemeRaw = trimmed.Substring(0, sepIdx).ToLowerInvariant();
            var rest = trimmed.Substring(sepIdx + 3).TrimEnd('/');
            string scheme;
            ushort defaultPort;
            switch (schemeRaw)
            {
                case "nexus": scheme = "nexus"; defaultPort = RpcDefaultPort; break;
                case "http": scheme = "http"; defaultPort = HttpDefaultPort; break;
                case "https": scheme = "https"; defaultPort = HttpsDefaultPort; break;
                case "resp3": scheme = "resp3"; defaultPort = Resp3DefaultPort; break;
                default:
                    throw new ArgumentException(
                        $"unsupported URL scheme '{schemeRaw}://' (expected 'nexus://', 'http://', 'https://', or 'resp3://')",
                        nameof(raw));
            }
            var (host, port) = SplitHostPort(rest);
            return new Endpoint(scheme, host, port ?? defaultPort);
        }

        var (h, p) = SplitHostPort(trimmed);
        return new Endpoint("nexus", h, p ?? RpcDefaultPort);
    }

    private static (string host, ushort? port) SplitHostPort(string s)
    {
        if (string.IsNullOrEmpty(s))
            throw new ArgumentException("missing host");
        if (s.StartsWith("[", StringComparison.Ordinal))
        {
            var end = s.IndexOf(']');
            if (end == -1)
                throw new ArgumentException($"unterminated IPv6 literal in '{s}'");
            var host = s.Substring(1, end - 1);
            var tail = s.Substring(end + 1);
            if (tail.Length == 0) return (host, null);
            if (!tail.StartsWith(":", StringComparison.Ordinal))
                throw new ArgumentException($"unexpected characters after IPv6 literal: '{tail}'");
            return (host, ParsePort(tail.Substring(1)));
        }
        var colonIdx = s.LastIndexOf(':');
        if (colonIdx == -1) return (s, null);
        var hostPart = s.Substring(0, colonIdx);
        if (hostPart.Length == 0)
            throw new ArgumentException($"missing host in '{s}'");
        return (hostPart, ParsePort(s.Substring(colonIdx + 1)));
    }

    private static ushort ParsePort(string s)
    {
        if (!int.TryParse(s, out var n) || n < 0 || n > 65535)
            throw new ArgumentException($"invalid port '{s}': must be 0..=65535");
        return (ushort)n;
    }
}
