using TClient = HiveLLM.Thunder.ThunderClient;
using TValue = HiveLLM.Thunder.Value;
using TValueKind = HiveLLM.Thunder.ValueKind;
using TConfig = HiveLLM.Thunder.Config;
using TClientConfig = HiveLLM.Thunder.ClientConfig;
using TCredentials = HiveLLM.Thunder.Credentials;
using Handshake = HiveLLM.Thunder.Handshake;
using HelloStyle = HiveLLM.Thunder.HelloStyle;

namespace Nexus.SDK.Transports;

/// <summary>
/// Native binary RPC transport — a thin wrapper around the Thunder client.
///
/// Thunder (<c>HiveLLM.Thunder</c>) owns the wire (frame + MessagePack
/// codec), the single-connection multiplexer, and the handshake / reconnect
/// / credential re-send. This class adapts the SDK's value model
/// (<see cref="NexusValue"/>) to Thunder's and preserves the
/// <see cref="ITransport"/> contract the rest of the SDK depends on.
///
/// Handshake: the Nexus server ignores HELLO arguments and gates on a
/// separate AUTH, so Thunder's <c>AuthCommand</c> handshake with the
/// <c>ArgLess</c> HELLO style (bare HELLO [] then AUTH [...]) matches the
/// wire and re-authenticates on reconnect.
/// </summary>
public class RpcTransport : ITransport
{
    private readonly Endpoint _endpoint;
    private readonly Credentials _credentials;
    private readonly TimeSpan _connectTimeout;
    private readonly SemaphoreSlim _connectLock = new(1, 1);
    private TClient? _client;

    public RpcTransport(Endpoint endpoint, Credentials credentials, TimeSpan? connectTimeout = null)
    {
        _endpoint = endpoint;
        _credentials = credentials;
        _connectTimeout = connectTimeout ?? TimeSpan.FromSeconds(5);
    }

    public string Describe() => $"{_endpoint} (RPC)";
    public bool IsRpc() => true;

    public async ValueTask DisposeAsync()
    {
        var client = _client;
        _client = null;
        if (client is not null) await client.DisposeAsync().ConfigureAwait(false);
        GC.SuppressFinalize(this);
    }

    public async Task<TransportResponse> ExecuteAsync(
        TransportRequest request,
        CancellationToken cancellationToken = default)
    {
        var client = await EnsureConnectedAsync(cancellationToken).ConfigureAwait(false);
        var args = request.Args.Select(NexusToThunder).ToArray();
        // CallAsync returns the Ok payload directly and throws a typed
        // Thunder exception on Err; the SDK's higher layers surface those.
        var result = await client.CallAsync(request.Command, args, cancellationToken).ConfigureAwait(false);
        return new TransportResponse { Value = ThunderToNexus(result) };
    }

    // ── Value adapters ──────────────────────────────────────────────────

    private static TValue NexusToThunder(NexusValue v) => v.Kind switch
    {
        NexusValueKind.Null => TValue.Null,
        NexusValueKind.Bool => TValue.Bool((bool)v.Value!),
        NexusValueKind.Int => TValue.Int((long)v.Value!),
        NexusValueKind.Float => TValue.Float((double)v.Value!),
        NexusValueKind.Bytes => TValue.Bytes((byte[])v.Value!),
        NexusValueKind.Str => TValue.Str((string)v.Value!),
        NexusValueKind.Array => TValue.Array(((List<NexusValue>)v.Value!).Select(NexusToThunder)),
        NexusValueKind.Map => TValue.Map(
            ((List<(NexusValue Key, NexusValue Value)>)v.Value!).Select(
                p => new KeyValuePair<TValue, TValue>(NexusToThunder(p.Key), NexusToThunder(p.Value)))),
        _ => TValue.Null,
    };

    private static NexusValue ThunderToNexus(TValue v) => v.Kind switch
    {
        TValueKind.Null => NexusValue.Null(),
        TValueKind.Bool => NexusValue.Bool(v.AsBool() ?? false),
        TValueKind.Int => NexusValue.Int(v.AsInt() ?? 0L),
        TValueKind.Float => NexusValue.Float(v.AsFloat() ?? 0d),
        TValueKind.Bytes => NexusValue.Bytes(v.AsBytes() ?? Array.Empty<byte>()),
        TValueKind.Str => NexusValue.Str(v.AsStr() ?? ""),
        TValueKind.Array => NexusValue.Array(
            (v.AsArray() ?? new List<TValue>()).Select(ThunderToNexus).ToList()),
        TValueKind.Map => NexusValue.Map(
            (v.AsMap() ?? new List<KeyValuePair<TValue, TValue>>())
                .Select(p => (ThunderToNexus(p.Key), ThunderToNexus(p.Value)))
                .ToList()),
        _ => NexusValue.Null(),
    };

    // ── Connection ──────────────────────────────────────────────────────

    private TConfig BuildConfig() => TConfig.Standard() with
    {
        Scheme = "nexus",
        DefaultPort = _endpoint.Port,
        Handshake = Handshake.AuthCommand,
        HelloStyle = HelloStyle.ArgLess,
    };

    private TCredentials? BuildCredentials()
    {
        if (!string.IsNullOrEmpty(_credentials.ApiKey))
            return TCredentials.ApiKey(_credentials.ApiKey!);
        if (!string.IsNullOrEmpty(_credentials.Username) && !string.IsNullOrEmpty(_credentials.Password))
            return TCredentials.UserPass(_credentials.Username!, _credentials.Password!);
        return null;
    }

    private async Task<TClient> EnsureConnectedAsync(CancellationToken cancellationToken)
    {
        if (_client is not null) return _client;
        await _connectLock.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            if (_client is not null) return _client;
            var clientConfig = new TClientConfig
            {
                ConnectTimeout = _connectTimeout,
                Credentials = BuildCredentials(),
            };
            // Bare host:port endpoint sidesteps Thunder's scheme matching.
            _client = await TClient
                .ConnectAsync(_endpoint.Authority, BuildConfig(), clientConfig, cancellationToken)
                .ConfigureAwait(false);
            return _client;
        }
        finally
        {
            _connectLock.Release();
        }
    }
}
