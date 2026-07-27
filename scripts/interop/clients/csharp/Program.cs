// Interop cell: Nexus C# SDK (Nexus.SDK.Transports.RpcTransport) against a
// Thunder-based server.
//
// Drives RpcTransport directly rather than the sugar layer (NexusClient) —
// the matrix is about the wire, and the transport is where the wire lives.
//
//     argv:   <host> <port> <user> <pass>
//     stdout: one `STEP <name> PASS|FAIL <detail>` line per step
//     exit:   0 iff every step passed

using Nexus.SDK.Transports;
using ThunderAuthException = HiveLLM.Thunder.ThunderAuthException;

const long Marker = 424204;

// A vector whose f32-LE encoding is emphatically not valid UTF-8, so a
// transport that quietly round-trips Bytes through a string cannot pass
// the knn_bytes cell.
var vec = new[] { 1.5f, -2.5f, 3.5f, float.PositiveInfinity };
var vecBytes = EncodeFloatsLittleEndian(vec);

var host = args[0];
var port = ushort.Parse(args[1]);
var user = args[2];
var password = args[3];
var endpoint = new Endpoint("nexus", host, port);

var failures = 0;

// 1. auth — PING answers before auth; STATS is refused before AUTH and
//    succeeds after. An unauthenticated transport is one built with no
//    credentials.
RpcTransport authed;
try
{
    var anon = new RpcTransport(endpoint, new Credentials());
    var pong = await anon.ExecuteAsync(new TransportRequest { Command = "PING" });
    var pingOk = pong.Value.Kind == NexusValueKind.Str && (string?)pong.Value.Value == "PONG";

    var statsPreRefused = false;
    try
    {
        await anon.ExecuteAsync(new TransportRequest { Command = "STATS" });
    }
    catch (Exception exc)
    {
        var msg = exc.Message ?? "";
        statsPreRefused = exc is ThunderAuthException
            || msg.ToLowerInvariant().Contains("auth")
            || msg.Contains("NOAUTH");
    }
    await anon.DisposeAsync();

    authed = new RpcTransport(endpoint, new Credentials { Username = user, Password = password });
    var stats = await authed.ExecuteAsync(new TransportRequest { Command = "STATS" });
    var statsPostOk = stats.Value.Kind is NexusValueKind.Map or NexusValueKind.Str;

    var ok = pingOk && statsPreRefused && statsPostOk;
    Report("auth", ok, $"ping={pingOk} stats_pre_refused={statsPreRefused} stats_post={statsPostOk}");
    failures += ok ? 0 : 1;
}
catch (Exception exc)
{
    Report("auth", false, $"{exc.GetType().Name}: {exc.Message}");
    return 1;
}

// 2. cypher — CREATE then MATCH round-trips the id back.
try
{
    var idParams = new List<(NexusValue, NexusValue)> { (NexusValue.Str("id"), NexusValue.Int(Marker)) };
    await CypherRowsAsync(authed, "CREATE (n:InteropCs {id: $id}) RETURN n.id", idParams);
    var rows = await CypherRowsAsync(authed, "MATCH (n:InteropCs {id: $id}) RETURN n.id", idParams);
    var cell = rows.Count > 0 && rows[0].Count > 0 ? rows[0][0] : (NexusValue?)null;
    long? got = cell is NexusValue c && c.Kind == NexusValueKind.Int ? (long)c.Value! : null;
    var ok = got == Marker;
    Report("cypher", ok, $"round-trip id -> {got?.ToString() ?? "null"}");
    failures += ok ? 0 : 1;
}
catch (Exception exc)
{
    Report("cypher", false, $"{exc.GetType().Name}: {exc.Message}");
    failures++;
}

// 3. knn_bytes — a raw f32-LE vector carried as Bytes round-trips
//    byte-for-byte (PING echoes its argument).
try
{
    var echoed = await authed.ExecuteAsync(
        new TransportRequest { Command = "PING", Args = new List<NexusValue> { NexusValue.Bytes(vecBytes) } });
    var got = echoed.Value.Kind == NexusValueKind.Bytes ? (byte[])echoed.Value.Value! : [];
    var ok = got.AsSpan().SequenceEqual(vecBytes) && vecBytes.Length == 4 * vec.Length;
    Report("knn_bytes", ok, $"{Convert.ToHexString(vecBytes).ToLowerInvariant()} -> {Convert.ToHexString(got).ToLowerInvariant()}");
    failures += ok ? 0 : 1;
}
catch (Exception exc)
{
    Report("knn_bytes", false, $"{exc.GetType().Name}: {exc.Message}");
    failures++;
}

// 4. error — a broken CYPHER surfaces a typed server error, not a
//    transport crash, and the same connection stays usable.
try
{
    try
    {
        await CypherRowsAsync(authed, "MATCH (n RETURN", null);
        Report("error", false, "expected a server error, got a result");
        failures++;
    }
    catch (Exception exc)
    {
        var pong = await authed.ExecuteAsync(new TransportRequest { Command = "PING" });
        var alive = pong.Value.Kind == NexusValueKind.Str && (string?)pong.Value.Value == "PONG";
        Report("error", alive, $"raised {exc.GetType().Name}; connection alive={alive}");
        failures += alive ? 0 : 1;
    }
}
finally
{
    await authed.DisposeAsync();
}

return failures > 0 ? 1 : 0;

// ── Helpers ──────────────────────────────────────────────────────────────

static void Report(string step, bool ok, string detail) =>
    Console.WriteLine($"STEP {step} {(ok ? "PASS" : "FAIL")} {detail}");

/// Little-endian f32 encoding of a vector, matching the wire format the
/// server expects for raw embedding bytes.
static byte[] EncodeFloatsLittleEndian(float[] values)
{
    var bytes = new byte[values.Length * 4];
    for (var i = 0; i < values.Length; i++)
    {
        var b = BitConverter.GetBytes(values[i]);
        if (!BitConverter.IsLittleEndian) Array.Reverse(b);
        Array.Copy(b, 0, bytes, i * 4, 4);
    }
    return bytes;
}

/// Look up a string key in a Map-kind NexusValue.
static NexusValue? MapGet(NexusValue container, string key)
{
    if (container.Kind != NexusValueKind.Map) return null;
    foreach (var (k, v) in (List<(NexusValue Key, NexusValue Value)>)container.Value!)
    {
        if (k.Kind == NexusValueKind.Str && (string?)k.Value == key) return v;
    }
    return null;
}

/// Run CYPHER over the transport and return rows as NexusValue cells.
static async Task<List<List<NexusValue>>> CypherRowsAsync(
    RpcTransport transport, string query, List<(NexusValue, NexusValue)>? parameters)
{
    var callArgs = new List<NexusValue> { NexusValue.Str(query) };
    if (parameters is { Count: > 0 }) callArgs.Add(NexusValue.Map(parameters));

    var response = await transport.ExecuteAsync(new TransportRequest { Command = "CYPHER", Args = callArgs });

    var errorField = MapGet(response.Value, "error");
    if (errorField is NexusValue ev && ev.Kind == NexusValueKind.Str && !string.IsNullOrEmpty((string?)ev.Value))
        throw new InvalidOperationException((string)ev.Value!);

    var rowsField = MapGet(response.Value, "rows");
    if (rowsField is not NexusValue rf || rf.Kind != NexusValueKind.Array)
        return [];

    var rows = new List<List<NexusValue>>();
    foreach (var row in (List<NexusValue>)rf.Value!)
    {
        if (row.Kind == NexusValueKind.Array) rows.Add((List<NexusValue>)row.Value!);
    }
    return rows;
}
