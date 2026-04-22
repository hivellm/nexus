using MessagePack;

namespace Nexus.SDK.Transports;

/// <summary>
/// Wire codec for the Nexus RPC protocol.
///
/// The server-side types live in <c>nexus_protocol::rpc::types</c>.
/// rmp-serde encodes Rust enums using the externally-tagged
/// representation by default, which means:
///
/// <list type="bullet">
/// <item>Unit variants (<c>NexusValue::Null</c>) → the string <c>"Null"</c>.</item>
/// <item>Data-bearing variants (<c>NexusValue::Str("hi")</c>) → a single-key
///   map <c>{"Str": "hi"}</c>.</item>
/// <item><c>Result&lt;T, E&gt;</c> → <c>{"Ok": v}</c> / <c>{"Err": s}</c>.</item>
/// </list>
///
/// <see cref="ToWire"/> / <see cref="FromWire"/> translate between the
/// tagged .NET type and the on-wire shape.
/// <see cref="EncodeRequestFrame"/> / <see cref="DecodeResponseBody"/>
/// handle the <c>u32 LE length prefix + MessagePack body</c> frame format.
/// </summary>
public static class Codec
{
    /// <summary>Encode a <see cref="NexusValue"/> into its on-wire shape.</summary>
    public static object? ToWire(NexusValue v)
    {
        switch (v.Kind)
        {
            case NexusValueKind.Null:
                return "Null";
            case NexusValueKind.Bool:
                return new Dictionary<object, object?> { ["Bool"] = v.Value };
            case NexusValueKind.Int:
                return new Dictionary<object, object?> { ["Int"] = v.Value };
            case NexusValueKind.Float:
                return new Dictionary<object, object?> { ["Float"] = v.Value };
            case NexusValueKind.Bytes:
                return new Dictionary<object, object?> { ["Bytes"] = v.Value };
            case NexusValueKind.Str:
                return new Dictionary<object, object?> { ["Str"] = v.Value };
            case NexusValueKind.Array:
                var arr = (List<NexusValue>)v.Value!;
                var wireArr = new List<object?>(arr.Count);
                foreach (var e in arr) wireArr.Add(ToWire(e));
                return new Dictionary<object, object?> { ["Array"] = wireArr };
            case NexusValueKind.Map:
                var pairs = (List<(NexusValue Key, NexusValue Value)>)v.Value!;
                var wireMap = new List<object?>(pairs.Count);
                foreach (var (k, val) in pairs)
                    wireMap.Add(new List<object?> { ToWire(k), ToWire(val) });
                return new Dictionary<object, object?> { ["Map"] = wireMap };
            default:
                throw new InvalidOperationException($"unknown NexusValueKind '{v.Kind}'");
        }
    }

    /// <summary>Decode the wire-level shape back into a tagged <see cref="NexusValue"/>.</summary>
    public static NexusValue FromWire(object? raw)
    {
        switch (raw)
        {
            case null:
                return NexusValue.Null();
            case string s:
                return s == "Null" ? NexusValue.Null() : NexusValue.Str(s);
            case bool b:
                return NexusValue.Bool(b);
            case byte ub:
                return NexusValue.Int(ub);
            case sbyte sb:
                return NexusValue.Int(sb);
            case ushort us:
                return NexusValue.Int(us);
            case short ss:
                return NexusValue.Int(ss);
            case uint u32:
                return NexusValue.Int(u32);
            case int i32:
                return NexusValue.Int(i32);
            case ulong u64:
                return NexusValue.Int((long)u64);
            case long i64:
                return NexusValue.Int(i64);
            case float f32:
                return NexusValue.Float(f32);
            case double f64:
                return NexusValue.Float(f64);
            case byte[] bytes:
                return NexusValue.Bytes(bytes);
            case List<object?> list:
                {
                    var outArr = new List<NexusValue>(list.Count);
                    foreach (var e in list) outArr.Add(FromWire(e));
                    return NexusValue.Array(outArr);
                }
            case object[] arr:
                {
                    var outArr = new List<NexusValue>(arr.Length);
                    foreach (var e in arr) outArr.Add(FromWire(e));
                    return NexusValue.Array(outArr);
                }
            case IDictionary<object, object?> dict:
                return FromWireMap(dict);
            case IDictionary<string, object?> sDict:
                {
                    var convertedTag = new Dictionary<object, object?>();
                    foreach (var kv in sDict) convertedTag[kv.Key] = kv.Value;
                    return FromWireMap(convertedTag);
                }
        }
        throw new InvalidOperationException($"decode: unexpected NexusValue wire type {raw.GetType().Name}");
    }

    private static NexusValue FromWireMap(IDictionary<object, object?> dict)
    {
        if (dict.Count != 1)
            throw new InvalidOperationException(
                $"decode: expected single-key tagged NexusValue, got {dict.Count} keys");
        foreach (var kv in dict)
        {
            var tagStr = kv.Key as string ?? kv.Key.ToString();
            return FromTagged(tagStr!, kv.Value);
        }
        throw new InvalidOperationException("decode: empty tagged map");
    }

