namespace Nexus.SDK.Transports;

/// <summary>
/// Options for <see cref="TransportFactory.Build"/>.
/// </summary>
public class TransportBuildOptions
{
    public string? BaseUrl { get; set; }
    public TransportMode? Transport { get; set; }
    public ushort? RpcPort { get; set; }
    public ushort? Resp3Port { get; set; }
    public TimeSpan? Timeout { get; set; }
    /// <summary>Injected NEXUS_SDK_TRANSPORT value for tests. Null reads from the env.</summary>
    public string? EnvTransport { get; set; }
}

/// <summary>
/// The resolved transport tuple returned by <see cref="TransportFactory.Build"/>.
/// </summary>
public class BuiltTransport
{
    public ITransport Transport { get; set; } = default!;
    public Endpoint Endpoint { get; set; } = default!;
    public TransportMode Mode { get; set; }
}

/// <summary>
/// Transport factory — applies the precedence chain and returns a
/// concrete transport instance.
/// </summary>
public static class TransportFactory
{
    /// <summary>
    /// Build the transport for the given options + credentials.
    ///
    /// Precedence (highest wins):
    /// <list type="number">
    /// <item>URL scheme in <c>BaseUrl</c> (<c>nexus://</c> → RPC, <c>http://</c> → HTTP, …)</item>
    /// <item><c>NEXUS_SDK_TRANSPORT</c> env var</item>
    /// <item><c>Transport</c> hint</item>
    /// <item>Default: <see cref="TransportMode.NexusRpc"/></item>
    /// </list>
    /// </summary>
    public static BuiltTransport Build(TransportBuildOptions opts, Credentials credentials)
    {
        var endpoint = string.IsNullOrEmpty(opts.BaseUrl)
            ? Endpoint.DefaultLocal()
            : Endpoint.Parse(opts.BaseUrl!);

        // 1. URL scheme wins.
        var mode = SchemeToMode(endpoint.Scheme);

        // 2. Env var overrides a bare URL (no scheme).
        var explicitScheme = !string.IsNullOrEmpty(opts.BaseUrl)
            && opts.BaseUrl!.Contains("://", StringComparison.Ordinal);
        var envRaw = opts.EnvTransport ?? Environment.GetEnvironmentVariable("NEXUS_SDK_TRANSPORT");
        var envMode = TransportModeParser.Parse(envRaw);
        if (envMode.HasValue && !explicitScheme)
        {
            mode = envMode.Value;
            endpoint = RealignEndpoint(endpoint, mode, opts);
        }

        // 3. Config hint.
        if (opts.Transport.HasValue && !explicitScheme && !envMode.HasValue)
        {
            mode = opts.Transport.Value;
            endpoint = RealignEndpoint(endpoint, mode, opts);
        }

        switch (mode)
        {
            case TransportMode.NexusRpc:
                return new BuiltTransport
                {
                    Transport = new RpcTransport(endpoint, credentials),
                    Endpoint = endpoint,
                    Mode = mode,
                };
            case TransportMode.Http:
            case TransportMode.Https:
                return new BuiltTransport
                {
                    Transport = new HttpTransport(endpoint, credentials, opts.Timeout),
                    Endpoint = endpoint,
                    Mode = mode,
                };
            case TransportMode.Resp3:
                throw new ArgumentException(
                    "resp3 transport is not yet shipped in the .NET SDK — use 'nexus' (RPC) or 'http' for now");
        }
        throw new InvalidOperationException($"unknown transport mode: {mode}");
    }

    private static TransportMode SchemeToMode(string scheme) => scheme switch
    {
        "nexus" => TransportMode.NexusRpc,
        "resp3" => TransportMode.Resp3,
        "https" => TransportMode.Https,
        _ => TransportMode.Http,
    };

    private static Endpoint RealignEndpoint(Endpoint ep, TransportMode mode, TransportBuildOptions opts)
    {
        return mode switch
        {
            TransportMode.NexusRpc => new Endpoint("nexus", ep.Host,
                opts.RpcPort ?? Endpoint.RpcDefaultPort),
            TransportMode.Resp3 => new Endpoint("resp3", ep.Host,
                opts.Resp3Port ?? Endpoint.Resp3DefaultPort),
            TransportMode.Https => new Endpoint("https", ep.Host, Endpoint.HttpsDefaultPort),
            _ => new Endpoint("http", ep.Host, Endpoint.HttpDefaultPort),
        };
    }
}
