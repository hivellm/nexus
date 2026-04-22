using System.Buffers.Binary;
using System.Collections.Concurrent;
using System.Net.Sockets;

namespace Nexus.SDK.Transports;

/// <summary>
/// Native binary RPC transport — single-socket async implementation.
///
/// Holds one TCP stream per client (frames cannot interleave). Writes
/// are serialised through <see cref="_writeLock"/>; a single reader
/// task multiplexes responses back to pending <see cref="TaskCompletionSource"/>s
/// keyed by request id. HELLO+AUTH handshake runs on connect.
/// Monotonic <c>uint32</c> ids skip the reserved <c>PUSH_ID</c>.
/// </summary>
public class RpcTransport : ITransport
{
    private const uint PushId = 0xFFFFFFFFu;

    private readonly Endpoint _endpoint;
    private readonly Credentials _credentials;
    private readonly TimeSpan _connectTimeout;

    private readonly SemaphoreSlim _connectLock = new(1, 1);
    private readonly SemaphoreSlim _writeLock = new(1, 1);
    private readonly ConcurrentDictionary<uint, TaskCompletionSource<Codec.RpcResponse>> _pending = new();

    private TcpClient? _tcp;
    private NetworkStream? _stream;
    private Task? _readerTask;
    private uint _nextId = 1;
    private bool _closed;

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
        _closed = true;
        try
        {
            _stream?.Close();
            _tcp?.Close();
            if (_readerTask is not null) await _readerTask.ConfigureAwait(false);
        }
        catch { }
        FailAll(new IOException("RPC transport closed"));
    }

    public async Task<TransportResponse> ExecuteAsync(
        TransportRequest request,
        CancellationToken cancellationToken = default)
    {
        var resp = await CallAsync(request.Command, request.Args, cancellationToken).ConfigureAwait(false);
        return new TransportResponse { Value = resp.Unwrap() };
    }

    public async Task<Codec.RpcResponse> CallAsync(
        string command,
        List<NexusValue> args,
        CancellationToken cancellationToken = default)
    {
        await EnsureConnectedAsync(cancellationToken).ConfigureAwait(false);
        var id = AllocId();
        return await SendAsync(new Codec.RpcRequest { Id = id, Command = command, Args = args },
            cancellationToken).ConfigureAwait(false);
    }

    // ── Internals ──────────────────────────────────────────────────────

    private uint AllocId()
    {
        lock (_connectLock)
        {
            var id = _nextId++;
            if (id == PushId) id = _nextId++;
            if (_nextId >= 0xFFFFFFFE) _nextId = 1;
            return id;
        }
    }

    private async Task EnsureConnectedAsync(CancellationToken cancellationToken)
    {
        if (_tcp is { Connected: true } && !_closed) return;
        if (_closed) throw new IOException("RPC transport closed");

        await _connectLock.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            if (_tcp is { Connected: true } && !_closed) return;
            await ConnectAsync(cancellationToken).ConfigureAwait(false);
        }
        finally
        {
            _connectLock.Release();
        }
    }

    private async Task ConnectAsync(CancellationToken cancellationToken)
    {
        var tcp = new TcpClient { NoDelay = true };
        using var linked = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        linked.CancelAfter(_connectTimeout);
        try
        {
            await tcp.ConnectAsync(_endpoint.Host, _endpoint.Port, linked.Token).ConfigureAwait(false);
        }
        catch (Exception e)
        {
            throw new IOException($"failed to connect to {_endpoint.Authority}: {e.Message}", e);
        }

        _tcp = tcp;
        _stream = tcp.GetStream();
        _readerTask = Task.Run(() => ReadLoopAsync(_stream!));

        // HELLO handshake.
        var hello = await SendAsync(new Codec.RpcRequest
        {
            Id = 0,
            Command = "HELLO",
            Args = new List<NexusValue> { NexusValue.Int(1) },
        }, cancellationToken).ConfigureAwait(false);
        if (!hello.Ok)
            throw new IOException($"HELLO rejected by server: {hello.Err}");

        if (_credentials.HasAny())
        {
            var args = _credentials.ApiKey is { Length: > 0 }
                ? new List<NexusValue> { NexusValue.Str(_credentials.ApiKey) }
                : new List<NexusValue>
                {
                    NexusValue.Str(_credentials.Username ?? ""),
                    NexusValue.Str(_credentials.Password ?? ""),
                };
            var auth = await SendAsync(new Codec.RpcRequest
            {
                Id = 0,
                Command = "AUTH",
                Args = args,
            }, cancellationToken).ConfigureAwait(false);
            if (!auth.Ok)
                throw new IOException($"authentication failed: {auth.Err}");
        }
    }

    private async Task<Codec.RpcResponse> SendAsync(Codec.RpcRequest req, CancellationToken cancellationToken)
    {
        if (_stream is null) throw new IOException("RPC transport is not connected");
        var tcs = new TaskCompletionSource<Codec.RpcResponse>(TaskCreationOptions.RunContinuationsAsynchronously);
        _pending[req.Id] = tcs;

        var frame = Codec.EncodeRequestFrame(req);
        await _writeLock.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            await _stream.WriteAsync(frame, cancellationToken).ConfigureAwait(false);
            await _stream.FlushAsync(cancellationToken).ConfigureAwait(false);
        }
        catch (Exception e)
        {
            _pending.TryRemove(req.Id, out _);
            throw new IOException($"failed to send RPC frame: {e.Message}", e);
        }
        finally
        {
            _writeLock.Release();
        }

        using var reg = cancellationToken.Register(() =>
        {
            if (_pending.TryRemove(req.Id, out var t))
                t.TrySetCanceled(cancellationToken);
        });
        return await tcs.Task.ConfigureAwait(false);
    }

    private async Task ReadLoopAsync(NetworkStream stream)
    {
        var header = new byte[4];
        try
        {
            while (!_closed)
            {
                var read = await ReadExactAsync(stream, header, 4).ConfigureAwait(false);
                if (!read) { FailAll(new IOException("RPC connection closed")); return; }
                var length = BinaryPrimitives.ReadUInt32LittleEndian(header);
                var body = new byte[length];
                if (!await ReadExactAsync(stream, body, (int)length).ConfigureAwait(false))
                {
                    FailAll(new IOException("RPC connection closed"));
                    return;
                }
                Codec.RpcResponse resp;
                try
                {
                    resp = Codec.DecodeResponseBody(body);
                }
                catch (Exception e)
                {
                    FailAll(new IOException($"malformed RPC frame: {e.Message}", e));
                    return;
                }
                if (_pending.TryRemove(resp.Id, out var tcs))
                    tcs.TrySetResult(resp);
                // Unknown ids (including PUSH_ID) are dropped.
            }
        }
        catch (Exception e)
        {
            FailAll(new IOException($"RPC socket error: {e.Message}", e));
        }
    }

    private static async Task<bool> ReadExactAsync(NetworkStream s, byte[] buf, int count)
    {
        int total = 0;
        while (total < count)
        {
            int n = await s.ReadAsync(buf.AsMemory(total, count - total)).ConfigureAwait(false);
            if (n == 0) return false;
            total += n;
        }
        return true;
    }

    private void FailAll(Exception e)
    {
        foreach (var kv in _pending)
        {
            if (_pending.TryRemove(kv.Key, out var t))
                t.TrySetException(e);
        }
    }
}