    private static NexusValue FromTagged(string tag, object? payload)
    {
        switch (tag)
        {
            case "Null": return NexusValue.Null();
            case "Bool": return NexusValue.Bool(Convert.ToBoolean(payload));
            case "Int": return NexusValue.Int(Convert.ToInt64(payload));
            case "Float": return NexusValue.Float(Convert.ToDouble(payload));
            case "Bytes":
                if (payload is byte[] b) return NexusValue.Bytes(b);
                if (payload is List<object?> lb)
                {
                    var bytes = new byte[lb.Count];
                    for (int i = 0; i < lb.Count; i++)
                        bytes[i] = Convert.ToByte(lb[i]);
                    return NexusValue.Bytes(bytes);
                }
                throw new InvalidOperationException("decode: Bytes payload must be bytes");
            case "Str": return NexusValue.Str(Convert.ToString(payload) ?? "");
            case "Array":
                if (payload is List<object?> larr)
                {
                    var outArr = new List<NexusValue>(larr.Count);
                    foreach (var e in larr) outArr.Add(FromWire(e));
                    return NexusValue.Array(outArr);
                }
                throw new InvalidOperationException("decode: Array payload must be list");
            case "Map":
                if (payload is List<object?> lmap)
                {
                    var pairs = new List<(NexusValue, NexusValue)>(lmap.Count);
                    foreach (var e in lmap)
                    {
                        if (e is List<object?> kv && kv.Count == 2)
                            pairs.Add((FromWire(kv[0]), FromWire(kv[1])));
                        else
                            throw new InvalidOperationException("decode: Map entry must be [key, value] pair");
                    }
                    return NexusValue.Map(pairs);
                }
                throw new InvalidOperationException("decode: Map payload must be list");
        }
        throw new InvalidOperationException($"decode: unknown NexusValue tag '{tag}'");
    }

    /// <summary>The decoded wire-level response.</summary>
    public class RpcResponse
    {
        public uint Id { get; set; }
        public bool Ok { get; set; }
        public NexusValue Value { get; set; }
        public string Err { get; set; } = "";

        public NexusValue Unwrap() =>
            Ok ? Value : throw new InvalidOperationException($"server: {Err}");
    }

    /// <summary>The wire-level request frame.</summary>
    public class RpcRequest
    {
        public uint Id { get; set; }
        public string Command { get; set; } = "";
        public List<NexusValue> Args { get; set; } = new();
    }

    /// <summary>
    /// Encode a request into a length-prefixed MessagePack frame:
    /// <c>u32_le(body_len) ++ msgpack(body)</c>.
    /// </summary>
    public static byte[] EncodeRequestFrame(RpcRequest req)
    {
        var args = new List<object?>(req.Args.Count);
        foreach (var a in req.Args) args.Add(ToWire(a));
        var body = MessagePackSerializer.Typeless.Serialize(new Dictionary<object, object?>
        {
            ["id"] = req.Id,
            ["command"] = req.Command,
            ["args"] = args,
        });

        var frame = new byte[4 + body.Length];
        frame[0] = (byte)(body.Length & 0xFF);
        frame[1] = (byte)((body.Length >> 8) & 0xFF);
        frame[2] = (byte)((body.Length >> 16) & 0xFF);
        frame[3] = (byte)((body.Length >> 24) & 0xFF);
        Buffer.BlockCopy(body, 0, frame, 4, body.Length);
        return frame;
    }

    /// <summary>
    /// Decode a response body (MessagePack bytes **after** the length
    /// prefix).
    /// </summary>
    public static RpcResponse DecodeResponseBody(byte[] body)
    {
        var raw = MessagePackSerializer.Typeless.Deserialize(body);
        if (raw is not IDictionary<object, object?> map)
            throw new InvalidOperationException($"decode: response must be a map, got {raw?.GetType().Name}");

        if (!map.TryGetValue("id", out var idRaw))
            throw new InvalidOperationException("decode: response missing 'id'");
        var id = Convert.ToUInt32(idRaw);

        if (!map.TryGetValue("result", out var resultRaw))
            throw new InvalidOperationException("decode: response missing 'result'");
        if (resultRaw is not IDictionary<object, object?> resultMap || resultMap.Count != 1)
            throw new InvalidOperationException("decode: Result must be a single-key tagged map");

        foreach (var kv in resultMap)
        {
            var tag = kv.Key as string ?? kv.Key.ToString();
            switch (tag)
            {
                case "Ok":
                    return new RpcResponse { Id = id, Ok = true, Value = FromWire(kv.Value) };
                case "Err":
                    return new RpcResponse { Id = id, Ok = false, Err = Convert.ToString(kv.Value) ?? "" };
                default:
                    throw new InvalidOperationException(
                        $"decode: Result must be 'Ok' or 'Err', got '{tag}'");
            }
        }
        throw new InvalidOperationException("decode: empty result map");
    }
}
